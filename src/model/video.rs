use serde::{Deserialize, Serialize};

use crate::diagnostic::{Diagnostic, Result, SourceSpan};
use crate::model::FrameCount;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct FrameRate {
    pub numerator: u32,
    pub denominator: u32,
}

impl FrameRate {
    #[must_use]
    pub fn new(numerator: u32, denominator: u32) -> Option<Self> {
        (numerator > 0 && denominator > 0).then_some(Self {
            numerator,
            denominator,
        })
    }

    /// Parse an integer or rational frame rate.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic for malformed or zero components.
    pub fn parse(text: &str, span: &SourceSpan) -> Result<Self> {
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
pub enum PixelFormat {
    Yuv420p,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFit {
    Cover,
    Contain,
    Stretch,
}

impl ImageFit {
    /// Parse a supported image fitting policy.
    ///
    /// # Errors
    ///
    /// Returns `E_INVALID_IMAGE_FIT` for unknown policies.
    pub fn parse(text: &str, span: &SourceSpan) -> Result<Self> {
        match text {
            "cover" => Ok(Self::Cover),
            "contain" => Ok(Self::Contain),
            "stretch" => Ok(Self::Stretch),
            _ => Err(Diagnostic::new(
                "E_INVALID_IMAGE_FIT",
                format!("unknown image fit `{text}`; expected cover, contain, or stretch"),
                span.clone(),
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct VideoSpec {
    pub width: u32,
    pub height: u32,
    pub fps: FrameRate,
    pub pixel_format: PixelFormat,
    pub square_pixels: bool,
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
            pixel_format: PixelFormat::Yuv420p,
            square_pixels: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VideoDomain {
    pub frames: FrameCount,
    pub spec: VideoSpec,
}
