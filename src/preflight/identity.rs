use std::collections::BTreeMap;
use std::num::NonZeroU64;

use serde::Serialize;

use crate::diagnostic::Result;
use crate::model::{AudioSpec, FrameCount, FrameRange, ImageFit, NodeId, SampleRange, VideoSpec};

use super::plan::{PreparedMedia, WorkingArtifactContract};
#[cfg(feature = "native")]
use super::policy::{ArtifactCachePolicy, RenderPolicy};
#[cfg(feature = "native")]
use super::tools::ToolIdentity;
use super::{
    PreparedAudioKind, PreparedExternalArgument, PreparedExternalParameterValue, PreparedNode,
    PreparedVideoKind,
};

const PREPARED_IDENTITY_REVISION: u32 = 14;

#[derive(Serialize)]
struct PreparedNodeIdentity<'a> {
    semantic_version: u32,
    artifact_contract: WorkingArtifactContract<'a>,
    has_audio: bool,
    operation: PreparedOperationIdentity<'a>,
    upstream: Vec<&'a str>,
}

#[derive(Serialize)]
struct PreparedPlanIdentity<'a> {
    identity_revision: u32,
    video: &'a VideoSpec,
    audio: AudioSpec,
    result: &'a str,
    names: BTreeMap<&'a str, &'a str>,
}

#[derive(Serialize)]
#[cfg(feature = "native")]
struct CacheIdentity<'a> {
    artifact_cache_policy: ArtifactCachePolicy,
    ffmpeg_build_fingerprint: &'a str,
    ffprobe_build_fingerprint: &'a str,
}

#[derive(Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum PreparedOperationIdentity<'a> {
    ImageVideo {
        content_hash: &'a str,
        color: &'a crate::preflight::PreparedSourceColor,
        frames: FrameCount,
        fit: ImageFit,
    },
    VideoSource {
        content_hash: &'a str,
        color: &'a crate::preflight::PreparedSourceColor,
        frames: FrameCount,
        fit: ImageFit,
    },
    Slice {
        range: FrameRange,
    },
    Repeat {
        count: NonZeroU64,
        frames: FrameCount,
    },
    ZoomIn {
        curve: &'a str,
    },
    FlashCut {
        frames: FrameCount,
    },
    #[serde(rename = "crossfade")]
    CrossfadeFrames {
        frames: FrameCount,
    },
    #[serde(rename = "crossfade")]
    CrossfadeSamples {
        samples: u64,
    },
    Concat,
    SetAudio,
    AudioOnBlack,
    ExternalVideo {
        protocol_version: u32,
        executable_content_hash: &'a str,
        arguments: Vec<PreparedExternalArgumentIdentity<'a>>,
        input_names: Vec<&'a str>,
        parameters: BTreeMap<&'a str, PreparedExternalParameterIdentity<'a>>,
        preserve_input: &'a str,
    },
    AudioSource {
        content_hash: &'a str,
    },
    AudioSlice {
        range: SampleRange,
    },
    AudioRepeat {
        count: NonZeroU64,
    },
    AudioConcat,
    ExtractAudio,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PreparedExternalArgumentIdentity<'a> {
    Text { value: &'a str },
    File { content_hash: &'a str },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PreparedExternalParameterIdentity<'a> {
    Integer { value: i64 },
    Keyword { value: &'a str },
    File { content_hash: &'a str },
}

pub(super) fn node_fingerprint(
    media: &PreparedMedia,
    semantic_version: u32,
    existing: &[PreparedNode],
) -> Result<String> {
    let mut inputs = Vec::new();
    let (has_audio, operation) = match media {
        PreparedMedia::Video {
            kind,
            domain: _,
            working_audio: _,
            has_audio,
        } => {
            kind.visit_inputs(|input| inputs.push(input));
            (*has_audio, video_identity(kind))
        }
        PreparedMedia::Audio { kind, domain: _ } => {
            kind.visit_inputs(|input| inputs.push(input));
            (false, audio_identity(kind))
        }
    };
    let upstream = inputs
        .iter()
        .map(|input| existing[input.get() as usize].fingerprint())
        .collect::<Vec<_>>();
    crate::identity::hash_serializable(&PreparedNodeIdentity {
        semantic_version,
        artifact_contract: media.artifact_contract(),
        has_audio,
        operation,
        upstream,
    })
}

fn video_identity(kind: &PreparedVideoKind) -> PreparedOperationIdentity<'_> {
    match kind {
        PreparedVideoKind::ImageVideo {
            asset,
            color,
            frames,
            fit,
        } => PreparedOperationIdentity::ImageVideo {
            content_hash: asset.content_hash(),
            color,
            frames: *frames,
            fit: *fit,
        },
        PreparedVideoKind::VideoSource {
            asset,
            color,
            frames,
            fit,
        } => PreparedOperationIdentity::VideoSource {
            content_hash: asset.content_hash(),
            color,
            frames: *frames,
            fit: *fit,
        },
        PreparedVideoKind::Slice { input: _, range } => {
            PreparedOperationIdentity::Slice { range: *range }
        }
        PreparedVideoKind::Repeat {
            input: _,
            count,
            frames,
        } => PreparedOperationIdentity::Repeat {
            count: *count,
            frames: *frames,
        },
        PreparedVideoKind::ZoomIn { input: _, curve } => PreparedOperationIdentity::ZoomIn {
            curve: curve.identity(),
        },
        PreparedVideoKind::FlashCut {
            before: _,
            after: _,
            frames,
        } => PreparedOperationIdentity::FlashCut { frames: *frames },
        PreparedVideoKind::Crossfade {
            before: _,
            after: _,
            frames,
        } => PreparedOperationIdentity::CrossfadeFrames { frames: *frames },
        PreparedVideoKind::Concat { inputs: _ } => PreparedOperationIdentity::Concat,
        PreparedVideoKind::SetAudio { audio: _, video: _ } => PreparedOperationIdentity::SetAudio,
        PreparedVideoKind::AudioOnBlack { audio: _ } => PreparedOperationIdentity::AudioOnBlack,
        PreparedVideoKind::ExternalVideo {
            executable,
            arguments,
            inputs,
            parameters,
            preserve_input,
        } => PreparedOperationIdentity::ExternalVideo {
            protocol_version: crate::contracts::EXTERNAL_PROGRAM_PROTOCOL_VERSION,
            executable_content_hash: executable.content_hash(),
            arguments: arguments
                .iter()
                .map(|argument| match argument {
                    PreparedExternalArgument::Text(value) => {
                        PreparedExternalArgumentIdentity::Text { value }
                    }
                    PreparedExternalArgument::File(asset) => {
                        PreparedExternalArgumentIdentity::File {
                            content_hash: asset.content_hash(),
                        }
                    }
                })
                .collect(),
            input_names: inputs.keys().map(String::as_str).collect(),
            parameters: parameters
                .iter()
                .map(|(name, value)| {
                    let value = match value {
                        PreparedExternalParameterValue::Integer(value) => {
                            PreparedExternalParameterIdentity::Integer { value: *value }
                        }
                        PreparedExternalParameterValue::Keyword(value) => {
                            PreparedExternalParameterIdentity::Keyword { value }
                        }
                        PreparedExternalParameterValue::File(asset) => {
                            PreparedExternalParameterIdentity::File {
                                content_hash: asset.content_hash(),
                            }
                        }
                    };
                    (name.as_str(), value)
                })
                .collect(),
            preserve_input,
        },
    }
}

fn audio_identity(kind: &PreparedAudioKind) -> PreparedOperationIdentity<'_> {
    match kind {
        PreparedAudioKind::AudioSource { asset } => PreparedOperationIdentity::AudioSource {
            content_hash: asset.content_hash(),
        },
        PreparedAudioKind::AudioSlice { input: _, range } => {
            PreparedOperationIdentity::AudioSlice { range: *range }
        }
        PreparedAudioKind::AudioRepeat { input: _, count } => {
            PreparedOperationIdentity::AudioRepeat { count: *count }
        }
        PreparedAudioKind::AudioConcat { inputs: _ } => PreparedOperationIdentity::AudioConcat,
        PreparedAudioKind::Crossfade {
            before: _,
            after: _,
            samples,
        } => PreparedOperationIdentity::CrossfadeSamples { samples: *samples },
        PreparedAudioKind::ExtractAudio { video: _ } => PreparedOperationIdentity::ExtractAudio,
    }
}

pub(super) fn prepared_semantic_hash(
    video: &VideoSpec,
    audio: AudioSpec,
    result: NodeId,
    names: &BTreeMap<String, NodeId>,
    nodes: &[PreparedNode],
) -> Result<String> {
    let names = names
        .iter()
        .map(|(name, id)| (name.as_str(), nodes[id.get() as usize].fingerprint()))
        .collect::<BTreeMap<_, _>>();
    crate::identity::hash_serializable(&PreparedPlanIdentity {
        identity_revision: PREPARED_IDENTITY_REVISION,
        video,
        audio,
        result: nodes[result.get() as usize].fingerprint(),
        names,
    })
}

#[cfg(feature = "native")]
pub(super) fn cache_execution_namespace(
    render_policy: RenderPolicy,
    ffmpeg: &ToolIdentity,
    ffprobe: &ToolIdentity,
) -> Result<String> {
    crate::identity::hash_serializable(&CacheIdentity {
        artifact_cache_policy: render_policy.cache_contract(),
        ffmpeg_build_fingerprint: ffmpeg.build_fingerprint(),
        ffprobe_build_fingerprint: ffprobe.build_fingerprint(),
    })
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn cache_identity_includes_working_policy_and_tool_builds() {
        let ffmpeg = ToolIdentity {
            executable: PathBuf::from("/tools/ffmpeg"),
            version_summary: "ffmpeg test".to_owned(),
            build_fingerprint: "ffmpeg-build".to_owned(),
        };
        let ffprobe = ToolIdentity {
            executable: PathBuf::from("/tools/ffprobe"),
            version_summary: "ffprobe test".to_owned(),
            build_fingerprint: "ffprobe-build".to_owned(),
        };
        let identity =
            cache_execution_namespace(RenderPolicy::CURRENT, &ffmpeg, &ffprobe).expect("identity");
        assert!(!identity.is_empty());

        let mut changed_ffmpeg = ffmpeg.clone();
        changed_ffmpeg.build_fingerprint = "different-ffmpeg-build".to_owned();
        assert_ne!(
            cache_execution_namespace(RenderPolicy::CURRENT, &changed_ffmpeg, &ffprobe)
                .expect("changed FFmpeg identity"),
            identity
        );

        let mut changed_ffprobe = ffprobe.clone();
        changed_ffprobe.build_fingerprint = "different-ffprobe-build".to_owned();
        assert_ne!(
            cache_execution_namespace(RenderPolicy::CURRENT, &ffmpeg, &changed_ffprobe)
                .expect("changed FFprobe identity"),
            identity
        );
    }
}
