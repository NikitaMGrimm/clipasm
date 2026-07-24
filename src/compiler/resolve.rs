use std::collections::{BTreeMap, BTreeSet};

use crate::compiler::evaluate::Evaluation;
use crate::compiler::{CompiledProgram, ExplainEntry, ExplainOutput};
use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, ValueId, ValueRef, ValueType, VideoDomain, VideoSpec};
use crate::semantic::{CompiledNode, DraftNode, SemanticNodeKind, SymbolId};
use crate::source::{SourceUnit, Spanned};
use std::path::PathBuf;

struct SymbolFrame {
    symbol: SymbolId,
    next_target: usize,
}

pub(super) fn finalize(
    entrypoint: &SourceUnit,
    output: Option<Spanned<PathBuf>>,
    video: VideoSpec,
    evaluation: Evaluation,
    format_version: u32,
) -> Result<CompiledProgram> {
    validate_references(&evaluation)?;
    detect_cycles(&evaluation)?;
    let symbol_values = evaluation
        .symbols
        .iter()
        .map(|(id, symbol)| {
            (
                *id,
                symbol.value.expect("every collected symbol is evaluated"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let roots = evaluation
        .public_symbols
        .values()
        .map(|symbol| symbol_values[symbol])
        .chain(evaluation.outputs.iter().copied());
    let order = super::traversal::topological_order(&evaluation.nodes, &symbol_values, roots)?;
    let domains = infer_domains(&evaluation, &video, &order)?;
    let structure_hash = super::fingerprint::compiled_structure_hash(
        &evaluation,
        &domains,
        &video,
        format_version,
        &order,
    )?;

    let nodes = evaluation
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            CompiledNode::from_draft(
                ValueId::new(u32::try_from(index).expect("draft node IDs already fit in u32")),
                node,
                domains[index].clone(),
            )
        })
        .collect();
    let named_values = evaluation
        .public_symbols
        .iter()
        .map(|(name, key)| {
            (
                name.clone(),
                evaluation.symbols[key]
                    .value
                    .expect("every collected symbol is evaluated"),
            )
        })
        .collect();
    let explain = evaluation
        .surface
        .into_iter()
        .map(|record| ExplainEntry {
            construct: record.construct,
            outputs: record
                .outputs
                .into_iter()
                .map(|output| ExplainOutput {
                    value: output.value,
                    id: output.id,
                })
                .collect(),
            span: record.span,
        })
        .collect::<Vec<_>>();
    Ok(CompiledProgram {
        format_version,
        engine_version: env!("CARGO_PKG_VERSION").to_owned(),
        structure_hash,
        video,
        nodes,
        outputs: evaluation.outputs,
        named_values,
        symbol_values,
        explain,
        output,
        entrypoint_source: entrypoint.source().clone(),
    })
}

fn validate_references(evaluation: &Evaluation) -> Result<()> {
    for node in &evaluation.nodes {
        if let SemanticNodeKind::Reference { symbol } = node.kind() {
            let Some(binding) = evaluation.symbols.get(symbol) else {
                return Err(Diagnostic::new(
                    "E_MISSING_REFERENCE",
                    format!("reference names unknown symbol {}", symbol.index()),
                    node.origin().span.clone(),
                ));
            };
            if binding.value.is_none() {
                return Err(Diagnostic::new(
                    "E_MISSING_REFERENCE",
                    format!("name `{}` has no compiled value", binding.name),
                    node.origin().span.clone(),
                ));
            }
            let symbol_type = binding
                .value_type
                .expect("symbol types are resolved before evaluation");
            if symbol_type != node.value_type() {
                return Err(Diagnostic::new(
                    "E_TYPE_MISMATCH",
                    format!(
                        "reference `${}` has type {}, but its expression was recorded as {}",
                        binding.name,
                        symbol_type,
                        node.value_type()
                    ),
                    node.origin().span.clone(),
                ));
            }
        }
    }
    Ok(())
}

fn detect_cycles(evaluation: &Evaluation) -> Result<()> {
    let mut edges = BTreeMap::<SymbolId, Vec<SymbolId>>::new();
    for symbol in &evaluation.symbol_order {
        let value = evaluation.symbols[symbol]
            .value
            .expect("every collected symbol is evaluated");
        let mut references = BTreeSet::new();
        collect_direct_references(value, &evaluation.nodes, &mut references);
        edges.insert(*symbol, references.into_iter().collect());
    }
    let mut states = BTreeMap::<SymbolId, u8>::new();
    let mut path = Vec::<SymbolId>::new();
    let mut positions = BTreeMap::<SymbolId, usize>::new();
    let mut stack = Vec::<SymbolFrame>::new();

    for root in &evaluation.symbol_order {
        if states.get(root).copied().unwrap_or(0) != 0 {
            continue;
        }
        states.insert(*root, 1);
        positions.insert(*root, 0);
        path.push(*root);
        stack.push(SymbolFrame {
            symbol: *root,
            next_target: 0,
        });

        while let Some(frame) = stack.last_mut() {
            if let Some(target) = edges[&frame.symbol].get(frame.next_target).copied() {
                frame.next_target += 1;
                match states.get(&target).copied().unwrap_or(0) {
                    0 => {
                        states.insert(target, 1);
                        positions.insert(target, path.len());
                        path.push(target);
                        stack.push(SymbolFrame {
                            symbol: target,
                            next_target: 0,
                        });
                    }
                    1 => {
                        let start = positions[&target];
                        let mut cycle = path[start..]
                            .iter()
                            .map(|symbol| evaluation.symbols[symbol].name.clone())
                            .collect::<Vec<_>>();
                        cycle.push(evaluation.symbols[&target].name.clone());
                        return Err(Diagnostic::new(
                            "E_DEPENDENCY_CYCLE",
                            format!("named-value dependency cycle: {}", cycle.join(" -> ")),
                            evaluation.symbols[&target].declared_at.clone(),
                        ));
                    }
                    2 => {}
                    _ => unreachable!("cycle state is closed"),
                }
            } else {
                let frame = stack.pop().expect("active cycle frame");
                let popped = path.pop().expect("active cycle path");
                debug_assert_eq!(popped, frame.symbol);
                positions.remove(&frame.symbol);
                states.insert(frame.symbol, 2);
            }
        }
    }
    Ok(())
}

fn collect_direct_references(
    value: ValueRef,
    nodes: &[DraftNode],
    output: &mut BTreeSet<SymbolId>,
) {
    let mut visited = vec![false; nodes.len()];
    let mut stack = vec![value];
    while let Some(value) = stack.pop() {
        let index = value.id().get() as usize;
        if visited[index] {
            continue;
        }
        visited[index] = true;
        match nodes[index].kind() {
            SemanticNodeKind::ImageVideo { .. } | SemanticNodeKind::VideoSource { .. } => {}
            SemanticNodeKind::Reference { symbol } => {
                output.insert(*symbol);
            }
            SemanticNodeKind::Repeat { input, .. }
            | SemanticNodeKind::Zoom { input, .. }
            | SemanticNodeKind::Wobble { input, .. }
            | SemanticNodeKind::Slice { input, .. } => {
                stack.push(*input);
            }
            SemanticNodeKind::Concat { inputs } => stack.extend(inputs.iter().copied()),
            SemanticNodeKind::FlashJoin { before, after, .. } => {
                stack.push(*after);
                stack.push(*before);
            }
            SemanticNodeKind::ReplaceRange {
                base, replacement, ..
            } => {
                stack.push(*replacement);
                stack.push(*base);
            }
        }
    }
}

#[derive(Clone)]
enum DomainKnowledge {
    NotVideo,
    Deferred,
    Known(VideoDomain),
}

fn infer_domains(
    evaluation: &Evaluation,
    video: &VideoSpec,
    order: &[ValueRef],
) -> Result<Vec<Option<VideoDomain>>> {
    let mut knowledge = vec![DomainKnowledge::NotVideo; evaluation.nodes.len()];
    for value in order {
        let index = value.id().get() as usize;
        if value.value_type() != ValueType::Video {
            continue;
        }
        let node = &evaluation.nodes[index];
        knowledge[index] = match node.kind() {
            SemanticNodeKind::ImageVideo { frames, .. } => {
                DomainKnowledge::Known(project_domain(video, *frames))
            }
            SemanticNodeKind::VideoSource { .. } => DomainKnowledge::Deferred,
            SemanticNodeKind::Reference { symbol } => {
                let target = evaluation.symbols[symbol]
                    .value
                    .expect("references were resolved before domain inference");
                knowledge[target.id().get() as usize].clone()
            }
            SemanticNodeKind::Repeat { input, count } => {
                match &knowledge[input.id().get() as usize] {
                    DomainKnowledge::Known(domain) => DomainKnowledge::Known(project_domain(
                        video,
                        domain
                            .frames
                            .checked_mul(count.get(), &node.origin().span)?,
                    )),
                    DomainKnowledge::Deferred => DomainKnowledge::Deferred,
                    DomainKnowledge::NotVideo => DomainKnowledge::NotVideo,
                }
            }
            SemanticNodeKind::Zoom { input, .. } | SemanticNodeKind::Wobble { input, .. } => {
                knowledge[input.id().get() as usize].clone()
            }
            SemanticNodeKind::FlashJoin {
                before,
                after,
                frames,
            } => infer_flash_domain(
                &knowledge[before.id().get() as usize],
                &knowledge[after.id().get() as usize],
                *frames,
                video,
                &node.origin().span,
            )?,
            SemanticNodeKind::Concat { inputs } => {
                infer_concat_domain(inputs, &knowledge, video, &node.origin().span)?
            }
            SemanticNodeKind::Slice { input, range } => {
                if let DomainKnowledge::Known(input_domain) = &knowledge[input.id().get() as usize]
                {
                    validate_range(*range, input_domain.frames, &node.origin().span)?;
                }
                DomainKnowledge::Known(project_domain(video, range.frames()))
            }
            SemanticNodeKind::ReplaceRange {
                base,
                replacement,
                range,
            } => {
                let base_domain = &knowledge[base.id().get() as usize];
                if let DomainKnowledge::Known(base_domain) = base_domain {
                    validate_range(*range, base_domain.frames, &node.origin().span)?;
                }
                let replacement_domain = &knowledge[replacement.id().get() as usize];
                match (base_domain, replacement_domain) {
                    (
                        DomainKnowledge::Known(base_domain),
                        DomainKnowledge::Known(replacement_domain),
                    ) => DomainKnowledge::Known(project_domain(
                        video,
                        FrameCount(base_domain.frames.0 - range.frames().0)
                            .checked_add(replacement_domain.frames, &node.origin().span)?,
                    )),
                    (DomainKnowledge::NotVideo, _) | (_, DomainKnowledge::NotVideo) => {
                        unreachable!("replace-range inputs are typed Video")
                    }
                    _ => DomainKnowledge::Deferred,
                }
            }
        };
    }

    Ok(knowledge
        .into_iter()
        .map(|knowledge| match knowledge {
            DomainKnowledge::Known(domain) => Some(domain),
            DomainKnowledge::NotVideo | DomainKnowledge::Deferred => None,
        })
        .collect())
}

fn project_domain(video: &VideoSpec, frames: FrameCount) -> VideoDomain {
    VideoDomain {
        frames,
        width: video.width,
        height: video.height,
        frame_rate: video.fps,
    }
}

fn validate_range(
    range: crate::model::FrameRange,
    input: FrameCount,
    span: &crate::source::SourceSpan,
) -> Result<()> {
    if range.end() > input.0 {
        return Err(Diagnostic::new(
            "E_INVALID_TIME_RANGE",
            format!(
                "frame range {}..{} is outside the base Video domain of {} frames",
                range.start(),
                range.end(),
                input.0
            ),
            span.clone(),
        ));
    }
    Ok(())
}

fn validate_flash_frames(
    frames: FrameCount,
    after: FrameCount,
    span: &crate::source::SourceSpan,
) -> Result<()> {
    if frames > after {
        return Err(Diagnostic::new(
            "E_INVALID_FLASH_FRAMES",
            format!(
                "`flash.frames` is {} frames, but `after` contains only {} frames",
                frames.0, after.0
            ),
            span.clone(),
        ));
    }
    Ok(())
}

fn infer_flash_domain(
    before: &DomainKnowledge,
    after: &DomainKnowledge,
    frames: FrameCount,
    video: &VideoSpec,
    span: &crate::source::SourceSpan,
) -> Result<DomainKnowledge> {
    if let DomainKnowledge::Known(after) = after {
        validate_flash_frames(frames, after.frames, span)?;
    }
    Ok(match (before, after) {
        (DomainKnowledge::Known(before), DomainKnowledge::Known(after)) => DomainKnowledge::Known(
            project_domain(video, before.frames.checked_add(after.frames, span)?),
        ),
        (DomainKnowledge::NotVideo, _) | (_, DomainKnowledge::NotVideo) => {
            unreachable!("flash inputs are typed Video")
        }
        _ => DomainKnowledge::Deferred,
    })
}

fn infer_concat_domain(
    inputs: &[ValueRef],
    knowledge: &[DomainKnowledge],
    video: &VideoSpec,
    span: &crate::source::SourceSpan,
) -> Result<DomainKnowledge> {
    if inputs.iter().any(|input| {
        matches!(
            knowledge[input.id().get() as usize],
            DomainKnowledge::Deferred
        )
    }) {
        return Ok(DomainKnowledge::Deferred);
    }
    let mut total = FrameCount(0);
    for input in inputs {
        match &knowledge[input.id().get() as usize] {
            DomainKnowledge::Known(domain) => {
                total = total.checked_add(domain.frames, span)?;
            }
            DomainKnowledge::Deferred => unreachable!("deferred concat handled before summing"),
            DomainKnowledge::NotVideo => unreachable!("concat inputs are typed Video"),
        }
    }
    Ok(DomainKnowledge::Known(project_domain(video, total)))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use super::*;
    use crate::compiler::evaluate::{DeclaredValueType, SurfaceRecord, Symbol};
    use crate::model::ImageFit;
    use crate::semantic::{GraphBuilder, SourceOrigin};
    use crate::source::SourceSpan;

    fn origin() -> SourceOrigin {
        SourceOrigin::new("test", SourceSpan::file_start("test.yaml"))
    }

    fn symbol(name: String, value: ValueRef) -> Symbol {
        Symbol {
            name,
            declared_at: SourceSpan::file_start("test.yaml"),
            value: Some(value),
            declared_type: DeclaredValueType::Known(ValueType::Video),
            value_type: Some(ValueType::Video),
        }
    }

    fn make_evaluation(
        nodes: Vec<DraftNode>,
        symbols: BTreeMap<SymbolId, Symbol>,
        symbol_order: Vec<SymbolId>,
        root: ValueRef,
    ) -> Evaluation {
        Evaluation {
            nodes,
            symbols,
            symbol_order,
            public_symbols: BTreeMap::new(),
            surface: Vec::<SurfaceRecord>::new(),
            outputs: vec![root],
        }
    }

    #[test]
    fn detects_a_deep_named_cycle_iteratively() {
        const NAMES: usize = 20_001;
        let video = VideoSpec::default();
        let mut nodes = Vec::with_capacity(NAMES);
        let mut symbols = BTreeMap::new();
        let mut symbol_order = Vec::with_capacity(NAMES);
        let mut builder = GraphBuilder::for_program(&mut nodes, &video, 1, origin());
        let mut root = None;
        for index in 0..NAMES {
            let symbol_id = SymbolId::new(u32::try_from(index).expect("test symbol ID"));
            let target =
                SymbolId::new(u32::try_from((index + 1) % NAMES).expect("test target symbol ID"));
            let name = format!("name_{index:05}");
            let value = builder
                .reference(target, ValueType::Video)
                .expect("reference");
            root.get_or_insert(value);
            symbol_order.push(symbol_id);
            symbols.insert(symbol_id, symbol(name, value));
        }
        let evaluation = make_evaluation(nodes, symbols, symbol_order, root.expect("root"));

        let error = detect_cycles(&evaluation).expect_err("named cycle");

        assert_eq!(error.code, "E_DEPENDENCY_CYCLE");
        assert!(error.message.starts_with("named-value dependency cycle:"));
        assert!(error.message.ends_with("name_00000"));
    }

    #[test]
    fn deferred_repeat_chains_are_memoized_and_remain_deferred() {
        let video = VideoSpec::default();
        let mut nodes = Vec::new();
        let mut builder = GraphBuilder::for_program(&mut nodes, &video, 2, origin());
        let mut root = builder
            .video_source("source.mp4".into(), ImageFit::Cover)
            .expect("video source");
        for _ in 0..64 {
            root = builder
                .repeat(root, NonZeroU64::new(2).expect("nonzero"))
                .expect("repeat");
        }
        let evaluation = make_evaluation(nodes, BTreeMap::new(), Vec::new(), root);
        let order =
            super::super::traversal::topological_order(&evaluation.nodes, &BTreeMap::new(), [root])
                .expect("order");

        let domains = infer_domains(&evaluation, &video, &order).expect("domains");

        assert_eq!(order.len(), 65);
        assert!(domains.into_iter().all(|domain| domain.is_none()));
    }

    #[test]
    fn repeat_domains_multiply_exactly_and_check_overflow() {
        let video = VideoSpec::default();
        let mut nodes = Vec::new();
        let mut builder = GraphBuilder::for_program(&mut nodes, &video, 2, origin());
        let source = builder
            .image_video("source.png".into(), FrameCount(5), ImageFit::Cover)
            .expect("source");
        let root = builder
            .repeat(source, NonZeroU64::new(3).expect("nonzero"))
            .expect("repeat");
        let evaluation = make_evaluation(nodes, BTreeMap::new(), Vec::new(), root);
        let order =
            super::super::traversal::topological_order(&evaluation.nodes, &BTreeMap::new(), [root])
                .expect("order");
        let domains = infer_domains(&evaluation, &video, &order).expect("domains");
        assert_eq!(
            domains[root.id().get() as usize]
                .as_ref()
                .expect("known repeat")
                .frames,
            FrameCount(15)
        );

        let mut nodes = Vec::new();
        let mut builder = GraphBuilder::for_program(&mut nodes, &video, 2, origin());
        let source = builder
            .image_video("huge.png".into(), FrameCount(u64::MAX), ImageFit::Cover)
            .expect("source");
        let root = builder
            .repeat(source, NonZeroU64::new(2).expect("nonzero"))
            .expect("repeat");
        let evaluation = make_evaluation(nodes, BTreeMap::new(), Vec::new(), root);
        let order =
            super::super::traversal::topological_order(&evaluation.nodes, &BTreeMap::new(), [root])
                .expect("order");
        let error = infer_domains(&evaluation, &video, &order).expect_err("overflow");
        assert_eq!(error.code, "E_FRAME_OVERFLOW");
    }
}
