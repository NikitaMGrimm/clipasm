use std::collections::BTreeMap;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::compiler::evaluate::Evaluation;
use crate::diagnostic::{Diagnostic, Result, SourceSpan};
use crate::model::{ValueId, ValueRef, VideoDomain, VideoSpec};
use crate::semantic::SemanticNodeKind;

#[derive(Serialize)]
struct CompiledIdentity<'a> {
    format_version: u32,
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
    format_version: u32,
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
        format_version,
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
    let node = &evaluation.nodes[value.id().get() as usize];
    if let SemanticNodeKind::Reference { name } = node.kind() {
        let target = evaluation.symbols[name]
            .value
            .expect("references are resolved before fingerprinting");
        let hash = value_hash(target, evaluation, domains, memo)?;
        memo.insert(value.id(), hash.clone());
        return Ok(hash);
    }
    let upstream = match node.kind() {
        SemanticNodeKind::ImageVideo { .. } | SemanticNodeKind::VideoSource { .. } => Vec::new(),
        SemanticNodeKind::Reference { .. } => unreachable!("handled above"),
        SemanticNodeKind::Concat { inputs } => inputs
            .iter()
            .map(|input| value_hash(*input, evaluation, domains, memo))
            .collect::<Result<Vec<_>>>()?,
        SemanticNodeKind::Slice { input, .. } => {
            vec![value_hash(*input, evaluation, domains, memo)?]
        }
        SemanticNodeKind::ReplaceRange {
            base, replacement, ..
        } => vec![
            value_hash(*base, evaluation, domains, memo)?,
            value_hash(*replacement, evaluation, domains, memo)?,
        ],
    };
    let operation = match node.kind() {
        SemanticNodeKind::ImageVideo { frames, fit, .. } => serde_json::json!({
            "operation": "image_video",
            "frames": frames,
            "fit": fit,
        }),
        SemanticNodeKind::VideoSource { fit, .. } => serde_json::json!({
            "operation": "video_source",
            "fit": fit,
        }),
        SemanticNodeKind::Reference { .. } => unreachable!("handled above"),
        SemanticNodeKind::Concat { .. } => serde_json::json!({
            "operation": "concat",
        }),
        SemanticNodeKind::Slice { range, .. } => serde_json::json!({
            "operation": "slice",
            "range": range,
        }),
        SemanticNodeKind::ReplaceRange { range, .. } => serde_json::json!({
            "operation": "replace_range",
            "range": range,
        }),
    };
    let hash = hash_serializable(&ValueIdentity {
        semantic_version: node.semantic_version(),
        value_type: node.value_type(),
        domain: &domains[value.id().get() as usize],
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::evaluate::{DeclaredValueType, Evaluation, Symbol};
    use crate::diagnostic::SourceSpan;
    use crate::model::{FrameCount, ImageFit, ValueType, VideoDomain, VideoSpec};
    use crate::semantic::{GraphBuilder, SourceOrigin};

    #[test]
    fn reference_hash_is_exactly_its_target_hash() {
        let target = ValueRef::new(ValueId::new(0), ValueType::Video);
        let reference = ValueRef::new(ValueId::new(1), ValueType::Video);
        let span = SourceSpan::file_start("workflow.yaml");
        let domain = VideoDomain {
            frames: FrameCount(1),
            width: 1280,
            height: 720,
            frame_rate: VideoSpec::default().fps,
        };
        let mut symbols = BTreeMap::new();
        symbols.insert(
            "source".to_owned(),
            Symbol {
                declared_at: span.clone(),
                value: Some(target),
                declared_type: DeclaredValueType::Known(ValueType::Video),
                value_type: Some(ValueType::Video),
            },
        );
        let mut nodes = Vec::new();
        GraphBuilder::for_program(
            &mut nodes,
            &VideoSpec::default(),
            7,
            SourceOrigin {
                construct: "source",
                span: span.clone(),
            },
        )
        .image_video("source.png".into(), FrameCount(1), ImageFit::Cover)
        .expect("source");
        GraphBuilder::for_program(
            &mut nodes,
            &VideoSpec::default(),
            1,
            SourceOrigin {
                construct: "reference",
                span,
            },
        )
        .reference("source".to_owned(), ValueType::Video)
        .expect("reference");
        let evaluation = Evaluation {
            nodes,
            symbols,
            symbol_order: vec!["source".to_owned()],
            surface: Vec::new(),
            root: reference,
        };
        let domains = vec![Some(domain.clone()), Some(domain)];
        let mut memo = BTreeMap::new();
        let target_hash = value_hash(target, &evaluation, &domains, &mut memo).expect("target");
        let reference_hash =
            value_hash(reference, &evaluation, &domains, &mut memo).expect("reference");
        assert_eq!(target_hash, reference_hash);
    }
}
