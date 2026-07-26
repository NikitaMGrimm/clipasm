use std::path::PathBuf;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{ExactNumber, SourceTime, SourceTimeRange, TimelineExpression, TimelineViewId};
use crate::program::{ParameterType, ParameterValue, TimeRangeValue};
use crate::source::{
    Literal, ScalarBinaryOperator, ScalarExpression, ScalarPostfixOperator, ScalarUnaryOperator,
    SourceSpan, Spanned,
};

use super::checked::{CheckedScalarExpression, ParameterId, ReferenceTarget, ScalarLocalId};

pub(super) enum ScalarReference {
    Parameter(ParameterId, ParameterType),
    Local(ScalarLocalId, ScalarKind),
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
pub(super) enum ScalarKind {
    Number,
    Duration,
    File,
    TimeRange,
    TimelineCoordinate,
    TimelineRange,
    Keyword,
    Text,
}

impl ScalarKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Number => "Number",
            Self::Duration => "Duration",
            Self::File => "File",
            Self::TimeRange => "TimeRange",
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
    Duration(ExactNumber),
    File(PathBuf),
    TimeRange(SourceTimeRange),
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
            Self::Duration(_) => ScalarKind::Duration,
            Self::File(_) => ScalarKind::File,
            Self::TimeRange(_) => ScalarKind::TimeRange,
            Self::TimelineCoordinate { .. } => ScalarKind::TimelineCoordinate,
            Self::TimelineRange { .. } => ScalarKind::TimelineRange,
            Self::Keyword(_) => ScalarKind::Keyword,
            Self::Text(_) => ScalarKind::Text,
        }
    }
}

pub(super) fn check_expression(
    program: &str,
    parameter: &str,
    expected: &ParameterType,
    expression: &ScalarExpression,
    resolve_scalar: &mut ScalarResolver<'_>,
    resolve_timeline: &mut TimelineResolver<'_>,
) -> Result<CheckedScalarExpression> {
    let (checked, actual) = check(expression, resolve_scalar, resolve_timeline, true)?;
    if kind_matches_parameter(actual, expected) {
        Ok(checked)
    } else {
        Err(Diagnostic::new(
            "E_INVALID_ARGUMENT_TYPE",
            format!(
                "parameter `{program}.{parameter}` requires {}, but the expression has type {}",
                parameter_type_label(expected),
                actual.label()
            ),
            expression.span().clone(),
        ))
    }
}

pub(super) fn check_inferred_expression(
    expression: &ScalarExpression,
    resolve_scalar: &mut ScalarResolver<'_>,
    resolve_timeline: &mut TimelineResolver<'_>,
) -> Result<(CheckedScalarExpression, ScalarKind)> {
    check(expression, resolve_scalar, resolve_timeline, false)
}

fn check(
    expression: &ScalarExpression,
    resolve_scalar: &mut ScalarResolver<'_>,
    resolve_timeline: &mut TimelineResolver<'_>,
    contextual_selectors: bool,
) -> Result<(CheckedScalarExpression, ScalarKind)> {
    match expression {
        ScalarExpression::Literal(literal) => Ok((
            CheckedScalarExpression::Literal(literal.clone()),
            literal_kind(literal)?,
        )),
        ScalarExpression::Reference(reference) => check_reference(reference, resolve_scalar),
        ScalarExpression::Selector { root, path, span } => {
            check_selector(root, path, span, contextual_selectors, resolve_timeline)
        }
        ScalarExpression::Unary {
            operator,
            operand,
            span,
        } => {
            let (operand, kind) = check(
                operand,
                resolve_scalar,
                resolve_timeline,
                contextual_selectors,
            )?;
            if !matches!(kind, ScalarKind::Number | ScalarKind::Duration) {
                return Err(operator_type_error(
                    unary_label(*operator),
                    &[kind],
                    span,
                    "Number or Duration",
                ));
            }
            Ok((
                CheckedScalarExpression::Unary {
                    operator: *operator,
                    operand: Box::new(operand),
                    span: span.clone(),
                },
                kind,
            ))
        }
        ScalarExpression::Binary {
            operator,
            left,
            right,
            span,
        } => check_binary(
            *operator,
            left,
            right,
            span,
            resolve_scalar,
            resolve_timeline,
            contextual_selectors,
        ),
        ScalarExpression::Postfix {
            operator,
            operand,
            span,
        } => {
            let (operand, kind) = check(
                operand,
                resolve_scalar,
                resolve_timeline,
                contextual_selectors,
            )?;
            let result = match operator {
                ScalarPostfixOperator::Percent if kind == ScalarKind::Number => ScalarKind::Number,
                ScalarPostfixOperator::Milliseconds | ScalarPostfixOperator::Seconds
                    if kind == ScalarKind::Number =>
                {
                    ScalarKind::Duration
                }
                ScalarPostfixOperator::Percent => {
                    return Err(operator_type_error("%", &[kind], span, "Number"));
                }
                ScalarPostfixOperator::Milliseconds | ScalarPostfixOperator::Seconds => {
                    return Err(operator_type_error(
                        postfix_label(*operator),
                        &[kind],
                        span,
                        "Integer",
                    ));
                }
            };
            Ok((
                CheckedScalarExpression::Postfix {
                    operator: *operator,
                    operand: Box::new(operand),
                    span: span.clone(),
                },
                result,
            ))
        }
    }
}

fn check_reference(
    reference: &Spanned<String>,
    resolve_scalar: &mut ScalarResolver<'_>,
) -> Result<(CheckedScalarExpression, ScalarKind)> {
    match resolve_scalar(reference)? {
        ScalarReference::Parameter(id, parameter_type) => Ok((
            CheckedScalarExpression::Parameter {
                id,
                name: reference.value.clone(),
                span: reference.span.clone(),
            },
            parameter_kind(&parameter_type),
        )),
        ScalarReference::Local(id, kind) => Ok((
            CheckedScalarExpression::ScalarLocal {
                id,
                name: reference.value.clone(),
                span: reference.span.clone(),
            },
            kind,
        )),
    }
}

fn check_selector(
    root: &Spanned<String>,
    path: &[Spanned<String>],
    span: &SourceSpan,
    contextual: bool,
    resolve_timeline: &mut TimelineResolver<'_>,
) -> Result<(CheckedScalarExpression, ScalarKind)> {
    if path.is_empty() {
        return Err(Diagnostic::new(
            "E_INVALID_TIMELINE_SELECTOR",
            "a timeline selector requires a placement or boundary after `::`",
            span.clone(),
        ));
    }
    let kind = if path
        .last()
        .is_some_and(|name| matches!(name.value.as_str(), "start" | "middle" | "end"))
    {
        ScalarKind::TimelineCoordinate
    } else {
        ScalarKind::TimelineRange
    };
    Ok((
        CheckedScalarExpression::TimelineSelector {
            root: resolve_timeline(root)?,
            root_name: root.value.clone(),
            path: path.iter().map(|part| part.value.clone()).collect(),
            contextual,
            span: span.clone(),
        },
        kind,
    ))
}

fn check_binary(
    operator: ScalarBinaryOperator,
    left: &ScalarExpression,
    right: &ScalarExpression,
    span: &SourceSpan,
    resolve_scalar: &mut ScalarResolver<'_>,
    resolve_timeline: &mut TimelineResolver<'_>,
    contextual_selectors: bool,
) -> Result<(CheckedScalarExpression, ScalarKind)> {
    let (left, left_kind) = check(left, resolve_scalar, resolve_timeline, contextual_selectors)?;
    let (right, right_kind) = check(
        right,
        resolve_scalar,
        resolve_timeline,
        contextual_selectors,
    )?;
    let kind = match operator {
        ScalarBinaryOperator::Range
            if left_kind == ScalarKind::Duration && right_kind == ScalarKind::Duration =>
        {
            ScalarKind::TimeRange
        }
        ScalarBinaryOperator::Range
            if left_kind == ScalarKind::TimelineCoordinate
                && right_kind == ScalarKind::TimelineCoordinate =>
        {
            ScalarKind::TimelineRange
        }
        ScalarBinaryOperator::Add | ScalarBinaryOperator::Subtract
            if left_kind == right_kind
                && matches!(left_kind, ScalarKind::Number | ScalarKind::Duration) =>
        {
            left_kind
        }
        ScalarBinaryOperator::Add | ScalarBinaryOperator::Subtract
            if left_kind == ScalarKind::TimelineCoordinate
                && right_kind == ScalarKind::TimelineCoordinate =>
        {
            ScalarKind::TimelineCoordinate
        }
        ScalarBinaryOperator::Add
            if matches!(
                (left_kind, right_kind),
                (ScalarKind::TimelineCoordinate, ScalarKind::Duration)
                    | (ScalarKind::Duration, ScalarKind::TimelineCoordinate)
            ) =>
        {
            ScalarKind::TimelineCoordinate
        }
        ScalarBinaryOperator::Subtract
            if left_kind == ScalarKind::TimelineCoordinate
                && right_kind == ScalarKind::Duration =>
        {
            ScalarKind::TimelineCoordinate
        }
        ScalarBinaryOperator::Multiply | ScalarBinaryOperator::Divide
            if left_kind == ScalarKind::Number && right_kind == ScalarKind::Number =>
        {
            ScalarKind::Number
        }
        ScalarBinaryOperator::Multiply
            if matches!(
                (left_kind, right_kind),
                (ScalarKind::Number, ScalarKind::TimelineCoordinate)
                    | (ScalarKind::TimelineCoordinate, ScalarKind::Number)
            ) =>
        {
            ScalarKind::TimelineCoordinate
        }
        ScalarBinaryOperator::Divide
            if left_kind == ScalarKind::TimelineCoordinate && right_kind == ScalarKind::Number =>
        {
            ScalarKind::TimelineCoordinate
        }
        ScalarBinaryOperator::Range => {
            return Err(operator_type_error(
                "..",
                &[left_kind, right_kind],
                span,
                "matching Duration operands or matching timeline coordinates",
            ));
        }
        ScalarBinaryOperator::Add | ScalarBinaryOperator::Subtract => {
            return Err(operator_type_error(
                binary_label(operator),
                &[left_kind, right_kind],
                span,
                "compatible Number, Duration, or timeline-coordinate operands",
            ));
        }
        ScalarBinaryOperator::Multiply | ScalarBinaryOperator::Divide => {
            return Err(operator_type_error(
                binary_label(operator),
                &[left_kind, right_kind],
                span,
                "Number operands, or a timeline coordinate scaled by Number",
            ));
        }
    };
    Ok((
        CheckedScalarExpression::Binary {
            operator,
            left: Box::new(left),
            right: Box::new(right),
            span: span.clone(),
        },
        kind,
    ))
}

pub(super) fn evaluate_expression(
    program: &str,
    parameter: &str,
    expected: &ParameterType,
    expression: &CheckedScalarExpression,
    parameters: &[Spanned<ParameterValue>],
    scalar_locals: &[Option<CheckedScalarExpression>],
    resolve_selector: &mut SelectorEvaluator<'_>,
) -> Result<Spanned<ParameterValue>> {
    let result = (|| {
        let value = evaluate(expression, parameters, scalar_locals, resolve_selector)?;
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
            Err(Diagnostic::new(
                "E_INVALID_PARAMETER_DEFAULT",
                "parameter defaults cannot contain references",
                reference.span.clone(),
            ))
        },
        &mut |reference| {
            Err(Diagnostic::new(
                "E_INVALID_PARAMETER_DEFAULT",
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
            Err(Diagnostic::new(
                "E_INVALID_PARAMETER_DEFAULT",
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
    scalar_locals: &[Option<CheckedScalarExpression>],
    resolve_selector: &mut SelectorEvaluator<'_>,
) -> Result<ScalarValue> {
    match expression {
        CheckedScalarExpression::Literal(literal) => evaluate_literal(literal),
        CheckedScalarExpression::Parameter { id, name, span } => {
            let parameter = parameters.get(id.index()).ok_or_else(|| {
                Diagnostic::new(
                    "E_INTERNAL_BINDING",
                    format!("scalar parameter `${name}` was not bound"),
                    span.clone(),
                )
            })?;
            Ok(match &parameter.value {
                ParameterValue::Number(value) => ScalarValue::Number(value.clone()),
                ParameterValue::Integer(value) => {
                    ScalarValue::Number(ExactNumber::from_integer(*value))
                }
                ParameterValue::File(value) => ScalarValue::File(value.clone()),
                ParameterValue::Duration(value) => ScalarValue::Duration(value.exact_seconds()),
                ParameterValue::TimeRange(TimeRangeValue::Absolute(value)) => {
                    ScalarValue::TimeRange(*value)
                }
                ParameterValue::TimeRange(TimeRangeValue::VideoMarker { owner, range }) => {
                    ScalarValue::TimelineRange {
                        owner: *owner,
                        start: range.start.clone(),
                        end: range.end.clone(),
                    }
                }
                ParameterValue::Keyword(value) => ScalarValue::Keyword(value.clone()),
            })
        }
        CheckedScalarExpression::ScalarLocal { id, name, span } => {
            evaluate_scalar_local(*id, name, span, parameters, scalar_locals, resolve_selector)
        }
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
            let operand = evaluate(operand, parameters, scalar_locals, resolve_selector)?;
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
            let left = evaluate(left, parameters, scalar_locals, resolve_selector)?;
            let right = evaluate(right, parameters, scalar_locals, resolve_selector)?;
            evaluate_binary(*operator, left, right, span)
        }
        CheckedScalarExpression::Postfix {
            operator,
            operand,
            span,
        } => {
            let operand = evaluate(operand, parameters, scalar_locals, resolve_selector)?;
            evaluate_postfix(*operator, operand, span)
        }
    }
}

fn evaluate_scalar_local(
    id: ScalarLocalId,
    name: &str,
    span: &SourceSpan,
    parameters: &[Spanned<ParameterValue>],
    scalar_locals: &[Option<CheckedScalarExpression>],
    resolve_selector: &mut SelectorEvaluator<'_>,
) -> Result<ScalarValue> {
    let expression = scalar_locals
        .get(id.index())
        .and_then(Option::as_ref)
        .ok_or_else(|| {
            Diagnostic::new(
                "E_INTERNAL_BINDING",
                format!("scalar local `${name}` was not checked before evaluation"),
                span.clone(),
            )
        })?;
    evaluate(expression, parameters, scalar_locals, resolve_selector)
}

fn evaluate_literal(literal: &Literal) -> Result<ScalarValue> {
    match literal {
        Literal::Integer(value, _) => Ok(ScalarValue::Number(ExactNumber::from_integer(*value))),
        Literal::Atom(value, span) if is_number_text(value) => {
            Ok(ScalarValue::Number(ExactNumber::parse(value, span)?))
        }
        Literal::Atom(value, span) if !value.contains("..") && looks_like_duration(value) => {
            Ok(ScalarValue::Duration(parse_duration(value, span)?))
        }
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
            let start = SourceTime::from_exact_seconds(&start, span)?;
            let end = SourceTime::from_exact_seconds(&end, span)?;
            SourceTimeRange::new(start, end)
                .map(ScalarValue::TimeRange)
                .ok_or_else(|| {
                    Diagnostic::new(
                        "E_INVALID_TIME_RANGE",
                        "time-range start must be earlier than its end",
                        span.clone(),
                    )
                })
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
                Diagnostic::new("E_DIVISION_BY_ZERO", "cannot divide by zero", span.clone())
            })
        }
        (ScalarBinaryOperator::Add, ScalarValue::Duration(left), ScalarValue::Duration(right)) => {
            Ok(ScalarValue::Duration(left.add(&right)))
        }
        (
            ScalarBinaryOperator::Subtract,
            ScalarValue::Duration(left),
            ScalarValue::Duration(right),
        ) => Ok(ScalarValue::Duration(left.subtract(&right))),
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
        Err(Diagnostic::new(
            "E_TIMELINE_ROOT_MISMATCH",
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
                expression.add(&TimelineExpression::constant(duration))
            } else {
                expression.subtract(&TimelineExpression::constant(duration))
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
            expression: TimelineExpression::constant(duration).add(&expression),
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
                Diagnostic::new("E_DIVISION_BY_ZERO", "cannot divide by zero", span.clone())
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
            Ok(ScalarValue::Duration(seconds))
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
                    return Err(Diagnostic::new(
                        "E_INVALID_ARGUMENT_VALUE",
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
        (ParameterType::Duration, ScalarValue::Duration(value)) => Ok(ParameterValue::Duration(
            SourceTime::from_exact_seconds(&value, span)?,
        )),
        (ParameterType::Duration, ScalarValue::Text(value)) => {
            Ok(ParameterValue::Duration(SourceTime::parse(&value, span)?))
        }
        (ParameterType::File, ScalarValue::Text(value)) => {
            Ok(ParameterValue::File(PathBuf::from(value)))
        }
        (ParameterType::File, ScalarValue::File(value)) => Ok(ParameterValue::File(value)),
        (ParameterType::TimeRange, ScalarValue::Text(value)) => Ok(ParameterValue::TimeRange(
            TimeRangeValue::Absolute(SourceTimeRange::parse(&value, span)?),
        )),
        (ParameterType::TimeRange, ScalarValue::TimeRange(value)) => {
            Ok(ParameterValue::TimeRange(TimeRangeValue::Absolute(value)))
        }
        (ParameterType::TimeRange, ScalarValue::TimelineRange { owner, start, end }) => {
            Ok(ParameterValue::TimeRange(TimeRangeValue::VideoMarker {
                owner,
                range: crate::model::TimelineRangeExpression { start, end },
            }))
        }
        (
            ParameterType::Keyword(allowed),
            ScalarValue::Text(value) | ScalarValue::Keyword(value),
        ) => keyword(program, parameter, allowed, &value, span),
        (expected, value) => Err(Diagnostic::new(
            "E_INVALID_ARGUMENT_TYPE",
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
            SourceTimeRange::parse(value, span)?;
            Ok(ScalarKind::TimeRange)
        }
        Literal::Atom(value, span) if looks_like_duration(value) => {
            parse_duration(value, span)?;
            Ok(ScalarKind::Duration)
        }
        Literal::String(_, _) | Literal::Atom(_, _) => Ok(ScalarKind::Text),
    }
}

fn parameter_kind(parameter_type: &ParameterType) -> ScalarKind {
    match parameter_type {
        ParameterType::Number | ParameterType::Integer => ScalarKind::Number,
        ParameterType::File => ScalarKind::File,
        ParameterType::Duration => ScalarKind::Duration,
        ParameterType::TimeRange => ScalarKind::TimeRange,
        ParameterType::Keyword(_) => ScalarKind::Keyword,
    }
}

fn kind_matches_parameter(kind: ScalarKind, parameter_type: &ParameterType) -> bool {
    match parameter_type {
        ParameterType::Number | ParameterType::Integer => kind == ScalarKind::Number,
        ParameterType::File => matches!(kind, ScalarKind::File | ScalarKind::Text),
        ParameterType::Duration => matches!(kind, ScalarKind::Duration | ScalarKind::Text),
        ParameterType::TimeRange => matches!(
            kind,
            ScalarKind::TimeRange | ScalarKind::TimelineRange | ScalarKind::Text
        ),
        ParameterType::Keyword(_) => matches!(kind, ScalarKind::Keyword | ScalarKind::Text),
    }
}

fn parse_duration(value: &str, span: &SourceSpan) -> Result<ExactNumber> {
    let (number, scale) = if let Some(number) = value.strip_suffix("ms") {
        (number, ExactNumber::from_ratio(1, 1_000))
    } else if let Some(number) = value.strip_suffix('s') {
        (number, ExactNumber::from_integer(1))
    } else {
        return Err(Diagnostic::new(
            "E_INVALID_DURATION",
            format!("`{value}` is not a duration"),
            span.clone(),
        ));
    };
    if !number.bytes().all(|byte| byte.is_ascii_digit()) || number.is_empty() {
        return Err(Diagnostic::new(
            "E_INVALID_DURATION",
            format!("`{value}` is not a supported integer duration"),
            span.clone(),
        ));
    }
    Ok(ExactNumber::parse(number, span)?.multiply(&scale))
}

fn looks_like_duration(value: &str) -> bool {
    value.ends_with("ms") || value.ends_with('s')
}

fn is_number_text(value: &str) -> bool {
    let mut dots = 0_u8;
    !value.is_empty()
        && value.bytes().all(|byte| {
            if byte == b'.' {
                dots = dots.saturating_add(1);
                dots <= 1
            } else {
                byte.is_ascii_digit()
            }
        })
}

fn expression_span(expression: &CheckedScalarExpression) -> &SourceSpan {
    match expression {
        CheckedScalarExpression::Literal(literal) => literal.span(),
        CheckedScalarExpression::Parameter { span, .. }
        | CheckedScalarExpression::ScalarLocal { span, .. }
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
        | CheckedScalarExpression::ScalarLocal { .. }
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
            ParameterValue::Duration(value) => {
                format!("{}s", value.exact_seconds().authored_display())
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
        | CheckedScalarExpression::ScalarLocal { .. }
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
    }
}

fn operator_type_error(
    operator: &str,
    actual: &[ScalarKind],
    span: &SourceSpan,
    expected: &str,
) -> Diagnostic {
    Diagnostic::new(
        "E_INVALID_SCALAR_OPERATION",
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
    let diagnostic = Diagnostic::new(
        "E_INVALID_ARGUMENT_TYPE",
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
            Diagnostic::new(
                "E_INVALID_ARGUMENT_VALUE",
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
