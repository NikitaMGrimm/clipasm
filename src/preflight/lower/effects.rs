use crate::diagnostic::Result;
use crate::model::{NodeId, ValueRef};
use crate::semantic::CompiledNode;

use super::super::PreparedVideoKind;
use super::PreflightLowerer;

pub(super) fn zoom(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    input: ValueRef,
    percent: u32,
) -> Result<NodeId> {
    let input = lowerer.prepared_dependency(input, node.origin())?;
    let (input_domain, input_has_audio) = lowerer.video_domain(input, node.origin())?;
    lowerer.add_video_node(
        PreparedVideoKind::Zoom { input, percent },
        *input_domain,
        input_has_audio,
        node.semantic_version(),
        node.origin().clone(),
    )
}

pub(super) fn wobble(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    input: ValueRef,
    pixels: u32,
) -> Result<NodeId> {
    let input = lowerer.prepared_dependency(input, node.origin())?;
    let (input_domain, input_has_audio) = lowerer.video_domain(input, node.origin())?;
    lowerer.add_video_node(
        PreparedVideoKind::Wobble { input, pixels },
        *input_domain,
        input_has_audio,
        node.semantic_version(),
        node.origin().clone(),
    )
}
