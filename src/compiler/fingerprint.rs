use std::collections::BTreeMap;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::compiler::evaluate::Evaluation;
use crate::diagnostic::{Diagnostic, Result};
use crate::model::{AudioSpec, ValueRef, VideoDomain, VideoSpec};
use crate::semantic::SemanticNodeKind;
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
            let value = evaluation.symbols[key]
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
        if let SemanticNodeKind::Reference { symbol } = node.kind() {
            let target = evaluation.symbols[symbol]
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
        let operation = operation_identity(node.kind());
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
    match kind {
        SemanticNodeKind::ImageVideo { .. }
        | SemanticNodeKind::VideoSource { .. }
        | SemanticNodeKind::AudioSource { .. } => Vec::new(),
        SemanticNodeKind::Reference { .. } => unreachable!("references are handled separately"),
        SemanticNodeKind::Repeat { input, .. }
        | SemanticNodeKind::AudioRepeat { input, .. }
        | SemanticNodeKind::AudioSlice { input, .. }
        | SemanticNodeKind::Zoom { input, .. }
        | SemanticNodeKind::Wobble { input, .. }
        | SemanticNodeKind::Slice { input, .. }
        | SemanticNodeKind::ExtractAudio { video: input }
        | SemanticNodeKind::AudioOnBlack { audio: input } => {
            vec![node_hash(*input, hashes).to_owned()]
        }
        SemanticNodeKind::Concat { inputs } | SemanticNodeKind::AudioConcat { inputs } => inputs
            .iter()
            .map(|input| node_hash(*input, hashes).to_owned())
            .collect(),
        SemanticNodeKind::FlashJoin { before, after, .. } => vec![
            node_hash(*before, hashes).to_owned(),
            node_hash(*after, hashes).to_owned(),
        ],
        SemanticNodeKind::ReplaceRange {
            base, replacement, ..
        } => vec![
            node_hash(*base, hashes).to_owned(),
            node_hash(*replacement, hashes).to_owned(),
        ],
        SemanticNodeKind::SetAudio { audio, video } => vec![
            node_hash(*audio, hashes).to_owned(),
            node_hash(*video, hashes).to_owned(),
        ],
        SemanticNodeKind::ExternalVideo { invocation } => invocation
            .inputs
            .values()
            .map(|input| node_hash(*input, hashes).to_owned())
            .collect(),
    }
}

fn operation_identity(kind: &SemanticNodeKind) -> serde_json::Value {
    match kind {
        SemanticNodeKind::ImageVideo { path, frames, fit } => serde_json::json!({
            "operation": "image_video", "path": path, "frames": frames, "fit": fit,
        }),
        SemanticNodeKind::VideoSource { path, fit } => serde_json::json!({
            "operation": "video_source", "path": path, "fit": fit,
        }),
        SemanticNodeKind::AudioSource { path } => {
            serde_json::json!({"operation": "audio_source", "path": path})
        }
        SemanticNodeKind::Reference { .. } => unreachable!("references are handled separately"),
        SemanticNodeKind::Repeat { count, .. } => {
            serde_json::json!({"operation": "repeat", "count": count})
        }
        SemanticNodeKind::AudioRepeat { count, .. } => {
            serde_json::json!({"operation": "audio_repeat", "count": count})
        }
        SemanticNodeKind::Zoom { percent, .. } => {
            serde_json::json!({"operation": "zoom", "percent": percent})
        }
        SemanticNodeKind::Wobble { pixels, .. } => {
            serde_json::json!({"operation": "wobble", "pixels": pixels})
        }
        SemanticNodeKind::FlashJoin { frames, .. } => {
            serde_json::json!({"operation": "flash_join", "frames": frames})
        }
        SemanticNodeKind::Concat { .. } => serde_json::json!({"operation": "concat"}),
        SemanticNodeKind::AudioConcat { .. } => serde_json::json!({"operation": "audio_concat"}),
        SemanticNodeKind::Slice { range, .. } => {
            serde_json::json!({"operation": "slice", "range": range})
        }
        SemanticNodeKind::AudioSlice { range, .. } => {
            serde_json::json!({"operation": "audio_slice", "range": range})
        }
        SemanticNodeKind::ReplaceRange { range, .. } => {
            serde_json::json!({"operation": "replace_range", "range": range})
        }
        SemanticNodeKind::ExtractAudio { .. } => {
            serde_json::json!({"operation": "extract_audio"})
        }
        SemanticNodeKind::SetAudio { .. } => serde_json::json!({"operation": "set_audio"}),
        SemanticNodeKind::AudioOnBlack { .. } => {
            serde_json::json!({"operation": "audio_on_black"})
        }
        SemanticNodeKind::ExternalVideo { invocation } => serde_json::json!({
            "operation": "external_video",
            "command": invocation.command.value,
            "preserve_input": invocation.preserve_input,
            "input_names": invocation.inputs.keys().collect::<Vec<_>>(),
            "parameters": invocation.parameters,
        }),
    }
}

fn node_hash(value: ValueRef, hashes: &[Option<String>]) -> &str {
    hashes[value.id().get() as usize]
        .as_deref()
        .expect("semantic dependency precedes its consumer")
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
    use std::num::NonZeroU64;

    use super::*;
    use crate::compiler::evaluate::{DeclaredValueType, Evaluation, Symbol};
    use crate::model::{FrameCount, ImageFit, ValueId, ValueType, VideoDomain, VideoSpec};
    use crate::semantic::{GraphBuilder, SourceOrigin, SymbolId};
    use crate::source::SourceSpan;

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
        let source_symbol = SymbolId::new(0);
        symbols.insert(
            source_symbol,
            Symbol {
                name: "source".to_owned(),
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
            SourceOrigin::new("source", span.clone()),
        )
        .image_video("source.png".into(), FrameCount(1), ImageFit::Cover)
        .expect("source");
        GraphBuilder::for_program(
            &mut nodes,
            &VideoSpec::default(),
            1,
            SourceOrigin::new("reference", span),
        )
        .reference(source_symbol, ValueType::Video)
        .expect("reference");
        let evaluation = Evaluation {
            nodes,
            symbols,
            symbol_order: vec![source_symbol],
            public_symbols: BTreeMap::new(),
            surface: Vec::new(),
            outputs: vec![reference],
        };
        let domains = vec![Some(domain.clone()), Some(domain)];
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
        let span = SourceSpan::file_start("workflow.yaml");
        let video = VideoSpec::default();
        let domain = VideoDomain {
            frames: FrameCount(1),
            width: video.width,
            height: video.height,
            frame_rate: video.fps,
        };
        let mut nodes = Vec::with_capacity(ALIASES + 1);
        let mut symbols = BTreeMap::new();
        let mut symbol_order = Vec::with_capacity(ALIASES);
        let mut builder = GraphBuilder::for_program(
            &mut nodes,
            &video,
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
            symbols.insert(
                symbol,
                Symbol {
                    name,
                    declared_at: span.clone(),
                    value: Some(root),
                    declared_type: DeclaredValueType::Known(ValueType::Video),
                    value_type: Some(ValueType::Video),
                },
            );
            symbol_order.push(symbol);
            root = builder
                .reference(symbol, ValueType::Video)
                .expect("reference");
            order.push(root);
        }
        let evaluation = Evaluation {
            nodes,
            symbols,
            symbol_order,
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
            let span = SourceSpan::file_start("workflow.yaml");
            let video = VideoSpec::default();
            let mut nodes = Vec::new();
            let mut builder =
                GraphBuilder::for_program(&mut nodes, &video, 2, SourceOrigin::new("repeat", span));
            let source = builder
                .image_video("source.png".into(), FrameCount(5), ImageFit::Cover)
                .expect("source");
            let root = builder
                .repeat(source, NonZeroU64::new(count).expect("nonzero"))
                .expect("repeat");
            let evaluation = Evaluation {
                nodes,
                symbols: BTreeMap::new(),
                symbol_order: Vec::new(),
                public_symbols: BTreeMap::new(),
                surface: Vec::new(),
                outputs: vec![root],
            };
            let domains = vec![
                Some(VideoDomain {
                    frames: FrameCount(5),
                    width: video.width,
                    height: video.height,
                    frame_rate: video.fps,
                }),
                Some(VideoDomain {
                    frames: FrameCount(5 * count),
                    width: video.width,
                    height: video.height,
                    frame_rate: video.fps,
                }),
            ];
            value_hashes(&evaluation, &domains, &[source, root]).expect("hashes")
                [root.id().get() as usize]
                .clone()
                .expect("root hash")
        }

        assert_ne!(repeat_hash(2), repeat_hash(3));
    }
}
