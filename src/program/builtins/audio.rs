use crate::diagnostic::Result;
use crate::model::ValueType;
use crate::program::{Cardinality, ProgramDefinition, ProgramOutputs, ResolvedCall};
use crate::semantic::GraphBuilder;

use super::support::{direct, direct_with_timeline, exact_descriptor, input, one_output};

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
    direct_with_timeline(
        exact_descriptor(
            "set_audio",
            1,
            vec![
                input("video", ValueType::Video, Cardinality::One),
                input("audio", ValueType::Audio, Cardinality::One),
            ],
            vec![],
            ValueType::Video,
        ),
        lower_set_audio,
        crate::program::TimelineBehavior::Identity {
            input: crate::program::InputSlot::new(0),
        },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_audio_exposes_video_before_audio() {
        let definition = set_audio();
        assert_eq!(definition.descriptor.inputs[0].name, "video");
        assert_eq!(definition.descriptor.inputs[1].name, "audio");
    }
}
