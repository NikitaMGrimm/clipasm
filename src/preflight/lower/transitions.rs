use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, NodeId, TimelineRate, ValueRef};
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

pub(super) fn crossfade(
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
    let before_frames = before_domain.frames();
    let after_frames = after_domain.frames();
    validate_crossfade_frames(frames, before_frames, after_frames, &node.origin().span)?;
    let overlap_start = before_frames.0 - frames.0;
    let overlap_samples =
        TimelineRate::new(*lowerer.compiled.video(), *lowerer.compiled.audio())
            .samples_between_frames(overlap_start, before_frames.0, &node.origin().span)?;
    if overlap_samples > i64::MAX as u64 {
        return Err(Diagnostic::new(
            "E_CROSSFADE_AUDIO_DURATION",
            format!(
                "crossfade overlap requires {overlap_samples} audio samples, but FFmpeg supports at most {}",
                i64::MAX
            ),
            node.origin().span.clone(),
        ));
    }
    let combined = before_frames.checked_add(after_frames, &node.origin().span)?;
    lowerer.add_video_node(
        PreparedVideoKind::Crossfade {
            before,
            after,
            frames,
        },
        project_domain(lowerer.compiled.video(), FrameCount(combined.0 - frames.0)),
        before_has_audio || after_has_audio,
        node.semantic_version(),
        node.origin().clone(),
    )
}

fn validate_crossfade_frames(
    frames: FrameCount,
    before: FrameCount,
    after: FrameCount,
    span: &SourceSpan,
) -> Result<()> {
    if frames.0 == 0 {
        return Err(Diagnostic::new(
            "E_INVALID_CROSSFADE_DURATION",
            "`crossfade.duration` must cover at least one project frame",
            span.clone(),
        ));
    }
    for (name, available) in [("before", before), ("after", after)] {
        if frames > available {
            return Err(Diagnostic::new(
                "E_INVALID_CROSSFADE_DURATION",
                format!(
                    "`crossfade.duration` covers {} frames, but `{name}` contains only {} frames",
                    frames.0, available.0
                ),
                span.clone(),
            ));
        }
    }
    Ok(())
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
