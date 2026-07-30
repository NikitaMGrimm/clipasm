//! Static scalar-expression typing and checked-expression construction.

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::program::ParameterType;
use crate::source::{
    ScalarBinaryOperator, ScalarExpression, ScalarPostfixOperator, SourceSpan, Spanned,
};

use super::{
    CheckedScalarExpression, DurationFamily, ScalarKind, ScalarReference, ScalarResolver,
    TimelineResolver, binary_label, duration_family_mismatch, kind_matches_parameter, literal_kind,
    operator_type_error, parameter_kind, parameter_type_label, postfix_label, unary_label,
};

pub(in crate::compiler) fn check_expression(
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
        Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidArgumentType,
            format!(
                "parameter `{program}.{parameter}` requires {}, but the expression has type {}",
                parameter_type_label(expected),
                actual.label()
            ),
            expression.span().clone(),
        ))
    }
}

pub(in crate::compiler) fn check_inferred_expression(
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
            if !matches!(kind, ScalarKind::Number | ScalarKind::Duration(_)) {
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
                    ScalarKind::Duration(DurationFamily::WallClock)
                }
                ScalarPostfixOperator::Frames if kind == ScalarKind::Number => {
                    ScalarKind::Duration(DurationFamily::ProjectFrames)
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
                ScalarPostfixOperator::Frames => {
                    return Err(operator_type_error("f", &[kind], span, "Integer"));
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
        ScalarReference::Alias(id, kind) => Ok((
            CheckedScalarExpression::ScalarAlias {
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
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidTimelineSelector,
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
    let kind = check_binary_kind(operator, left_kind, right_kind, span)?;
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

fn check_binary_kind(
    operator: ScalarBinaryOperator,
    left: ScalarKind,
    right: ScalarKind,
    span: &SourceSpan,
) -> Result<ScalarKind> {
    if left == ScalarKind::Number && right == ScalarKind::Number {
        return Ok(ScalarKind::Number);
    }
    match operator {
        ScalarBinaryOperator::Range => match (left, right) {
            (ScalarKind::Duration(left), ScalarKind::Duration(right)) => left
                .compatible(right)
                .map(ScalarKind::TimeRange)
                .ok_or_else(|| duration_family_mismatch("..", left, right, span)),
            (ScalarKind::TimelineCoordinate, ScalarKind::TimelineCoordinate) => {
                Ok(ScalarKind::TimelineRange)
            }
            _ => Err(operator_type_error(
                "..",
                &[left, right],
                span,
                "matching Duration operands or matching timeline coordinates",
            )),
        },
        ScalarBinaryOperator::Add | ScalarBinaryOperator::Subtract => match (left, right) {
            (ScalarKind::Duration(left), ScalarKind::Duration(right)) => left
                .compatible(right)
                .map(ScalarKind::Duration)
                .ok_or_else(|| duration_family_mismatch(binary_label(operator), left, right, span)),
            (
                ScalarKind::TimelineCoordinate,
                ScalarKind::TimelineCoordinate | ScalarKind::Duration(_),
            ) => Ok(ScalarKind::TimelineCoordinate),
            (ScalarKind::Duration(_), ScalarKind::TimelineCoordinate)
                if operator == ScalarBinaryOperator::Add =>
            {
                Ok(ScalarKind::TimelineCoordinate)
            }
            _ => Err(operator_type_error(
                binary_label(operator),
                &[left, right],
                span,
                "compatible Number, Duration, or timeline-coordinate operands",
            )),
        },
        ScalarBinaryOperator::Multiply => match (left, right) {
            (ScalarKind::Number, ScalarKind::TimelineCoordinate)
            | (ScalarKind::TimelineCoordinate, ScalarKind::Number) => {
                Ok(ScalarKind::TimelineCoordinate)
            }
            _ => Err(operator_type_error(
                "*",
                &[left, right],
                span,
                "Number operands, or a timeline coordinate scaled by Number",
            )),
        },
        ScalarBinaryOperator::Divide => match (left, right) {
            (ScalarKind::TimelineCoordinate, ScalarKind::Number) => {
                Ok(ScalarKind::TimelineCoordinate)
            }
            _ => Err(operator_type_error(
                "/",
                &[left, right],
                span,
                "Number operands, or a timeline coordinate scaled by Number",
            )),
        },
    }
}
