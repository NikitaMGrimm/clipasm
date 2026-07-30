use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{
    AudioDomain, FrameCount, NativeDuration, NodeId, TimelineRate, ValueRef, ValueType,
};
use crate::semantic::CompiledNode;
use crate::source::SourceSpan;

use super::super::{PreparedAudioKind, PreparedVideoKind};
use super::{PreflightLowerer, project_domain};

pub(super) fn flash_cut(
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
    validate_flash_cut_frames(frames, after_frames, &node.origin().span)?;
    let total = before_domain
        .frames()
        .checked_add(after_frames, &node.origin().span)?;
    lowerer.add_video_node(
        PreparedVideoKind::FlashCut {
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
    duration: NativeDuration,
) -> Result<NodeId> {
    match (before.value_type(), duration) {
        (ValueType::Video, NativeDuration::Frames(frames)) => {
            crossfade_video(lowerer, node, before, after, frames)
        }
        (ValueType::Audio, NativeDuration::Samples(samples)) => {
            crossfade_audio(lowerer, node, before, after, samples)
        }
        _ => Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidGraph,
            "crossfade value type and native duration unit do not match",
            node.origin().span.clone(),
        )),
    }
}

fn crossfade_video(
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
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::CrossfadeAudioDuration,
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

fn crossfade_audio(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    before: ValueRef,
    after: ValueRef,
    samples: u64,
) -> Result<NodeId> {
    let before = lowerer.prepared_dependency(before, node.origin())?;
    let after = lowerer.prepared_dependency(after, node.origin())?;
    let before_samples = lowerer.audio_domain(before, node.origin())?.samples();
    let after_samples = lowerer.audio_domain(after, node.origin())?.samples();
    validate_crossfade_samples(samples, before_samples, after_samples, &node.origin().span)?;
    if samples > i64::MAX as u64 {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::CrossfadeAudioDuration,
            format!(
                "crossfade overlap requires {samples} audio samples, but FFmpeg supports at most {}",
                i64::MAX
            ),
            node.origin().span.clone(),
        ));
    }
    let output_samples = before_samples
        .checked_add(after_samples)
        .and_then(|combined| combined.checked_sub(samples))
        .ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::AudioDurationOverflow,
                "crossfade output exceeds the supported audio sample count",
                node.origin().span.clone(),
            )
        })?;
    lowerer.add_audio_node(
        PreparedAudioKind::Crossfade {
            before,
            after,
            samples,
        },
        AudioDomain::new(output_samples, *lowerer.compiled.audio()),
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
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidCrossfadeDuration,
            "`crossfade.duration` must cover at least one project frame",
            span.clone(),
        ));
    }
    for (name, available) in [("before", before), ("after", after)] {
        if frames > available {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidCrossfadeDuration,
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

fn validate_crossfade_samples(
    samples: u64,
    before: u64,
    after: u64,
    span: &SourceSpan,
) -> Result<()> {
    if samples == 0 {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidCrossfadeDuration,
            "`crossfade.duration` must cover at least one project sample",
            span.clone(),
        ));
    }
    for (name, available) in [("before", before), ("after", after)] {
        if samples > available {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidCrossfadeDuration,
                format!(
                    "`crossfade.duration` covers {samples} samples, but `{name}` contains only {available} samples"
                ),
                span.clone(),
            ));
        }
    }
    Ok(())
}

fn validate_flash_cut_frames(
    frames: FrameCount,
    after: FrameCount,
    span: &SourceSpan,
) -> Result<()> {
    if frames > after {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidFlashCutDuration,
            format!(
                "`flash_cut.duration` covers {} frames, but `after` contains only {} frames",
                frames.0, after.0
            ),
            span.clone(),
        ));
    }
    Ok(())
}
