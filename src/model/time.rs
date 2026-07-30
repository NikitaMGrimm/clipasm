use serde::{Deserialize, Serialize};

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{AudioSpec, ExactNumber, FrameRate, TimelineRate, ValueType};
use crate::source::SourceSpan;

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
/// An exact number of project frames.
///
/// Frame counts are integral and are never silently rounded from authored
/// times.
pub struct FrameCount(pub u64);

impl FrameCount {
    /// Add frame counts without wrapping.
    ///
    /// # Errors
    ///
    /// Returns `E_FRAME_OVERFLOW` if the sum exceeds `u64`.
    pub(crate) fn checked_add(self, other: Self, span: &SourceSpan) -> Result<Self> {
        self.0.checked_add(other.0).map(Self).ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::FrameOverflow,
                "video duration exceeds the supported frame count",
                span.clone(),
            )
        })
    }

    /// Multiply a frame count without wrapping.
    ///
    /// # Errors
    ///
    /// Returns `E_FRAME_OVERFLOW` if the product exceeds `u64`.
    pub(crate) fn checked_mul(self, multiplier: u64, span: &SourceSpan) -> Result<Self> {
        self.0.checked_mul(multiplier).map(Self).ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::FrameOverflow,
                "video duration exceeds the supported frame count",
                span.clone(),
            )
        })
    }

    /// Return the smallest frame count covering an exact rational duration.
    ///
    /// `duration_numerator / duration_denominator` is measured in seconds.
    ///
    /// # Errors
    ///
    /// Returns `E_FRAME_OVERFLOW` if the rational conversion cannot be
    /// represented with checked `u128` intermediates or as a `u64` frame count.
    pub(crate) fn covering_duration(
        duration_numerator: u128,
        duration_denominator: u128,
        fps: FrameRate,
        span: &SourceSpan,
    ) -> Result<Self> {
        let overflow = || {
            Diagnostic::builtin(
                BuiltinDiagnostic::FrameOverflow,
                "duration exceeds the supported frame count",
                span.clone(),
            )
        };
        let numerator = duration_numerator
            .checked_mul(u128::from(fps.numerator()))
            .ok_or_else(&overflow)?;
        let denominator = duration_denominator
            .checked_mul(u128::from(fps.denominator()))
            .filter(|denominator| *denominator != 0)
            .ok_or_else(&overflow)?;
        let quotient = numerator / denominator;
        let frames = if numerator % denominator == 0 {
            quotient
        } else {
            quotient.checked_add(1).ok_or_else(overflow)?
        };
        u64::try_from(frames).map(Self).map_err(|_| overflow())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
/// A nonempty closed-open range of frame indexes.
///
/// ```compile_fail
/// use clipasm::model::FrameRange;
///
/// let invalid = FrameRange { start: 10, end: 5 };
/// ```
pub struct FrameRange {
    start: u64,
    end: u64,
}

impl FrameRange {
    /// Construct a nonempty closed-open frame range.
    ///
    /// Returns `None` when `start` is not earlier than `end`.
    #[must_use]
    pub const fn new(start: u64, end: u64) -> Option<Self> {
        if start < end {
            Some(Self { start, end })
        } else {
            None
        }
    }

    #[must_use]
    /// Return the inclusive starting frame index.
    pub const fn start(self) -> u64 {
        self.start
    }

    #[must_use]
    /// Return the exclusive ending frame index.
    pub const fn end(self) -> u64 {
        self.end
    }

    #[must_use]
    /// Return the exact number of frames in the range.
    pub const fn frames(self) -> FrameCount {
        FrameCount(self.end - self.start)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
/// A nonempty closed-open range of audio sample indexes.
pub struct SampleRange {
    start: u64,
    end: u64,
}

impl SampleRange {
    /// Construct a nonempty closed-open sample range.
    #[must_use]
    pub const fn new(start: u64, end: u64) -> Option<Self> {
        if start < end {
            Some(Self { start, end })
        } else {
            None
        }
    }

    /// Return the inclusive starting sample index.
    #[must_use]
    pub const fn start(self) -> u64 {
        self.start
    }

    /// Return the exclusive ending sample index.
    #[must_use]
    pub const fn end(self) -> u64 {
        self.end
    }

    /// Return the exact number of samples in the range.
    #[must_use]
    pub const fn samples(self) -> u64 {
        self.end - self.start
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum NativeRange {
    Frames(FrameRange),
    Samples(SampleRange),
}

impl NativeRange {
    #[must_use]
    pub(crate) const fn value_type(self) -> ValueType {
        match self {
            Self::Frames(_) => ValueType::Video,
            Self::Samples(_) => ValueType::Audio,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum NativeDuration {
    Frames(FrameCount),
    Samples(u64),
}

impl NativeDuration {
    #[must_use]
    pub(crate) const fn value_type(self) -> ValueType {
        match self {
            Self::Frames(_) => ValueType::Video,
            Self::Samples(_) => ValueType::Audio,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DurationValue {
    WallClock(SourceTime),
    ProjectFrames(FrameCount),
}

impl DurationValue {
    pub(crate) fn to_frames(self, fps: FrameRate, span: &SourceSpan) -> Result<FrameCount> {
        match self {
            Self::WallClock(duration) => duration.to_frames(fps, span).map(FrameCount),
            Self::ProjectFrames(frames) => Ok(frames),
        }
    }

    pub(crate) fn to_covering_frames(
        self,
        fps: FrameRate,
        span: &SourceSpan,
    ) -> Result<FrameCount> {
        match self {
            Self::WallClock(duration) => duration.to_covering_frames(fps, span),
            Self::ProjectFrames(frames) => Ok(frames),
        }
    }

    pub(crate) fn to_covering_samples(
        self,
        video: crate::model::VideoSpec,
        audio: AudioSpec,
        span: &SourceSpan,
    ) -> Result<u64> {
        match self {
            Self::WallClock(duration) => duration.to_covering_samples(audio.sample_rate(), span),
            Self::ProjectFrames(frames) => {
                TimelineRate::new(video, audio).samples_for_frames(frames, span)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SourceTime {
    nanoseconds: u64,
}

impl SourceTime {
    pub(crate) fn exact_seconds(self) -> ExactNumber {
        ExactNumber::from_unsigned_integer(self.nanoseconds)
            .divide(&ExactNumber::from_integer(1_000_000_000))
            .expect("time scale is nonzero")
    }

    /// Convert an exact rational number of seconds into the authored time grid.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic for negative, sub-nanosecond, or overflowing values.
    pub(crate) fn from_exact_seconds(value: &ExactNumber, span: &SourceSpan) -> Result<Self> {
        if !value.is_positive() && !value.is_zero() {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidDuration,
                format!(
                    "duration cannot be negative; got {}",
                    value.authored_display()
                ),
                span.clone(),
            ));
        }
        let nanoseconds = value.multiply(&ExactNumber::from_integer(1_000_000_000));
        let Some(nanoseconds) = nanoseconds.to_u64() else {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidDuration,
                format!(
                    "duration {}s is not representable as an exact nonnegative nanosecond value",
                    value.authored_display()
                ),
                span.clone(),
            ));
        };
        Ok(Self { nanoseconds })
    }

    /// Return the smallest project frame count covering this duration.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic if the conversion exceeds the supported frame count.
    pub(crate) fn to_covering_frames(
        self,
        fps: FrameRate,
        span: &SourceSpan,
    ) -> Result<FrameCount> {
        FrameCount::covering_duration(u128::from(self.nanoseconds), 1_000_000_000, fps, span)
    }

    /// Convert this duration to an exact project frame boundary.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic if the duration is not exactly frame-aligned or
    /// exceeds the supported frame count.
    pub(crate) fn to_frames(self, fps: FrameRate, span: &SourceSpan) -> Result<u64> {
        let numerator = u128::from(self.nanoseconds) * u128::from(fps.numerator());
        let denominator = 1_000_000_000_u128 * u128::from(fps.denominator());
        if numerator % denominator != 0 {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::TimeNotFrameAligned,
                format!(
                    "time is not exactly representable at {}/{} fps",
                    fps.numerator(),
                    fps.denominator()
                ),
                span.clone(),
            ));
        }
        u64::try_from(numerator / denominator).map_err(|_| {
            Diagnostic::builtin(
                BuiltinDiagnostic::FrameOverflow,
                "duration exceeds the supported frame count",
                span.clone(),
            )
        })
    }

    pub(crate) fn to_samples(self, sample_rate: u32, span: &SourceSpan) -> Result<u64> {
        let numerator = u128::from(self.nanoseconds) * u128::from(sample_rate);
        let denominator = 1_000_000_000_u128;
        if numerator % denominator != 0 {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::TimeNotSampleAligned,
                format!("time is not exactly representable at {sample_rate} Hz"),
                span.clone(),
            ));
        }
        u64::try_from(numerator / denominator).map_err(|_| {
            Diagnostic::builtin(
                BuiltinDiagnostic::AudioDurationOverflow,
                "duration exceeds the supported audio sample count",
                span.clone(),
            )
        })
    }

    pub(crate) fn to_covering_samples(self, sample_rate: u32, span: &SourceSpan) -> Result<u64> {
        let numerator = u128::from(self.nanoseconds) * u128::from(sample_rate);
        let denominator = 1_000_000_000_u128;
        let samples = numerator.div_ceil(denominator);
        u64::try_from(samples).map_err(|_| {
            Diagnostic::builtin(
                BuiltinDiagnostic::AudioDurationOverflow,
                "duration exceeds the supported audio sample count",
                span.clone(),
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SourceTimeRange {
    start: SourceTime,
    end: SourceTime,
}

impl SourceTimeRange {
    #[must_use]
    pub(crate) const fn new(start: SourceTime, end: SourceTime) -> Option<Self> {
        if start.nanoseconds < end.nanoseconds {
            Some(Self { start, end })
        } else {
            None
        }
    }

    #[must_use]
    pub(crate) const fn start(self) -> SourceTime {
        self.start
    }

    #[must_use]
    pub(crate) const fn end(self) -> SourceTime {
        self.end
    }

    /// Convert both endpoints to exact frame indexes.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when either endpoint is not frame-aligned.
    pub(crate) fn to_frames(self, fps: FrameRate, span: &SourceSpan) -> Result<FrameRange> {
        FrameRange::new(
            self.start.to_frames(fps, span)?,
            self.end.to_frames(fps, span)?,
        )
        .ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::InvalidTimeRange,
                "time range must contain at least one frame",
                span.clone(),
            )
        })
    }

    pub(crate) fn to_samples(self, sample_rate: u32, span: &SourceSpan) -> Result<SampleRange> {
        SampleRange::new(
            self.start.to_samples(sample_rate, span)?,
            self.end.to_samples(sample_rate, span)?,
        )
        .ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::InvalidTimeRange,
                "time range must contain at least one audio sample",
                span.clone(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn span() -> SourceSpan {
        SourceSpan::file_start(PathBuf::from("test.clipasm"))
    }

    fn source_time(numerator: i64, denominator: i64) -> SourceTime {
        SourceTime::from_exact_seconds(&ExactNumber::from_ratio(numerator, denominator), &span())
            .expect("source time")
    }

    #[test]
    fn converts_exact_seconds_to_frame_boundaries() {
        let fps = FrameRate::new(30, 1).expect("valid fps");
        assert_eq!(
            source_time(3, 1).to_frames(fps, &span()).expect("frames"),
            90
        );
        assert_eq!(
            source_time(1, 2).to_frames(fps, &span()).expect("frames"),
            15
        );
    }

    #[test]
    fn rejects_non_aligned_duration() {
        let fps = FrameRate::new(30, 1).expect("valid fps");
        let error = source_time(1, 1_000)
            .to_frames(fps, &span())
            .expect_err("not frame aligned");
        assert_eq!(error.code, "E_TIME_NOT_FRAME_ALIGNED");
    }

    #[test]
    fn source_duration_can_be_quantized_by_frame_coverage() {
        let duration = source_time(1, 2);
        assert_eq!(
            duration
                .to_covering_frames(FrameRate::new(29, 1).expect("valid fps"), &span())
                .expect("covering frames"),
            FrameCount(15)
        );
        assert_eq!(
            source_time(0, 1)
                .to_covering_frames(FrameRate::new(29, 1).expect("valid fps"), &span())
                .expect("zero frames"),
            FrameCount(0)
        );
    }

    #[test]
    fn covering_duration_uses_checked_ceiling_division() {
        let ntsc = FrameRate::new(30_000, 1_001).expect("valid fps");
        assert_eq!(
            FrameCount::covering_duration(1, 1, ntsc, &span()).expect("covering frames"),
            FrameCount(30)
        );
        assert_eq!(
            FrameCount::covering_duration(1_001, 30_000, ntsc, &span()).expect("aligned frame"),
            FrameCount(1)
        );
        assert_eq!(
            FrameCount::covering_duration(
                1,
                1_000,
                FrameRate::new(30, 1).expect("valid fps"),
                &span()
            )
            .expect("covering frames"),
            FrameCount(1)
        );
    }

    #[test]
    fn covering_duration_reports_checked_arithmetic_overflow() {
        let error = FrameCount::covering_duration(
            u128::MAX,
            1,
            FrameRate::new(2, 1).expect("valid fps"),
            &span(),
        )
        .expect_err("overflow");
        assert_eq!(error.code, "E_FRAME_OVERFLOW");
    }

    #[test]
    fn source_time_ranges_require_increasing_endpoints() {
        assert!(SourceTimeRange::new(source_time(4, 1), source_time(2, 1)).is_none());
    }

    #[test]
    fn frame_ranges_require_increasing_endpoints() {
        assert!(FrameRange::new(10, 5).is_none());
        assert!(FrameRange::new(5, 5).is_none());
        let range = FrameRange::new(5, 10).expect("range");
        assert_eq!(range.start(), 5);
        assert_eq!(range.end(), 10);
        assert_eq!(range.frames(), FrameCount(5));
    }
}
