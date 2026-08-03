#[cfg(feature = "native")]
use std::path::Path;

use serde::Serialize;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{ColorSpec, VideoSpec};
use crate::source::SourceSpan;

use super::ChromaLocation;

const ARTIFACT_CONTRACT_REVISION: u32 = 14;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct VideoEncoding {
    pixel_format: &'static str,
    component_bits: u8,
    color: ColorSpec,
    chroma_location: Option<ChromaLocation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct AudioEncoding {
    sample_format: &'static str,
    component_bits: u8,
    channel_layout: &'static str,
}

impl AudioEncoding {
    pub(crate) const fn sample_format(self) -> &'static str {
        self.sample_format
    }

    #[cfg(feature = "native")]
    pub(crate) const fn component_bits(self) -> u8 {
        self.component_bits
    }

    pub(crate) const fn channel_layout(self) -> &'static str {
        self.channel_layout
    }
}

impl VideoEncoding {
    pub(crate) const fn pixel_format(self) -> &'static str {
        self.pixel_format
    }

    pub(crate) const fn component_bits(self) -> u8 {
        self.component_bits
    }

    pub(crate) const fn color(self) -> ColorSpec {
        self.color
    }

    pub(crate) const fn chroma_location(self) -> Option<ChromaLocation> {
        self.chroma_location
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RenderPolicy {
    artifact_cache: ArtifactCachePolicy,
    export: ExportPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ArtifactCachePolicy {
    contract_revision: u32,
    video_extension: &'static str,
    audio_extension: &'static str,
    native_video_encoder: &'static str,
    native_video_level: u8,
    native_audio_encoder: &'static str,
    native_container: &'static str,
    video_encoding: VideoEncoding,
    audio_encoding: AudioEncoding,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ExportPolicy {
    extension: &'static str,
    video_encoder: &'static str,
    audio_encoder: &'static str,
    container: &'static str,
    video_encoding: VideoEncoding,
    movflags: &'static str,
}

impl RenderPolicy {
    pub(crate) const CURRENT: Self = Self {
        artifact_cache: ArtifactCachePolicy {
            contract_revision: ARTIFACT_CONTRACT_REVISION,
            video_extension: "mkv",
            audio_extension: "mka",
            native_video_encoder: "ffv1",
            native_video_level: 3,
            native_audio_encoder: "flac",
            native_container: "matroska",
            video_encoding: VideoEncoding {
                pixel_format: "yuv444p10le",
                component_bits: 10,
                color: ColorSpec::SDR_BT709,
                chroma_location: None,
            },
            audio_encoding: AudioEncoding {
                sample_format: "s16",
                component_bits: 16,
                channel_layout: "stereo",
            },
        },
        export: ExportPolicy {
            extension: "mp4",
            video_encoder: "libx264",
            audio_encoder: "aac",
            container: "mp4",
            video_encoding: VideoEncoding {
                pixel_format: "yuv420p",
                component_bits: 8,
                color: ColorSpec::SDR_BT709,
                chroma_location: Some(ChromaLocation::Left),
            },
            movflags: "+faststart",
        },
    };

    #[cfg(feature = "native")]
    pub(crate) const fn cache_contract(self) -> ArtifactCachePolicy {
        self.artifact_cache
    }

    pub(crate) const fn working_video_extension(self) -> &'static str {
        self.artifact_cache.video_extension
    }

    pub(crate) const fn working_audio_extension(self) -> &'static str {
        self.artifact_cache.audio_extension
    }

    pub(crate) const fn native_video_encoder(self) -> &'static str {
        self.artifact_cache.native_video_encoder
    }

    pub(crate) const fn native_video_level(self) -> u8 {
        self.artifact_cache.native_video_level
    }

    pub(crate) const fn native_audio_encoder(self) -> &'static str {
        self.artifact_cache.native_audio_encoder
    }

    pub(crate) const fn native_container(self) -> &'static str {
        self.artifact_cache.native_container
    }

    pub(crate) const fn working_video_encoding(self) -> VideoEncoding {
        self.artifact_cache.video_encoding
    }

    pub(crate) const fn working_audio_encoding(self) -> AudioEncoding {
        self.artifact_cache.audio_encoding
    }

    pub(crate) const fn export_video_encoder(self) -> &'static str {
        self.export.video_encoder
    }

    pub(crate) const fn export_audio_encoder(self) -> &'static str {
        self.export.audio_encoder
    }

    pub(crate) const fn export_container(self) -> &'static str {
        self.export.container
    }

    #[cfg(feature = "native")]
    pub(crate) const fn export_extension(self) -> &'static str {
        self.export.extension
    }

    pub(crate) const fn export_pixel_format(self) -> &'static str {
        self.export.video_encoding.pixel_format
    }

    pub(crate) const fn export_video_encoding(self) -> VideoEncoding {
        self.export.video_encoding
    }

    pub(crate) const fn export_movflags(self) -> &'static str {
        self.export.movflags
    }

    #[cfg(feature = "native")]
    pub(crate) fn validate_output_path(self, output: &Path, span: &SourceSpan) -> Result<()> {
        if output
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case(self.export_extension()))
        {
            return Ok(());
        }
        Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidOutputExtension,
            format!(
                "the foundation export profile requires an `.{}` output path",
                self.export_extension()
            ),
            span.clone(),
        ))
    }

    pub(crate) fn validate_video_spec(self, video: &VideoSpec, span: &SourceSpan) -> Result<()> {
        if video.width().is_multiple_of(2) && video.height().is_multiple_of(2) {
            return Ok(());
        }
        Err(Diagnostic::builtin(
            BuiltinDiagnostic::ExportDimensions,
            format!(
                "the MP4/H.264/{} export profile requires even width and height",
                self.export_pixel_format()
            ),
            span.clone(),
        ))
    }
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use super::*;

    #[test]
    fn cache_contract_serializes_every_working_artifact_value() {
        let value =
            serde_json::to_value(RenderPolicy::CURRENT.cache_contract()).expect("cache contract");
        assert_eq!(
            value,
            serde_json::json!({
                "contract_revision": ARTIFACT_CONTRACT_REVISION,
                "video_extension": "mkv",
                "audio_extension": "mka",
                "native_video_encoder": "ffv1",
                "native_video_level": 3,
                "native_audio_encoder": "flac",
                "native_container": "matroska",
                "video_encoding": {
                    "pixel_format": "yuv444p10le",
                    "component_bits": 10,
                    "color": {
                        "primaries": "bt709",
                        "transfer": "bt709",
                        "matrix": "bt709",
                        "range": "limited"
                    },
                    "chroma_location": null
                },
                "audio_encoding": {
                    "sample_format": "s16",
                    "component_bits": 16,
                    "channel_layout": "stereo"
                },
            })
        );
    }

    #[test]
    fn export_only_changes_do_not_change_the_cache_contract() {
        let current = RenderPolicy::CURRENT;
        let mut changed_export = current;
        changed_export.export.movflags = "different";
        changed_export.export.audio_encoder = "different";
        assert_ne!(changed_export, current);
        assert_eq!(changed_export.cache_contract(), current.cache_contract());
    }

    #[test]
    fn every_working_policy_change_changes_the_cache_contract() {
        let current = RenderPolicy::CURRENT;
        let variants = [
            ArtifactCachePolicy {
                contract_revision: ARTIFACT_CONTRACT_REVISION + 1,
                ..current.artifact_cache
            },
            ArtifactCachePolicy {
                video_extension: "different",
                ..current.artifact_cache
            },
            ArtifactCachePolicy {
                audio_extension: "different",
                ..current.artifact_cache
            },
            ArtifactCachePolicy {
                native_video_encoder: "different",
                ..current.artifact_cache
            },
            ArtifactCachePolicy {
                native_video_level: 4,
                ..current.artifact_cache
            },
            ArtifactCachePolicy {
                native_audio_encoder: "different",
                ..current.artifact_cache
            },
            ArtifactCachePolicy {
                native_container: "different",
                ..current.artifact_cache
            },
            ArtifactCachePolicy {
                video_encoding: VideoEncoding {
                    pixel_format: "different",
                    ..current.artifact_cache.video_encoding
                },
                ..current.artifact_cache
            },
            ArtifactCachePolicy {
                audio_encoding: AudioEncoding {
                    sample_format: "different",
                    ..current.artifact_cache.audio_encoding
                },
                ..current.artifact_cache
            },
        ];
        for variant in variants {
            assert_ne!(variant, current.cache_contract());
        }
    }
}
