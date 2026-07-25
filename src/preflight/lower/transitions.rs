use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, NodeId, ValueRef};
use crate::semantic::CompiledNode;
use crate::source::SourceSpan;

use super::super::PreparedVideoKind;
use super::{PreflightLowerer, project_domain};

pub(super) fn flash(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    before: ValueRef,
    after: ValueRef,
    frames: FrameCount,
) -> Result<NodeId> {
    let before = lowerer.prepared_dependency(before, node.origin())?;
    let after = lowerer.prepared_dependency(after, node.origin())?;
    let (before_domain, before_has_audio) = lowerer.video_domain(before, node.origin())?;
    let (after_domain, after_has_audio) = lowerer.video_domain(after, node.origin())?;
    let after_frames = after_domain.frames();
    validate_flash_frames(frames, after_frames, &node.origin().span)?;
    let total = before_domain
        .frames()
        .checked_add(after_frames, &node.origin().span)?;
    lowerer.add_video_node(
        PreparedVideoKind::FlashJoin {
            before,
            after,
            frames,
        },
        project_domain(lowerer.compiled.video(), total),
        before_has_audio || after_has_audio,
        node.semantic_version(),
        node.origin().clone(),
    )
}

fn validate_flash_frames(frames: FrameCount, after: FrameCount, span: &SourceSpan) -> Result<()> {
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
