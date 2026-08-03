use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{ExactNumber, NodeId, ValueRef};
use crate::semantic::CompiledNode;

use super::super::PreparedVideoKind;
use super::PreflightLowerer;

const MAX_COMPOSED_ZOOM_FILTER_BYTES: usize = 24 * 1024;

pub(super) fn zoom_in(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    input: ValueRef,
    by: ExactNumber,
) -> Result<NodeId> {
    let input = lowerer.prepared_dependency(input, node.origin())?;
    let (input_domain, input_has_audio) = lowerer.video_domain(input, node.origin())?;
    let input_domain = *input_domain;
    let (input, curve) = match lowerer.nodes[input.get() as usize].video_kind() {
        Some(PreparedVideoKind::ZoomIn {
            input,
            curve: preceding,
        }) => (*input, preceding.appended(by)?),
        _ => (input, crate::preflight::PreparedZoomCurve::new(by)?),
    };
    if curve.estimated_filter_bytes(input_domain.frames()) > MAX_COMPOSED_ZOOM_FILTER_BYTES {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::GraphTooLarge,
            format!(
                "adjacent zoom composition exceeds the {MAX_COMPOSED_ZOOM_FILTER_BYTES}-byte FFmpeg filter limit"
            ),
            node.origin().span.clone(),
        ));
    }
    lowerer.add_video_node(
        PreparedVideoKind::ZoomIn { input, curve },
        input_domain,
        input_has_audio,
        node.semantic_version(),
        node.origin().clone(),
    )
}
