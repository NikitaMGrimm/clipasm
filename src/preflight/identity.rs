use std::collections::BTreeMap;

use serde::Serialize;

use crate::diagnostic::Result;
use crate::model::{AudioDomain, AudioSpec, NodeId, ValueType, VideoDomain, VideoSpec};

use super::plan::PreparedMedia;
use super::policy::{ArtifactCachePolicy, RenderPolicy};
use super::tools::ToolIdentity;
use super::{PREPARED_FORMAT_VERSION, PreparedAudioKind, PreparedNode, PreparedVideoKind};

#[derive(Serialize)]
struct PreparedNodeIdentity<'a> {
    semantic_version: u32,
    value_type: ValueType,
    domain: Option<VideoDomain>,
    audio_domain: Option<AudioDomain>,
    has_audio: bool,
    operation: serde_json::Value,
    upstream: Vec<&'a str>,
}

#[derive(Serialize)]
struct PreparedPlanIdentity<'a> {
    format_version: u32,
    video: &'a VideoSpec,
    audio: AudioSpec,
    result: &'a str,
    names: BTreeMap<&'a str, &'a str>,
}

#[derive(Serialize)]
struct CacheIdentity<'a> {
    artifact_cache_policy: ArtifactCachePolicy,
    ffmpeg_build_fingerprint: &'a str,
    ffprobe_build_fingerprint: &'a str,
}

pub(super) fn node_fingerprint(
    media: &PreparedMedia,
    semantic_version: u32,
    existing: &[PreparedNode],
) -> Result<String> {
    let mut inputs = Vec::new();
    let (value_type, domain, audio_domain, has_audio, operation) = match media {
        PreparedMedia::Video {
            kind,
            domain,
            has_audio,
        } => {
            kind.visit_inputs(|input| inputs.push(input));
            (
                ValueType::Video,
                Some(domain),
                None,
                *has_audio,
                video_identity(kind),
            )
        }
        PreparedMedia::Audio { kind, domain } => {
            kind.visit_inputs(|input| inputs.push(input));
            (
                ValueType::Audio,
                None,
                Some(domain),
                false,
                audio_identity(kind),
            )
        }
    };
    let upstream = inputs
        .iter()
        .map(|input| existing[input.get() as usize].fingerprint())
        .collect::<Vec<_>>();
    crate::compiler::fingerprint::hash_serializable(&PreparedNodeIdentity {
        semantic_version,
        value_type,
        domain: domain.copied(),
        audio_domain: audio_domain.copied(),
        has_audio,
        operation,
        upstream,
    })
}

fn video_identity(kind: &PreparedVideoKind) -> serde_json::Value {
    match kind {
        PreparedVideoKind::ImageVideo { asset, frames, fit } => serde_json::json!({
            "operation": "image_video",
            "content_hash": asset.content_hash(),
            "frames": frames,
            "fit": fit,
        }),
        PreparedVideoKind::VideoSource { asset, frames, fit } => serde_json::json!({
            "operation": "video_source",
            "content_hash": asset.content_hash(),
            "frames": frames,
            "fit": fit,
        }),
        PreparedVideoKind::Slice { range, .. } => {
            serde_json::json!({"operation": "slice", "range": range})
        }
        PreparedVideoKind::Repeat { count, frames, .. } => serde_json::json!({
            "operation": "repeat",
            "count": count,
            "frames": frames,
        }),
        PreparedVideoKind::Zoom { percent, .. } => {
            serde_json::json!({"operation": "zoom", "percent": percent})
        }
        PreparedVideoKind::Wobble { pixels, .. } => {
            serde_json::json!({"operation": "wobble", "pixels": pixels})
        }
        PreparedVideoKind::FlashJoin { frames, .. } => {
            serde_json::json!({"operation": "flash_join", "frames": frames})
        }
        PreparedVideoKind::Crossfade { frames, .. } => {
            serde_json::json!({"operation": "crossfade", "frames": frames})
        }
        PreparedVideoKind::Concat { .. } => serde_json::json!({"operation": "concat"}),
        PreparedVideoKind::SetAudio { .. } => serde_json::json!({"operation": "set_audio"}),
        PreparedVideoKind::AudioOnBlack { .. } => {
            serde_json::json!({"operation": "audio_on_black"})
        }
        PreparedVideoKind::ExternalVideo {
            executable,
            arguments,
            inputs,
            parameters,
            preserve_input,
        } => serde_json::json!({
            "operation": "external_video",
            "protocol_version": crate::external::EXTERNAL_PROTOCOL_VERSION,
            "executable_content_hash": executable.content_hash(),
            "arguments": arguments.iter().map(|argument| match argument {
                super::PreparedExternalArgument::Text(value) => {
                    serde_json::json!({"kind": "text", "value": value})
                }
                super::PreparedExternalArgument::File(asset) => serde_json::json!({
                    "kind": "file",
                    "content_hash": asset.content_hash(),
                }),
            }).collect::<Vec<_>>(),
            "input_names": inputs.keys().collect::<Vec<_>>(),
            "parameters": external_parameter_identity(parameters),
            "preserve_input": preserve_input,
        }),
    }
}

fn external_parameter_identity(
    parameters: &BTreeMap<String, super::PreparedExternalParameterValue>,
) -> BTreeMap<&str, serde_json::Value> {
    parameters
        .iter()
        .map(|(name, value)| {
            let identity = match value {
                super::PreparedExternalParameterValue::Integer(value) => {
                    serde_json::json!(value)
                }
                super::PreparedExternalParameterValue::Keyword(value) => {
                    serde_json::json!(value)
                }
                super::PreparedExternalParameterValue::File(asset) => serde_json::json!({
                    "content_hash": asset.content_hash(),
                }),
            };
            (name.as_str(), identity)
        })
        .collect()
}

fn audio_identity(kind: &PreparedAudioKind) -> serde_json::Value {
    match kind {
        PreparedAudioKind::AudioSource { asset } => serde_json::json!({
            "operation": "audio_source",
            "content_hash": asset.content_hash(),
        }),
        PreparedAudioKind::AudioSlice { range, .. } => {
            serde_json::json!({"operation": "audio_slice", "range": range})
        }
        PreparedAudioKind::AudioRepeat { count, .. } => {
            serde_json::json!({"operation": "audio_repeat", "count": count})
        }
        PreparedAudioKind::AudioConcat { .. } => {
            serde_json::json!({"operation": "audio_concat"})
        }
        PreparedAudioKind::ExtractAudio { .. } => {
            serde_json::json!({"operation": "extract_audio"})
        }
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
    crate::compiler::fingerprint::hash_serializable(&PreparedPlanIdentity {
        format_version: PREPARED_FORMAT_VERSION,
        video,
        audio,
        result: nodes[result.get() as usize].fingerprint(),
        names,
    })
}

pub(super) fn cache_execution_namespace(
    render_policy: RenderPolicy,
    ffmpeg: &ToolIdentity,
    ffprobe: &ToolIdentity,
) -> Result<String> {
    crate::compiler::fingerprint::hash_serializable(&CacheIdentity {
        artifact_cache_policy: render_policy.cache_contract(),
        ffmpeg_build_fingerprint: ffmpeg.build_fingerprint(),
        ffprobe_build_fingerprint: ffprobe.build_fingerprint(),
    })
}

#[cfg(test)]
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
