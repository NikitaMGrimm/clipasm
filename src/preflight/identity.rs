use std::collections::BTreeMap;

use serde::Serialize;

use crate::diagnostic::Result;
use crate::model::{AudioDomain, AudioSpec, NodeId, ValueType, VideoDomain, VideoSpec};

use super::{
    CACHE_FORMAT_VERSION, PREPARED_FORMAT_VERSION, PreparedNode, PreparedNodeKind,
    RenderMediaPolicy, ToolIdentity,
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
    media_policy: RenderMediaPolicy,
}

pub(super) fn node_fingerprint(
    kind: &PreparedNodeKind,
    value_type: ValueType,
    domain: Option<&VideoDomain>,
    audio_domain: Option<&AudioDomain>,
    has_audio: bool,
    semantic_version: u32,
    existing: &[PreparedNode],
) -> Result<String> {
    let (operation, inputs) = match kind {
        PreparedNodeKind::ImageVideo { asset, frames, fit } => (
            serde_json::json!({
                "operation": "image_video",
                "content_hash": asset.content_hash,
                "frames": frames,
                "fit": fit,
            }),
            Vec::new(),
        ),
        PreparedNodeKind::VideoSource { asset, frames, fit } => (
            serde_json::json!({
                "operation": "video_source",
                "content_hash": asset.content_hash,
                "frames": frames,
                "fit": fit,
            }),
            Vec::new(),
        ),
        PreparedNodeKind::AudioSource { asset } => (
            serde_json::json!({
                "operation": "audio_source",
                "content_hash": asset.content_hash,
            }),
            Vec::new(),
        ),
        PreparedNodeKind::Slice { input, range } => (
            serde_json::json!({
                "operation": "slice",
                "range": range,
            }),
            vec![*input],
        ),
        PreparedNodeKind::Repeat {
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
        PreparedNodeKind::Zoom { input, percent } => (
            serde_json::json!({
                "operation": "zoom",
                "percent": percent,
            }),
            vec![*input],
        ),
        PreparedNodeKind::Wobble { input, pixels } => (
            serde_json::json!({
                "operation": "wobble",
                "pixels": pixels,
            }),
            vec![*input],
        ),
        PreparedNodeKind::FlashJoin {
            before,
            after,
            frames,
        } => (
            serde_json::json!({
                "operation": "flash_join",
                "frames": frames,
            }),
            vec![*before, *after],
        ),
        PreparedNodeKind::Concat { inputs } => {
            (serde_json::json!({"operation": "concat"}), inputs.clone())
        }
        PreparedNodeKind::ExtractAudio { video } => (
            serde_json::json!({"operation": "extract_audio"}),
            vec![*video],
        ),
        PreparedNodeKind::SetAudio { audio, video } => (
            serde_json::json!({"operation": "set_audio"}),
            vec![*audio, *video],
        ),
        PreparedNodeKind::AudioOnBlack { audio } => (
            serde_json::json!({"operation": "audio_on_black"}),
            vec![*audio],
        ),
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
    media_policy: RenderMediaPolicy,
) -> Result<String> {
    crate::compiler::fingerprint::hash_serializable(&CacheIdentity {
        format_version: CACHE_FORMAT_VERSION,
        ffmpeg_build_fingerprint: ffmpeg.build_fingerprint(),
        ffprobe_build_fingerprint: ffprobe.build_fingerprint(),
        media_policy,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::preflight::{ExportPixelFormat, WorkingPixelFormat};

    #[test]
    fn cache_identity_includes_the_working_media_policy() {
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
        let default = cache_execution_namespace(&ffmpeg, &ffprobe, RenderMediaPolicy::default())
            .expect("default identity");
        let changed = cache_execution_namespace(
            &ffmpeg,
            &ffprobe,
            RenderMediaPolicy {
                working_pixel_format: WorkingPixelFormat::Test,
                export_pixel_format: ExportPixelFormat::Yuv420p,
            },
        )
        .expect("changed identity");
        assert_ne!(default, changed);
    }
}
