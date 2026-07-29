use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::path::Path;

use serde::Serialize;

use crate::compiler::evaluate::Evaluation;
use crate::diagnostic::Result;
use crate::external::{ExternalArgumentValue, ExternalInvocation, ExternalParameterValue};
use crate::model::{
    AudioSpec, ExactNumber, FrameCount, FrameRange, ImageFit, NativeRange, SampleRange,
    TimelineExpression, TimelineRangeExpression, ValueRef, ValueType, VideoDomain, VideoSpec,
};
use crate::semantic::{SemanticDependency, SemanticNodeKind};

const COMPILED_IDENTITY_REVISION: u32 = 22;

#[derive(Serialize)]
struct CompiledIdentity<'a> {
    identity_revision: u32,
    video: &'a VideoSpec,
    audio: &'a AudioSpec,
    outputs: Vec<&'a str>,
    names: &'a BTreeMap<&'a str, &'a str>,
}

#[derive(Serialize)]
struct ValueIdentity<'a> {
    semantic_version: u32,
    value_type: ValueType,
    domain: Option<&'a VideoDomain>,
    operation: SemanticOperationIdentity<'a>,
    upstream: Vec<&'a str>,
}

#[derive(Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum SemanticOperationIdentity<'a> {
    ImageVideo {
        path: &'a Path,
        frames: FrameCount,
        fit: ImageFit,
    },
    DeferredImageVideo {
        path: &'a Path,
        extent: TimelineExpressionIdentity<'a>,
        fit: ImageFit,
    },
    VideoSource {
        path: &'a Path,
        fit: ImageFit,
    },
    AudioSource {
        path: &'a Path,
    },
    Repeat {
        count: NonZeroU64,
    },
    ZoomIn {
        by: &'a ExactNumber,
    },
    FlashCut {
        frames: FrameCount,
    },
    Crossfade {
        frames: FrameCount,
    },
    Concat,
    Slice {
        unit: &'static str,
        range: NativeRangeIdentity,
    },
    DeferredSlice {
        unit: &'static str,
        range: TimelineRangeIdentity<'a>,
    },
    ReplaceRange {
        unit: &'static str,
        range: NativeRangeIdentity,
    },
    DeferredReplaceRange {
        unit: &'static str,
        range: TimelineRangeIdentity<'a>,
    },
    ExtractAudio,
    SetAudio,
    AudioOnBlack,
    ExternalVideo {
        executable: &'a Path,
        arguments: Vec<ExternalArgumentIdentity<'a>>,
        preserve_input: &'a str,
        input_names: Vec<&'a str>,
        parameters: BTreeMap<&'a str, ExternalParameterIdentity<'a>>,
    },
}

#[derive(Serialize)]
#[serde(untagged)]
enum NativeRangeIdentity {
    Frames(FrameRange),
    Samples(SampleRange),
}

#[derive(Serialize)]
struct TimelineRangeIdentity<'a> {
    start: TimelineExpressionIdentity<'a>,
    end: TimelineExpressionIdentity<'a>,
}

#[derive(Serialize)]
struct TimelineExpressionIdentity<'a> {
    constant: &'a ExactNumber,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_frames: Option<&'a ExactNumber>,
    terms: Vec<TimelineTermIdentity<'a>>,
}

#[derive(Serialize)]
struct TimelineTermIdentity<'a> {
    value: &'a str,
    coefficient: &'a ExactNumber,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ExternalArgumentIdentity<'a> {
    Text { value: &'a str },
    File { path: &'a Path },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ExternalParameterIdentity<'a> {
    Integer { value: i64 },
    Keyword { value: &'a str },
    File { path: &'a Path },
}

pub(super) fn compiled_structure_hash(
    evaluation: &Evaluation,
    domains: &[Option<VideoDomain>],
    video: &VideoSpec,
    audio: AudioSpec,
    order: &[ValueRef],
) -> Result<String> {
    let hashes = value_hashes(evaluation, domains, order)?;
    let outputs = evaluation
        .outputs
        .iter()
        .map(|output| node_hash(*output, &hashes))
        .collect::<Vec<_>>();
    let names = evaluation
        .public_symbols
        .iter()
        .map(|(name, key)| {
            let value = evaluation.symbols[key.index()]
                .value
                .expect("every collected symbol is evaluated");
            (name.as_str(), node_hash(value, &hashes))
        })
        .collect::<BTreeMap<_, _>>();

    crate::identity::hash_serializable(&CompiledIdentity {
        identity_revision: COMPILED_IDENTITY_REVISION,
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
        if let SemanticNodeKind::Reference {
            symbol,
            value_type: _,
        } = node.kind()
        {
            let target = evaluation.symbols[symbol.index()]
                .value
                .expect("references are resolved before fingerprinting");
            hashes[index] = Some(node_hash(target, &hashes).to_owned());
            continue;
        }
        let upstream = upstream_hashes(node.kind(), &hashes);
        let operation = operation_identity(node.kind(), &hashes);
        hashes[index] = Some(crate::identity::hash_serializable(&ValueIdentity {
            semantic_version: node.semantic_version(),
            value_type: node.value_type(),
            domain: domains[index].as_ref(),
            operation,
            upstream,
        })?);
    }
    Ok(hashes)
}

fn upstream_hashes<'a>(kind: &SemanticNodeKind, hashes: &'a [Option<String>]) -> Vec<&'a str> {
    let mut upstream = Vec::new();
    kind.visit_dependencies(|dependency| match dependency {
        SemanticDependency::Value(value) => upstream.push(node_hash(value, hashes)),
        SemanticDependency::Symbol(_) => {
            unreachable!("references are handled before upstream hashing")
        }
    });
    upstream
}

fn operation_identity<'a>(
    kind: &'a SemanticNodeKind,
    hashes: &'a [Option<String>],
) -> SemanticOperationIdentity<'a> {
    match kind {
        SemanticNodeKind::ImageVideo { path, frames, fit } => {
            SemanticOperationIdentity::ImageVideo {
                path,
                frames: *frames,
                fit: *fit,
            }
        }
        SemanticNodeKind::DeferredImageVideo { path, extent, fit } => {
            SemanticOperationIdentity::DeferredImageVideo {
                path,
                extent: timeline_expression_identity(extent, hashes),
                fit: *fit,
            }
        }
        SemanticNodeKind::VideoSource { path, fit } => {
            SemanticOperationIdentity::VideoSource { path, fit: *fit }
        }
        SemanticNodeKind::AudioSource { path } => SemanticOperationIdentity::AudioSource { path },
        SemanticNodeKind::Reference { .. } => unreachable!("references are handled separately"),
        SemanticNodeKind::Repeat { input: _, count } => {
            SemanticOperationIdentity::Repeat { count: *count }
        }
        SemanticNodeKind::ZoomIn { input: _, by } => SemanticOperationIdentity::ZoomIn { by },
        SemanticNodeKind::FlashCut {
            before: _,
            after: _,
            frames,
        } => SemanticOperationIdentity::FlashCut { frames: *frames },
        SemanticNodeKind::Crossfade {
            before: _,
            after: _,
            frames,
        } => SemanticOperationIdentity::Crossfade { frames: *frames },
        SemanticNodeKind::Concat { inputs: _ } => SemanticOperationIdentity::Concat,
        SemanticNodeKind::Slice { input: _, range } => {
            let (unit, range) = native_range_identity(*range);
            SemanticOperationIdentity::Slice { unit, range }
        }
        SemanticNodeKind::DeferredSlice { input, range } => {
            SemanticOperationIdentity::DeferredSlice {
                unit: input.value_type().native_unit_name(),
                range: timeline_range_identity(range, hashes),
            }
        }
        SemanticNodeKind::ReplaceRange {
            base: _,
            replacement: _,
            range,
        } => {
            let (unit, range) = native_range_identity(*range);
            SemanticOperationIdentity::ReplaceRange { unit, range }
        }
        SemanticNodeKind::DeferredReplaceRange {
            base,
            replacement: _,
            range,
        } => SemanticOperationIdentity::DeferredReplaceRange {
            unit: base.value_type().native_unit_name(),
            range: timeline_range_identity(range, hashes),
        },
        SemanticNodeKind::ExtractAudio { video: _ } => SemanticOperationIdentity::ExtractAudio,
        SemanticNodeKind::SetAudio { audio: _, video: _ } => SemanticOperationIdentity::SetAudio,
        SemanticNodeKind::AudioOnBlack { audio: _ } => SemanticOperationIdentity::AudioOnBlack,
        SemanticNodeKind::ExternalVideo { invocation } => external_video_identity(invocation),
    }
}

fn external_video_identity(invocation: &ExternalInvocation) -> SemanticOperationIdentity<'_> {
    SemanticOperationIdentity::ExternalVideo {
        executable: &invocation.executable.value,
        arguments: invocation
            .arguments
            .iter()
            .map(|argument| match argument {
                ExternalArgumentValue::Text { value } => ExternalArgumentIdentity::Text { value },
                ExternalArgumentValue::File { path } => {
                    ExternalArgumentIdentity::File { path: &path.value }
                }
            })
            .collect(),
        preserve_input: &invocation.preserve_input,
        input_names: invocation.inputs.keys().map(String::as_str).collect(),
        parameters: invocation
            .parameters
            .iter()
            .map(|(name, value)| {
                let value = match value {
                    ExternalParameterValue::Integer(value) => {
                        ExternalParameterIdentity::Integer { value: *value }
                    }
                    ExternalParameterValue::Keyword(value) => {
                        ExternalParameterIdentity::Keyword { value }
                    }
                    ExternalParameterValue::File(path) => {
                        ExternalParameterIdentity::File { path: &path.value }
                    }
                };
                (name.as_str(), value)
            })
            .collect(),
    }
}

fn native_range_identity(range: NativeRange) -> (&'static str, NativeRangeIdentity) {
    match range {
        NativeRange::Frames(range) => ("frames", NativeRangeIdentity::Frames(range)),
        NativeRange::Samples(range) => ("samples", NativeRangeIdentity::Samples(range)),
    }
}

fn timeline_range_identity<'a>(
    range: &'a TimelineRangeExpression,
    hashes: &'a [Option<String>],
) -> TimelineRangeIdentity<'a> {
    TimelineRangeIdentity {
        start: timeline_expression_identity(&range.start, hashes),
        end: timeline_expression_identity(&range.end, hashes),
    }
}

fn timeline_expression_identity<'a>(
    expression: &'a TimelineExpression,
    hashes: &'a [Option<String>],
) -> TimelineExpressionIdentity<'a> {
    TimelineExpressionIdentity {
        constant: expression.constant_part(),
        project_frames: (!expression.project_frame_part().is_zero())
            .then(|| expression.project_frame_part()),
        terms: expression
            .terms()
            .iter()
            .map(|term| TimelineTermIdentity {
                value: node_hash(term.value, hashes),
                coefficient: &term.coefficient,
            })
            .collect(),
    }
}

fn node_hash(value: ValueRef, hashes: &[Option<String>]) -> &str {
    hashes[value.id().get() as usize]
        .as_deref()
        .expect("semantic dependency precedes its consumer")
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
