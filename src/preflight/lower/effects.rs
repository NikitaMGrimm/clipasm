use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{ExactNumber, NodeId, ValueRef};
use crate::semantic::CompiledNode;

use super::super::PreparedVideoKind;
use super::PreflightLowerer;

pub(super) fn zoom_in(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    input: ValueRef,
    by: ExactNumber,
) -> Result<NodeId> {
    let input = lowerer.prepared_dependency(input, node.origin())?;
    let (input_domain, input_has_audio) = lowerer.video_domain(input, node.origin())?;
    let input_domain = *input_domain;
    let curve = crate::preflight::PreparedZoomCurve::new(by.clone())?;
    if curve.estimated_filter_bytes(input_domain.frames())
        > crate::preflight::MAX_COMPOSED_ZOOM_FILTER_BYTES
    {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::GraphTooLarge,
            format!(
                "zoom expression exceeds the {}-byte FFmpeg filter limit",
                crate::preflight::MAX_COMPOSED_ZOOM_FILTER_BYTES
            ),
            node.origin().span.clone(),
        ));
    }
    let (input, curve) = match lowerer.nodes[input.get() as usize].video_kind() {
        Some(PreparedVideoKind::ZoomIn {
            input: preceding_input,
            curve: preceding,
        }) => {
            let composed = preceding.appended(by)?;
            if composed.estimated_filter_bytes(input_domain.frames())
                <= crate::preflight::MAX_COMPOSED_ZOOM_FILTER_BYTES
            {
                (*preceding_input, composed)
            } else {
                (input, curve)
            }
        }
        _ => (input, curve),
    };
    lowerer.add_video_node(
        PreparedVideoKind::ZoomIn { input, curve },
        input_domain,
        input_has_audio,
        node.semantic_version(),
        node.origin().clone(),
    )
}
