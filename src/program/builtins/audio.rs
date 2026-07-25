use crate::diagnostic::Result;
use crate::model::ValueType;
use crate::program::{Cardinality, ProgramDefinition, ProgramOutputs, ResolvedCall};
use crate::semantic::GraphBuilder;

use super::support::{direct, exact_descriptor, input, one_output};

pub(super) fn extract_audio() -> ProgramDefinition {
    direct(
        exact_descriptor(
            "extract_audio",
            1,
            vec![input("video", ValueType::Video, Cardinality::One)],
            vec![],
            ValueType::Audio,
        ),
        lower_extract_audio,
    )
}

pub(super) fn set_audio() -> ProgramDefinition {
    direct(
        exact_descriptor(
            "set_audio",
            1,
            vec![
                input("audio", ValueType::Audio, Cardinality::One),
                input("video", ValueType::Video, Cardinality::One),
            ],
            vec![],
            ValueType::Video,
        ),
        lower_set_audio,
    )
}

fn lower_extract_audio(
    call: &ResolvedCall,
    builder: &mut GraphBuilder<'_>,
) -> Result<ProgramOutputs> {
    one_output(builder.extract_audio(call.one_input("video")?))
}

fn lower_set_audio(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    one_output(builder.set_audio(call.one_input("audio")?, call.one_input("video")?))
}
