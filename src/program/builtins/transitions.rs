use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{FrameCount, NativeDuration, ValueType};
use crate::program::{
    Cardinality, InputPort, ParameterType, ProgramDefinition, ProgramDescriptor, ProgramOutputs,
    ResolvedCall, StackAccess, ValueTypeSpec,
};
use crate::semantic::GraphBuilder;

use super::support::{direct_with_timeline, exact_descriptor, input, one_output, parameter};
use super::{DEFAULT_CROSSFADE_DURATION, DEFAULT_FLASH_CUT_DURATION};

pub(super) fn crossfade() -> ProgramDefinition {
    direct_with_timeline(
        ProgramDescriptor {
            name: "crossfade".to_owned(),
            semantic_version: 3,
            default_stack_access: StackAccess::Owned,
            inputs: ["before", "after"]
                .into_iter()
                .map(|name| InputPort {
                    name: name.to_owned(),
                    value_type: ValueTypeSpec::Generic,
                    cardinality: Cardinality::One,
                })
                .collect(),
            parameters: vec![parameter("duration", ParameterType::Duration, false)],
            outputs: vec![ValueTypeSpec::Generic],
        },
        lower_crossfade,
        crate::program::TimelineBehavior::Crossfade {
            before: crate::program::InputSlot::new(0),
            after: crate::program::InputSlot::new(1),
        },
    )
}

pub(super) fn flash_cut() -> ProgramDefinition {
    direct_with_timeline(
        exact_descriptor(
            "flash_cut",
            4,
            vec![
                input("before", ValueType::Video, Cardinality::One),
                input("after", ValueType::Video, Cardinality::One),
            ],
            vec![parameter("duration", ParameterType::Duration, false)],
            ValueType::Video,
        ),
        lower_flash_cut,
        crate::program::TimelineBehavior::FlashCut {
            before: crate::program::InputSlot::new(0),
            after: crate::program::InputSlot::new(1),
        },
    )
}

fn lower_crossfade(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    let before = call.one_input("before")?;
    let after = call.one_input("after")?;
    let duration = crossfade_duration(call, builder, before.value_type())?;
    one_output(builder.crossfade(before, after, duration))
}

fn lower_flash_cut(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    let before = call.one_input("before")?;
    let after = call.one_input("after")?;
    let frames = flash_cut_duration_frames(
        call,
        builder,
        DEFAULT_FLASH_CUT_DURATION.duration_milliseconds(),
    )?;
    one_output(builder.flash_cut(before, after, frames))
}

fn flash_cut_duration_frames(
    call: &ResolvedCall,
    builder: &GraphBuilder<'_>,
    default_milliseconds: u64,
) -> Result<FrameCount> {
    let (frames, duration_span) = match call.optional_duration_parameter("duration")? {
        Some((duration, span)) => (
            duration.to_covering_frames(builder.video_spec().fps(), span)?,
            span,
        ),
        None => (
            FrameCount::covering_duration(
                u128::from(default_milliseconds),
                1_000,
                builder.video_spec().fps(),
                &call.origin().span,
            )?,
            &call.origin().span,
        ),
    };
    if frames.0 == 0 {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidFlashCutDuration,
            "`flash_cut.duration` must cover at least one project frame",
            duration_span.clone(),
        ));
    }
    Ok(frames)
}

fn crossfade_duration(
    call: &ResolvedCall,
    builder: &GraphBuilder<'_>,
    value_type: ValueType,
) -> Result<NativeDuration> {
    let milliseconds = DEFAULT_CROSSFADE_DURATION.duration_milliseconds();
    let (duration, duration_span) = match call.optional_duration_parameter("duration")? {
        Some((duration, span)) => (
            match value_type {
                ValueType::Video => NativeDuration::Frames(
                    duration.to_covering_frames(builder.video_spec().fps(), span)?,
                ),
                ValueType::Audio => NativeDuration::Samples(duration.to_covering_samples(
                    *builder.video_spec(),
                    *builder.audio_spec(),
                    span,
                )?),
            },
            span,
        ),
        None => (
            match value_type {
                ValueType::Video => NativeDuration::Frames(FrameCount::covering_duration(
                    u128::from(milliseconds),
                    1_000,
                    builder.video_spec().fps(),
                    &call.origin().span,
                )?),
                ValueType::Audio => {
                    let samples =
                        u128::from(milliseconds) * u128::from(builder.audio_spec().sample_rate());
                    NativeDuration::Samples(u64::try_from(samples.div_ceil(1_000)).map_err(
                        |_| {
                            Diagnostic::builtin(
                                BuiltinDiagnostic::AudioDurationOverflow,
                                "crossfade duration exceeds the supported audio sample count",
                                call.origin().span.clone(),
                            )
                        },
                    )?)
                }
            },
            &call.origin().span,
        ),
    };
    let is_zero = match duration {
        NativeDuration::Frames(frames) => frames.0 == 0,
        NativeDuration::Samples(samples) => samples == 0,
    };
    if is_zero {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidCrossfadeDuration,
            format!(
                "`crossfade.duration` must cover at least one project {}",
                value_type.native_unit_name().trim_end_matches('s')
            ),
            duration_span.clone(),
        ));
    }
    Ok(duration)
}
