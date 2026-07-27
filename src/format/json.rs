use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::compiler::{CompiledProgram, ExplainEntry};
use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
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
        Diagnostic::builtin(
            BuiltinDiagnostic::CompiledJson,
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
        SemanticNodeKind::DeferredImageVideo { path, extent, fit } => serde_json::json!({
            "operation": "deferred_image_video",
            "path": path,
            "extent": extent,
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
        SemanticNodeKind::Reference { symbol, .. } => reference_document(program, node, *symbol)?,
        SemanticNodeKind::Repeat { input, count } => serde_json::json!({
            "operation": "repeat", "input": input, "count": count,
        }),
        SemanticNodeKind::ZoomIn { input, by } => serde_json::json!({
            "operation": "zoom_in",
            "input": input,
            "by": by,
        }),
        SemanticNodeKind::FlashCut {
            before,
            after,
            frames,
        } => transition_document("flash_cut", *before, *after, *frames),
        SemanticNodeKind::Crossfade {
            before,
            after,
            frames,
        } => transition_document("crossfade", *before, *after, *frames),
        SemanticNodeKind::Concat { inputs } => serde_json::json!({
            "operation": "concat", "inputs": inputs,
        }),
        SemanticNodeKind::Slice { input, range } => native_range_document("slice", *input, *range),
        SemanticNodeKind::DeferredSlice { input, range } => serde_json::json!({
            "operation": "slice",
            "input": input,
            "unit": input.value_type().native_unit_name(),
            "range": range,
        }),
        SemanticNodeKind::ReplaceRange {
            base,
            replacement,
            range,
        } => native_replace_document(*base, *replacement, *range),
        SemanticNodeKind::DeferredReplaceRange {
            base,
            replacement,
            range,
        } => serde_json::json!({
            "operation": "replace_range",
            "base": base,
            "replacement": replacement,
            "unit": base.value_type().native_unit_name(),
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
        SemanticNodeKind::ExternalVideo { invocation } => external_video_document(invocation),
    })
}

fn native_range_document(
    operation: &str,
    input: ValueRef,
    range: crate::model::NativeRange,
) -> serde_json::Value {
    match range {
        crate::model::NativeRange::Frames(range) => serde_json::json!({
            "operation": operation,
            "input": input,
            "unit": "frames",
            "range": range,
        }),
        crate::model::NativeRange::Samples(range) => serde_json::json!({
            "operation": operation,
            "input": input,
            "unit": "samples",
            "range": range,
        }),
    }
}

fn native_replace_document(
    base: ValueRef,
    replacement: ValueRef,
    range: crate::model::NativeRange,
) -> serde_json::Value {
    let (unit, range) = match range {
        crate::model::NativeRange::Frames(range) => ("frames", serde_json::json!(range)),
        crate::model::NativeRange::Samples(range) => ("samples", serde_json::json!(range)),
    };
    serde_json::json!({
        "operation": "replace_range",
        "base": base,
        "replacement": replacement,
        "unit": unit,
        "range": range,
    })
}

fn reference_document(
    program: &CompiledProgram,
    node: &CompiledNode,
    symbol: crate::semantic::SymbolId,
) -> Result<serde_json::Value> {
    let target = program.symbol_value(symbol).ok_or_else(|| {
        Diagnostic::builtin(
            BuiltinDiagnostic::CompiledJson,
            format!("reference names unknown symbol {}", symbol.index()),
            node.origin().span.clone(),
        )
    })?;
    Ok(serde_json::json!({
        "operation": "reference",
        "target": target,
    }))
}

fn external_video_document(invocation: &crate::external::ExternalInvocation) -> serde_json::Value {
    serde_json::json!({
        "operation": "external_video",
        "executable": invocation.executable.value,
        "arguments": external_argument_documents(&invocation.arguments),
        "preserve_input": invocation.preserve_input,
        "inputs": invocation.inputs,
        "parameters": invocation.parameters,
    })
}

fn external_argument_documents(
    arguments: &[crate::external::ExternalArgumentValue],
) -> Vec<serde_json::Value> {
    arguments
        .iter()
        .map(|argument| match argument {
            crate::external::ExternalArgumentValue::Text { value } => {
                serde_json::json!({"kind": "text", "value": value})
            }
            crate::external::ExternalArgumentValue::File { path } => {
                serde_json::json!({"kind": "file", "path": path.value})
            }
        })
        .collect()
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
