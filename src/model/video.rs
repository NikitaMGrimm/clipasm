use serde::{Deserialize, Serialize};

use crate::diagnostic::{Diagnostic, Result};
use crate::model::FrameCount;
use crate::source::SourceSpan;

/// A positive rational project frame rate in canonical reduced form.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct FrameRate {
    numerator: u32,
    denominator: u32,
}

impl FrameRate {
    /// Construct and reduce a positive rational frame rate.
    ///
    /// Returns `None` when either component is zero.
    #[must_use]
    pub const fn new(numerator: u32, denominator: u32) -> Option<Self> {
        if numerator == 0 || denominator == 0 {
            return None;
        }
        let divisor = gcd(numerator, denominator);
        Some(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    #[must_use]
    /// Return the reduced numerator.
    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    #[must_use]
    /// Return the reduced denominator.
    pub const fn denominator(self) -> u32 {
        self.denominator
    }

    /// Parse an integer or rational frame rate.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic for malformed or zero components.
    pub(crate) fn parse(text: &str, span: &SourceSpan) -> Result<Self> {
        let (numerator, denominator) = if let Some((n, d)) = text.split_once('/') {
            (parse_component(n, span)?, parse_component(d, span)?)
        } else {
            (parse_component(text, span)?, 1)
        };
        Self::new(numerator, denominator).ok_or_else(|| {
            Diagnostic::new(
                "E_INVALID_VIDEO_SPEC",
                "frame rate must be greater than zero",
                span.clone(),
            )
        })
    }
}

const fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn parse_component(text: &str, span: &SourceSpan) -> Result<u32> {
    text.parse::<u32>().map_err(|_| {
        Diagnostic::new(
            "E_INVALID_VIDEO_SPEC",
            format!("`{text}` is not a valid frame-rate component"),
            span.clone(),
        )
    })
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
/// Policy for fitting a source into the project video dimensions.
pub enum ImageFit {
    /// Scale to fill the frame and crop overflow without distortion.
    Cover,
    /// Scale to fit entirely within the frame and pad the remainder.
    Contain,
    /// Scale independently in each dimension to fill the frame.
    Stretch,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
/// Project-wide output dimensions and frame rate.
pub struct VideoSpec {
    /// Output width in pixels.
    pub width: u32,
    /// Output height in pixels.
    pub height: u32,
    /// Canonical project frame rate.
    pub fps: FrameRate,
}

impl Default for VideoSpec {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            fps: FrameRate {
                numerator: 30,
                denominator: 1,
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
/// Exact video properties for a compiled or prepared value.
pub struct VideoDomain {
    /// Exact duration in project frames.
    pub frames: FrameCount,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Project frame rate used by the value.
    pub frame_rate: FrameRate,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceSpan;

    #[test]
    fn frame_rates_are_reduced_on_construction_and_parsing() {
        assert_eq!(FrameRate::new(60, 2), FrameRate::new(30, 1));
        assert_eq!(FrameRate::new(60_000, 2_002), FrameRate::new(30_000, 1_001));
        assert_eq!(
            FrameRate::parse("60/2", &SourceSpan::file_start("test.yaml")).expect("frame rate"),
            FrameRate::new(30, 1).expect("frame rate")
        );
    }

    #[test]
    fn equivalent_frame_rates_serialize_identically() {
        let reducible = FrameRate::new(60, 2).expect("frame rate");
        let canonical = FrameRate::new(30, 1).expect("frame rate");
        assert_eq!(
            serde_json::to_string(&reducible).expect("serialize"),
            serde_json::to_string(&canonical).expect("serialize")
        );
    }

    #[test]
    fn frame_rate_accessors_expose_the_canonical_components() {
        let rate = FrameRate::new(60, 2).expect("frame rate");
        assert_eq!(rate.numerator(), 30);
        assert_eq!(rate.denominator(), 1);
    }
}
