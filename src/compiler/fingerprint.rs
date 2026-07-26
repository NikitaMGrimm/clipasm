use std::collections::BTreeMap;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::compiler::evaluate::Evaluation;
use crate::diagnostic::{Diagnostic, Result};
use crate::model::{AudioSpec, ValueRef, VideoDomain, VideoSpec};
use crate::semantic::{SemanticDependency, SemanticNodeKind};
use crate::source::SourceSpan;

#[derive(Serialize)]
struct CompiledIdentity<'a> {
    format_version: u32,
    video: &'a VideoSpec,
    audio: &'a AudioSpec,
    outputs: Vec<&'a str>,
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
    audio: AudioSpec,
    format_version: u32,
    order: &[ValueRef],
) -> Result<String> {
    let hashes = value_hashes(evaluation, domains, order)?;
    let outputs = evaluation
        .outputs
        .iter()
        .map(|output| {
            hashes[output.id().get() as usize]
                .as_deref()
                .expect("topological order includes every output")
        })
        .collect::<Vec<_>>();
    let names = evaluation
        .public_symbols
        .iter()
        .map(|(name, key)| {
            let value = evaluation.symbols[key.index()]
                .value
                .expect("every collected symbol is evaluated");
            (
                name.as_str(),
                hashes[value.id().get() as usize]
                    .as_ref()
                    .expect("topological order includes named values")
                    .clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();

    hash_serializable(&CompiledIdentity {
        format_version,
        video,
        audio: &audio,
        outputs,
        names: &names,
    })
}

fn value_hashes(
    evaluation: &Evaluation,
    domains: &[Option<VideoDomain>],
    order: &[ValueRef],
) -> Result<Vec<Option<String>>> {
    let mut hashes = vec![None::<String>; evaluation.nodes.len()];
    for value in order {
        let index = value.id().get() as usize;
        let node = &evaluation.nodes[index];
        if let SemanticNodeKind::Reference { symbol, .. } = node.kind() {
            let target = evaluation.symbols[symbol.index()]
                .value
                .expect("references are resolved before fingerprinting");
            hashes[index] = Some(
                hashes[target.id().get() as usize]
                    .as_ref()
                    .expect("reference target precedes its alias")
                    .clone(),
            );
            continue;
        }
        let upstream = upstream_hashes(node.kind(), &hashes);
        let operation = operation_identity(node.kind(), &hashes)?;
        hashes[index] = Some(hash_serializable(&ValueIdentity {
            semantic_version: node.semantic_version(),
            value_type: node.value_type(),
            domain: &domains[index],
            operation,
            upstream,
        })?);
    }
    Ok(hashes)
}

fn upstream_hashes(kind: &SemanticNodeKind, hashes: &[Option<String>]) -> Vec<String> {
    let mut upstream = Vec::new();
    kind.visit_dependencies(|dependency| match dependency {
        SemanticDependency::Value(value) => {
            upstream.push(node_hash(value, hashes).to_owned());
        }
        SemanticDependency::Symbol(_) => {
            unreachable!("references are handled before upstream hashing")
        }
    });
    upstream
}

fn operation_identity(
    kind: &SemanticNodeKind,
    hashes: &[Option<String>],
) -> Result<serde_json::Value> {
    match kind {
        SemanticNodeKind::ImageVideo { path, frames, fit } => {
            let path = identity_value(path)?;
            Ok(serde_json::json!({
                "operation": "image_video", "path": path, "frames": frames, "fit": fit,
            }))
        }
        SemanticNodeKind::DeferredImageVideo { path, extent, fit } => {
            let path = identity_value(path)?;
            Ok(serde_json::json!({
                "operation": "deferred_image_video",
                "path": path,
                "extent": timeline_expression_identity(extent, hashes),
                "fit": fit,
            }))
        }
        SemanticNodeKind::VideoSource { path, fit } => {
            let path = identity_value(path)?;
            Ok(serde_json::json!({
                "operation": "video_source", "path": path, "fit": fit,
            }))
        }
        SemanticNodeKind::AudioSource { path } => {
            let path = identity_value(path)?;
            Ok(serde_json::json!({"operation": "audio_source", "path": path}))
        }
        SemanticNodeKind::Reference { .. } => unreachable!("references are handled separately"),
        SemanticNodeKind::Repeat { count, .. } => {
            Ok(serde_json::json!({"operation": "repeat", "count": count}))
        }
        SemanticNodeKind::AudioRepeat { count, .. } => {
            Ok(serde_json::json!({"operation": "audio_repeat", "count": count}))
        }
        SemanticNodeKind::ZoomIn { by, .. } => {
            Ok(serde_json::json!({"operation": "zoom_in", "by": by}))
        }
        SemanticNodeKind::FlashCut { frames, .. } => {
            Ok(serde_json::json!({"operation": "flash_cut", "frames": frames}))
        }
        SemanticNodeKind::Crossfade { frames, .. } => {
            Ok(serde_json::json!({"operation": "crossfade", "frames": frames}))
        }
        SemanticNodeKind::Concat { .. } => Ok(serde_json::json!({"operation": "concat"})),
        SemanticNodeKind::AudioConcat { .. } => {
            Ok(serde_json::json!({"operation": "audio_concat"}))
        }
        SemanticNodeKind::Slice { range, .. } => {
            Ok(serde_json::json!({"operation": "slice", "range": range}))
        }
        SemanticNodeKind::DeferredSlice { range, .. } => Ok(deferred_slice_identity(range, hashes)),
        SemanticNodeKind::AudioSlice { range, .. } => {
            Ok(serde_json::json!({"operation": "audio_slice", "range": range}))
        }
        SemanticNodeKind::ReplaceRange { range, .. } => {
            Ok(serde_json::json!({"operation": "replace_range", "range": range}))
        }
        SemanticNodeKind::DeferredReplaceRange { range, .. } => Ok(serde_json::json!({
            "operation": "deferred_replace_range",
            "range": timeline_range_identity(range, hashes),
        })),
        SemanticNodeKind::ExtractAudio { .. } => {
            Ok(serde_json::json!({"operation": "extract_audio"}))
        }
        SemanticNodeKind::SetAudio { .. } => Ok(serde_json::json!({"operation": "set_audio"})),
        SemanticNodeKind::AudioOnBlack { .. } => {
            Ok(serde_json::json!({"operation": "audio_on_black"}))
        }
        SemanticNodeKind::ExternalVideo { invocation } => {
            let executable = identity_value(&invocation.executable.value)?;
            let arguments = invocation
                .arguments
                .iter()
                .map(|argument| match argument {
                    crate::external::ExternalArgumentValue::Text { value } => {
                        Ok(serde_json::json!({"kind": "text", "value": value}))
                    }
                    crate::external::ExternalArgumentValue::File { path } => {
                        let path = identity_value(&path.value)?;
                        Ok(serde_json::json!({"kind": "file", "path": path}))
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            let parameters = identity_value(&invocation.parameters)?;
            Ok(serde_json::json!({
                "operation": "external_video",
                "executable": executable,
                "arguments": arguments,
                "preserve_input": invocation.preserve_input,
                "input_names": invocation.inputs.keys().collect::<Vec<_>>(),
                "parameters": parameters,
            }))
        }
    }
}

fn deferred_slice_identity(
    range: &crate::model::TimelineRangeExpression,
    hashes: &[Option<String>],
) -> serde_json::Value {
    serde_json::json!({
        "operation": "deferred_slice",
        "range": timeline_range_identity(range, hashes),
    })
}

fn timeline_range_identity(
    range: &crate::model::TimelineRangeExpression,
    hashes: &[Option<String>],
) -> serde_json::Value {
    serde_json::json!({
        "start": timeline_expression_identity(&range.start, hashes),
        "end": timeline_expression_identity(&range.end, hashes),
    })
}

fn timeline_expression_identity(
    expression: &crate::model::TimelineExpression,
    hashes: &[Option<String>],
) -> serde_json::Value {
    serde_json::json!({
        "constant": expression.constant_part(),
        "terms": expression
            .terms()
            .iter()
            .map(|term| serde_json::json!({
                "value": node_hash(term.value, hashes),
                "coefficient": term.coefficient,
            }))
            .collect::<Vec<_>>(),
    })
}

fn identity_value(value: &impl Serialize) -> Result<serde_json::Value> {
    serde_json::to_value(value).map_err(|error| fingerprint_error(&error))
}

fn fingerprint_error(error: &serde_json::Error) -> Diagnostic {
    Diagnostic::new(
        "E_FINGERPRINT",
        format!("could not serialize semantic identity: {error}"),
        SourceSpan::file_start("<fingerprint>"),
    )
}

fn node_hash(value: ValueRef, hashes: &[Option<String>]) -> &str {
    hashes[value.id().get() as usize]
        .as_deref()
        .expect("semantic dependency precedes its consumer")
}

pub(crate) fn hash_serializable(value: &impl Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| fingerprint_error(&error))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use super::*;
    use crate::compiler::evaluate::{Evaluation, Symbol};
    use crate::model::{FrameCount, ImageFit, ValueId, ValueType, VideoDomain, VideoSpec};
    use crate::semantic::{GraphBuilder, SourceOrigin, SymbolId};
    use crate::source::SourceSpan;

    #[test]
    fn reference_hash_is_exactly_its_target_hash() {
        let target = ValueRef::new(ValueId::new(0), ValueType::Video);
        let reference = ValueRef::new(ValueId::new(1), ValueType::Video);
        let span = SourceSpan::file_start("workflow.clipasm");
        let domain = VideoDomain::new(FrameCount(1), VideoSpec::default());
        let source_symbol = SymbolId::new(0);
        let symbols = vec![Symbol {
            name: "source".to_owned(),
            declared_at: span.clone(),
            value: Some(target),
            timeline_view: None,
            value_type: ValueType::Video,
        }];
        let mut nodes = Vec::new();
        GraphBuilder::for_program(
            &mut nodes,
            &VideoSpec::default(),
            crate::model::AudioSpec::default(),
            7,
            SourceOrigin::new("source", span.clone()),
        )
        .image_video("source.png".into(), FrameCount(1), ImageFit::Cover)
        .expect("source");
        GraphBuilder::for_program(
            &mut nodes,
            &VideoSpec::default(),
            crate::model::AudioSpec::default(),
            1,
            SourceOrigin::new("reference", span),
        )
        .reference(source_symbol, ValueType::Video)
        .expect("reference");
        let evaluation = Evaluation {
            nodes,
            symbols,
            public_symbols: BTreeMap::new(),
            surface: Vec::new(),
            outputs: vec![reference],
        };
        let domains = vec![Some(domain), Some(domain)];
        let hashes =
            value_hashes(&evaluation, &domains, &[target, reference]).expect("value hashes");
        let target_hash = hashes[target.id().get() as usize].as_ref().expect("target");
        let reference_hash = hashes[reference.id().get() as usize]
            .as_ref()
            .expect("reference");
        assert_eq!(target_hash, reference_hash);
    }

    #[test]
    fn hashes_a_deep_reference_chain_iteratively() {
        const ALIASES: usize = 20_001;
        let span = SourceSpan::file_start("workflow.clipasm");
        let video = VideoSpec::default();
        let domain = VideoDomain::new(FrameCount(1), video);
        let mut nodes = Vec::with_capacity(ALIASES + 1);
        let mut symbols = Vec::with_capacity(ALIASES);
        let mut builder = GraphBuilder::for_program(
            &mut nodes,
            &video,
            crate::model::AudioSpec::default(),
            1,
            SourceOrigin::new("test", span.clone()),
        );
        let source = builder
            .image_video("source.png".into(), FrameCount(1), ImageFit::Cover)
            .expect("source");
        let mut root = source;
        let mut order = vec![source];
        for index in 0..ALIASES {
            let symbol = SymbolId::new(u32::try_from(index).expect("test symbol ID"));
            let name = format!("alias_{index:05}");
            debug_assert_eq!(symbol.index(), symbols.len());
            symbols.push(Symbol {
                name,
                declared_at: span.clone(),
                value: Some(root),
                timeline_view: None,
                value_type: ValueType::Video,
            });
            root = builder
                .reference(symbol, ValueType::Video)
                .expect("reference");
            order.push(root);
        }
        let evaluation = Evaluation {
            nodes,
            symbols,
            public_symbols: BTreeMap::new(),
            surface: Vec::new(),
            outputs: vec![root],
        };
        let domains = vec![Some(domain); ALIASES + 1];

        let hashes = value_hashes(&evaluation, &domains, &order).expect("hashes");

        assert_eq!(
            hashes[source.id().get() as usize],
            hashes[root.id().get() as usize]
        );
    }

    #[test]
    fn repeat_fingerprints_include_the_count() {
        fn repeat_hash(count: u64) -> String {
            let span = SourceSpan::file_start("workflow.clipasm");
            let video = VideoSpec::default();
            let mut nodes = Vec::new();
            let mut builder = GraphBuilder::for_program(
                &mut nodes,
                &video,
                crate::model::AudioSpec::default(),
                2,
                SourceOrigin::new("repeat", span),
            );
            let source = builder
                .image_video("source.png".into(), FrameCount(5), ImageFit::Cover)
                .expect("source");
            let root = builder
                .repeat(source, NonZeroU64::new(count).expect("nonzero"))
                .expect("repeat");
            let evaluation = Evaluation {
                nodes,
                symbols: Vec::new(),
                public_symbols: BTreeMap::new(),
                surface: Vec::new(),
                outputs: vec![root],
            };
            let domains = vec![
                Some(VideoDomain::new(FrameCount(5), video)),
                Some(VideoDomain::new(FrameCount(5 * count), video)),
            ];
            value_hashes(&evaluation, &domains, &[source, root]).expect("hashes")
                [root.id().get() as usize]
                .clone()
                .expect("root hash")
        }

        assert_ne!(repeat_hash(2), repeat_hash(3));
    }
}
