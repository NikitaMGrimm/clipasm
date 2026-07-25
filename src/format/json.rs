use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::compiler::{CompiledProgram, ExplainEntry};
use crate::diagnostic::{Diagnostic, Result};
use crate::model::{AudioSpec, ValueId, ValueRef, ValueType, VideoDomain, VideoSpec};
use crate::semantic::{CompiledNode, SemanticNodeKind, SourceOrigin};
use crate::source::{SourceSpan, Spanned};

#[derive(Serialize)]
struct CompiledDocument<'a> {
    format_version: u32,
    engine_version: &'a str,
    structure_hash: &'a str,
    video: &'a VideoSpec,
    audio: &'a AudioSpec,
    nodes: Vec<CompiledNodeDocument<'a>>,
    outputs: &'a [ValueRef],
    named_values: &'a BTreeMap<String, ValueRef>,
    explain: Vec<ExplainDocument<'a>>,
    output: Option<&'a Spanned<PathBuf>>,
}

#[derive(Serialize)]
struct CompiledNodeDocument<'a> {
    id: ValueId,
    kind: serde_json::Value,
    value_type: ValueType,
    domain: Option<&'a VideoDomain>,
    semantic_version: u32,
    origin: &'a SourceOrigin,
}

#[derive(Serialize)]
struct ExplainDocument<'a> {
    construct: &'a str,
    outputs: Vec<ExplainOutputDocument<'a>>,
    span: &'a SourceSpan,
}

#[derive(Serialize)]
struct ExplainOutputDocument<'a> {
    value: ValueRef,
    id: Option<&'a str>,
}

pub(crate) fn compiled_program(program: &CompiledProgram) -> Result<String> {
    let document = CompiledDocument {
        format_version: program.format_version(),
        engine_version: program.engine_version(),
        structure_hash: program.structure_hash(),
        video: program.video(),
        audio: program.audio(),
        nodes: program
            .nodes()
            .iter()
            .map(|node| node_document(program, node))
            .collect::<Result<Vec<_>>>()?,
        outputs: program.outputs(),
        named_values: program.named_values(),
        explain: program.explain().iter().map(explain_document).collect(),
        output: program.output(),
    };
    serde_json::to_string_pretty(&document).map_err(|error| {
        Diagnostic::new(
            "E_COMPILED_JSON",
            format!("could not serialize compiled program: {error}"),
            SourceSpan::file_start("<compiled-program>"),
        )
    })
}

fn node_document<'a>(
    program: &CompiledProgram,
    node: &'a CompiledNode,
) -> Result<CompiledNodeDocument<'a>> {
    Ok(CompiledNodeDocument {
        id: node.id(),
        kind: operation_document(program, node)?,
        value_type: node.value_type(),
        domain: node.domain(),
        semantic_version: node.semantic_version(),
        origin: node.origin(),
    })
}

fn operation_document(program: &CompiledProgram, node: &CompiledNode) -> Result<serde_json::Value> {
    Ok(match node.kind() {
        SemanticNodeKind::ImageVideo { path, frames, fit } => serde_json::json!({
            "operation": "image_video",
            "path": path,
            "frames": frames,
            "fit": fit,
        }),
        SemanticNodeKind::VideoSource { path, fit } => serde_json::json!({
            "operation": "video_source",
            "path": path,
            "fit": fit,
        }),
        SemanticNodeKind::AudioSource { path } => serde_json::json!({
            "operation": "audio_source",
            "path": path,
        }),
        SemanticNodeKind::Reference { symbol, .. } => {
            let target = program.symbol_value(*symbol).ok_or_else(|| {
                Diagnostic::new(
                    "E_COMPILED_JSON",
                    format!("reference names unknown symbol {}", symbol.index()),
                    node.origin().span.clone(),
                )
            })?;
            serde_json::json!({
                "operation": "reference",
                "target": target,
            })
        }
        SemanticNodeKind::Repeat { input, count } => serde_json::json!({
            "operation": "repeat", "input": input, "count": count,
        }),
        SemanticNodeKind::AudioRepeat { input, count } => serde_json::json!({
            "operation": "audio_repeat", "input": input, "count": count,
        }),
        SemanticNodeKind::Zoom { input, percent } => serde_json::json!({
            "operation": "zoom",
            "input": input,
            "percent": percent,
        }),
        SemanticNodeKind::Wobble { input, pixels } => serde_json::json!({
            "operation": "wobble",
            "input": input,
            "pixels": pixels,
        }),
        SemanticNodeKind::FlashJoin {
            before,
            after,
            frames,
        } => transition_document("flash_join", *before, *after, *frames),
        SemanticNodeKind::Crossfade {
            before,
            after,
            frames,
        } => transition_document("crossfade", *before, *after, *frames),
        SemanticNodeKind::Concat { inputs } => serde_json::json!({
            "operation": "concat", "inputs": inputs,
        }),
        SemanticNodeKind::AudioConcat { inputs } => serde_json::json!({
            "operation": "audio_concat", "inputs": inputs,
        }),
        SemanticNodeKind::Slice { input, range } => serde_json::json!({
            "operation": "slice", "input": input, "range": range,
        }),
        SemanticNodeKind::AudioSlice { input, range } => serde_json::json!({
            "operation": "audio_slice", "input": input, "range": range,
        }),
        SemanticNodeKind::ReplaceRange {
            base,
            replacement,
            range,
        } => serde_json::json!({
            "operation": "replace_range",
            "base": base,
            "replacement": replacement,
            "range": range,
        }),
        SemanticNodeKind::ExtractAudio { video } => serde_json::json!({
            "operation": "extract_audio",
            "video": video,
        }),
        SemanticNodeKind::SetAudio { audio, video } => serde_json::json!({
            "operation": "set_audio",
            "audio": audio,
            "video": video,
        }),
        SemanticNodeKind::AudioOnBlack { audio } => serde_json::json!({
            "operation": "audio_on_black",
            "audio": audio,
        }),
        SemanticNodeKind::ExternalVideo { invocation } => serde_json::json!({
            "operation": "external_video",
            "command": invocation.command.value,
            "preserve_input": invocation.preserve_input,
            "inputs": invocation.inputs,
            "parameters": invocation.parameters,
        }),
    })
}

fn transition_document(
    operation: &str,
    before: ValueRef,
    after: ValueRef,
    frames: crate::model::FrameCount,
) -> serde_json::Value {
    serde_json::json!({
        "operation": operation,
        "before": before,
        "after": after,
        "frames": frames,
    })
}

fn explain_document(entry: &ExplainEntry) -> ExplainDocument<'_> {
    ExplainDocument {
        construct: entry.construct(),
        outputs: entry
            .outputs()
            .iter()
            .map(|output| ExplainOutputDocument {
                value: output.value(),
                id: output.id(),
            })
            .collect(),
        span: entry.span(),
    }
}
