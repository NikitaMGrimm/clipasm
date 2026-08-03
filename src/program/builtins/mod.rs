mod audio;
mod body;
mod effects;
mod sources;
mod support;
mod timeline;
mod transitions;

use std::collections::BTreeSet;

use crate::diagnostic::{BuiltinDiagnostic, Result};
use crate::model::{ExactNumber, ValueType};

use super::{
    BodyOutputConstraint, ParameterType, ProgramDefinition, ProgramImplementation, ValueTypeSpec,
    definition_error, validate_definitions,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BuiltinCategory {
    Sources,
    Timeline,
    Audio,
    Effects,
    Transitions,
    BodyPrograms,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BuiltinDefault {
    NumberRatio { numerator: i64, denominator: i64 },
    DurationMilliseconds(u64),
    Keyword(&'static str),
}

impl BuiltinDefault {
    pub(crate) fn number(self) -> ExactNumber {
        let Self::NumberRatio {
            numerator,
            denominator,
        } = self
        else {
            unreachable!("built-in default is not a Number")
        };
        ExactNumber::from_ratio(numerator, denominator)
    }

    pub(crate) fn duration_milliseconds(self) -> u64 {
        let Self::DurationMilliseconds(milliseconds) = self else {
            unreachable!("built-in default is not a Duration")
        };
        milliseconds
    }

    pub(crate) fn keyword(self) -> &'static str {
        let Self::Keyword(value) = self else {
            unreachable!("built-in default is not a Keyword")
        };
        value
    }
}

pub(super) const DEFAULT_FIT: BuiltinDefault = BuiltinDefault::Keyword("cover");
pub(super) const DEFAULT_ZOOM_BY: BuiltinDefault = BuiltinDefault::NumberRatio {
    numerator: 2,
    denominator: 25,
};
pub(super) const DEFAULT_FLASH_CUT_DURATION: BuiltinDefault =
    BuiltinDefault::DurationMilliseconds(160);
pub(super) const DEFAULT_CROSSFADE_DURATION: BuiltinDefault =
    BuiltinDefault::DurationMilliseconds(500);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BuiltinParameterDefault {
    pub(crate) parameter: &'static str,
    pub(crate) value: BuiltinDefault,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BuiltinParameterOmission {
    pub(crate) parameter: &'static str,
    pub(crate) behavior: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BuiltinBodyInitialValue {
    Input(&'static str),
    SelectedRange {
        input: &'static str,
        parameter: &'static str,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct BuiltinMetadata {
    pub(crate) category: BuiltinCategory,
    pub(crate) summary: &'static str,
    pub(crate) defaults: &'static [BuiltinParameterDefault],
    pub(crate) parameter_omissions: &'static [BuiltinParameterOmission],
    pub(crate) body_initial_values: &'static [BuiltinBodyInitialValue],
    pub(crate) example: &'static str,
    pub(crate) example_expected_outputs: Option<&'static [ValueType]>,
    pub(crate) example_expected_frames: Option<u64>,
    pub(crate) diagnostics: &'static [BuiltinDiagnostic],
    pub(crate) behavior_notes: &'static [&'static str],
    pub(crate) constraints: &'static [&'static str],
    pub(crate) related_programs: &'static [&'static str],
}

impl BuiltinMetadata {
    fn new(category: BuiltinCategory, summary: &'static str, example: &'static str) -> Self {
        Self {
            category,
            summary,
            defaults: &[],
            parameter_omissions: &[],
            body_initial_values: &[],
            example,
            example_expected_outputs: None,
            example_expected_frames: None,
            diagnostics: &[],
            behavior_notes: &[],
            constraints: &[],
            related_programs: &[],
        }
    }

    fn with_defaults(mut self, defaults: &'static [BuiltinParameterDefault]) -> Self {
        self.defaults = defaults;
        self
    }

    fn with_parameter_omissions(
        mut self,
        parameter_omissions: &'static [BuiltinParameterOmission],
    ) -> Self {
        self.parameter_omissions = parameter_omissions;
        self
    }

    fn with_body_initial_values(
        mut self,
        body_initial_values: &'static [BuiltinBodyInitialValue],
    ) -> Self {
        self.body_initial_values = body_initial_values;
        self
    }

    fn with_diagnostics(mut self, diagnostics: &'static [BuiltinDiagnostic]) -> Self {
        self.diagnostics = diagnostics;
        self
    }

    fn with_example_expected_frames(mut self, frames: u64) -> Self {
        self.example_expected_frames = Some(frames);
        self
    }

    fn with_example_expected_outputs(mut self, outputs: &'static [ValueType]) -> Self {
        self.example_expected_outputs = Some(outputs);
        self
    }

    fn with_behavior_notes(mut self, behavior_notes: &'static [&'static str]) -> Self {
        self.behavior_notes = behavior_notes;
        self
    }

    fn with_constraints(mut self, constraints: &'static [&'static str]) -> Self {
        self.constraints = constraints;
        self
    }

    fn with_related_programs(mut self, related_programs: &'static [&'static str]) -> Self {
        self.related_programs = related_programs;
        self
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BuiltinProgram {
    pub(crate) definition: ProgramDefinition,
    pub(crate) metadata: BuiltinMetadata,
}

impl BuiltinProgram {
    fn new(definition: ProgramDefinition, metadata: BuiltinMetadata) -> Self {
        Self {
            definition,
            metadata,
        }
    }

    fn into_definition(self) -> ProgramDefinition {
        self.definition
    }
}

const FIT_DEFAULT: [BuiltinParameterDefault; 1] = [BuiltinParameterDefault {
    parameter: "fit",
    value: DEFAULT_FIT,
}];
const IMAGE_PARAMETER_OMISSIONS: [BuiltinParameterOmission; 1] = [BuiltinParameterOmission {
    parameter: "duration",
    behavior: "uses a requested Video extent from the surrounding body. Without one, the call reports a missing image duration",
}];
const ZOOM_DEFAULT: [BuiltinParameterDefault; 1] = [BuiltinParameterDefault {
    parameter: "by",
    value: DEFAULT_ZOOM_BY,
}];
const FLASH_CUT_DEFAULT: [BuiltinParameterDefault; 1] = [BuiltinParameterDefault {
    parameter: "duration",
    value: DEFAULT_FLASH_CUT_DURATION,
}];
const CROSSFADE_DEFAULT: [BuiltinParameterDefault; 1] = [BuiltinParameterDefault {
    parameter: "duration",
    value: DEFAULT_CROSSFADE_DURATION,
}];
const JOIN_INITIAL_VALUES: [BuiltinBodyInitialValue; 2] = [
    BuiltinBodyInitialValue::Input("before"),
    BuiltinBodyInitialValue::Input("after"),
];
const DURING_INITIAL_VALUES: [BuiltinBodyInitialValue; 1] =
    [BuiltinBodyInitialValue::SelectedRange {
        input: "timeline",
        parameter: "range",
    }];
const VIDEO_EXAMPLE_OUTPUT: [ValueType; 1] = [ValueType::Video];
const AUDIO_EXAMPLE_OUTPUT: [ValueType; 1] = [ValueType::Audio];
const NO_EXAMPLE_OUTPUTS: [ValueType; 0] = [];

#[expect(
    clippy::too_many_lines,
    reason = "the canonical catalog keeps all built-in entries visible together for completeness review"
)]
pub(crate) fn builtin_catalog() -> Vec<BuiltinProgram> {
    let catalog = vec![
        BuiltinProgram::new(
            sources::image(),
            BuiltinMetadata::new(
                BuiltinCategory::Sources,
                "Create a Video from an image file.",
                "image(\"assets/title.png\", 2s, contain)",
            )
            .with_defaults(&FIT_DEFAULT)
            .with_example_expected_outputs(&VIDEO_EXAMPLE_OUTPUT)
            .with_example_expected_frames(60)
            .with_parameter_omissions(&IMAGE_PARAMETER_OMISSIONS)
            .with_diagnostics(&[
                BuiltinDiagnostic::MissingImageDuration,
                BuiltinDiagnostic::InvalidDuration,
            ])
            .with_behavior_notes(&[
                "ClipAsm fits the image to the project Video dimensions.",
                "Untagged opaque RGB stills use the sRGB convention, and fitting interpolation runs in display-linear light.",
                "The `cover` mode fills the frame and crops overflow. The `contain` mode adds padding. The `stretch` mode can distort the image.",
                "A surrounding Video body may supply the requested duration when the author omits `duration`.",
            ])
            .with_constraints(&["The resolved duration must contain at least one project frame."])
            .with_related_programs(&["video", "during"]),
        ),
        BuiltinProgram::new(
            sources::video(),
            BuiltinMetadata::new(
                BuiltinCategory::Sources,
                "Load a Video from a video file.",
                "video(\"assets/scene.mp4\", contain)",
            )
            .with_defaults(&FIT_DEFAULT)
            .with_example_expected_outputs(&VIDEO_EXAMPLE_OUTPUT)
            .with_behavior_notes(&[
                "Compilation remains media-pure. Preflight probes the source and resolves its exact project-frame domain.",
                "Preflight requires complete BT.709 SDR color metadata; it does not guess missing metadata or silently tone-map HDR.",
                "Preflight fits the source to the project Video dimensions and preserves its resolved duration.",
            ])
            .with_related_programs(&["image", "extract_audio", "set_audio"]),
        ),
        BuiltinProgram::new(
            sources::audio(),
            BuiltinMetadata::new(
                BuiltinCategory::Sources,
                "Load standalone Audio from an audio file.",
                "audio(\"assets/music.wav\")",
            )
            .with_example_expected_outputs(&AUDIO_EXAMPLE_OUTPUT)
            .with_behavior_notes(&[
                "Compilation remains media-pure. Preflight probes and normalizes the source to the project Audio domain.",
            ])
            .with_related_programs(&["set_audio"]),
        ),
        BuiltinProgram::new(
            audio::extract_audio(),
            BuiltinMetadata::new(
                BuiltinCategory::Audio,
                "Extract the meaningful Audio from a Video.",
                "video(\"assets/interview.mp4\")\nextract_audio",
            )
            .with_example_expected_outputs(&AUDIO_EXAMPLE_OUTPUT)
            .with_behavior_notes(&[
                "The standalone Audio output covers the complete Video duration on the project sample grid.",
            ])
            .with_constraints(&[
                "The Video must carry meaningful attached Audio. A silent Video cannot produce Audio content.",
            ])
            .with_related_programs(&["video", "set_audio"]),
        ),
        BuiltinProgram::new(
            audio::set_audio(),
            BuiltinMetadata::new(
                BuiltinCategory::Audio,
                "Replace a Video's Audio with standalone Audio.",
                "set_audio(\n    video=video(\"assets/scene.mp4\"),\n    audio=audio(\"assets/music.wav\"),\n)",
            )
            .with_example_expected_outputs(&VIDEO_EXAMPLE_OUTPUT)
            .with_behavior_notes(&[
                "The output preserves the Video timeline. ClipAsm marks the output as carrying meaningful Audio.",
                "The supplied standalone Audio replaces any Audio already attached to the Video.",
            ])
            .with_related_programs(&["audio", "extract_audio"]),
        ),
        BuiltinProgram::new(
            timeline::concat(),
            BuiltinMetadata::new(
                BuiltinCategory::Timeline,
                "Concatenate one or more homogeneous timelines.",
                "image(\"assets/one.png\", 1s)\nimage(\"assets/two.png\", 1s)\nconcat",
            )
            .with_example_expected_outputs(&VIDEO_EXAMPLE_OUTPUT)
            .with_example_expected_frames(60)
            .with_behavior_notes(&[
                "Every bound value must use the same inferred Video or Audio type.",
                "The program concatenates the bound values in stack order.",
                "Use `concat<Video>` or `concat<Audio>` when both homogeneous bindings are possible.",
            ])
            .with_related_programs(&["join"]),
        ),
        BuiltinProgram::new(
            timeline::repeat(),
            BuiltinMetadata::new(
                BuiltinCategory::Timeline,
                "Repeat a Video or Audio timeline.",
                "image(\"assets/card.png\", 1s)\nrepeat(3)",
            )
            .with_example_expected_outputs(&VIDEO_EXAMPLE_OUTPUT)
            .with_example_expected_frames(90)
            .with_diagnostics(&[BuiltinDiagnostic::InvalidRepeatCount])
            .with_behavior_notes(&[
                "repeat(1) is a true identity and preserves nested timeline placements.",
                "Larger counts create a new repeated timeline. Child placements are unavailable until ClipAsm supports occurrence indexing.",
            ])
            .with_constraints(&["count must be an Integer greater than or equal to one."])
            .with_related_programs(&["concat"]),
        ),
        BuiltinProgram::new(
            timeline::trim(),
            BuiltinMetadata::new(
                BuiltinCategory::Timeline,
                "Keep a selected range of a Video or Audio timeline.",
                "video(\"assets/scene.mp4\")\ntrim(1s..3s)",
            )
            .with_example_expected_outputs(&VIDEO_EXAMPLE_OUTPUT)
            .with_example_expected_frames(60)
            .with_diagnostics(&[BuiltinDiagnostic::InvalidTimeRange])
            .with_behavior_notes(&[
                "ClipAsm accepts absolute ranges and rooted timeline-marker ranges for both Video and Audio.",
                "ClipAsm preserves and rebases complete child placements inside the selected range.",
                "ClipAsm omits partial or uncertain placements.",
                "Media-dependent marker boundaries remain deferred until preflight resolves the source domain.",
            ])
            .with_constraints(&[
                "The range must be nonempty, native-grid aligned, within the bound timeline, and owned by that timeline.",
            ])
            .with_related_programs(&["during"]),
        ),
        BuiltinProgram::new(
            timeline::drop_value(),
            BuiltinMetadata::new(
                BuiltinCategory::Timeline,
                "Remove one Video or Audio value from the stack.",
                "audio(\"assets/music.wav\")\ndrop",
            )
            .with_example_expected_outputs(&NO_EXAMPLE_OUTPUTS)
            .with_behavior_notes(&[
                "The program consumes the bound value from the stack and produces no output value.",
            ]),
        ),
        BuiltinProgram::new(
            effects::zoom_in(),
            BuiltinMetadata::new(
                BuiltinCategory::Effects,
                "Apply a linear zoom-in effect to a Video.",
                "image(\"assets/card.png\", 2s)\nzoom_in(12%)",
            )
            .with_defaults(&ZOOM_DEFAULT)
            .with_example_expected_outputs(&VIDEO_EXAMPLE_OUTPUT)
            .with_example_expected_frames(60)
            .with_diagnostics(&[BuiltinDiagnostic::InvalidZoomAmount])
            .with_behavior_notes(&[
                "For a multi-frame Video, scale increases linearly from 100% on the first frame to exactly 100% + by on the last frame.",
                "The program preserves the Video timeline and the attached meaningful-Audio state.",
            ])
            .with_constraints(&["by must be positive."]),
        ),
        BuiltinProgram::new(
            transitions::flash_cut(),
            BuiltinMetadata::new(
                BuiltinCategory::Transitions,
                "Join two Videos with a brief white-flash transition.",
                "image(\"assets/before.png\", 2s)\nimage(\"assets/after.png\", 2s)\nflash_cut",
            )
            .with_defaults(&FLASH_CUT_DEFAULT)
            .with_example_expected_outputs(&VIDEO_EXAMPLE_OUTPUT)
            .with_example_expected_frames(120)
            .with_diagnostics(&[BuiltinDiagnostic::InvalidFlashCutDuration])
            .with_behavior_notes(&[
                "duration becomes the smallest whole project-frame count that covers the authored duration.",
                "The white fade is evaluated in display-linear BT.709 RGB.",
                "The output exposes sequential before and after timeline regions.",
            ])
            .with_constraints(&["duration must cover at least one project frame."])
            .with_related_programs(&["crossfade"]),
        ),
        BuiltinProgram::new(
            transitions::crossfade(),
            BuiltinMetadata::new(
                BuiltinCategory::Transitions,
                "Overlap two Videos or Audio values with a crossfade transition.",
                "image(\"assets/before.png\", 2s)\nimage(\"assets/after.png\", 2s)\ncrossfade",
            )
            .with_defaults(&CROSSFADE_DEFAULT)
            .with_example_expected_outputs(&VIDEO_EXAMPLE_OUTPUT)
            .with_example_expected_frames(105)
            .with_diagnostics(&[
                BuiltinDiagnostic::InvalidCrossfadeDuration,
                BuiltinDiagnostic::CrossfadeAudioDuration,
            ])
            .with_behavior_notes(&[
                "For Video, duration becomes the smallest whole project-frame count that covers the authored duration; for Audio, it becomes the smallest whole project-sample count.",
                "Video pictures blend in display-linear BT.709 RGB, while standalone and attached Audio use equal-power fade curves.",
                "The output exposes before, overlap, and after timeline regions.",
            ])
            .with_constraints(&[
                "before and after must have the same Video or Audio type.",
                "duration must cover at least one native frame or sample and cannot exceed either input.",
            ])
            .with_related_programs(&["flash_cut"]),
        ),
        BuiltinProgram::new(
            body::join(),
            BuiltinMetadata::new(
                BuiltinCategory::BodyPrograms,
                "Transform and concatenate two Video or Audio timelines in a body.",
                "image(\"assets/before.png\", 1s)\nimage(\"assets/after.png\", 1s)\njoin {\n    zoom_in(4%)\n}",
            )
            .with_body_initial_values(&JOIN_INITIAL_VALUES)
            .with_example_expected_outputs(&VIDEO_EXAMPLE_OUTPUT)
            .with_example_expected_frames(60)
            .with_diagnostics(&[BuiltinDiagnostic::EmptyJoin])
            .with_behavior_notes(&[
                "The body starts with `before` followed by `after`.",
                "The body exposes the inputs as the lexical `$before` and `$after` references.",
                "ClipAsm concatenates each homogeneous value from the body into one output timeline.",
                "Named values created by the body remain addressable as placements in the result.",
            ])
            .with_constraints(&[
                "The body must leave at least one value of the selected homogeneous Video or Audio type.",
            ])
            .with_related_programs(&["concat", "during"]),
        ),
        BuiltinProgram::new(
            body::during(),
            BuiltinMetadata::new(
                BuiltinCategory::BodyPrograms,
                "Replace a selected timeline range with the result of a body.",
                "image(\"assets/card.png\", 3s)\nduring(1s..2s) {\n    zoom_in(4%)\n}",
            )
            .with_body_initial_values(&DURING_INITIAL_VALUES)
            .with_example_expected_outputs(&VIDEO_EXAMPLE_OUTPUT)
            .with_example_expected_frames(90)
            .with_diagnostics(&[BuiltinDiagnostic::BodyOutputCount])
            .with_behavior_notes(&[
                "The body starts with the selected range.",
                "The body exposes the complete bound input as the lexical `$timeline` reference.",
                "The body must return exactly one matching value. ClipAsm inserts that value into the original timeline.",
                "ClipAsm preserves or shifts placements before and after the range.",
                "ClipAsm omits intersecting or uncertain placements. The `replacement` name identifies the inserted body.",
                "A Video selection supplies its requested extent when the author omits the image call's `duration`.",
            ])
            .with_constraints(&[
                "The range must be native-grid aligned, within the bound timeline, and owned by that timeline.",
                "Use `during<Video>` or `during<Audio>` when a mixed stack makes the generic type ambiguous.",
            ])
            .with_related_programs(&["trim", "join", "image"]),
        ),
    ];
    validate_builtin_catalog(&catalog).expect("built-in catalog is valid");
    catalog
}

pub(crate) fn builtin_programs() -> Vec<ProgramDefinition> {
    builtin_catalog()
        .into_iter()
        .map(BuiltinProgram::into_definition)
        .collect()
}

#[expect(
    clippy::too_many_lines,
    reason = "catalog validation checks each connected definition, default, body, example, and reference invariant together"
)]
fn validate_builtin_catalog(catalog: &[BuiltinProgram]) -> Result<()> {
    let definitions = catalog
        .iter()
        .map(|program| program.definition.clone())
        .collect::<Vec<_>>();
    validate_definitions(&definitions)?;

    let mut names = BTreeSet::new();
    let mut routes = BTreeSet::new();
    for program in catalog {
        let descriptor = &program.definition.descriptor;
        if !names.insert(descriptor.name.as_str()) {
            return Err(definition_error(format!(
                "duplicate built-in reference for `{}`",
                descriptor.name
            )));
        }
        let route = format!("reference/programs/{}.html", descriptor.name);
        if !routes.insert(route) {
            return Err(definition_error(format!(
                "duplicate built-in reference route for `{}`",
                descriptor.name
            )));
        }
        if program.metadata.summary.trim().is_empty() {
            return Err(definition_error(format!(
                "built-in program `{}` has no reference summary",
                descriptor.name
            )));
        }
        if program.metadata.example.trim().is_empty() {
            return Err(definition_error(format!(
                "built-in program `{}` has no reference example",
                descriptor.name
            )));
        }
        let Some(example_outputs) = program.metadata.example_expected_outputs else {
            return Err(definition_error(format!(
                "built-in program `{}` has no example output expectation",
                descriptor.name
            )));
        };
        if program.metadata.example_expected_frames.is_some()
            && example_outputs != [ValueType::Video]
        {
            return Err(definition_error(format!(
                "built-in program `{}` gives frames to an example without exactly one Video output",
                descriptor.name
            )));
        }

        let mut defaulted_parameters = BTreeSet::new();
        for default in program.metadata.defaults {
            if !defaulted_parameters.insert(default.parameter) {
                return Err(definition_error(format!(
                    "built-in program `{}` repeats the default for parameter `{}`",
                    descriptor.name, default.parameter
                )));
            }
            let parameter = descriptor
                .parameters
                .iter()
                .find(|parameter| parameter.name == default.parameter)
                .ok_or_else(|| {
                    definition_error(format!(
                        "built-in program `{}` gives unknown parameter `{}` a default",
                        descriptor.name, default.parameter
                    ))
                })?;
            if parameter.required {
                return Err(definition_error(format!(
                    "built-in program `{}` gives required parameter `{}` a default",
                    descriptor.name, default.parameter
                )));
            }
            validate_default_type(&descriptor.name, parameter, default.value)?;
        }

        let mut omitted_parameters = BTreeSet::new();
        for omission in program.metadata.parameter_omissions {
            if !omitted_parameters.insert(omission.parameter) {
                return Err(definition_error(format!(
                    "built-in program `{}` repeats omission behavior for parameter `{}`",
                    descriptor.name, omission.parameter
                )));
            }
            let parameter = descriptor
                .parameters
                .iter()
                .find(|parameter| parameter.name == omission.parameter)
                .ok_or_else(|| {
                    definition_error(format!(
                        "built-in program `{}` describes omission of unknown parameter `{}`",
                        descriptor.name, omission.parameter
                    ))
                })?;
            if parameter.required || defaulted_parameters.contains(omission.parameter) {
                return Err(definition_error(format!(
                    "built-in program `{}` gives parameter `{}` incompatible omission behavior",
                    descriptor.name, omission.parameter
                )));
            }
            if omission.behavior.trim().is_empty() {
                return Err(definition_error(format!(
                    "built-in program `{}` has empty omission behavior for parameter `{}`",
                    descriptor.name, omission.parameter
                )));
            }
        }
        for parameter in descriptor
            .parameters
            .iter()
            .filter(|parameter| !parameter.required)
        {
            if !defaulted_parameters.contains(parameter.name.as_str())
                && !omitted_parameters.contains(parameter.name.as_str())
            {
                return Err(definition_error(format!(
                    "optional built-in parameter `{}.{}` has neither a default nor omission behavior",
                    descriptor.name, parameter.name
                )));
            }
        }

        for (role, values) in [
            ("behavior note", program.metadata.behavior_notes),
            ("constraint", program.metadata.constraints),
            ("related program", program.metadata.related_programs),
        ] {
            if values.iter().any(|value| value.trim().is_empty()) {
                return Err(definition_error(format!(
                    "built-in program `{}` has empty {role} metadata",
                    descriptor.name
                )));
            }
        }

        match &program.definition.implementation {
            ProgramImplementation::Body { contract, .. } => {
                if contract.initial_values.len() != program.metadata.body_initial_values.len() {
                    return Err(definition_error(format!(
                        "built-in body program `{}` reference declares {} initial value(s), but its body contract declares {}",
                        descriptor.name,
                        program.metadata.body_initial_values.len(),
                        contract.initial_values.len()
                    )));
                }
                for (role, expected_type) in program
                    .metadata
                    .body_initial_values
                    .iter()
                    .zip(&contract.initial_values)
                {
                    validate_body_initial_value(descriptor, *role, *expected_type)?;
                }
                if matches!(
                    contract.outputs,
                    BodyOutputConstraint::Exactly(ref outputs) if outputs.is_empty()
                ) {
                    return Err(definition_error(format!(
                        "built-in body program `{}` has an empty body output contract",
                        descriptor.name
                    )));
                }
            }
            ProgramImplementation::Direct(_)
            | ProgramImplementation::ClipAsm(_)
            | ProgramImplementation::External(_) => {
                if !program.metadata.body_initial_values.is_empty() {
                    return Err(definition_error(format!(
                        "built-in direct program `{}` declares body initial values",
                        descriptor.name
                    )));
                }
            }
        }
    }
    for program in catalog {
        for related in program.metadata.related_programs {
            if !names.contains(related) {
                return Err(definition_error(format!(
                    "built-in program `{}` relates to unknown built-in program `{related}`",
                    program.definition.descriptor.name
                )));
            }
        }
    }
    Ok(())
}

fn validate_default_type(
    program: &str,
    parameter: &super::ParameterDescriptor,
    value: BuiltinDefault,
) -> Result<()> {
    let valid = match (&parameter.parameter_type, value) {
        (ParameterType::Number, BuiltinDefault::NumberRatio { denominator, .. }) => {
            denominator != 0
        }
        (ParameterType::Duration, BuiltinDefault::DurationMilliseconds(_)) => true,
        (ParameterType::Keyword(values), BuiltinDefault::Keyword(value)) => {
            values.iter().any(|candidate| candidate == value)
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(definition_error(format!(
            "built-in program `{program}` has an invalid default for parameter `{}`",
            parameter.name
        )))
    }
}

fn validate_body_initial_value(
    descriptor: &super::ProgramDescriptor,
    role: BuiltinBodyInitialValue,
    expected_type: ValueTypeSpec,
) -> Result<()> {
    let input_name = match role {
        BuiltinBodyInitialValue::Input(input) => input,
        BuiltinBodyInitialValue::SelectedRange { input, parameter } => {
            let Some(parameter) = descriptor
                .parameters
                .iter()
                .find(|candidate| candidate.name == parameter)
            else {
                return Err(definition_error(format!(
                    "built-in body program `{}` selects an unknown range parameter `{parameter}`",
                    descriptor.name
                )));
            };
            if parameter.parameter_type != ParameterType::TimeRange {
                return Err(definition_error(format!(
                    "built-in body program `{}` selects non-TimeRange parameter `{}`",
                    descriptor.name, parameter.name
                )));
            }
            input
        }
    };
    let Some(input) = descriptor
        .inputs
        .iter()
        .find(|candidate| candidate.name == input_name)
    else {
        return Err(definition_error(format!(
            "built-in body program `{}` initializes from unknown input `{input_name}`",
            descriptor.name
        )));
    };
    if input.value_type != expected_type {
        return Err(definition_error(format!(
            "built-in body program `{}` initial value from `{input_name}` does not match its body contract type",
            descriptor.name
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn catalog_has_reference_metadata_for_all_current_builtins() {
        let catalog = builtin_catalog();
        validate_builtin_catalog(&catalog).expect("valid catalog");
        assert!(
            catalog
                .iter()
                .all(|program| !program.metadata.summary.trim().is_empty())
        );
    }

    #[test]
    fn reference_metadata_cannot_change_semantic_identity_or_json() {
        fn compile(catalog: &[BuiltinProgram]) -> crate::compiler::CompiledProgram {
            let registry = crate::program::ProgramRegistry::from_definitions(
                catalog
                    .iter()
                    .map(|program| program.definition.clone())
                    .collect(),
            )
            .expect("registry");
            let package = crate::language::parse_str_with_registry(
                Path::new("reference-identity.clipasm"),
                "clipasm 1\nimage(\"card.png\", 1s)\n",
                &registry,
            )
            .expect("source");
            crate::compiler::compile_with_registry(&package, &registry).expect("compile")
        }

        let baseline_catalog = builtin_catalog();
        let baseline = compile(&baseline_catalog);
        let mut changed_catalog = baseline_catalog.clone();
        changed_catalog[0].metadata.summary =
            "Reference-only text that must never enter compiled meaning.";
        changed_catalog[0].metadata.behavior_notes =
            &["Another reference-only fact used to exercise the boundary."];
        let changed = compile(&changed_catalog);

        assert_eq!(baseline.structure_hash(), changed.structure_hash());
        assert_eq!(
            baseline.compiled_json().expect("baseline JSON"),
            changed.compiled_json().expect("changed JSON")
        );
        assert!(
            !changed
                .compiled_json()
                .expect("compiled JSON")
                .contains("Reference-only")
        );
    }
}
