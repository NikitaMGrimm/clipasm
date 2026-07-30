//! Authored duration parsing, refinement, and timeline conversion.

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{
    DurationValue, ExactNumber, FrameCount, FrameRange, SourceTime, SourceTimeRange,
    TimelineExpression,
};
use crate::program::TimeRangeValue;
use crate::source::SourceSpan;

use super::{DurationFamily, DurationRangeScalar, DurationScalar, integer_refinement_error};

pub(super) fn parse_duration(value: &str, span: &SourceSpan) -> Result<ExactNumber> {
    let (number, scale) = if let Some(number) = value.strip_suffix("ms") {
        (number, ExactNumber::from_ratio(1, 1_000))
    } else if let Some(number) = value.strip_suffix('s') {
        (number, ExactNumber::from_integer(1))
    } else {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidDuration,
            format!("`{value}` is not a duration"),
            span.clone(),
        ));
    };
    if !number.bytes().all(|byte| byte.is_ascii_digit()) || number.is_empty() {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidDuration,
            format!("`{value}` is not a supported integer duration"),
            span.clone(),
        ));
    }
    Ok(ExactNumber::parse(number, span)?.multiply(&scale))
}

pub(super) fn looks_like_duration(value: &str) -> bool {
    value.ends_with("ms") || value.ends_with('s')
}

pub(super) fn parse_frame_duration(value: &str, span: &SourceSpan) -> Result<DurationScalar> {
    let Some(number) = value.strip_suffix('f') else {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidDuration,
            format!("`{value}` is not a project-frame duration"),
            span.clone(),
        ));
    };
    if number.is_empty() {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidDuration,
            "project-frame duration requires a number before `f`",
            span.clone(),
        ));
    }
    Ok(DurationScalar::ProjectFrames(parse_signed_number(
        number, span,
    )?))
}

fn parse_signed_number(value: &str, span: &SourceSpan) -> Result<ExactNumber> {
    if let Some(value) = value.strip_prefix('-') {
        return ExactNumber::parse(value, span).map(|value| value.negated());
    }
    ExactNumber::parse(value.strip_prefix('+').unwrap_or(value), span)
}

pub(super) fn parse_duration_text(value: &str, span: &SourceSpan) -> Result<DurationScalar> {
    if value.ends_with('f') {
        parse_frame_duration(value, span)
    } else {
        parse_duration(value, span).map(DurationScalar::WallClock)
    }
}

pub(super) fn parse_duration_range_text(
    value: &str,
    span: &SourceSpan,
) -> Result<DurationRangeScalar> {
    let Some((start, end)) = value.split_once("..") else {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidTimeRange,
            "a time range requires both endpoints, for example `2s..4s` or `12f..24f`",
            span.clone(),
        ));
    };
    if start.is_empty() || end.is_empty() || end.contains("..") {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidTimeRange,
            "a time range requires exactly two endpoints",
            span.clone(),
        ));
    }
    let start = parse_duration_text(start, span)?;
    let end = parse_duration_text(end, span)?;
    if start.family() != end.family() {
        return Err(duration_family_mismatch(
            "..",
            start.family(),
            end.family(),
            span,
        ));
    }
    Ok(DurationRangeScalar { start, end })
}

pub(super) fn refine_duration(value: DurationScalar, span: &SourceSpan) -> Result<DurationValue> {
    match value {
        DurationScalar::WallClock(value) => {
            SourceTime::from_exact_seconds(&value, span).map(DurationValue::WallClock)
        }
        DurationScalar::ProjectFrames(value) => {
            if !value.is_integer() {
                return Err(integer_refinement_error(
                    "project-frame duration",
                    &value,
                    span,
                ));
            }
            let Some(frames) = value.to_u64() else {
                let (code, message) = if value.is_positive() || value.is_zero() {
                    (
                        BuiltinDiagnostic::FrameOverflow,
                        "project-frame duration exceeds the supported frame count",
                    )
                } else {
                    (
                        BuiltinDiagnostic::InvalidDuration,
                        "project-frame duration cannot be negative",
                    )
                };
                return Err(Diagnostic::builtin(code, message, span.clone())
                    .note(format!("exact value: {}f", value.canonical())));
            };
            Ok(DurationValue::ProjectFrames(FrameCount(frames)))
        }
    }
}

pub(super) fn refine_duration_range(
    value: DurationRangeScalar,
    span: &SourceSpan,
) -> Result<TimeRangeValue> {
    match (
        refine_duration(value.start, span)?,
        refine_duration(value.end, span)?,
    ) {
        (DurationValue::WallClock(start), DurationValue::WallClock(end)) => {
            SourceTimeRange::new(start, end)
                .map(TimeRangeValue::WallClock)
                .ok_or_else(|| {
                    Diagnostic::builtin(
                        BuiltinDiagnostic::InvalidTimeRange,
                        "time-range start must be earlier than its end",
                        span.clone(),
                    )
                })
        }
        (DurationValue::ProjectFrames(start), DurationValue::ProjectFrames(end)) => {
            FrameRange::new(start.0, end.0)
                .map(TimeRangeValue::ProjectFrames)
                .ok_or_else(|| {
                    Diagnostic::builtin(
                        BuiltinDiagnostic::InvalidTimeRange,
                        "frame-range start must be earlier than its end",
                        span.clone(),
                    )
                })
        }
        _ => unreachable!("duration range parser preserves one family"),
    }
}

pub(super) fn duration_family_mismatch(
    operator: &str,
    left: DurationFamily,
    right: DurationFamily,
    span: &SourceSpan,
) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::InvalidScalarOperation,
        format!(
            "operator `{operator}` requires matching Duration families, but got {} and {}",
            left.label(),
            right.label()
        ),
        span.clone(),
    )
}

pub(super) fn duration_timeline_expression(duration: DurationScalar) -> TimelineExpression {
    match duration {
        DurationScalar::WallClock(seconds) => TimelineExpression::constant(seconds),
        DurationScalar::ProjectFrames(frames) => TimelineExpression::project_frames(frames),
    }
}

pub(super) fn is_number_text(value: &str) -> bool {
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
