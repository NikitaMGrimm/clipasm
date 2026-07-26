use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, ValueType};
use crate::program::{Cardinality, ParameterType, ProgramDefinition, ProgramOutputs, ResolvedCall};
use crate::semantic::GraphBuilder;

use super::support::{direct_with_timeline, exact_descriptor, input, one_output, parameter};

const DEFAULT_FLASH_CUT_MILLISECONDS: u64 = 160;
const DEFAULT_CROSSFADE_MILLISECONDS: u64 = 500;

pub(super) fn crossfade() -> ProgramDefinition {
    direct_with_timeline(
        exact_descriptor(
            "crossfade",
            1,
            vec![
                input("before", ValueType::Video, Cardinality::One),
                input("after", ValueType::Video, Cardinality::One),
            ],
            vec![parameter("duration", ParameterType::Duration, false)],
            ValueType::Video,
        ),
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
            2,
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
    let frames = duration_frames(call, builder, DEFAULT_CROSSFADE_MILLISECONDS)?;
    one_output(builder.crossfade(before, after, frames))
}

fn lower_flash_cut(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    let before = call.one_input("before")?;
    let after = call.one_input("after")?;
    let frames = duration_frames(call, builder, DEFAULT_FLASH_CUT_MILLISECONDS)?;
    one_output(builder.flash_cut(before, after, frames))
}

fn duration_frames(
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
        return Err(Diagnostic::new(
            match call.program_name() {
                "flash_cut" => "E_INVALID_FLASH_CUT_DURATION",
                "crossfade" => "E_INVALID_CROSSFADE_DURATION",
                _ => unreachable!("only transitions with duration use this helper"),
            },
            format!(
                "`{}.duration` must cover at least one project frame",
                call.program_name()
            ),
            duration_span.clone(),
        ));
    }
    Ok(frames)
}
