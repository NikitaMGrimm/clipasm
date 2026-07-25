use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, ValueType};
use crate::program::{Cardinality, ParameterType, ProgramDefinition, ProgramOutputs, ResolvedCall};
use crate::semantic::GraphBuilder;

use super::support::{direct, exact_descriptor, input, one_output, parameter};

const DEFAULT_FLASH_MILLISECONDS: u64 = 160;

pub(super) fn flash() -> ProgramDefinition {
    direct(
        exact_descriptor(
            "flash",
            2,
            vec![
                input("before", ValueType::Video, Cardinality::One),
                input("after", ValueType::Video, Cardinality::One),
            ],
            vec![parameter("frames", ParameterType::Integer, false)],
            ValueType::Video,
        ),
        lower_flash,
    )
}

fn lower_flash(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    let before = call.one_input("before")?;
    let after = call.one_input("after")?;
    let frames = match call.optional_integer_parameter("frames")? {
        Some((frames, span)) => FrameCount(
            u64::try_from(frames)
                .ok()
                .filter(|frames| *frames > 0)
                .ok_or_else(|| {
                    Diagnostic::new(
                        "E_INVALID_FLASH_FRAMES",
                        "`flash.frames` must be an integer greater than or equal to one",
                        span.clone(),
                    )
                })?,
        ),
        None => FrameCount::covering_duration(
            u128::from(DEFAULT_FLASH_MILLISECONDS),
            1_000,
            builder.video_spec().fps(),
            &call.origin().span,
        )?,
    };
    one_output(builder.flash_join(before, after, frames))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flash_version_covers_cut_effect_semantics() {
        assert_eq!(flash().descriptor.semantic_version, 2);
    }
}
