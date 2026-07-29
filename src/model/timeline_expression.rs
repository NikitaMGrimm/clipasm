use std::collections::BTreeMap;

use serde::Serialize;

use super::{AudioSpec, ExactNumber, FrameRate, TimelineRate, ValueRef, VideoSpec};
use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::source::SourceSpan;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct TimelineTerm {
    pub(crate) value: ValueRef,
    pub(crate) coefficient: ExactNumber,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct TimelineExpression {
    constant: ExactNumber,
    #[serde(skip_serializing_if = "ExactNumber::is_zero")]
    project_frames: ExactNumber,
    terms: Vec<TimelineTerm>,
}

impl TimelineExpression {
    pub(crate) fn constant(value: ExactNumber) -> Self {
        Self {
            constant: value,
            project_frames: ExactNumber::from_integer(0),
            terms: Vec::new(),
        }
    }

    pub(crate) fn project_frames(value: ExactNumber) -> Self {
        Self {
            constant: ExactNumber::from_integer(0),
            project_frames: value,
            terms: Vec::new(),
        }
    }

    pub(crate) fn extent(value: ValueRef, seconds_per_unit: ExactNumber) -> Self {
        Self {
            constant: ExactNumber::from_integer(0),
            project_frames: ExactNumber::from_integer(0),
            terms: vec![TimelineTerm {
                value,
                coefficient: seconds_per_unit,
            }],
        }
    }

    pub(crate) fn constant_value(&self) -> Option<&ExactNumber> {
        (self.terms.is_empty() && self.project_frames.is_zero()).then_some(&self.constant)
    }

    pub(crate) fn is_nonnegative_constant(&self) -> bool {
        self.constant_value()
            .is_some_and(|value| value >= &ExactNumber::from_integer(0))
    }

    pub(crate) fn constant_part(&self) -> &ExactNumber {
        &self.constant
    }

    pub(crate) fn project_frame_part(&self) -> &ExactNumber {
        &self.project_frames
    }

    pub(crate) fn terms(&self) -> &[TimelineTerm] {
        &self.terms
    }

    pub(crate) fn add(&self, other: &Self) -> Self {
        self.combine(other, false)
    }

    pub(crate) fn subtract(&self, other: &Self) -> Self {
        self.combine(other, true)
    }

    pub(crate) fn multiply(&self, scale: &ExactNumber) -> Self {
        Self::normalized(
            self.constant.multiply(scale),
            self.project_frames.multiply(scale),
            self.terms
                .iter()
                .map(|term| (term.value, term.coefficient.multiply(scale))),
        )
    }

    pub(crate) fn divide(&self, divisor: &ExactNumber) -> Option<Self> {
        let constant = self.constant.divide(divisor)?;
        let project_frames = self.project_frames.divide(divisor)?;
        let terms = self
            .terms
            .iter()
            .map(|term| {
                term.coefficient
                    .divide(divisor)
                    .map(|coefficient| (term.value, coefficient))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self::normalized(constant, project_frames, terms))
    }

    pub(crate) fn resolve(
        &self,
        mut units: impl FnMut(ValueRef) -> Result<u64>,
    ) -> Result<ExactNumber> {
        let mut value = self.constant.clone();
        for term in &self.terms {
            let extent = ExactNumber::from_unsigned_integer(units(term.value)?);
            value = value.add(&extent.multiply(&term.coefficient));
        }
        Ok(value)
    }

    pub(crate) fn resolve_frame_boundary(
        &self,
        fps: FrameRate,
        units: impl FnMut(ValueRef) -> Result<u64>,
        span: &SourceSpan,
    ) -> Result<u64> {
        let seconds = self.resolve(units)?;
        let frames = seconds
            .multiply(&ExactNumber::from_unsigned_integer(u64::from(
                fps.numerator(),
            )))
            .divide(&ExactNumber::from_unsigned_integer(u64::from(
                fps.denominator(),
            )))
            .expect("frame-rate denominator is nonzero")
            .add(&self.project_frames);
        frames.to_u64().ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::TimeNotFrameAligned,
                format!(
                    "timeline coordinate resolves to {} project frames, not an exact supported frame boundary",
                    frames.authored_display()
                ),
                span.clone(),
            )
        })
    }

    pub(crate) fn resolve_sample_boundary(
        &self,
        video: VideoSpec,
        audio: AudioSpec,
        units: impl FnMut(ValueRef) -> Result<u64>,
        span: &SourceSpan,
    ) -> Result<u64> {
        let seconds = self.resolve(units)?;
        let base = seconds
            .multiply(&ExactNumber::from_unsigned_integer(u64::from(
                audio.sample_rate(),
            )))
            .to_u64()
            .ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::TimeNotSampleAligned,
                    format!(
                        "timeline coordinate {}s is not an exact nonnegative boundary at {} Hz",
                        seconds.authored_display(),
                        audio.sample_rate()
                    ),
                    span.clone(),
                )
            })?;
        let frames = self.project_frames.to_i64().ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::TimeNotSampleAligned,
                format!(
                    "project-frame offset {}f is not an exact supported frame displacement",
                    self.project_frames.authored_display()
                ),
                span.clone(),
            )
        })?;
        let displacement =
            TimelineRate::new(video, audio).signed_sample_displacement(frames, span)?;
        if displacement >= 0 {
            base.checked_add(
                u64::try_from(displacement).expect("nonnegative displacement fits u64"),
            )
            .ok_or_else(|| sample_overflow(span))
        } else {
            let magnitude =
                u64::try_from(displacement.unsigned_abs()).map_err(|_| sample_overflow(span))?;
            base.checked_sub(magnitude).ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::TimeNotSampleAligned,
                    "timeline coordinate resolves before the start of the audio timeline",
                    span.clone(),
                )
            })
        }
    }

    fn combine(&self, other: &Self, subtract: bool) -> Self {
        let constant = if subtract {
            self.constant.subtract(&other.constant)
        } else {
            self.constant.add(&other.constant)
        };
        let project_frames = if subtract {
            self.project_frames.subtract(&other.project_frames)
        } else {
            self.project_frames.add(&other.project_frames)
        };
        let left = self
            .terms
            .iter()
            .map(|term| (term.value, term.coefficient.clone()));
        let right = other.terms.iter().map(|term| {
            (
                term.value,
                if subtract {
                    term.coefficient.negated()
                } else {
                    term.coefficient.clone()
                },
            )
        });
        Self::normalized(constant, project_frames, left.chain(right))
    }

    fn normalized(
        constant: ExactNumber,
        project_frames: ExactNumber,
        terms: impl IntoIterator<Item = (ValueRef, ExactNumber)>,
    ) -> Self {
        let mut combined = BTreeMap::<ValueRef, ExactNumber>::new();
        for (value, coefficient) in terms {
            combined
                .entry(value)
                .and_modify(|current| *current = current.add(&coefficient))
                .or_insert(coefficient);
        }
        Self {
            constant,
            project_frames,
            terms: combined
                .into_iter()
                .filter(|(_, coefficient)| !coefficient.is_zero())
                .map(|(value, coefficient)| TimelineTerm { value, coefficient })
                .collect(),
        }
    }
}

fn sample_overflow(span: &SourceSpan) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::AudioDurationOverflow,
        "audio timeline coordinate exceeds the supported sample range",
        span.clone(),
    )
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct TimelineRangeExpression {
    pub(crate) start: TimelineExpression,
    pub(crate) end: TimelineExpression,
}
