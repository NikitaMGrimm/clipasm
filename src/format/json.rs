use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::compiler::{CompiledProgram, ExplainEntry};
use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::external::{ExternalArgumentValue, ExternalInvocation, ExternalParameterValue};
use crate::model::{
    AudioSpec, ExactNumber, FrameCount, FrameRange, ImageFit, NativeRange, SampleRange,
    TimelineExpression, TimelineRangeExpression, ValueId, ValueRef, ValueType, VideoDomain,
    VideoSpec,
};
use crate::semantic::{CompiledNode, SemanticNodeKind, SymbolId};
use crate::source::SourceSpan;

use super::{
    SourceOriginDocument, SourceSpanDocument, SpannedDocument, source_origin_document,
    source_span_document, spanned_document,
};

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
    output: Option<SpannedDocument<'a, PathBuf>>,
}

#[derive(Serialize)]
struct CompiledNodeDocument<'a> {
    id: ValueId,
    kind: CompiledOperationDocument<'a>,
    value_type: ValueType,
    domain: Option<&'a VideoDomain>,
    semantic_version: u32,
    origin: SourceOriginDocument<'a>,
}

#[derive(Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum CompiledOperationDocument<'a> {
    ImageVideo {
        path: &'a Path,
        frames: FrameCount,
        fit: ImageFit,
    },
    DeferredImageVideo {
        path: &'a Path,
        extent: &'a TimelineExpression,
        fit: ImageFit,
    },
    VideoSource {
        path: &'a Path,
        fit: ImageFit,
    },
    AudioSource {
        path: &'a Path,
    },
    Reference {
        target: ValueRef,
    },
    Repeat {
        input: ValueRef,
        count: NonZeroU64,
    },
    ZoomIn {
        input: ValueRef,
        by: &'a ExactNumber,
    },
    FlashCut {
        before: ValueRef,
        after: ValueRef,
        frames: FrameCount,
    },
    Crossfade {
        before: ValueRef,
        after: ValueRef,
        frames: FrameCount,
    },
    Concat {
        inputs: &'a [ValueRef],
    },
    Slice {
        input: ValueRef,
        unit: &'static str,
        range: NativeRangeDocument,
    },
    #[serde(rename = "slice")]
    DeferredSlice {
        input: ValueRef,
        unit: &'static str,
        range: &'a TimelineRangeExpression,
    },
    ReplaceRange {
        base: ValueRef,
        replacement: ValueRef,
        unit: &'static str,
        range: NativeRangeDocument,
    },
    #[serde(rename = "replace_range")]
    DeferredReplaceRange {
        base: ValueRef,
        replacement: ValueRef,
        unit: &'static str,
        range: &'a TimelineRangeExpression,
    },
    ExtractAudio {
        video: ValueRef,
    },
    SetAudio {
        audio: ValueRef,
        video: ValueRef,
    },
    AudioOnBlack {
        audio: ValueRef,
    },
    ExternalVideo {
        executable: &'a Path,
        arguments: Vec<ExternalArgumentDocument<'a>>,
        preserve_input: &'a str,
        inputs: &'a BTreeMap<String, ValueRef>,
        parameters: BTreeMap<&'a str, ExternalParameterDocument<'a>>,
    },
}

#[derive(Serialize)]
#[serde(untagged)]
enum NativeRangeDocument {
    Frames(FrameRange),
    Samples(SampleRange),
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ExternalArgumentDocument<'a> {
    Text { value: &'a str },
    File { path: &'a Path },
}

#[derive(Serialize)]
#[serde(untagged)]
enum ExternalParameterDocument<'a> {
    Integer(i64),
    Keyword(&'a str),
    File(SpannedDocument<'a, PathBuf>),
}

#[derive(Serialize)]
struct ExplainDocument<'a> {
    construct: &'a str,
    outputs: Vec<ExplainOutputDocument<'a>>,
    span: SourceSpanDocument<'a>,
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
        output: program.output().map(spanned_document),
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
        origin: source_origin_document(node.origin()),
    })
}

fn operation_document<'a>(
    program: &CompiledProgram,
    node: &'a CompiledNode,
) -> Result<CompiledOperationDocument<'a>> {
    Ok(match node.kind() {
        SemanticNodeKind::ImageVideo { path, frames, fit } => {
            CompiledOperationDocument::ImageVideo {
                path,
                frames: *frames,
                fit: *fit,
            }
        }
        SemanticNodeKind::DeferredImageVideo { path, extent, fit } => {
            CompiledOperationDocument::DeferredImageVideo {
                path,
                extent,
                fit: *fit,
            }
        }
        SemanticNodeKind::VideoSource { path, fit } => {
            CompiledOperationDocument::VideoSource { path, fit: *fit }
        }
        SemanticNodeKind::AudioSource { path } => CompiledOperationDocument::AudioSource { path },
        SemanticNodeKind::Reference {
            symbol,
            value_type: _,
        } => CompiledOperationDocument::Reference {
            target: reference_target(program, node, *symbol)?,
        },
        SemanticNodeKind::Repeat { input, count } => CompiledOperationDocument::Repeat {
            input: *input,
            count: *count,
        },
        SemanticNodeKind::ZoomIn { input, by } => {
            CompiledOperationDocument::ZoomIn { input: *input, by }
        }
        SemanticNodeKind::FlashCut {
            before,
            after,
            frames,
        } => CompiledOperationDocument::FlashCut {
            before: *before,
            after: *after,
            frames: *frames,
        },
        SemanticNodeKind::Crossfade {
            before,
            after,
            frames,
        } => CompiledOperationDocument::Crossfade {
            before: *before,
            after: *after,
            frames: *frames,
        },
        SemanticNodeKind::Concat { inputs } => CompiledOperationDocument::Concat { inputs },
        SemanticNodeKind::Slice { input, range } => slice_document(*input, *range),
        SemanticNodeKind::DeferredSlice { input, range } => {
            CompiledOperationDocument::DeferredSlice {
                input: *input,
                unit: input.value_type().native_unit_name(),
                range,
            }
        }
        SemanticNodeKind::ReplaceRange {
            base,
            replacement,
            range,
        } => {
            let (unit, range) = native_range_document(*range);
            CompiledOperationDocument::ReplaceRange {
                base: *base,
                replacement: *replacement,
                unit,
                range,
            }
        }
        SemanticNodeKind::DeferredReplaceRange {
            base,
            replacement,
            range,
        } => CompiledOperationDocument::DeferredReplaceRange {
            base: *base,
            replacement: *replacement,
            unit: base.value_type().native_unit_name(),
            range,
        },
        SemanticNodeKind::ExtractAudio { video } => {
            CompiledOperationDocument::ExtractAudio { video: *video }
        }
        SemanticNodeKind::SetAudio { audio, video } => CompiledOperationDocument::SetAudio {
            audio: *audio,
            video: *video,
        },
        SemanticNodeKind::AudioOnBlack { audio } => {
            CompiledOperationDocument::AudioOnBlack { audio: *audio }
        }
        SemanticNodeKind::ExternalVideo { invocation } => external_video_document(invocation),
    })
}

fn slice_document<'a>(input: ValueRef, range: NativeRange) -> CompiledOperationDocument<'a> {
    let (unit, range) = native_range_document(range);
    CompiledOperationDocument::Slice { input, unit, range }
}

fn reference_target(
    program: &CompiledProgram,
    node: &CompiledNode,
    symbol: SymbolId,
) -> Result<ValueRef> {
    program.symbol_value(symbol).ok_or_else(|| {
        Diagnostic::builtin(
            BuiltinDiagnostic::CompiledJson,
            format!("reference names unknown symbol {}", symbol.index()),
            node.origin().span.clone(),
        )
    })
}

fn native_range_document(range: NativeRange) -> (&'static str, NativeRangeDocument) {
    match range {
        NativeRange::Frames(range) => ("frames", NativeRangeDocument::Frames(range)),
        NativeRange::Samples(range) => ("samples", NativeRangeDocument::Samples(range)),
    }
}

fn external_video_document(invocation: &ExternalInvocation) -> CompiledOperationDocument<'_> {
    CompiledOperationDocument::ExternalVideo {
        executable: &invocation.executable.value,
        arguments: invocation
            .arguments
            .iter()
            .map(|argument| match argument {
                ExternalArgumentValue::Text { value } => ExternalArgumentDocument::Text { value },
                ExternalArgumentValue::File { path } => {
                    ExternalArgumentDocument::File { path: &path.value }
                }
            })
            .collect(),
        preserve_input: &invocation.preserve_input,
        inputs: &invocation.inputs,
        parameters: invocation
            .parameters
            .iter()
            .map(|(name, value)| {
                let value = match value {
                    ExternalParameterValue::Integer(value) => {
                        ExternalParameterDocument::Integer(*value)
                    }
                    ExternalParameterValue::Keyword(value) => {
                        ExternalParameterDocument::Keyword(value)
                    }
                    ExternalParameterValue::File(path) => {
                        ExternalParameterDocument::File(spanned_document(path))
                    }
                };
                (name.as_str(), value)
            })
            .collect(),
    }
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
        span: source_span_document(entry.span()),
    }
}
