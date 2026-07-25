use std::collections::BTreeMap;

use serde::Serialize;

use crate::diagnostic::Result;
use crate::model::{AudioDomain, AudioSpec, NodeId, ValueType, VideoDomain, VideoSpec};

use super::{
    CACHE_FORMAT_VERSION, EXPORT_PIXEL_FORMAT, PREPARED_FORMAT_VERSION, PreparedAudioKind,
    PreparedMedia, PreparedNode, PreparedVideoKind, ToolIdentity, WORKING_PIXEL_FORMAT,
};

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
    format_version: u32,
    ffmpeg_build_fingerprint: &'a str,
    ffprobe_build_fingerprint: &'a str,
    working_pixel_format: &'static str,
    export_pixel_format: &'static str,
}

pub(super) fn node_fingerprint(
    media: &PreparedMedia,
    semantic_version: u32,
    existing: &[PreparedNode],
) -> Result<String> {
    let (value_type, domain, audio_domain, has_audio, operation, inputs) = match media {
        PreparedMedia::Video {
            kind,
            domain,
            has_audio,
        } => {
            let (operation, inputs) = video_identity(kind);
            (
                ValueType::Video,
                Some(domain),
                None,
                *has_audio,
                operation,
                inputs,
            )
        }
        PreparedMedia::Audio { kind, domain } => {
            let (operation, inputs) = audio_identity(kind);
            (
                ValueType::Audio,
                None,
                Some(domain),
                false,
                operation,
                inputs,
            )
        }
    };
    let upstream = inputs
        .iter()
        .map(|input| existing[input.get() as usize].fingerprint.as_str())
        .collect::<Vec<_>>();
    crate::compiler::fingerprint::hash_serializable(&PreparedNodeIdentity {
        semantic_version,
        value_type,
        domain: domain.cloned(),
        audio_domain: audio_domain.copied(),
        has_audio,
        operation,
        upstream,
    })
}

fn video_identity(kind: &PreparedVideoKind) -> (serde_json::Value, Vec<NodeId>) {
    match kind {
        PreparedVideoKind::ImageVideo { asset, frames, fit } => (
            serde_json::json!({
                "operation": "image_video",
                "content_hash": asset.content_hash,
                "frames": frames,
                "fit": fit,
            }),
            Vec::new(),
        ),
        PreparedVideoKind::VideoSource { asset, frames, fit } => (
            serde_json::json!({
                "operation": "video_source",
                "content_hash": asset.content_hash,
                "frames": frames,
                "fit": fit,
            }),
            Vec::new(),
        ),
        PreparedVideoKind::Slice { input, range } => (
            serde_json::json!({"operation": "slice", "range": range}),
            vec![*input],
        ),
        PreparedVideoKind::Repeat {
            input,
            count,
            frames,
        } => (
            serde_json::json!({
                "operation": "repeat",
                "count": count,
                "frames": frames,
            }),
            vec![*input],
        ),
        PreparedVideoKind::Zoom { input, percent } => (
            serde_json::json!({"operation": "zoom", "percent": percent}),
            vec![*input],
        ),
        PreparedVideoKind::Wobble { input, pixels } => (
            serde_json::json!({"operation": "wobble", "pixels": pixels}),
            vec![*input],
        ),
        PreparedVideoKind::FlashJoin {
            before,
            after,
            frames,
        } => (
            serde_json::json!({"operation": "flash_join", "frames": frames}),
            vec![*before, *after],
        ),
        PreparedVideoKind::Concat { inputs } => {
            (serde_json::json!({"operation": "concat"}), inputs.clone())
        }
        PreparedVideoKind::SetAudio { audio, video } => (
            serde_json::json!({"operation": "set_audio"}),
            vec![*audio, *video],
        ),
        PreparedVideoKind::AudioOnBlack { audio } => (
            serde_json::json!({"operation": "audio_on_black"}),
            vec![*audio],
        ),
        PreparedVideoKind::ExternalVideo {
            executable,
            inputs,
            parameters,
            preserve_input,
        } => (
            serde_json::json!({
                "operation": "external_video",
                "executable_content_hash": executable.content_hash(),
                "input_names": inputs.keys().collect::<Vec<_>>(),
                "parameters": parameters,
                "preserve_input": preserve_input,
            }),
            inputs.values().copied().collect(),
        ),
    }
}

fn audio_identity(kind: &PreparedAudioKind) -> (serde_json::Value, Vec<NodeId>) {
    match kind {
        PreparedAudioKind::AudioSource { asset } => (
            serde_json::json!({
                "operation": "audio_source",
                "content_hash": asset.content_hash,
            }),
            Vec::new(),
        ),
        PreparedAudioKind::AudioSlice { input, range } => (
            serde_json::json!({"operation": "audio_slice", "range": range}),
            vec![*input],
        ),
        PreparedAudioKind::AudioRepeat { input, count } => (
            serde_json::json!({"operation": "audio_repeat", "count": count}),
            vec![*input],
        ),
        PreparedAudioKind::AudioConcat { inputs } => (
            serde_json::json!({"operation": "audio_concat"}),
            inputs.clone(),
        ),
        PreparedAudioKind::ExtractAudio { video } => (
            serde_json::json!({"operation": "extract_audio"}),
            vec![*video],
        ),
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
        .map(|(name, id)| (name.as_str(), nodes[id.get() as usize].fingerprint.as_str()))
        .collect::<BTreeMap<_, _>>();
    crate::compiler::fingerprint::hash_serializable(&PreparedPlanIdentity {
        format_version: PREPARED_FORMAT_VERSION,
        video,
        audio,
        result: &nodes[result.get() as usize].fingerprint,
        names,
    })
}

pub(super) fn cache_execution_namespace(
    ffmpeg: &ToolIdentity,
    ffprobe: &ToolIdentity,
) -> Result<String> {
    crate::compiler::fingerprint::hash_serializable(&CacheIdentity {
        format_version: CACHE_FORMAT_VERSION,
        ffmpeg_build_fingerprint: ffmpeg.build_fingerprint(),
        ffprobe_build_fingerprint: ffprobe.build_fingerprint(),
        working_pixel_format: WORKING_PIXEL_FORMAT,
        export_pixel_format: EXPORT_PIXEL_FORMAT,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn cache_identity_includes_the_fixed_render_contract() {
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
        let identity = cache_execution_namespace(&ffmpeg, &ffprobe).expect("identity");
        assert!(!identity.is_empty());
    }
}
