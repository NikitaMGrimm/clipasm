use std::collections::BTreeMap;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::ValueRef;
use crate::semantic::{CompiledNode, DraftNode, SemanticNodeKind, SourceOrigin, SymbolId};

pub(crate) trait SemanticNodeView {
    fn kind(&self) -> &SemanticNodeKind;
    fn origin(&self) -> &SourceOrigin;
}

impl SemanticNodeView for DraftNode {
    fn kind(&self) -> &SemanticNodeKind {
        self.kind()
    }

    fn origin(&self) -> &SourceOrigin {
        self.origin()
    }
}

impl SemanticNodeView for CompiledNode {
    fn kind(&self) -> &SemanticNodeKind {
        self.kind()
    }

    fn origin(&self) -> &SourceOrigin {
        self.origin()
    }
}

struct Frame {
    value: ValueRef,
    next_dependency: usize,
}

pub(crate) fn topological_order<N: SemanticNodeView>(
    nodes: &[N],
    symbols: &BTreeMap<SymbolId, ValueRef>,
    roots: impl IntoIterator<Item = ValueRef>,
) -> Result<Vec<ValueRef>> {
    let mut states = vec![0_u8; nodes.len()];
    let mut order = Vec::with_capacity(nodes.len());
    let mut stack = Vec::<Frame>::new();

    for root in roots {
        let root_index = node_index(root, nodes)?;
        if states[root_index] == 2 {
            continue;
        }
        stack.push(Frame {
            value: root,
            next_dependency: 0,
        });
        while let Some(frame) = stack.last_mut() {
            let index = node_index(frame.value, nodes)?;
            if states[index] == 0 {
                states[index] = 1;
            }
            if let Some(dependency) = dependency_at(&nodes[index], symbols, frame.next_dependency)?
            {
                frame.next_dependency += 1;
                let dependency_index = node_index(dependency, nodes)?;
                match states[dependency_index] {
                    0 => stack.push(Frame {
                        value: dependency,
                        next_dependency: 0,
                    }),
                    1 => {
                        return Err(Diagnostic::new(
                            "E_DEPENDENCY_CYCLE",
                            "semantic graph contains a dependency cycle",
                            nodes[dependency_index].origin().span.clone(),
                        ));
                    }
                    2 => {}
                    _ => unreachable!("traversal state is closed"),
                }
            } else {
                states[index] = 2;
                order.push(frame.value);
                stack.pop();
            }
        }
    }

    Ok(order)
}

fn dependency_at<N: SemanticNodeView>(
    node: &N,
    symbols: &BTreeMap<SymbolId, ValueRef>,
    index: usize,
) -> Result<Option<ValueRef>> {
    Ok(match node.kind() {
        SemanticNodeKind::Reference { symbol } if index == 0 => {
            Some(*symbols.get(symbol).ok_or_else(|| {
                Diagnostic::new(
                    "E_MISSING_REFERENCE",
                    format!("reference names unknown symbol {}", symbol.index()),
                    node.origin().span.clone(),
                )
            })?)
        }
        SemanticNodeKind::Repeat { input, .. }
        | SemanticNodeKind::AudioRepeat { input, .. }
        | SemanticNodeKind::AudioSlice { input, .. }
        | SemanticNodeKind::Zoom { input, .. }
        | SemanticNodeKind::Wobble { input, .. }
        | SemanticNodeKind::Slice { input, .. }
        | SemanticNodeKind::ExtractAudio { video: input }
        | SemanticNodeKind::AudioOnBlack { audio: input }
            if index == 0 =>
        {
            Some(*input)
        }
        SemanticNodeKind::Concat { inputs } | SemanticNodeKind::AudioConcat { inputs } => {
            inputs.get(index).copied()
        }
        SemanticNodeKind::FlashJoin { before, after, .. } => [*before, *after].get(index).copied(),
        SemanticNodeKind::ReplaceRange {
            base, replacement, ..
        } => [*base, *replacement].get(index).copied(),
        SemanticNodeKind::SetAudio { audio, video } => [*audio, *video].get(index).copied(),
        SemanticNodeKind::ExternalVideo { invocation } => {
            invocation.inputs.values().nth(index).copied()
        }
        SemanticNodeKind::ImageVideo { .. }
        | SemanticNodeKind::VideoSource { .. }
        | SemanticNodeKind::AudioSource { .. }
        | SemanticNodeKind::Reference { .. }
        | SemanticNodeKind::Repeat { .. }
        | SemanticNodeKind::AudioRepeat { .. }
        | SemanticNodeKind::AudioSlice { .. }
        | SemanticNodeKind::Zoom { .. }
        | SemanticNodeKind::Wobble { .. }
        | SemanticNodeKind::Slice { .. }
        | SemanticNodeKind::ExtractAudio { .. }
        | SemanticNodeKind::AudioOnBlack { .. } => None,
    })
}

fn node_index<N>(value: ValueRef, nodes: &[N]) -> Result<usize> {
    let index = value.id().get() as usize;
    if index < nodes.len() {
        Ok(index)
    } else {
        Err(Diagnostic::new(
            "E_INVALID_GRAPH",
            format!(
                "semantic value {} is outside the graph of {} values",
                value.id().get(),
                nodes.len()
            ),
            crate::source::SourceSpan::file_start("<semantic-graph>"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FrameCount, ImageFit, ValueType, VideoSpec};
    use crate::semantic::{GraphBuilder, SourceOrigin};
    use crate::source::SourceSpan;

    fn origin() -> SourceOrigin {
        SourceOrigin::new("test", SourceSpan::file_start("test.yaml"))
    }

    #[test]
    fn orders_a_deep_reference_chain_without_recursion() {
        const ALIASES: usize = 20_001;
        let video = VideoSpec::default();
        let mut nodes = Vec::with_capacity(ALIASES + 1);
        let mut symbols = BTreeMap::new();
        let mut builder = GraphBuilder::for_program(&mut nodes, &video, 1, origin());
        let source = builder
            .image_video("source.png".into(), FrameCount(1), ImageFit::Cover)
            .expect("source");
        let mut root = source;
        for index in 0..ALIASES {
            let symbol = SymbolId::new(u32::try_from(index).expect("test symbol ID"));
            symbols.insert(symbol, root);
            root = builder
                .reference(symbol, ValueType::Video)
                .expect("reference");
        }

        let order = topological_order(&nodes, &symbols, [root]).expect("topological order");

        assert_eq!(order.len(), ALIASES + 1);
        assert_eq!(order[0], source);
        assert_eq!(order.last(), Some(&root));
    }

    #[test]
    fn reports_a_semantic_reference_cycle() {
        let video = VideoSpec::default();
        let mut nodes = Vec::new();
        let mut builder = GraphBuilder::for_program(&mut nodes, &video, 1, origin());
        let a_symbol = SymbolId::new(0);
        let b_symbol = SymbolId::new(1);
        let c_symbol = SymbolId::new(2);
        let a = builder.reference(b_symbol, ValueType::Video).expect("a");
        let b = builder.reference(c_symbol, ValueType::Video).expect("b");
        let c = builder.reference(a_symbol, ValueType::Video).expect("c");
        let symbols = BTreeMap::from([(a_symbol, a), (b_symbol, b), (c_symbol, c)]);

        let error = topological_order(&nodes, &symbols, [a]).expect_err("cycle");

        assert_eq!(error.code, "E_DEPENDENCY_CYCLE");
    }
}
