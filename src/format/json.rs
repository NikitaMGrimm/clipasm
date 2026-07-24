use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::compiler::{CompiledProgram, ExplainEntry};
use crate::diagnostic::{Diagnostic, Result, SourceSpan, Spanned};
use crate::model::{ValueId, ValueRef, ValueType, VideoDomain, VideoSpec};
use crate::semantic::{CompiledNode, SemanticNodeKind, SourceOrigin};

#[derive(Serialize)]
struct CompiledDocument<'a> {
    format_version: u32,
    engine_version: &'a str,
    structure_hash: &'a str,
    video: &'a VideoSpec,
    nodes: Vec<CompiledNodeDocument<'a>>,
    outputs: &'a [ValueRef],
    named_values: &'a BTreeMap<String, ValueRef>,
    explain: Vec<ExplainDocument<'a>>,
    output: Option<&'a Spanned<PathBuf>>,
}

#[derive(Serialize)]
struct CompiledNodeDocument<'a> {
    id: ValueId,
    kind: &'a SemanticNodeKind,
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
        nodes: program.nodes().iter().map(node_document).collect(),
        outputs: program.outputs(),
        named_values: program.named_values(),
        explain: program.explain().iter().map(explain_document).collect(),
        output: program.output(),
    };
    serde_json::to_string_pretty(&document).map_err(|error| {
        Diagnostic::new(
            "E_PLAN_SERIALIZATION",
            format!("could not serialize compiled program: {error}"),
            SourceSpan::file_start("<compiled-program>"),
        )
    })
}

fn node_document(node: &CompiledNode) -> CompiledNodeDocument<'_> {
    CompiledNodeDocument {
        id: node.id(),
        kind: node.kind(),
        value_type: node.value_type(),
        domain: node.domain(),
        semantic_version: node.semantic_version(),
        origin: node.origin(),
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
        span: entry.span(),
    }
}
