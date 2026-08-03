use serde::Serialize;

use crate::model::ColorSpec;

/// Chroma sample position for a physically subsampled video signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum ChromaLocation {
    /// Chroma is horizontally co-sited with the left luma sample.
    #[serde(rename = "left")]
    Left,
    /// Chroma is centered between neighboring luma samples.
    #[serde(rename = "center")]
    Center,
    /// Chroma is co-sited with the top-left luma sample.
    #[serde(rename = "topleft")]
    TopLeft,
    /// Chroma is vertically co-sited with the top luma sample.
    #[serde(rename = "top")]
    Top,
    /// Chroma is co-sited with the bottom-left luma sample.
    #[serde(rename = "bottomleft")]
    BottomLeft,
    /// Chroma is vertically co-sited with the bottom luma sample.
    #[serde(rename = "bottom")]
    Bottom,
}

impl ChromaLocation {
    pub(crate) fn from_ffmpeg_name(name: &str) -> Option<Self> {
        match name {
            "left" => Some(Self::Left),
            "center" => Some(Self::Center),
            "topleft" => Some(Self::TopLeft),
            "top" => Some(Self::Top),
            "bottomleft" => Some(Self::BottomLeft),
            "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }

    pub(crate) const fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::TopLeft => "topleft",
            Self::Top => "top",
            Self::BottomLeft => "bottomleft",
            Self::Bottom => "bottom",
        }
    }
}

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
    chroma_location: Option<ChromaLocation>,
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

    pub(crate) fn image_srgb_yuv(
        pixel_format: String,
        chroma_location: Option<ChromaLocation>,
    ) -> Self {
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
        chroma_location: Option<ChromaLocation>,
    ) -> Self {
        Self {
            color,
            pixel_format,
            chroma_location,
            convention: SourceColorConvention::ExplicitVideo,
        }
    }

    pub(crate) const fn color(&self) -> ColorSpec {
        self.color
    }

    /// Return the decoder pixel format observed during preflight.
    #[must_use]
    pub fn pixel_format(&self) -> &str {
        &self.pixel_format
    }

    /// Return the resolved source chroma location, when applicable.
    #[must_use]
    pub const fn chroma_location(&self) -> Option<ChromaLocation> {
        self.chroma_location
    }

    /// Return how the source interpretation was established.
    #[must_use]
    pub const fn convention(&self) -> SourceColorConvention {
        self.convention
    }
}
