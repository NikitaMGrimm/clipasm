use serde::Serialize;

use crate::model::ColorSpec;

/// How preflight resolved the interpretation of a media source.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceColorConvention {
    /// The language-defined sRGB interpretation for an untagged RGB still.
    ImageSrgb,
    /// Complete color metadata carried by a video stream.
    ExplicitVideo,
}

/// Exact color interpretation resolved for one prepared media source.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PreparedSourceColor {
    color: ColorSpec,
    pixel_format: String,
    chroma_location: Option<String>,
    convention: SourceColorConvention,
}

impl PreparedSourceColor {
    pub(crate) fn image_srgb_rgb(pixel_format: String) -> Self {
        Self {
            color: ColorSpec::SRGB_RGB,
            pixel_format,
            chroma_location: None,
            convention: SourceColorConvention::ImageSrgb,
        }
    }

    pub(crate) fn image_srgb_yuv(pixel_format: String, chroma_location: Option<String>) -> Self {
        Self {
            color: ColorSpec::SRGB_BT601_FULL,
            pixel_format,
            chroma_location,
            convention: SourceColorConvention::ImageSrgb,
        }
    }

    pub(crate) fn explicit_video(
        color: ColorSpec,
        pixel_format: String,
        chroma_location: Option<String>,
    ) -> Self {
        Self {
            color,
            pixel_format,
            chroma_location,
            convention: SourceColorConvention::ExplicitVideo,
        }
    }

    /// Return the resolved color tuple.
    #[must_use]
    pub const fn color(&self) -> ColorSpec {
        self.color
    }

    /// Return the decoder pixel format observed during preflight.
    #[must_use]
    pub fn pixel_format(&self) -> &str {
        &self.pixel_format
    }

    /// Return the resolved source chroma location, when applicable.
    #[must_use]
    pub fn chroma_location(&self) -> Option<&str> {
        self.chroma_location.as_deref()
    }

    /// Return how the source interpretation was established.
    #[must_use]
    pub const fn convention(&self) -> SourceColorConvention {
        self.convention
    }
}
