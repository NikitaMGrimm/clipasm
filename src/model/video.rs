use std::num::NonZeroU32;

use serde::{Deserialize, Serialize, Serializer};

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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
/// Project-wide output dimensions and frame rate.
///
/// Width and height are always greater than zero, and the frame rate is a
/// positive reduced rational.
pub struct VideoSpec {
    width: NonZeroU32,
    height: NonZeroU32,
    fps: FrameRate,
}

impl VideoSpec {
    /// Construct a valid project video format.
    ///
    /// Returns `None` when either dimension is zero.
    #[must_use]
    pub const fn new(width: u32, height: u32, fps: FrameRate) -> Option<Self> {
        let Some(width) = NonZeroU32::new(width) else {
            return None;
        };
        let Some(height) = NonZeroU32::new(height) else {
            return None;
        };
        Some(Self { width, height, fps })
    }

    /// Return the frame width in pixels.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width.get()
    }

    /// Return the frame height in pixels.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height.get()
    }

    /// Return the canonical project frame rate.
    #[must_use]
    pub const fn fps(self) -> FrameRate {
        self.fps
    }
}

impl Default for VideoSpec {
    fn default() -> Self {
        Self::new(
            1280,
            720,
            FrameRate::new(30, 1).expect("default frame rate is positive"),
        )
        .expect("default video dimensions are positive")
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Exact video properties for a compiled or prepared value.
pub struct VideoDomain {
    frames: FrameCount,
    spec: VideoSpec,
}

impl VideoDomain {
    /// Construct an exact domain in the supplied project video format.
    #[must_use]
    pub const fn new(frames: FrameCount, spec: VideoSpec) -> Self {
        Self { frames, spec }
    }

    /// Return the exact duration in project frames.
    #[must_use]
    pub const fn frames(self) -> FrameCount {
        self.frames
    }

    /// Return the complete video format.
    #[must_use]
    pub const fn video_spec(self) -> VideoSpec {
        self.spec
    }

    /// Return the frame width in pixels.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.spec.width()
    }

    /// Return the frame height in pixels.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.spec.height()
    }

    /// Return the project frame rate used by this value.
    #[must_use]
    pub const fn frame_rate(self) -> FrameRate {
        self.spec.fps()
    }
}

impl Serialize for VideoDomain {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct Document {
            frames: FrameCount,
            width: u32,
            height: u32,
            frame_rate: FrameRate,
        }

        Document {
            frames: self.frames,
            width: self.width(),
            height: self.height(),
            frame_rate: self.frame_rate(),
        }
        .serialize(serializer)
    }
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

    #[test]
    fn video_specs_and_domains_protect_dimensions_without_changing_json() {
        let fps = FrameRate::new(30_000, 1_001).expect("frame rate");
        assert!(VideoSpec::new(0, 720, fps).is_none());
        assert!(VideoSpec::new(1280, 0, fps).is_none());

        let spec = VideoSpec::new(1280, 720, fps).expect("video spec");
        assert_eq!(spec.width(), 1280);
        assert_eq!(spec.height(), 720);
        assert_eq!(spec.fps(), fps);

        let domain = VideoDomain::new(FrameCount(30), spec);
        assert_eq!(domain.frames(), FrameCount(30));
        assert_eq!(domain.video_spec(), spec);
        assert_eq!(
            serde_json::to_value(domain).expect("domain JSON"),
            serde_json::json!({
                "frames": 30,
                "width": 1280,
                "height": 720,
                "frame_rate": {"numerator": 30000, "denominator": 1001},
            })
        );
    }
}
