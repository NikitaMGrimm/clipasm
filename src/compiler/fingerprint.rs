use std::collections::BTreeMap;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::compiler::{Evaluation, SemanticNodeKind};
use crate::diagnostic::{Diagnostic, Result, SourceSpan};
use crate::model::{ValueId, ValueRef, VideoDomain, VideoSpec};

#[derive(Serialize)]
struct CompiledIdentity<'a> {
    format_version: u32,
    engine_version: &'a str,
    video: &'a VideoSpec,
    root: &'a str,
    names: &'a BTreeMap<&'a str, String>,
}

#[derive(Serialize)]
struct ValueIdentity<'a> {
    semantic_version: u32,
    value_type: crate::model::ValueType,
    domain: &'a Option<VideoDomain>,
    operation: serde_json::Value,
    upstream: Vec<String>,
}

pub(super) fn compiled_structure_hash(
    evaluation: &Evaluation,
    domains: &[Option<VideoDomain>],
    video: &VideoSpec,
) -> Result<String> {
    let mut memo = BTreeMap::<ValueId, String>::new();
    let root = value_hash(evaluation.root, evaluation, domains, &mut memo)?;
    let names = evaluation
        .symbol_order
        .iter()
        .map(|name| {
            let value = evaluation.symbols[name]
                .value
                .expect("every collected symbol is evaluated");
            Ok((
                name.as_str(),
                value_hash(value, evaluation, domains, &mut memo)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;

    hash_serializable(&CompiledIdentity {
        format_version: 2,
        engine_version: env!("CARGO_PKG_VERSION"),
        video,
        root: &root,
        names: &names,
    })
}

fn value_hash(
    value: ValueRef,
    evaluation: &Evaluation,
    domains: &[Option<VideoDomain>],
    memo: &mut BTreeMap<ValueId, String>,
) -> Result<String> {
    if let Some(hash) = memo.get(&value.id()) {
        return Ok(hash.clone());
    }
    let node = &evaluation.nodes[value.id().0 as usize];
    let upstream = match &node.kind {
        SemanticNodeKind::ImageVideo { .. } => Vec::new(),
        SemanticNodeKind::Reference { name } => {
            let target = evaluation.symbols[name]
                .value
                .expect("references are resolved before fingerprinting");
            vec![value_hash(target, evaluation, domains, memo)?]
        }
        SemanticNodeKind::Concat { inputs } => inputs
            .iter()
            .map(|input| value_hash(*input, evaluation, domains, memo))
            .collect::<Result<Vec<_>>>()?,
        SemanticNodeKind::Slice { input, .. } => {
            vec![value_hash(*input, evaluation, domains, memo)?]
        }
        SemanticNodeKind::During {
            base, processed, ..
        } => vec![
            value_hash(*base, evaluation, domains, memo)?,
            value_hash(*processed, evaluation, domains, memo)?,
        ],
    };
    let operation = match &node.kind {
        SemanticNodeKind::ImageVideo { frames, fit, .. } => serde_json::json!({
            "operation": "image_video",
            "frames": frames,
            "fit": fit,
        }),
        SemanticNodeKind::Reference { .. } => serde_json::json!({
            "operation": "reference",
        }),
        SemanticNodeKind::Concat { .. } => serde_json::json!({
            "operation": "concat",
        }),
        SemanticNodeKind::Slice { range, .. } => serde_json::json!({
            "operation": "slice",
            "range": range,
        }),
        SemanticNodeKind::During { range, .. } => serde_json::json!({
            "operation": "during",
            "range": range,
        }),
    };
    let hash = hash_serializable(&ValueIdentity {
        semantic_version: node.semantic_version,
        value_type: node.value_type,
        domain: &domains[value.id().0 as usize],
        operation,
        upstream,
    })?;
    memo.insert(value.id(), hash.clone());
    Ok(hash)
}

pub(crate) fn hash_serializable(value: &impl Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        Diagnostic::new(
            "E_FINGERPRINT",
            format!("could not serialize semantic identity: {error}"),
            SourceSpan::file_start("<fingerprint>"),
        )
    })?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
