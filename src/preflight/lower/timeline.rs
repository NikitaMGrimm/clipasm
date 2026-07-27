use std::num::NonZeroU64;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{
    AudioDomain, FrameCount, FrameRange, NativeRange, NodeId, SampleRange, TimelineRangeExpression,
    ValueRef, ValueType, VideoDomain,
};
use crate::semantic::CompiledNode;
use crate::source::SourceSpan;

use super::super::{PreparedAudioKind, PreparedVideoKind};
use super::{PreflightLowerer, project_domain};

pub(super) fn slice(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    input: ValueRef,
    range: NativeRange,
) -> Result<NodeId> {
    match (input.value_type(), range) {
        (ValueType::Video, NativeRange::Frames(range)) => video_slice(lowerer, node, input, range),
        (ValueType::Audio, NativeRange::Samples(range)) => audio_slice(lowerer, node, input, range),
        _ => unreachable!("semantic slice range matches its input type"),
    }
}

pub(super) fn repeat(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    input: ValueRef,
    count: NonZeroU64,
) -> Result<NodeId> {
    match input.value_type() {
        ValueType::Video => video_repeat(lowerer, node, input, count),
        ValueType::Audio => audio_repeat(lowerer, node, input, count),
    }
}

pub(super) fn concat(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    inputs: &[ValueRef],
) -> Result<NodeId> {
    match inputs
        .first()
        .expect("semantic concat inputs are nonempty")
        .value_type()
    {
        ValueType::Video => video_concat(lowerer, node, inputs),
        ValueType::Audio => audio_concat(lowerer, node, inputs),
    }
}

pub(super) fn replace_range(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    base: ValueRef,
    replacement: ValueRef,
    range: NativeRange,
) -> Result<NodeId> {
    match (base.value_type(), range) {
        (ValueType::Video, NativeRange::Frames(range)) => {
            video_replace_range(lowerer, node, base, replacement, range)
        }
        (ValueType::Audio, NativeRange::Samples(range)) => {
            audio_replace_range(lowerer, node, base, replacement, range)
        }
        _ => unreachable!("semantic replacement range matches its base type"),
    }
}

fn audio_slice(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    input: ValueRef,
    range: SampleRange,
) -> Result<NodeId> {
    let input = lowerer.prepared_dependency(input, node.origin())?;
    let input_domain = *lowerer.audio_domain(input, node.origin())?;
    validate_prepared_audio_range(range, &input_domain, &node.origin().span)?;
    lowerer.add_audio_node(
        PreparedAudioKind::AudioSlice { input, range },
        AudioDomain::new(range.samples(), input_domain.audio_spec()),
        node.semantic_version(),
        node.origin().clone(),
    )
}

fn audio_repeat(
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
            Diagnostic::builtin(
                BuiltinDiagnostic::AudioDurationOverflow,
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

fn audio_concat(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    inputs: &[ValueRef],
) -> Result<NodeId> {
    let inputs = inputs
        .iter()
        .map(|input| lowerer.prepared_dependency(*input, node.origin()))
        .collect::<Result<Vec<_>>>()?;
    add_audio_concat(lowerer, node, inputs)
}

fn add_audio_concat(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    inputs: Vec<NodeId>,
) -> Result<NodeId> {
    let mut samples = 0_u64;
    for input in &inputs {
        samples = samples
            .checked_add(lowerer.audio_domain(*input, node.origin())?.samples())
            .ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::AudioDurationOverflow,
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

fn video_repeat(
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

fn video_concat(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    inputs: &[ValueRef],
) -> Result<NodeId> {
    let inputs = inputs
        .iter()
        .map(|input| lowerer.prepared_dependency(*input, node.origin()))
        .collect::<Result<Vec<_>>>()?;
    add_video_concat(lowerer, node, inputs)
}

fn add_video_concat(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    inputs: Vec<NodeId>,
) -> Result<NodeId> {
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

pub(super) fn deferred_slice(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    input: ValueRef,
    range: &TimelineRangeExpression,
) -> Result<NodeId> {
    match input.value_type() {
        ValueType::Video => {
            let range = resolve_video_range(lowerer, node, range)?;
            video_slice(lowerer, node, input, range)
        }
        ValueType::Audio => {
            let range = resolve_audio_range(lowerer, node, range)?;
            audio_slice(lowerer, node, input, range)
        }
    }
}

pub(super) fn deferred_replace_range(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    base: ValueRef,
    replacement: ValueRef,
    range: &TimelineRangeExpression,
) -> Result<NodeId> {
    match base.value_type() {
        ValueType::Video => {
            let range = resolve_video_range(lowerer, node, range)?;
            video_replace_range(lowerer, node, base, replacement, range)
        }
        ValueType::Audio => {
            let range = resolve_audio_range(lowerer, node, range)?;
            audio_replace_range(lowerer, node, base, replacement, range)
        }
    }
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
                    .map(|(domain, _)| domain.frames().0)
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
    let (start, end) = resolve_timeline_range(
        range,
        |expression: &crate::model::TimelineExpression| {
            expression.resolve_frame_boundary(
                fps,
                |value| {
                    let prepared = lowerer.prepared_dependency(value, node.origin())?;
                    lowerer
                        .video_domain(prepared, node.origin())
                        .map(|(domain, _)| domain.frames().0)
                },
                &node.origin().span,
            )
        },
        &node.origin().span,
    )?;
    Ok(FrameRange::new(start, end).expect("resolved timeline range is nonempty"))
}

fn resolve_audio_range(
    lowerer: &PreflightLowerer<'_>,
    node: &CompiledNode,
    range: &TimelineRangeExpression,
) -> Result<SampleRange> {
    let sample_rate = lowerer.compiled.audio().sample_rate();
    let (start, end) = resolve_timeline_range(
        range,
        |expression: &crate::model::TimelineExpression| {
            expression.resolve_sample_boundary(
                sample_rate,
                |value| {
                    let prepared = lowerer.prepared_dependency(value, node.origin())?;
                    lowerer
                        .audio_domain(prepared, node.origin())
                        .map(|domain| domain.samples())
                },
                &node.origin().span,
            )
        },
        &node.origin().span,
    )?;
    Ok(SampleRange::new(start, end).expect("resolved timeline range is nonempty"))
}

fn resolve_timeline_range(
    range: &TimelineRangeExpression,
    mut resolve: impl FnMut(&crate::model::TimelineExpression) -> Result<u64>,
    span: &SourceSpan,
) -> Result<(u64, u64)> {
    let start = resolve(&range.start)?;
    let end = resolve(&range.end)?;
    if start < end {
        Ok((start, end))
    } else {
        Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidTimeRange,
            "timeline-range start must be earlier than its end",
            span.clone(),
        ))
    }
}

fn video_slice(
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

fn video_replace_range(
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
        add_video_concat(lowerer, node, pieces)
    }
}

fn audio_replace_range(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    base: ValueRef,
    replacement: ValueRef,
    range: SampleRange,
) -> Result<NodeId> {
    let base_node = lowerer.prepared_dependency(base, node.origin())?;
    let replacement_node = lowerer.prepared_dependency(replacement, node.origin())?;
    let base_domain = *lowerer.audio_domain(base_node, node.origin())?;
    validate_prepared_audio_range(range, &base_domain, &node.origin().span)?;
    let mut pieces = Vec::new();
    if range.start() > 0 {
        let prefix = SampleRange::new(0, range.start()).expect("nonempty during prefix");
        pieces.push(lowerer.add_audio_node(
            PreparedAudioKind::AudioSlice {
                input: base_node,
                range: prefix,
            },
            AudioDomain::new(prefix.samples(), base_domain.audio_spec()),
            node.semantic_version(),
            node.origin().clone_with_construct("range prefix"),
        )?);
    }
    pieces.push(replacement_node);
    if range.end() < base_domain.samples() {
        let suffix =
            SampleRange::new(range.end(), base_domain.samples()).expect("nonempty during suffix");
        pieces.push(lowerer.add_audio_node(
            PreparedAudioKind::AudioSlice {
                input: base_node,
                range: suffix,
            },
            AudioDomain::new(suffix.samples(), base_domain.audio_spec()),
            node.semantic_version(),
            node.origin().clone_with_construct("range suffix"),
        )?);
    }
    if pieces.len() == 1 {
        Ok(pieces[0])
    } else {
        add_audio_concat(lowerer, node, pieces)
    }
}

fn validate_prepared_range(
    range: FrameRange,
    input: &VideoDomain,
    span: &SourceSpan,
) -> Result<()> {
    if range.end() > input.frames().0 {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidTimeRange,
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

fn validate_prepared_audio_range(
    range: SampleRange,
    input: &AudioDomain,
    span: &SourceSpan,
) -> Result<()> {
    if range.end() > input.samples() {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidTimeRange,
            format!(
                "sample range {}..{} is outside the base Audio domain of {} samples",
                range.start(),
                range.end(),
                input.samples()
            ),
            span.clone(),
        ));
    }
    Ok(())
}
