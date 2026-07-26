use crate::diagnostic::{Diagnostic, Result};
use crate::model::{ExactNumber, ValueType};
use crate::program::{Cardinality, ParameterType, ProgramDefinition, ProgramOutputs, ResolvedCall};
use crate::semantic::GraphBuilder;

use super::support::{direct_with_timeline, exact_descriptor, input, one_output, parameter};

pub(super) fn zoom_in() -> ProgramDefinition {
    direct_with_timeline(
        exact_descriptor(
            "zoom_in",
            4,
            vec![input("video", ValueType::Video, Cardinality::One)],
            vec![parameter("by", ParameterType::Number, false)],
            ValueType::Video,
        ),
        lower_zoom,
        crate::program::TimelineBehavior::Identity {
            input: crate::program::InputSlot::new(0),
        },
    )
}

fn lower_zoom(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    let video = call.one_input("video")?;
    let by = match call.optional_number_parameter("by")? {
        Some((by, _)) if by.is_positive() => by.clone(),
        Some((_, span)) => {
            return Err(Diagnostic::new(
                "E_INVALID_ZOOM_AMOUNT",
                "`zoom_in.by` must be positive",
                span.clone(),
            ));
        }
        None => ExactNumber::from_ratio(2, 25),
    };
    one_output(builder.zoom_in(video, by))
}
