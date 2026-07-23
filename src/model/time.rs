use serde::{Deserialize, Serialize};

use crate::diagnostic::{Diagnostic, Result, SourceSpan};
use crate::model::FrameRate;

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct FrameCount(pub u64);

impl FrameCount {
    /// Add frame counts without wrapping.
    ///
    /// # Errors
    ///
    /// Returns `E_FRAME_OVERFLOW` if the sum exceeds `u64`.
    pub fn checked_add(self, other: Self, span: &SourceSpan) -> Result<Self> {
        self.0.checked_add(other.0).map(Self).ok_or_else(|| {
            Diagnostic::new(
                "E_FRAME_OVERFLOW",
                "video duration exceeds the supported frame count",
                span.clone(),
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
/// A nonempty closed-open range of frame indexes.
///
/// ```compile_fail
/// use rhythmcut::model::FrameRange;
///
/// let invalid = FrameRange { start: 10, end: 5 };
/// ```
pub struct FrameRange {
    start: u64,
    end: u64,
}

impl FrameRange {
    #[must_use]
    pub const fn new(start: u64, end: u64) -> Option<Self> {
        if start < end {
            Some(Self { start, end })
        } else {
            None
        }
    }

    #[must_use]
    pub const fn start(self) -> u64 {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> u64 {
        self.end
    }

    #[must_use]
    pub const fn frames(self) -> FrameCount {
        FrameCount(self.end - self.start)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceTime {
    nanoseconds: u64,
}

impl SourceTime {
    /// Parse an integer seconds or milliseconds duration.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic for negative, malformed, or overflowing durations.
    pub fn parse(text: &str, span: &SourceSpan) -> Result<Self> {
        if text.starts_with('-') {
            return Err(Diagnostic::new(
                "E_INVALID_DURATION",
                "durations cannot be negative",
                span.clone(),
            ));
        }
        let (number, scale) = if let Some(number) = text.strip_suffix("ms") {
            (number, 1_000_000_u64)
        } else if let Some(number) = text.strip_suffix('s') {
            (number, 1_000_000_000_u64)
        } else {
            return Err(Diagnostic::new(
                "E_INVALID_DURATION",
                format!("`{text}` is not a duration; use forms such as `3s` or `500ms`"),
                span.clone(),
            ));
        };
        if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(Diagnostic::new(
                "E_INVALID_DURATION",
                format!("`{text}` is not a supported integer duration"),
                span.clone(),
            ));
        }
        let value = number.parse::<u64>().map_err(|_| {
            Diagnostic::new(
                "E_INVALID_DURATION",
                format!("`{text}` is too large"),
                span.clone(),
            )
        })?;
        let nanoseconds = value.checked_mul(scale).ok_or_else(|| {
            Diagnostic::new(
                "E_INVALID_DURATION",
                format!("`{text}` is too large"),
                span.clone(),
            )
        })?;
        Ok(Self { nanoseconds })
    }

    /// Convert this duration to an exact project frame boundary.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic if the duration is not exactly frame-aligned or
    /// exceeds the supported frame count.
    pub fn to_frames(self, fps: FrameRate, span: &SourceSpan) -> Result<u64> {
        let numerator = u128::from(self.nanoseconds) * u128::from(fps.numerator());
        let denominator = 1_000_000_000_u128 * u128::from(fps.denominator());
        if numerator % denominator != 0 {
            return Err(Diagnostic::new(
                "E_TIME_NOT_FRAME_ALIGNED",
                format!(
                    "time is not exactly representable at {}/{} fps",
                    fps.numerator(),
                    fps.denominator()
                ),
                span.clone(),
            ));
        }
        u64::try_from(numerator / denominator).map_err(|_| {
            Diagnostic::new(
                "E_FRAME_OVERFLOW",
                "duration exceeds the supported frame count",
                span.clone(),
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceTimeRange {
    start: SourceTime,
    end: SourceTime,
}

impl SourceTimeRange {
    #[must_use]
    pub const fn new(start: SourceTime, end: SourceTime) -> Option<Self> {
        if start.nanoseconds < end.nanoseconds {
            Some(Self { start, end })
        } else {
            None
        }
    }

    #[must_use]
    pub const fn start(self) -> SourceTime {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> SourceTime {
        self.end
    }

    /// Parse a closed-open duration range such as `2s..4s`.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic for missing, malformed, negative, or reversed
    /// endpoints.
    pub fn parse(text: &str, span: &SourceSpan) -> Result<Self> {
        let Some((start, end)) = text.split_once("..") else {
            return Err(Diagnostic::new(
                "E_INVALID_DURING_RANGE",
                "a `during` range requires both endpoints, for example `2s..4s`",
                span.clone(),
            ));
        };
        if start.is_empty() || end.is_empty() || end.contains("..") {
            return Err(Diagnostic::new(
                "E_INVALID_DURING_RANGE",
                "a `during` range requires exactly two endpoints",
                span.clone(),
            ));
        }
        Self::new(
            SourceTime::parse(start, span)?,
            SourceTime::parse(end, span)?,
        )
        .ok_or_else(|| {
            Diagnostic::new(
                "E_INVALID_DURING_RANGE",
                "`during` range start must be earlier than its end",
                span.clone(),
            )
        })
    }

    /// Convert both endpoints to exact frame indexes.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when either endpoint is not frame-aligned.
    pub fn to_frames(self, fps: FrameRate, span: &SourceSpan) -> Result<FrameRange> {
        FrameRange::new(
            self.start.to_frames(fps, span)?,
            self.end.to_frames(fps, span)?,
        )
        .ok_or_else(|| {
            Diagnostic::new(
                "E_INVALID_DURING_RANGE",
                "`during` range must contain at least one frame",
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
        SourceSpan::file_start(PathBuf::from("test.yaml"))
    }

    #[test]
    fn parses_exact_seconds_and_milliseconds() {
        let fps = FrameRate::new(30, 1).expect("valid fps");
        assert_eq!(
            SourceTime::parse("3s", &span())
                .expect("duration")
                .to_frames(fps, &span())
                .expect("frames"),
            90
        );
        assert_eq!(
            SourceTime::parse("500ms", &span())
                .expect("duration")
                .to_frames(fps, &span())
                .expect("frames"),
            15
        );
    }

    #[test]
    fn rejects_non_aligned_duration() {
        let fps = FrameRate::new(30, 1).expect("valid fps");
        let error = SourceTime::parse("1ms", &span())
            .expect("duration syntax")
            .to_frames(fps, &span())
            .expect_err("not frame aligned");
        assert_eq!(error.code, "E_TIME_NOT_FRAME_ALIGNED");
    }

    #[test]
    fn rejects_reversed_range() {
        let error = SourceTimeRange::parse("4s..2s", &span()).expect_err("reversed");
        assert_eq!(error.code, "E_INVALID_DURING_RANGE");
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
