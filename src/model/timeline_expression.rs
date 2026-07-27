use std::collections::BTreeMap;

use serde::Serialize;

use super::{ExactNumber, FrameRate, ValueRef, exact_seconds_to_frames, exact_seconds_to_samples};
use crate::diagnostic::Result;
use crate::source::SourceSpan;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct TimelineTerm {
    pub(crate) value: ValueRef,
    pub(crate) coefficient: ExactNumber,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct TimelineExpression {
    constant: ExactNumber,
    terms: Vec<TimelineTerm>,
}

impl TimelineExpression {
    pub(crate) fn constant(value: ExactNumber) -> Self {
        Self {
            constant: value,
            terms: Vec::new(),
        }
    }

    pub(crate) fn extent(value: ValueRef, seconds_per_unit: ExactNumber) -> Self {
        Self {
            constant: ExactNumber::from_integer(0),
            terms: vec![TimelineTerm {
                value,
                coefficient: seconds_per_unit,
            }],
        }
    }

    pub(crate) fn constant_value(&self) -> Option<&ExactNumber> {
        self.terms.is_empty().then_some(&self.constant)
    }

    pub(crate) fn is_nonnegative_constant(&self) -> bool {
        self.constant_value()
            .is_some_and(|value| value >= &ExactNumber::from_integer(0))
    }

    pub(crate) fn constant_part(&self) -> &ExactNumber {
        &self.constant
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
            self.terms
                .iter()
                .map(|term| (term.value, term.coefficient.multiply(scale))),
        )
    }

    pub(crate) fn divide(&self, divisor: &ExactNumber) -> Option<Self> {
        let constant = self.constant.divide(divisor)?;
        let terms = self
            .terms
            .iter()
            .map(|term| {
                term.coefficient
                    .divide(divisor)
                    .map(|coefficient| (term.value, coefficient))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self::normalized(constant, terms))
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
        exact_seconds_to_frames(&self.resolve(units)?, fps, span)
    }

    pub(crate) fn resolve_sample_boundary(
        &self,
        sample_rate: u32,
        units: impl FnMut(ValueRef) -> Result<u64>,
        span: &SourceSpan,
    ) -> Result<u64> {
        exact_seconds_to_samples(&self.resolve(units)?, sample_rate, span)
    }

    fn combine(&self, other: &Self, subtract: bool) -> Self {
        let constant = if subtract {
            self.constant.subtract(&other.constant)
        } else {
            self.constant.add(&other.constant)
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
        Self::normalized(constant, left.chain(right))
    }

    fn normalized(
        constant: ExactNumber,
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
            terms: combined
                .into_iter()
                .filter(|(_, coefficient)| !coefficient.is_zero())
                .map(|(value, coefficient)| TimelineTerm { value, coefficient })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct TimelineRangeExpression {
    pub(crate) start: TimelineExpression,
    pub(crate) end: TimelineExpression,
}
