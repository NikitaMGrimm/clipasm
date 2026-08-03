use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::ValueType;
use crate::program::{Cardinality, ParameterType, ProgramDefinition, ProgramOutputs, ResolvedCall};
use crate::semantic::GraphBuilder;

use super::DEFAULT_ZOOM_BY;
use super::support::{direct_with_timeline, exact_descriptor, input, one_output, parameter};

pub(super) fn zoom_in() -> ProgramDefinition {
    direct_with_timeline(
        exact_descriptor(
            "zoom_in",
            6,
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
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidZoomAmount,
                "`zoom_in.by` must be positive",
                span.clone(),
            ));
        }
        None => DEFAULT_ZOOM_BY.number(),
    };
    one_output(builder.zoom_in(video, by))
}
