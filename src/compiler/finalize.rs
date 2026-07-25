use std::collections::{BTreeMap, BTreeSet};

use crate::compiler::evaluate::Evaluation;
use crate::compiler::{CompiledProgram, ExplainEntry, ExplainOutput};
use crate::diagnostic::{Diagnostic, Result};
use crate::model::{AudioSpec, ValueId, ValueRef, VideoSpec};
use crate::semantic::{CompiledNode, DraftNode, SemanticDependency, SemanticNodeKind, SymbolId};
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
    audio: AudioSpec,
    evaluation: Evaluation,
    format_version: u32,
) -> Result<CompiledProgram> {
    validate_references(&evaluation)?;
    detect_cycles(&evaluation)?;
    let symbol_values = evaluation
        .symbols
        .iter()
        .map(|symbol| symbol.value.expect("every collected symbol is evaluated"))
        .collect::<Vec<_>>();
    let roots = evaluation
        .public_symbols
        .values()
        .map(|symbol| symbol_values[symbol.index()])
        .chain(evaluation.outputs.iter().copied());
    let order = super::traversal::topological_order(&evaluation.nodes, &symbol_values, roots)?;
    let domains = super::domain::infer_domains(&evaluation, &video, &order)?;
    let structure_hash = super::fingerprint::compiled_structure_hash(
        &evaluation,
        &domains,
        &video,
        audio,
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
                domains[index],
            )
        })
        .collect();
    let named_values = evaluation
        .public_symbols
        .iter()
        .map(|(name, key)| {
            (
                name.clone(),
                evaluation.symbols[key.index()]
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
        audio,
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
        if let SemanticNodeKind::Reference { symbol, .. } = node.kind() {
            let Some(binding) = evaluation.symbols.get(symbol.index()) else {
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
            let symbol_type = binding.value_type;
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
    for index in 0..evaluation.symbols.len() {
        let symbol = SymbolId::new(u32::try_from(index).expect("symbol ID"));
        let value = evaluation.symbols[symbol.index()]
            .value
            .expect("every collected symbol is evaluated");
        let mut references = BTreeSet::new();
        collect_direct_references(value, &evaluation.nodes, &mut references);
        edges.insert(symbol, references.into_iter().collect());
    }
    let mut states = BTreeMap::<SymbolId, u8>::new();
    let mut path = Vec::<SymbolId>::new();
    let mut positions = BTreeMap::<SymbolId, usize>::new();
    let mut stack = Vec::<SymbolFrame>::new();

    for index in 0..evaluation.symbols.len() {
        let root = SymbolId::new(u32::try_from(index).expect("symbol ID"));
        if states.get(&root).copied().unwrap_or(0) != 0 {
            continue;
        }
        states.insert(root, 1);
        positions.insert(root, 0);
        path.push(root);
        stack.push(SymbolFrame {
            symbol: root,
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
                            .map(|symbol| evaluation.symbols[symbol.index()].name.clone())
                            .collect::<Vec<_>>();
                        cycle.push(evaluation.symbols[target.index()].name.clone());
                        return Err(Diagnostic::new(
                            "E_DEPENDENCY_CYCLE",
                            format!("named-value dependency cycle: {}", cycle.join(" -> ")),
                            evaluation.symbols[target.index()].declared_at.clone(),
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
        nodes[index]
            .kind()
            .visit_dependencies(|dependency| match dependency {
                SemanticDependency::Value(value) => stack.push(value),
                SemanticDependency::Symbol(symbol) => {
                    output.insert(symbol);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use super::*;
    use crate::compiler::evaluate::{SurfaceRecord, Symbol};
    use crate::model::{FrameCount, ImageFit, ValueType};
    use crate::semantic::{GraphBuilder, SourceOrigin};
    use crate::source::SourceSpan;

    fn origin() -> SourceOrigin {
        SourceOrigin::new("test", SourceSpan::file_start("test.clipasm"))
    }

    fn symbol(name: String, value: ValueRef) -> Symbol {
        Symbol {
            name,
            declared_at: SourceSpan::file_start("test.clipasm"),
            value: Some(value),
            value_type: ValueType::Video,
        }
    }

    fn make_evaluation(nodes: Vec<DraftNode>, symbols: Vec<Symbol>, root: ValueRef) -> Evaluation {
        Evaluation {
            nodes,
            symbols,
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
        let mut symbols = Vec::with_capacity(NAMES);
        let mut builder = GraphBuilder::for_program(
            &mut nodes,
            &video,
            crate::model::AudioSpec::default(),
            1,
            origin(),
        );
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
            debug_assert_eq!(symbol_id.index(), symbols.len());
            symbols.push(symbol(name, value));
        }
        let evaluation = make_evaluation(nodes, symbols, root.expect("root"));

        let error = detect_cycles(&evaluation).expect_err("named cycle");

        assert_eq!(error.code, "E_DEPENDENCY_CYCLE");
        assert!(error.message.starts_with("named-value dependency cycle:"));
        assert!(error.message.ends_with("name_00000"));
    }

    #[test]
    fn deferred_repeat_chains_are_memoized_and_remain_deferred() {
        let video = VideoSpec::default();
        let mut nodes = Vec::new();
        let mut builder = GraphBuilder::for_program(
            &mut nodes,
            &video,
            crate::model::AudioSpec::default(),
            2,
            origin(),
        );
        let mut root = builder
            .video_source("source.mp4".into(), ImageFit::Cover)
            .expect("video source");
        for _ in 0..64 {
            root = builder
                .repeat(root, NonZeroU64::new(2).expect("nonzero"))
                .expect("repeat");
        }
        let evaluation = make_evaluation(nodes, Vec::new(), root);
        let order = super::super::traversal::topological_order(&evaluation.nodes, &[], [root])
            .expect("order");

        let domains =
            super::super::domain::infer_domains(&evaluation, &video, &order).expect("domains");

        assert_eq!(order.len(), 65);
        assert!(domains.into_iter().all(|domain| domain.is_none()));
    }

    #[test]
    fn repeat_domains_multiply_exactly_and_check_overflow() {
        let video = VideoSpec::default();
        let mut nodes = Vec::new();
        let mut builder = GraphBuilder::for_program(
            &mut nodes,
            &video,
            crate::model::AudioSpec::default(),
            2,
            origin(),
        );
        let source = builder
            .image_video("source.png".into(), FrameCount(5), ImageFit::Cover)
            .expect("source");
        let root = builder
            .repeat(source, NonZeroU64::new(3).expect("nonzero"))
            .expect("repeat");
        let evaluation = make_evaluation(nodes, Vec::new(), root);
        let order = super::super::traversal::topological_order(&evaluation.nodes, &[], [root])
            .expect("order");
        let domains =
            super::super::domain::infer_domains(&evaluation, &video, &order).expect("domains");
        assert_eq!(
            domains[root.id().get() as usize]
                .as_ref()
                .expect("known repeat")
                .frames(),
            FrameCount(15)
        );

        let mut nodes = Vec::new();
        let mut builder = GraphBuilder::for_program(
            &mut nodes,
            &video,
            crate::model::AudioSpec::default(),
            2,
            origin(),
        );
        let source = builder
            .image_video("huge.png".into(), FrameCount(u64::MAX), ImageFit::Cover)
            .expect("source");
        let root = builder
            .repeat(source, NonZeroU64::new(2).expect("nonzero"))
            .expect("repeat");
        let evaluation = make_evaluation(nodes, Vec::new(), root);
        let order = super::super::traversal::topological_order(&evaluation.nodes, &[], [root])
            .expect("order");
        let error =
            super::super::domain::infer_domains(&evaluation, &video, &order).expect_err("overflow");
        assert_eq!(error.code, "E_FRAME_OVERFLOW");
    }
}
