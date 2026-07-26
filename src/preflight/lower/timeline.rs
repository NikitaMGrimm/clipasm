use std::num::NonZeroU64;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{
    AudioDomain, FrameCount, FrameRange, NodeId, SampleRange, TimelineRangeExpression, ValueRef,
    VideoDomain,
};
use crate::semantic::CompiledNode;
use crate::source::SourceSpan;

use super::super::{PreparedAudioKind, PreparedVideoKind};
use super::{PreflightLowerer, project_domain};

pub(super) fn audio_slice(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    input: ValueRef,
    range: SampleRange,
) -> Result<NodeId> {
    let input = lowerer.prepared_dependency(input, node.origin())?;
    let input_domain = *lowerer.audio_domain(input, node.origin())?;
    if range.end() > input_domain.samples() {
        return Err(Diagnostic::new(
            "E_RANGE_OUT_OF_BOUNDS",
            format!(
                "audio range {}..{} exceeds input duration of {} samples",
                range.start(),
                range.end(),
                input_domain.samples()
            ),
            node.origin().span.clone(),
        ));
    }
    lowerer.add_audio_node(
        PreparedAudioKind::AudioSlice { input, range },
        AudioDomain::new(range.samples(), input_domain.audio_spec()),
        node.semantic_version(),
        node.origin().clone(),
    )
}

pub(super) fn audio_repeat(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    input: ValueRef,
    count: NonZeroU64,
) -> Result<NodeId> {
    let input = lowerer.prepared_dependency(input, node.origin())?;
    let input_domain = *lowerer.audio_domain(input, node.origin())?;
    let samples = input_domain
        .samples()
        .checked_mul(count.get())
        .ok_or_else(|| {
            Diagnostic::new(
                "E_AUDIO_DURATION_OVERFLOW",
                "repeated audio exceeds the supported sample count",
                node.origin().span.clone(),
            )
        })?;
    lowerer.add_audio_node(
        PreparedAudioKind::AudioRepeat { input, count },
        AudioDomain::new(samples, input_domain.audio_spec()),
        node.semantic_version(),
        node.origin().clone(),
    )
}

pub(super) fn audio_concat(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    inputs: &[ValueRef],
) -> Result<NodeId> {
    let inputs = inputs
        .iter()
        .map(|input| lowerer.prepared_dependency(*input, node.origin()))
        .collect::<Result<Vec<_>>>()?;
    let mut samples = 0_u64;
    for input in &inputs {
        samples = samples
            .checked_add(lowerer.audio_domain(*input, node.origin())?.samples())
            .ok_or_else(|| {
                Diagnostic::new(
                    "E_AUDIO_DURATION_OVERFLOW",
                    "concatenated audio exceeds the supported sample count",
                    node.origin().span.clone(),
                )
            })?;
    }
    lowerer.add_audio_node(
        PreparedAudioKind::AudioConcat { inputs },
        AudioDomain::new(samples, *lowerer.compiled.audio()),
        node.semantic_version(),
        node.origin().clone(),
    )
}

pub(super) fn video_repeat(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    input: ValueRef,
    count: NonZeroU64,
) -> Result<NodeId> {
    let input = lowerer.prepared_dependency(input, node.origin())?;
    let (input_domain, input_has_audio) = lowerer.video_domain(input, node.origin())?;
    let frames = input_domain
        .frames()
        .checked_mul(count.get(), &node.origin().span)?;
    lowerer.add_video_node(
        PreparedVideoKind::Repeat {
            input,
            count,
            frames,
        },
        project_domain(lowerer.compiled.video(), frames),
        input_has_audio,
        node.semantic_version(),
        node.origin().clone(),
    )
}

pub(super) fn video_concat(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    inputs: &[ValueRef],
) -> Result<NodeId> {
    let inputs = inputs
        .iter()
        .map(|input| lowerer.prepared_dependency(*input, node.origin()))
        .collect::<Result<Vec<_>>>()?;
    let domain = lowerer.concat_domain(&inputs, node.origin())?;
    let has_audio = inputs.iter().try_fold(false, |has_audio, input| {
        lowerer
            .video_domain(*input, node.origin())
            .map(|(_, input_has_audio)| has_audio || input_has_audio)
    })?;
    lowerer.add_video_node(
        PreparedVideoKind::Concat { inputs },
        domain,
        has_audio,
        node.semantic_version(),
        node.origin().clone(),
    )
}

pub(super) fn deferred_video_slice(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    input: ValueRef,
    range: &TimelineRangeExpression,
) -> Result<NodeId> {
    let input_node = lowerer.prepared_dependency(input, node.origin())?;
    let range = resolve_video_range(lowerer, node, range)?;
    let (input_domain, input_has_audio) = lowerer.video_domain(input_node, node.origin())?;
    validate_prepared_range(range, input_domain, &node.origin().span)?;
    lowerer.add_video_node(
        PreparedVideoKind::Slice {
            input: input_node,
            range,
        },
        project_domain(lowerer.compiled.video(), range.frames()),
        input_has_audio,
        node.semantic_version(),
        node.origin().clone(),
    )
}

pub(super) fn deferred_replace_range(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    base: ValueRef,
    replacement: ValueRef,
    range: &TimelineRangeExpression,
) -> Result<NodeId> {
    let range = resolve_video_range(lowerer, node, range)?;
    replace_range(lowerer, node, base, replacement, range)
}

pub(super) fn resolve_video_extent(
    lowerer: &PreflightLowerer<'_>,
    node: &CompiledNode,
    extent: &crate::model::TimelineExpression,
) -> Result<FrameCount> {
    extent
        .resolve_frame_boundary(
            lowerer.compiled.video().fps(),
            |value| {
                let prepared = lowerer.prepared_dependency(value, node.origin())?;
                lowerer
                    .video_domain(prepared, node.origin())
                    .map(|(domain, _)| domain.frames())
            },
            &node.origin().span,
        )
        .map(FrameCount)
}

fn resolve_video_range(
    lowerer: &PreflightLowerer<'_>,
    node: &CompiledNode,
    range: &TimelineRangeExpression,
) -> Result<FrameRange> {
    let fps = lowerer.compiled.video().fps();
    let resolve = |expression: &crate::model::TimelineExpression| {
        expression.resolve_frame_boundary(
            fps,
            |value| {
                let prepared = lowerer.prepared_dependency(value, node.origin())?;
                lowerer
                    .video_domain(prepared, node.origin())
                    .map(|(domain, _)| domain.frames())
            },
            &node.origin().span,
        )
    };
    let start = resolve(&range.start)?;
    let end = resolve(&range.end)?;
    FrameRange::new(start, end).ok_or_else(|| {
        Diagnostic::new(
            "E_INVALID_TIME_RANGE",
            "timeline-range start must be earlier than its end",
            node.origin().span.clone(),
        )
    })
}

pub(super) fn video_slice(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    input: ValueRef,
    range: FrameRange,
) -> Result<NodeId> {
    let input = lowerer.prepared_dependency(input, node.origin())?;
    let (input_domain, input_has_audio) = lowerer.video_domain(input, node.origin())?;
    validate_prepared_range(range, input_domain, &node.origin().span)?;
    lowerer.add_video_node(
        PreparedVideoKind::Slice { input, range },
        project_domain(lowerer.compiled.video(), range.frames()),
        input_has_audio,
        node.semantic_version(),
        node.origin().clone(),
    )
}

pub(super) fn replace_range(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    base: ValueRef,
    replacement: ValueRef,
    range: FrameRange,
) -> Result<NodeId> {
    let base_node = lowerer.prepared_dependency(base, node.origin())?;
    let replacement_node = lowerer.prepared_dependency(replacement, node.origin())?;
    let (base_domain, base_has_audio) = lowerer.video_domain(base_node, node.origin())?;
    let base_domain = *base_domain;
    validate_prepared_range(range, &base_domain, &node.origin().span)?;
    let mut pieces = Vec::new();
    if range.start() > 0 {
        pieces.push(lowerer.add_video_node(
            PreparedVideoKind::Slice {
                input: base_node,
                range: FrameRange::new(0, range.start()).expect("nonempty during prefix"),
            },
            project_domain(lowerer.compiled.video(), FrameCount(range.start())),
            base_has_audio,
            node.semantic_version(),
            node.origin().clone_with_construct("range prefix"),
        )?);
    }
    pieces.push(replacement_node);
    if range.end() < base_domain.frames().0 {
        pieces.push(
            lowerer.add_video_node(
                PreparedVideoKind::Slice {
                    input: base_node,
                    range: FrameRange::new(range.end(), base_domain.frames().0)
                        .expect("nonempty during suffix"),
                },
                project_domain(
                    lowerer.compiled.video(),
                    FrameCount(base_domain.frames().0 - range.end()),
                ),
                base_has_audio,
                node.semantic_version(),
                node.origin().clone_with_construct("range suffix"),
            )?,
        );
    }
    if pieces.len() == 1 {
        Ok(pieces[0])
    } else {
        let domain = lowerer.concat_domain(&pieces, node.origin())?;
        let has_audio = pieces.iter().try_fold(false, |has_audio, piece| {
            lowerer
                .video_domain(*piece, node.origin())
                .map(|(_, piece_has_audio)| has_audio || piece_has_audio)
        })?;
        lowerer.add_video_node(
            PreparedVideoKind::Concat { inputs: pieces },
            domain,
            has_audio,
            node.semantic_version(),
            node.origin().clone(),
        )
    }
}

fn validate_prepared_range(
    range: FrameRange,
    input: &VideoDomain,
    span: &SourceSpan,
) -> Result<()> {
    if range.end() > input.frames().0 {
        return Err(Diagnostic::new(
            "E_INVALID_TIME_RANGE",
            format!(
                "frame range {}..{} is outside the base Video domain of {} frames",
                range.start(),
                range.end(),
                input.frames().0
            ),
            span.clone(),
        ));
    }
    Ok(())
}
