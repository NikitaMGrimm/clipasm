use std::path::PathBuf;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{DurationValue, ExactNumber, TimelineExpression, TimelineViewId};
use crate::program::{ParameterType, ParameterValue, TimeRangeValue};
use crate::source::{
    Literal, ScalarBinaryOperator, ScalarExpression, ScalarPostfixOperator, ScalarUnaryOperator,
    SourceSpan, Spanned,
};

use super::checked::{CheckedScalarExpression, ParameterId, ReferenceTarget, ScalarAliasId};

mod check;
mod duration;

pub(super) use check::{check_expression, check_inferred_expression};
use duration::{
    duration_family_mismatch, duration_timeline_expression, is_number_text, looks_like_duration,
    parse_duration, parse_duration_range_text, parse_duration_text, parse_frame_duration,
    refine_duration, refine_duration_range,
};

pub(super) enum ScalarReference {
    Parameter(ParameterId, ParameterType),
    Alias(ScalarAliasId, ScalarKind),
}

type ScalarResolver<'a> = dyn FnMut(&Spanned<String>) -> Result<ScalarReference> + 'a;
type TimelineResolver<'a> = dyn FnMut(&Spanned<String>) -> Result<ReferenceTarget> + 'a;
type SelectorEvaluator<'a> = dyn FnMut(ReferenceTarget, &str, &[String], bool, &SourceSpan) -> Result<TimelineSelectorValue>
    + 'a;

pub(super) enum TimelineSelectorValue {
    Coordinate {
        owner: TimelineViewId,
        expression: TimelineExpression,
        layout: String,
    },
    Range {
        owner: TimelineViewId,
        start: TimelineExpression,
        end: TimelineExpression,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DurationFamily {
    WallClock,
    ProjectFrames,
    Either,
}

impl DurationFamily {
    const fn compatible(self, other: Self) -> Option<Self> {
        match (self, other) {
            (Self::WallClock, Self::WallClock) => Some(Self::WallClock),
            (Self::ProjectFrames, Self::ProjectFrames) => Some(Self::ProjectFrames),
            (Self::Either, family) | (family, Self::Either) => Some(family),
            (Self::WallClock, Self::ProjectFrames) | (Self::ProjectFrames, Self::WallClock) => None,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::WallClock => "wall-clock Duration",
            Self::ProjectFrames => "project-frame Duration",
            Self::Either => "Duration",
        }
    }
}

#[derive(Clone, Debug)]
enum DurationScalar {
    WallClock(ExactNumber),
    ProjectFrames(ExactNumber),
}

impl DurationScalar {
    const fn family(&self) -> DurationFamily {
        match self {
            Self::WallClock(_) => DurationFamily::WallClock,
            Self::ProjectFrames(_) => DurationFamily::ProjectFrames,
        }
    }

    fn value(&self) -> &ExactNumber {
        match self {
            Self::WallClock(value) | Self::ProjectFrames(value) => value,
        }
    }

    fn negated(self) -> Self {
        match self {
            Self::WallClock(value) => Self::WallClock(value.negated()),
            Self::ProjectFrames(value) => Self::ProjectFrames(value.negated()),
        }
    }

    fn combine(self, other: &Self, subtract: bool, span: &SourceSpan) -> Result<Self> {
        if self.family() != other.family() {
            return Err(duration_family_mismatch(
                if subtract { "-" } else { "+" },
                self.family(),
                other.family(),
                span,
            ));
        }
        let value = if subtract {
            self.value().subtract(other.value())
        } else {
            self.value().add(other.value())
        };
        Ok(match self.family() {
            DurationFamily::WallClock => Self::WallClock(value),
            DurationFamily::ProjectFrames => Self::ProjectFrames(value),
            DurationFamily::Either => unreachable!("evaluated durations have a concrete family"),
        })
    }
}

#[derive(Clone, Debug)]
struct DurationRangeScalar {
    start: DurationScalar,
    end: DurationScalar,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ScalarKind {
    Number,
    Duration(DurationFamily),
    File,
    TimeRange(DurationFamily),
    TimelineCoordinate,
    TimelineRange,
    Keyword,
    Text,
}

impl ScalarKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Number => "Number",
            Self::Duration(family) => family.label(),
            Self::File => "File",
            Self::TimeRange(_) => "TimeRange",
            Self::TimelineCoordinate => "TimelineCoordinate",
            Self::TimelineRange => "TimelineRange",
            Self::Keyword => "Keyword",
            Self::Text => "text",
        }
    }
}

#[derive(Clone, Debug)]
enum ScalarValue {
    Number(ExactNumber),
    Duration(DurationScalar),
    File(PathBuf),
    TimeRange(DurationRangeScalar),
    TimelineCoordinate {
        owner: TimelineViewId,
        expression: TimelineExpression,
        layout: String,
    },
    TimelineRange {
        owner: TimelineViewId,
        start: TimelineExpression,
        end: TimelineExpression,
    },
    Keyword(String),
    Text(String),
}

impl ScalarValue {
    const fn kind(&self) -> ScalarKind {
        match self {
            Self::Number(_) => ScalarKind::Number,
            Self::Duration(value) => ScalarKind::Duration(value.family()),
            Self::File(_) => ScalarKind::File,
            Self::TimeRange(value) => ScalarKind::TimeRange(value.start.family()),
            Self::TimelineCoordinate { .. } => ScalarKind::TimelineCoordinate,
            Self::TimelineRange { .. } => ScalarKind::TimelineRange,
            Self::Keyword(_) => ScalarKind::Keyword,
            Self::Text(_) => ScalarKind::Text,
        }
    }
}

pub(super) fn evaluate_expression(
    program: &str,
    parameter: &str,
    expected: &ParameterType,
    expression: &CheckedScalarExpression,
    parameters: &[Spanned<ParameterValue>],
    scalar_aliases: &[CheckedScalarExpression],
    resolve_selector: &mut SelectorEvaluator<'_>,
) -> Result<Spanned<ParameterValue>> {
    let result = (|| {
        let value = evaluate(expression, parameters, scalar_aliases, resolve_selector)?;
        let value = coerce(
            program,
            parameter,
            expected,
            value,
            expression_span(expression),
        )?;
        Ok(Spanned::new(
            value,
            evaluated_span(expression, parameters).clone(),
        ))
    })();
    result.map_err(|diagnostic| add_parameter_trace(diagnostic, expression, parameters))
}

pub(super) fn from_expression(
    program: &str,
    parameter: &str,
    expected: &ParameterType,
    expression: &ScalarExpression,
) -> Result<ParameterValue> {
    let checked = check_expression(
        program,
        parameter,
        expected,
        expression,
        &mut |reference| {
            Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidParameterDefault,
                "parameter defaults cannot contain references",
                reference.span.clone(),
            ))
        },
        &mut |reference| {
            Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidParameterDefault,
                "parameter defaults cannot contain timeline selectors",
                reference.span.clone(),
            ))
        },
    )?;
    evaluate_expression(
        program,
        parameter,
        expected,
        &checked,
        &[],
        &[],
        &mut |_, _, _, _, span| {
            Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidParameterDefault,
                "parameter defaults cannot contain timeline selectors",
                span.clone(),
            ))
        },
    )
    .map(|value| value.value)
}

pub(super) fn from_literal(
    program: &str,
    parameter: &str,
    parameter_type: &ParameterType,
    argument: &Literal,
) -> Result<ParameterValue> {
    from_expression(
        program,
        parameter,
        parameter_type,
        &ScalarExpression::Literal(argument.clone()),
    )
}

pub(super) fn from_text(
    program: &str,
    parameter: &str,
    parameter_type: &ParameterType,
    value: &str,
    span: &SourceSpan,
) -> Result<ParameterValue> {
    let literal = match parameter_type {
        ParameterType::File | ParameterType::Keyword(_) => {
            Literal::String(value.to_owned(), span.clone())
        }
        ParameterType::Number
        | ParameterType::Integer
        | ParameterType::Duration
        | ParameterType::TimeRange => Literal::Atom(value.to_owned(), span.clone()),
    };
    from_literal(program, parameter, parameter_type, &literal)
}

fn evaluate(
    expression: &CheckedScalarExpression,
    parameters: &[Spanned<ParameterValue>],
    scalar_aliases: &[CheckedScalarExpression],
    resolve_selector: &mut SelectorEvaluator<'_>,
) -> Result<ScalarValue> {
    match expression {
        CheckedScalarExpression::Literal(literal) => evaluate_literal(literal),
        CheckedScalarExpression::Parameter { id, name, span } => {
            let parameter = parameters.get(id.index()).ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::InternalBinding,
                    format!("scalar parameter `${name}` was not bound"),
                    span.clone(),
                )
            })?;
            Ok(parameter_scalar_value(&parameter.value))
        }
        CheckedScalarExpression::ScalarAlias { id, name, span } => evaluate_scalar_alias(
            *id,
            name,
            span,
            parameters,
            scalar_aliases,
            resolve_selector,
        ),
        CheckedScalarExpression::TimelineSelector {
            root,
            root_name,
            path,
            contextual,
            span,
        } => Ok(
            match resolve_selector(*root, root_name, path, *contextual, span)? {
                TimelineSelectorValue::Coordinate {
                    owner,
                    expression,
                    layout,
                } => ScalarValue::TimelineCoordinate {
                    owner,
                    expression,
                    layout,
                },
                TimelineSelectorValue::Range { owner, start, end } => {
                    ScalarValue::TimelineRange { owner, start, end }
                }
            },
        ),
        CheckedScalarExpression::Unary {
            operator,
            operand,
            span,
        } => {
            let operand = evaluate(operand, parameters, scalar_aliases, resolve_selector)?;
            match (operator, operand) {
                (
                    ScalarUnaryOperator::Positive,
                    value @ (ScalarValue::Number(_) | ScalarValue::Duration(_)),
                ) => Ok(value),
                (ScalarUnaryOperator::Negative, ScalarValue::Number(value)) => {
                    Ok(ScalarValue::Number(value.negated()))
                }
                (ScalarUnaryOperator::Negative, ScalarValue::Duration(value)) => {
                    Ok(ScalarValue::Duration(value.negated()))
                }
                (_, value) => Err(operator_type_error(
                    unary_label(*operator),
                    &[value.kind()],
                    span,
                    "Number or Duration",
                )),
            }
        }
        CheckedScalarExpression::Binary {
            operator,
            left,
            right,
            span,
        } => {
            let left = evaluate(left, parameters, scalar_aliases, resolve_selector)?;
            let right = evaluate(right, parameters, scalar_aliases, resolve_selector)?;
            evaluate_binary(*operator, left, right, span)
        }
        CheckedScalarExpression::Postfix {
            operator,
            operand,
            span,
        } => {
            let operand = evaluate(operand, parameters, scalar_aliases, resolve_selector)?;
            evaluate_postfix(*operator, operand, span)
        }
    }
}

fn parameter_scalar_value(parameter: &ParameterValue) -> ScalarValue {
    match parameter {
        ParameterValue::Number(value) => ScalarValue::Number(value.clone()),
        ParameterValue::Integer(value) => ScalarValue::Number(ExactNumber::from_integer(*value)),
        ParameterValue::File(value) => ScalarValue::File(value.clone()),
        ParameterValue::Duration(DurationValue::WallClock(value)) => {
            ScalarValue::Duration(DurationScalar::WallClock(value.exact_seconds()))
        }
        ParameterValue::Duration(DurationValue::ProjectFrames(value)) => ScalarValue::Duration(
            DurationScalar::ProjectFrames(ExactNumber::from_unsigned_integer(value.0)),
        ),
        ParameterValue::TimeRange(TimeRangeValue::WallClock(value)) => {
            ScalarValue::TimeRange(DurationRangeScalar {
                start: DurationScalar::WallClock(value.start().exact_seconds()),
                end: DurationScalar::WallClock(value.end().exact_seconds()),
            })
        }
        ParameterValue::TimeRange(TimeRangeValue::ProjectFrames(value)) => {
            ScalarValue::TimeRange(DurationRangeScalar {
                start: DurationScalar::ProjectFrames(ExactNumber::from_unsigned_integer(
                    value.start(),
                )),
                end: DurationScalar::ProjectFrames(ExactNumber::from_unsigned_integer(value.end())),
            })
        }
        ParameterValue::TimeRange(TimeRangeValue::Marker { owner, range }) => {
            ScalarValue::TimelineRange {
                owner: *owner,
                start: range.start.clone(),
                end: range.end.clone(),
            }
        }
        ParameterValue::Keyword(value) => ScalarValue::Keyword(value.clone()),
    }
}

fn evaluate_scalar_alias(
    id: ScalarAliasId,
    name: &str,
    span: &SourceSpan,
    parameters: &[Spanned<ParameterValue>],
    scalar_aliases: &[CheckedScalarExpression],
    resolve_selector: &mut SelectorEvaluator<'_>,
) -> Result<ScalarValue> {
    let expression = scalar_aliases.get(id.index()).ok_or_else(|| {
        Diagnostic::builtin(
            BuiltinDiagnostic::InternalBinding,
            format!("scalar alias `${name}` has no checked expression"),
            span.clone(),
        )
    })?;
    evaluate(expression, parameters, scalar_aliases, resolve_selector)
}

fn evaluate_literal(literal: &Literal) -> Result<ScalarValue> {
    match literal {
        Literal::Integer(value, _) => Ok(ScalarValue::Number(ExactNumber::from_integer(*value))),
        Literal::Atom(value, span) if is_number_text(value) => {
            Ok(ScalarValue::Number(ExactNumber::parse(value, span)?))
        }
        Literal::Atom(value, span) if !value.contains("..") && value.ends_with('f') => {
            Ok(ScalarValue::Duration(parse_frame_duration(value, span)?))
        }
        Literal::Atom(value, span) if !value.contains("..") && looks_like_duration(value) => Ok(
            ScalarValue::Duration(DurationScalar::WallClock(parse_duration(value, span)?)),
        ),
        Literal::String(value, _) | Literal::Atom(value, _) => Ok(ScalarValue::Text(value.clone())),
    }
}

fn evaluate_binary(
    operator: ScalarBinaryOperator,
    left: ScalarValue,
    right: ScalarValue,
    span: &SourceSpan,
) -> Result<ScalarValue> {
    if left.kind() == ScalarKind::TimelineCoordinate
        || right.kind() == ScalarKind::TimelineCoordinate
    {
        return evaluate_timeline_binary(operator, left, right, span);
    }
    match (operator, left, right) {
        (ScalarBinaryOperator::Range, ScalarValue::Duration(start), ScalarValue::Duration(end)) => {
            if start.family() != end.family() {
                return Err(duration_family_mismatch(
                    "..",
                    start.family(),
                    end.family(),
                    span,
                ));
            }
            Ok(ScalarValue::TimeRange(DurationRangeScalar { start, end }))
        }
        (ScalarBinaryOperator::Add, ScalarValue::Number(left), ScalarValue::Number(right)) => {
            Ok(ScalarValue::Number(left.add(&right)))
        }
        (ScalarBinaryOperator::Subtract, ScalarValue::Number(left), ScalarValue::Number(right)) => {
            Ok(ScalarValue::Number(left.subtract(&right)))
        }
        (ScalarBinaryOperator::Multiply, ScalarValue::Number(left), ScalarValue::Number(right)) => {
            Ok(ScalarValue::Number(left.multiply(&right)))
        }
        (ScalarBinaryOperator::Divide, ScalarValue::Number(left), ScalarValue::Number(right)) => {
            left.divide(&right).map(ScalarValue::Number).ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::DivisionByZero,
                    "cannot divide by zero",
                    span.clone(),
                )
            })
        }
        (ScalarBinaryOperator::Add, ScalarValue::Duration(left), ScalarValue::Duration(right)) => {
            left.combine(&right, false, span).map(ScalarValue::Duration)
        }
        (
            ScalarBinaryOperator::Subtract,
            ScalarValue::Duration(left),
            ScalarValue::Duration(right),
        ) => left.combine(&right, true, span).map(ScalarValue::Duration),
        (operator, left, right) => Err(operator_type_error(
            binary_label(operator),
            &[left.kind(), right.kind()],
            span,
            match operator {
                ScalarBinaryOperator::Range => "Duration operands",
                ScalarBinaryOperator::Add | ScalarBinaryOperator::Subtract => {
                    "matching Number operands or matching Duration operands"
                }
                ScalarBinaryOperator::Multiply | ScalarBinaryOperator::Divide => "Number operands",
            },
        )),
    }
}

fn require_same_timeline_root(
    left: TimelineViewId,
    left_layout: &str,
    right: TimelineViewId,
    right_layout: &str,
    span: &SourceSpan,
) -> Result<()> {
    if left == right {
        Ok(())
    } else {
        Err(Diagnostic::builtin(
            BuiltinDiagnostic::TimelineRootMismatch,
            "timeline coordinates belong to different timeline roots",
            span.clone(),
        )
        .note(format!(
            "left coordinate root:
{left_layout}"
        ))
        .note(format!(
            "right coordinate root:
{right_layout}"
        )))
    }
}

fn evaluate_timeline_coordinate_pair(
    operator: ScalarBinaryOperator,
    left: ScalarValue,
    right: ScalarValue,
    span: &SourceSpan,
) -> Result<ScalarValue> {
    let ScalarValue::TimelineCoordinate {
        owner: left_owner,
        expression: left,
        layout: left_layout,
    } = left
    else {
        unreachable!("coordinate-pair evaluator receives a left coordinate");
    };
    let ScalarValue::TimelineCoordinate {
        owner: right_owner,
        expression: right,
        layout: right_layout,
    } = right
    else {
        unreachable!("coordinate-pair evaluator receives a right coordinate");
    };
    require_same_timeline_root(left_owner, &left_layout, right_owner, &right_layout, span)?;
    match operator {
        ScalarBinaryOperator::Range => Ok(ScalarValue::TimelineRange {
            owner: left_owner,
            start: left,
            end: right,
        }),
        ScalarBinaryOperator::Add | ScalarBinaryOperator::Subtract => {
            Ok(ScalarValue::TimelineCoordinate {
                owner: left_owner,
                expression: if operator == ScalarBinaryOperator::Add {
                    left.add(&right)
                } else {
                    left.subtract(&right)
                },
                layout: left_layout,
            })
        }
        ScalarBinaryOperator::Multiply | ScalarBinaryOperator::Divide => {
            unreachable!("coordinate pairs cannot be multiplied or divided")
        }
    }
}

fn evaluate_timeline_binary(
    operator: ScalarBinaryOperator,
    left: ScalarValue,
    right: ScalarValue,
    span: &SourceSpan,
) -> Result<ScalarValue> {
    match (operator, left, right) {
        (
            operator @ (ScalarBinaryOperator::Range
            | ScalarBinaryOperator::Add
            | ScalarBinaryOperator::Subtract),
            left @ ScalarValue::TimelineCoordinate { .. },
            right @ ScalarValue::TimelineCoordinate { .. },
        ) => evaluate_timeline_coordinate_pair(operator, left, right, span),
        (
            operator @ (ScalarBinaryOperator::Add | ScalarBinaryOperator::Subtract),
            ScalarValue::TimelineCoordinate {
                owner,
                expression,
                layout,
            },
            ScalarValue::Duration(duration),
        ) => Ok(ScalarValue::TimelineCoordinate {
            owner,
            expression: if operator == ScalarBinaryOperator::Add {
                expression.add(&duration_timeline_expression(duration))
            } else {
                expression.subtract(&duration_timeline_expression(duration))
            },
            layout,
        }),
        (
            ScalarBinaryOperator::Add,
            ScalarValue::Duration(duration),
            ScalarValue::TimelineCoordinate {
                owner,
                expression,
                layout,
            },
        ) => Ok(ScalarValue::TimelineCoordinate {
            owner,
            expression: duration_timeline_expression(duration).add(&expression),
            layout,
        }),
        (
            ScalarBinaryOperator::Multiply,
            ScalarValue::Number(scale),
            ScalarValue::TimelineCoordinate {
                owner,
                expression,
                layout,
            },
        )
        | (
            ScalarBinaryOperator::Multiply,
            ScalarValue::TimelineCoordinate {
                owner,
                expression,
                layout,
            },
            ScalarValue::Number(scale),
        ) => Ok(ScalarValue::TimelineCoordinate {
            owner,
            expression: expression.multiply(&scale),
            layout,
        }),
        (
            ScalarBinaryOperator::Divide,
            ScalarValue::TimelineCoordinate {
                owner,
                expression,
                layout,
            },
            ScalarValue::Number(divisor),
        ) => expression
            .divide(&divisor)
            .map(|expression| ScalarValue::TimelineCoordinate {
                owner,
                expression,
                layout,
            })
            .ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::DivisionByZero,
                    "cannot divide by zero",
                    span.clone(),
                )
            }),
        (operator, left, right) => Err(operator_type_error(
            binary_label(operator),
            &[left.kind(), right.kind()],
            span,
            "compatible timeline-coordinate, Duration, or Number operands",
        )),
    }
}

fn evaluate_postfix(
    operator: ScalarPostfixOperator,
    operand: ScalarValue,
    span: &SourceSpan,
) -> Result<ScalarValue> {
    match (operator, operand) {
        (ScalarPostfixOperator::Percent, ScalarValue::Number(value)) => Ok(ScalarValue::Number(
            value.multiply(&ExactNumber::from_ratio(1, 100)),
        )),
        (
            operator @ (ScalarPostfixOperator::Milliseconds | ScalarPostfixOperator::Seconds),
            ScalarValue::Number(value),
        ) => {
            if !value.is_integer() {
                return Err(integer_refinement_error(
                    postfix_label(operator),
                    &value,
                    span,
                ));
            }
            let seconds = if operator == ScalarPostfixOperator::Milliseconds {
                value.multiply(&ExactNumber::from_ratio(1, 1_000))
            } else {
                value
            };
            Ok(ScalarValue::Duration(DurationScalar::WallClock(seconds)))
        }
        (ScalarPostfixOperator::Frames, ScalarValue::Number(value)) => {
            if !value.is_integer() {
                return Err(integer_refinement_error("f", &value, span));
            }
            Ok(ScalarValue::Duration(DurationScalar::ProjectFrames(value)))
        }
        (operator, value) => Err(operator_type_error(
            postfix_label(operator),
            &[value.kind()],
            span,
            if operator == ScalarPostfixOperator::Percent {
                "Number"
            } else {
                "Integer"
            },
        )),
    }
}

fn coerce(
    program: &str,
    parameter: &str,
    expected: &ParameterType,
    value: ScalarValue,
    span: &SourceSpan,
) -> Result<ParameterValue> {
    match (expected, value) {
        (ParameterType::Number, ScalarValue::Number(value)) => Ok(ParameterValue::Number(value)),
        (ParameterType::Integer, ScalarValue::Number(value)) => {
            let Some(integer) = value.to_i64() else {
                if value.is_integer() {
                    return Err(Diagnostic::builtin(
                        BuiltinDiagnostic::InvalidArgumentValue,
                        format!(
                            "parameter `{program}.{parameter}` evaluates outside the supported Integer range"
                        ),
                        span.clone(),
                    )
                    .note(format!("exact value: {}", value.canonical())));
                }
                return Err(integer_refinement_error(
                    &format!("parameter `{program}.{parameter}`"),
                    &value,
                    span,
                ));
            };
            Ok(ParameterValue::Integer(integer))
        }
        (ParameterType::Duration, ScalarValue::Duration(value)) => {
            Ok(ParameterValue::Duration(refine_duration(value, span)?))
        }
        (ParameterType::Duration, ScalarValue::Text(value)) => Ok(ParameterValue::Duration(
            refine_duration(parse_duration_text(&value, span)?, span)?,
        )),
        (ParameterType::File, ScalarValue::Text(value)) => {
            Ok(ParameterValue::File(PathBuf::from(value)))
        }
        (ParameterType::File, ScalarValue::File(value)) => Ok(ParameterValue::File(value)),
        (ParameterType::TimeRange, ScalarValue::Text(value)) => Ok(ParameterValue::TimeRange(
            refine_duration_range(parse_duration_range_text(&value, span)?, span)?,
        )),
        (ParameterType::TimeRange, ScalarValue::TimeRange(value)) => Ok(ParameterValue::TimeRange(
            refine_duration_range(value, span)?,
        )),
        (ParameterType::TimeRange, ScalarValue::TimelineRange { owner, start, end }) => {
            Ok(ParameterValue::TimeRange(TimeRangeValue::Marker {
                owner,
                range: Box::new(crate::model::TimelineRangeExpression { start, end }),
            }))
        }
        (
            ParameterType::Keyword(allowed),
            ScalarValue::Text(value) | ScalarValue::Keyword(value),
        ) => keyword(program, parameter, allowed, &value, span),
        (expected, value) => Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidArgumentType,
            format!(
                "parameter `{program}.{parameter}` requires {}, but the expression evaluates to {}",
                parameter_type_label(expected),
                value.kind().label()
            ),
            span.clone(),
        )),
    }
}

fn literal_kind(literal: &Literal) -> Result<ScalarKind> {
    match literal {
        Literal::Integer(_, _) => Ok(ScalarKind::Number),
        Literal::Atom(value, span) if is_number_text(value) => {
            ExactNumber::parse(value, span)?;
            Ok(ScalarKind::Number)
        }
        Literal::Atom(value, span) if value.contains("..") => {
            let range = parse_duration_range_text(value, span)?;
            Ok(ScalarKind::TimeRange(range.start.family()))
        }
        Literal::Atom(value, span) if value.ends_with('f') => {
            parse_frame_duration(value, span)?;
            Ok(ScalarKind::Duration(DurationFamily::ProjectFrames))
        }
        Literal::Atom(value, span) if looks_like_duration(value) => {
            parse_duration(value, span)?;
            Ok(ScalarKind::Duration(DurationFamily::WallClock))
        }
        Literal::String(_, _) | Literal::Atom(_, _) => Ok(ScalarKind::Text),
    }
}

fn parameter_kind(parameter_type: &ParameterType) -> ScalarKind {
    match parameter_type {
        ParameterType::Number | ParameterType::Integer => ScalarKind::Number,
        ParameterType::File => ScalarKind::File,
        ParameterType::Duration => ScalarKind::Duration(DurationFamily::Either),
        ParameterType::TimeRange => ScalarKind::TimeRange(DurationFamily::Either),
        ParameterType::Keyword(_) => ScalarKind::Keyword,
    }
}

fn kind_matches_parameter(kind: ScalarKind, parameter_type: &ParameterType) -> bool {
    match parameter_type {
        ParameterType::Number | ParameterType::Integer => kind == ScalarKind::Number,
        ParameterType::File => matches!(kind, ScalarKind::File | ScalarKind::Text),
        ParameterType::Duration => matches!(kind, ScalarKind::Duration(_) | ScalarKind::Text),
        ParameterType::TimeRange => matches!(
            kind,
            ScalarKind::TimeRange(_) | ScalarKind::TimelineRange | ScalarKind::Text
        ),
        ParameterType::Keyword(_) => matches!(kind, ScalarKind::Keyword | ScalarKind::Text),
    }
}

fn expression_span(expression: &CheckedScalarExpression) -> &SourceSpan {
    match expression {
        CheckedScalarExpression::Literal(literal) => literal.span(),
        CheckedScalarExpression::Parameter { span, .. }
        | CheckedScalarExpression::ScalarAlias { span, .. }
        | CheckedScalarExpression::TimelineSelector { span, .. }
        | CheckedScalarExpression::Unary { span, .. }
        | CheckedScalarExpression::Binary { span, .. }
        | CheckedScalarExpression::Postfix { span, .. } => span,
    }
}

fn evaluated_span<'a>(
    expression: &'a CheckedScalarExpression,
    parameters: &'a [Spanned<ParameterValue>],
) -> &'a SourceSpan {
    match expression {
        CheckedScalarExpression::Parameter { id, .. } => parameters
            .get(id.index())
            .map_or_else(|| expression_span(expression), |parameter| &parameter.span),
        CheckedScalarExpression::Literal(_)
        | CheckedScalarExpression::ScalarAlias { .. }
        | CheckedScalarExpression::TimelineSelector { .. }
        | CheckedScalarExpression::Unary { .. }
        | CheckedScalarExpression::Binary { .. }
        | CheckedScalarExpression::Postfix { .. } => expression_span(expression),
    }
}

fn add_parameter_trace(
    mut diagnostic: Diagnostic,
    expression: &CheckedScalarExpression,
    parameters: &[Spanned<ParameterValue>],
) -> Diagnostic {
    let mut references = Vec::new();
    collect_parameter_references(expression, &mut references);
    references.sort_by_key(|(id, _)| id.index());
    references.dedup_by_key(|(id, _)| id.index());
    for (id, name) in references {
        let Some(parameter) = parameters.get(id.index()) else {
            continue;
        };
        let value = match &parameter.value {
            ParameterValue::Number(value) => {
                if value.authored_display() == value.canonical() {
                    value.authored_display()
                } else {
                    format!(
                        "{} (exactly {})",
                        value.authored_display(),
                        value.canonical()
                    )
                }
            }
            ParameterValue::Integer(value) => value.to_string(),
            ParameterValue::Duration(DurationValue::WallClock(value)) => {
                format!("{}s", value.exact_seconds().authored_display())
            }
            ParameterValue::Duration(DurationValue::ProjectFrames(value)) => {
                format!("{}f", value.0)
            }
            ParameterValue::File(_) | ParameterValue::TimeRange(_) | ParameterValue::Keyword(_) => {
                continue;
            }
        };
        diagnostic
            .notes
            .push(format!("scalar parameter `${name}` evaluated to {value}"));
    }
    diagnostic
}

fn collect_parameter_references<'a>(
    expression: &'a CheckedScalarExpression,
    references: &mut Vec<(ParameterId, &'a str)>,
) {
    match expression {
        CheckedScalarExpression::Literal(_)
        | CheckedScalarExpression::ScalarAlias { .. }
        | CheckedScalarExpression::TimelineSelector { .. } => {}
        CheckedScalarExpression::Parameter { id, name, .. } => {
            references.push((*id, name));
        }
        CheckedScalarExpression::Unary { operand, .. }
        | CheckedScalarExpression::Postfix { operand, .. } => {
            collect_parameter_references(operand, references);
        }
        CheckedScalarExpression::Binary { left, right, .. } => {
            collect_parameter_references(left, references);
            collect_parameter_references(right, references);
        }
    }
}

fn parameter_type_label(parameter_type: &ParameterType) -> &'static str {
    match parameter_type {
        ParameterType::Number => "Number",
        ParameterType::Integer => "Integer",
        ParameterType::File => "File",
        ParameterType::Duration => "Duration",
        ParameterType::TimeRange => "TimeRange",
        ParameterType::Keyword(_) => "Keyword",
    }
}

fn unary_label(operator: ScalarUnaryOperator) -> &'static str {
    match operator {
        ScalarUnaryOperator::Positive => "+",
        ScalarUnaryOperator::Negative => "-",
    }
}

fn binary_label(operator: ScalarBinaryOperator) -> &'static str {
    match operator {
        ScalarBinaryOperator::Range => "..",
        ScalarBinaryOperator::Add => "+",
        ScalarBinaryOperator::Subtract => "-",
        ScalarBinaryOperator::Multiply => "*",
        ScalarBinaryOperator::Divide => "/",
    }
}

fn postfix_label(operator: ScalarPostfixOperator) -> &'static str {
    match operator {
        ScalarPostfixOperator::Percent => "%",
        ScalarPostfixOperator::Milliseconds => "ms",
        ScalarPostfixOperator::Seconds => "s",
        ScalarPostfixOperator::Frames => "f",
    }
}

fn operator_type_error(
    operator: &str,
    actual: &[ScalarKind],
    span: &SourceSpan,
    expected: &str,
) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::InvalidScalarOperation,
        format!(
            "operator `{operator}` requires {expected}, but got {}",
            actual
                .iter()
                .map(|kind| kind.label())
                .collect::<Vec<_>>()
                .join(" and ")
        ),
        span.clone(),
    )
}

fn integer_refinement_error(owner: &str, value: &ExactNumber, span: &SourceSpan) -> Diagnostic {
    let diagnostic = Diagnostic::builtin(
        BuiltinDiagnostic::InvalidArgumentType,
        format!(
            "`{owner}` requires Integer, but the expression evaluates to {}",
            value.authored_display()
        ),
        span.clone(),
    );
    if value.canonical() == value.authored_display() {
        diagnostic
    } else {
        diagnostic.note(format!("exact value: {}", value.canonical()))
    }
}

fn keyword(
    program: &str,
    parameter: &str,
    allowed: &[String],
    value: &str,
    span: &SourceSpan,
) -> Result<ParameterValue> {
    allowed
        .iter()
        .find(|candidate| candidate.as_str() == value)
        .cloned()
        .map(ParameterValue::Keyword)
        .ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::InvalidArgumentValue,
                format!(
                    "parameter `{program}.{parameter}` must be one of: {}",
                    allowed.join(", ")
                ),
                span.clone(),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> SourceSpan {
        SourceSpan::file_start("test.clipasm")
    }

    fn literal(value: &str) -> ScalarExpression {
        ScalarExpression::Literal(Literal::Atom(value.to_owned(), span()))
    }

    #[test]
    fn converts_number_and_integer_refinement_exactly() {
        assert_eq!(
            from_expression("repeat", "count", &ParameterType::Integer, &literal("3"))
                .expect("integer"),
            ParameterValue::Integer(3)
        );
        let error = from_expression("repeat", "count", &ParameterType::Integer, &literal("2.5"))
            .expect_err("fraction");
        assert!(error.message.contains("evaluates to 2.5"));
        assert_eq!(error.notes, ["exact value: 5/2"]);
    }

    #[test]
    fn converts_duration_literals_without_rounding() {
        assert!(matches!(
            from_expression(
                "image",
                "duration",
                &ParameterType::Duration,
                &literal("500ms"),
            )
            .expect("duration"),
            ParameterValue::Duration(_)
        ));
    }
}
