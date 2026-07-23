use std::collections::{BTreeMap, BTreeSet};

use crate::compiler::{CompiledNode, CompiledWorkflow, Evaluation, ExplainEntry, SemanticNodeKind};
use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, ValueId, ValueRef, ValueType, VideoDomain, VideoSpec};
use crate::syntax::Workflow;

pub(super) fn finalize(
    workflow: &Workflow,
    video: VideoSpec,
    evaluation: Evaluation,
    format_version: u32,
) -> Result<CompiledWorkflow> {
    validate_references(&evaluation)?;
    detect_cycles(&evaluation)?;
    let domains = infer_domains(&evaluation, &video)?;
    let structure_hash =
        super::fingerprint::compiled_structure_hash(&evaluation, &domains, &video, format_version)?;

    let nodes = evaluation
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| CompiledNode {
            id: ValueId::new(u32::try_from(index).expect("draft node IDs already fit in u32")),
            kind: node.kind.clone(),
            value_type: node.value_type,
            domain: domains[index].clone(),
            semantic_version: node.semantic_version,
            origin: node.origin.clone(),
        })
        .collect();
    let named_values = evaluation
        .symbol_order
        .iter()
        .map(|name| {
            (
                name.clone(),
                evaluation.symbols[name]
                    .value
                    .expect("every collected symbol is evaluated"),
            )
        })
        .collect();
    let mut explain = evaluation
        .surface
        .into_iter()
        .map(|record| ExplainEntry {
            construct: record.construct,
            output: record.value,
            id: record.id,
            span: record.span,
        })
        .collect::<Vec<_>>();
    explain.push(ExplainEntry {
        construct: "root timeline".to_owned(),
        output: evaluation.root,
        id: None,
        span: crate::diagnostic::SourceSpan::file_start(workflow.source_path()),
    });

    Ok(CompiledWorkflow {
        format_version,
        workflow_version: workflow.version(),
        engine_version: env!("CARGO_PKG_VERSION").to_owned(),
        structure_hash,
        video,
        nodes,
        root: evaluation.root,
        named_values,
        explain,
        output: workflow.output().cloned(),
        source_path: workflow.source_path().to_path_buf(),
    })
}

fn validate_references(evaluation: &Evaluation) -> Result<()> {
    for node in &evaluation.nodes {
        if let SemanticNodeKind::Reference { name } = &node.kind {
            let Some(symbol) = evaluation.symbols.get(name) else {
                return Err(Diagnostic::new(
                    "E_MISSING_REFERENCE",
                    format!("reference `${name}` does not name any clip or invocation id"),
                    node.origin.span.clone(),
                ));
            };
            if symbol.value.is_none() {
                return Err(Diagnostic::new(
                    "E_MISSING_REFERENCE",
                    format!("name `{name}` has no compiled value"),
                    node.origin.span.clone(),
                ));
            }
            let symbol_type = symbol
                .value_type
                .expect("symbol types are resolved before evaluation");
            if symbol_type != node.value_type {
                return Err(Diagnostic::new(
                    "E_TYPE_MISMATCH",
                    format!(
                        "reference `${name}` has type {}, but its expression was recorded as {}",
                        symbol_type, node.value_type
                    ),
                    node.origin.span.clone(),
                ));
            }
        }
    }
    Ok(())
}

fn detect_cycles(evaluation: &Evaluation) -> Result<()> {
    let mut edges = BTreeMap::<String, BTreeSet<String>>::new();
    for name in &evaluation.symbol_order {
        let value = evaluation.symbols[name]
            .value
            .expect("every collected symbol is evaluated");
        let mut references = BTreeSet::new();
        collect_direct_references(
            value,
            &evaluation.nodes,
            &mut references,
            &mut BTreeSet::new(),
        );
        edges.insert(name.clone(), references);
    }
    let mut states = BTreeMap::<String, u8>::new();
    let mut path = Vec::new();
    for name in &evaluation.symbol_order {
        detect_symbol_cycle(name, &edges, &mut states, &mut path, &evaluation.symbols)?;
    }
    Ok(())
}

fn collect_direct_references(
    value: ValueRef,
    nodes: &[super::DraftNode],
    output: &mut BTreeSet<String>,
    visited: &mut BTreeSet<ValueId>,
) {
    if !visited.insert(value.id()) {
        return;
    }
    match &nodes[value.id().get() as usize].kind {
        SemanticNodeKind::ImageVideo { .. } => {}
        SemanticNodeKind::Reference { name } => {
            output.insert(name.clone());
        }
        SemanticNodeKind::Concat { inputs } => {
            for input in inputs {
                collect_direct_references(*input, nodes, output, visited);
            }
        }
        SemanticNodeKind::Slice { input, .. } => {
            collect_direct_references(*input, nodes, output, visited);
        }
        SemanticNodeKind::During {
            base, processed, ..
        } => {
            collect_direct_references(*base, nodes, output, visited);
            collect_direct_references(*processed, nodes, output, visited);
        }
    }
}

fn detect_symbol_cycle(
    name: &str,
    edges: &BTreeMap<String, BTreeSet<String>>,
    states: &mut BTreeMap<String, u8>,
    path: &mut Vec<String>,
    symbols: &BTreeMap<String, super::Symbol>,
) -> Result<()> {
    match states.get(name).copied().unwrap_or(0) {
        2 => return Ok(()),
        1 => {
            let start = path.iter().position(|entry| entry == name).unwrap_or(0);
            let mut cycle = path[start..].to_vec();
            cycle.push(name.to_owned());
            return Err(Diagnostic::new(
                "E_DEPENDENCY_CYCLE",
                format!("named-value dependency cycle: {}", cycle.join(" -> ")),
                symbols[name].declared_at.clone(),
            ));
        }
        _ => {}
    }
    states.insert(name.to_owned(), 1);
    path.push(name.to_owned());
    if let Some(targets) = edges.get(name) {
        for target in targets {
            detect_symbol_cycle(target, edges, states, path, symbols)?;
        }
    }
    path.pop();
    states.insert(name.to_owned(), 2);
    Ok(())
}

fn infer_domains(evaluation: &Evaluation, video: &VideoSpec) -> Result<Vec<Option<VideoDomain>>> {
    let mut domains = vec![None; evaluation.nodes.len()];
    let mut visiting = BTreeSet::new();
    for name in &evaluation.symbol_order {
        let value = evaluation.symbols[name]
            .value
            .expect("every collected symbol is evaluated");
        infer_value(value, evaluation, video, &mut domains, &mut visiting)?;
    }
    infer_value(
        evaluation.root,
        evaluation,
        video,
        &mut domains,
        &mut visiting,
    )?;
    Ok(domains)
}

fn infer_value(
    value: ValueRef,
    evaluation: &Evaluation,
    video: &VideoSpec,
    domains: &mut [Option<VideoDomain>],
    visiting: &mut BTreeSet<ValueId>,
) -> Result<Option<VideoDomain>> {
    if let Some(domain) = &domains[value.id().get() as usize] {
        return Ok(Some(domain.clone()));
    }
    if value.value_type() != ValueType::Video {
        return Ok(None);
    }
    if !visiting.insert(value.id()) {
        return Err(Diagnostic::new(
            "E_DEPENDENCY_CYCLE",
            "dependency cycle encountered while inferring video duration",
            evaluation.nodes[value.id().get() as usize]
                .origin
                .span
                .clone(),
        ));
    }
    let node = &evaluation.nodes[value.id().get() as usize];
    let frames = match &node.kind {
        SemanticNodeKind::ImageVideo { frames, .. } => *frames,
        SemanticNodeKind::Reference { name } => {
            let target = evaluation.symbols[name]
                .value
                .expect("references were resolved before domain inference");
            infer_value(target, evaluation, video, domains, visiting)?
                .expect("Video references have Video domains")
                .frames
        }
        SemanticNodeKind::Concat { inputs } => {
            let mut total = FrameCount(0);
            for input in inputs {
                let input_domain = infer_value(*input, evaluation, video, domains, visiting)?
                    .expect("concat inputs are type-checked as Video");
                total = total.checked_add(input_domain.frames, &node.origin.span)?;
            }
            total
        }
        SemanticNodeKind::Slice { input, range } => {
            let input_domain = infer_value(*input, evaluation, video, domains, visiting)?
                .expect("slice input is type-checked as Video");
            validate_range(*range, input_domain.frames, &node.origin.span)?;
            range.frames()
        }
        SemanticNodeKind::During {
            base,
            processed,
            range,
        } => {
            let base_domain = infer_value(*base, evaluation, video, domains, visiting)?
                .expect("during base is type-checked as Video");
            validate_range(*range, base_domain.frames, &node.origin.span)?;
            let processed_domain = infer_value(*processed, evaluation, video, domains, visiting)?
                .expect("during output is type-checked as Video");
            FrameCount(base_domain.frames.0 - range.frames().0)
                .checked_add(processed_domain.frames, &node.origin.span)?
        }
    };
    visiting.remove(&value.id());
    let domain = VideoDomain {
        frames,
        width: video.width,
        height: video.height,
        frame_rate: video.fps,
    };
    domains[value.id().get() as usize] = Some(domain.clone());
    Ok(Some(domain))
}

fn validate_range(
    range: crate::model::FrameRange,
    input: FrameCount,
    span: &crate::diagnostic::SourceSpan,
) -> Result<()> {
    if range.end() > input.0 {
        return Err(Diagnostic::new(
            "E_INVALID_DURING_RANGE",
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
