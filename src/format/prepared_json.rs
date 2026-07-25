use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::path::Path;

use serde::Serialize;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{
    AudioDomain, AudioSpec, FrameCount, FrameRange, ImageFit, NodeId, SampleRange, ValueType,
    VideoDomain, VideoSpec,
};
use crate::preflight::tools::{ExternalToolIdentity, ToolIdentity};
use crate::preflight::{
    PreparedAsset, PreparedAudioKind, PreparedExternalParameterValue, PreparedNode,
    PreparedNodeMedia, PreparedPlan, PreparedVideoKind,
};
use crate::semantic::SourceOrigin;
use crate::source::SourceSpan;

#[derive(Serialize)]
struct PreparedDocument<'a> {
    format_version: u32,
    engine_version: &'a str,
    semantic_hash: &'a str,
    video: &'a VideoSpec,
    audio: &'a AudioSpec,
    nodes: Vec<PreparedNodeDocument<'a>>,
    result: NodeId,
    named_values: &'a BTreeMap<String, NodeId>,
    output: &'a Path,
    manifest: &'a Path,
    ffmpeg: ToolDocument<'a>,
    ffprobe: ToolDocument<'a>,
    execution_namespace: &'a str,
}

#[derive(Serialize)]
struct PreparedNodeDocument<'a> {
    id: NodeId,
    kind: PreparedOperationDocument<'a>,
    value_type: ValueType,
    domain: Option<&'a VideoDomain>,
    audio_domain: Option<&'a AudioDomain>,
    has_audio: bool,
    origin: &'a SourceOrigin,
    fingerprint: &'a str,
}

#[derive(Serialize)]
struct PreparedAssetDocument<'a> {
    source_path: &'a Path,
    content_hash: &'a str,
}

#[derive(Serialize)]
struct ExternalToolDocument<'a> {
    executable: &'a Path,
    content_hash: &'a str,
}

#[derive(Serialize)]
struct ToolDocument<'a> {
    executable: &'a Path,
    version_summary: &'a str,
    build_fingerprint: &'a str,
}

#[derive(Serialize)]
#[serde(untagged)]
enum ExternalParameterDocument<'a> {
    Integer(i64),
    Keyword(&'a str),
    File(PreparedAssetDocument<'a>),
}

#[derive(Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum PreparedOperationDocument<'a> {
    ImageVideo {
        asset: PreparedAssetDocument<'a>,
        frames: FrameCount,
        fit: ImageFit,
    },
    VideoSource {
        asset: PreparedAssetDocument<'a>,
        frames: FrameCount,
        fit: ImageFit,
    },
    Slice {
        input: NodeId,
        range: FrameRange,
    },
    Repeat {
        input: NodeId,
        count: NonZeroU64,
        frames: FrameCount,
    },
    Zoom {
        input: NodeId,
        percent: u32,
    },
    Wobble {
        input: NodeId,
        pixels: u32,
    },
    FlashJoin {
        before: NodeId,
        after: NodeId,
        frames: FrameCount,
    },
    Crossfade {
        before: NodeId,
        after: NodeId,
        frames: FrameCount,
    },
    Concat {
        inputs: &'a [NodeId],
    },
    SetAudio {
        audio: NodeId,
        video: NodeId,
    },
    AudioOnBlack {
        audio: NodeId,
    },
    ExternalVideo {
        executable: ExternalToolDocument<'a>,
        inputs: &'a BTreeMap<String, NodeId>,
        parameters: BTreeMap<&'a str, ExternalParameterDocument<'a>>,
        preserve_input: &'a str,
    },
    AudioSource {
        asset: PreparedAssetDocument<'a>,
    },
    AudioSlice {
        input: NodeId,
        range: SampleRange,
    },
    AudioRepeat {
        input: NodeId,
        count: NonZeroU64,
    },
    AudioConcat {
        inputs: &'a [NodeId],
    },
    ExtractAudio {
        video: NodeId,
    },
}

pub(crate) fn prepared_plan(plan: &PreparedPlan) -> Result<String> {
    let document = PreparedDocument {
        format_version: plan.format_version(),
        engine_version: plan.engine_version(),
        semantic_hash: plan.semantic_hash(),
        video: plan.video(),
        audio: plan.audio(),
        nodes: plan.nodes().iter().map(node_document).collect(),
        result: plan.result(),
        named_values: plan.named_values(),
        output: plan.output(),
        manifest: plan.manifest(),
        ffmpeg: tool_document(plan.ffmpeg()),
        ffprobe: tool_document(plan.ffprobe()),
        execution_namespace: plan.execution_namespace(),
    };
    serde_json::to_string_pretty(&document).map_err(|error| {
        Diagnostic::new(
            "E_PREPARED_JSON",
            format!("could not serialize prepared plan: {error}"),
            SourceSpan::source_start(plan.entrypoint_source().clone()),
        )
    })
}

fn node_document(node: &PreparedNode) -> PreparedNodeDocument<'_> {
    match node.media() {
        PreparedNodeMedia::Video {
            kind,
            domain,
            has_audio,
        } => PreparedNodeDocument {
            id: node.id(),
            kind: video_operation_document(kind),
            value_type: ValueType::Video,
            domain: Some(domain),
            audio_domain: None,
            has_audio,
            origin: node.origin(),
            fingerprint: node.fingerprint(),
        },
        PreparedNodeMedia::Audio { kind, domain } => PreparedNodeDocument {
            id: node.id(),
            kind: audio_operation_document(kind),
            value_type: ValueType::Audio,
            domain: None,
            audio_domain: Some(domain),
            has_audio: false,
            origin: node.origin(),
            fingerprint: node.fingerprint(),
        },
    }
}

fn video_operation_document(kind: &PreparedVideoKind) -> PreparedOperationDocument<'_> {
    match kind {
        PreparedVideoKind::ImageVideo { asset, frames, fit } => {
            PreparedOperationDocument::ImageVideo {
                asset: asset_document(asset),
                frames: *frames,
                fit: *fit,
            }
        }
        PreparedVideoKind::VideoSource { asset, frames, fit } => {
            PreparedOperationDocument::VideoSource {
                asset: asset_document(asset),
                frames: *frames,
                fit: *fit,
            }
        }
        PreparedVideoKind::Slice { input, range } => PreparedOperationDocument::Slice {
            input: *input,
            range: *range,
        },
        PreparedVideoKind::Repeat {
            input,
            count,
            frames,
        } => PreparedOperationDocument::Repeat {
            input: *input,
            count: *count,
            frames: *frames,
        },
        PreparedVideoKind::Zoom { input, percent } => PreparedOperationDocument::Zoom {
            input: *input,
            percent: *percent,
        },
        PreparedVideoKind::Wobble { input, pixels } => PreparedOperationDocument::Wobble {
            input: *input,
            pixels: *pixels,
        },
        PreparedVideoKind::FlashJoin {
            before,
            after,
            frames,
        } => PreparedOperationDocument::FlashJoin {
            before: *before,
            after: *after,
            frames: *frames,
        },
        PreparedVideoKind::Crossfade {
            before,
            after,
            frames,
        } => PreparedOperationDocument::Crossfade {
            before: *before,
            after: *after,
            frames: *frames,
        },
        PreparedVideoKind::Concat { inputs } => PreparedOperationDocument::Concat { inputs },
        PreparedVideoKind::SetAudio { audio, video } => PreparedOperationDocument::SetAudio {
            audio: *audio,
            video: *video,
        },
        PreparedVideoKind::AudioOnBlack { audio } => {
            PreparedOperationDocument::AudioOnBlack { audio: *audio }
        }
        PreparedVideoKind::ExternalVideo {
            executable,
            inputs,
            parameters,
            preserve_input,
        } => PreparedOperationDocument::ExternalVideo {
            executable: external_tool_document(executable),
            inputs,
            parameters: external_parameter_documents(parameters),
            preserve_input,
        },
    }
}

fn audio_operation_document(kind: &PreparedAudioKind) -> PreparedOperationDocument<'_> {
    match kind {
        PreparedAudioKind::AudioSource { asset } => PreparedOperationDocument::AudioSource {
            asset: asset_document(asset),
        },
        PreparedAudioKind::AudioSlice { input, range } => PreparedOperationDocument::AudioSlice {
            input: *input,
            range: *range,
        },
        PreparedAudioKind::AudioRepeat { input, count } => PreparedOperationDocument::AudioRepeat {
            input: *input,
            count: *count,
        },
        PreparedAudioKind::AudioConcat { inputs } => {
            PreparedOperationDocument::AudioConcat { inputs }
        }
        PreparedAudioKind::ExtractAudio { video } => {
            PreparedOperationDocument::ExtractAudio { video: *video }
        }
    }
}

fn asset_document(asset: &PreparedAsset) -> PreparedAssetDocument<'_> {
    PreparedAssetDocument {
        source_path: asset.source_path(),
        content_hash: asset.content_hash(),
    }
}

fn external_tool_document(tool: &ExternalToolIdentity) -> ExternalToolDocument<'_> {
    ExternalToolDocument {
        executable: tool.executable(),
        content_hash: tool.content_hash(),
    }
}

fn tool_document(tool: &ToolIdentity) -> ToolDocument<'_> {
    ToolDocument {
        executable: tool.executable(),
        version_summary: tool.version(),
        build_fingerprint: tool.build_fingerprint(),
    }
}

fn external_parameter_documents(
    parameters: &BTreeMap<String, PreparedExternalParameterValue>,
) -> BTreeMap<&str, ExternalParameterDocument<'_>> {
    parameters
        .iter()
        .map(|(name, value)| {
            let value = match value {
                PreparedExternalParameterValue::Integer(value) => {
                    ExternalParameterDocument::Integer(*value)
                }
                PreparedExternalParameterValue::Keyword(value) => {
                    ExternalParameterDocument::Keyword(value)
                }
                PreparedExternalParameterValue::File(asset) => {
                    ExternalParameterDocument::File(asset_document(asset))
                }
            };
            (name.as_str(), value)
        })
        .collect()
}
