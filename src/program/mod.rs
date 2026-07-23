use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::compiler::{GraphBuilder, ResolvedCall};
use crate::diagnostic::{Diagnostic, Result, SourceSpan};
use crate::model::{ImageFit, SourceTime, ValueRef, ValueType};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Cardinality {
    One,
    Variadic { min: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InputPort {
    pub(crate) name: &'static str,
    pub(crate) value_type: ValueType,
    pub(crate) cardinality: Cardinality,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ParameterType {
    #[allow(dead_code)]
    String,
    Integer,
    File,
    Duration,
    Enum(&'static [&'static str]),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ParameterDescriptor {
    pub(crate) name: &'static str,
    pub(crate) parameter_type: ParameterType,
    pub(crate) required: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProgramDescriptor {
    pub(crate) name: &'static str,
    pub(crate) version: u32,
    pub(crate) inputs: &'static [InputPort],
    pub(crate) parameters: &'static [ParameterDescriptor],
    pub(crate) primary_parameter: Option<&'static str>,
    pub(crate) output: ValueType,
}

pub(crate) type LowerFn =
    for<'call, 'graph> fn(&ResolvedCall<'call>, &mut GraphBuilder<'graph>) -> Result<ValueRef>;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProgramDefinition {
    pub(crate) descriptor: ProgramDescriptor,
    pub(crate) lower: LowerFn,
}

const VIDEO: ValueType = ValueType::Video;
const NO_INPUTS: &[InputPort] = &[];
const ONE_VIDEO: &[InputPort] = &[InputPort {
    name: "video",
    value_type: VIDEO,
    cardinality: Cardinality::One,
}];
const VIDEOS: &[InputPort] = &[InputPort {
    name: "videos",
    value_type: VIDEO,
    cardinality: Cardinality::Variadic { min: 1 },
}];
const IMAGE_PARAMETERS: &[ParameterDescriptor] = &[
    ParameterDescriptor {
        name: "path",
        parameter_type: ParameterType::File,
        required: true,
    },
    ParameterDescriptor {
        name: "duration",
        parameter_type: ParameterType::Duration,
        required: false,
    },
    ParameterDescriptor {
        name: "fit",
        parameter_type: ParameterType::Enum(&["cover", "contain", "stretch"]),
        required: false,
    },
];
const REPEAT_PARAMETERS: &[ParameterDescriptor] = &[ParameterDescriptor {
    name: "count",
    parameter_type: ParameterType::Integer,
    required: true,
}];

pub(crate) const IMAGE: ProgramDefinition = ProgramDefinition {
    descriptor: ProgramDescriptor {
        name: "image",
        version: 1,
        inputs: NO_INPUTS,
        parameters: IMAGE_PARAMETERS,
        primary_parameter: Some("path"),
        output: VIDEO,
    },
    lower: lower_image,
};
pub(crate) const CONCAT: ProgramDefinition = ProgramDefinition {
    descriptor: ProgramDescriptor {
        name: "concat",
        version: 1,
        inputs: VIDEOS,
        parameters: &[],
        primary_parameter: None,
        output: VIDEO,
    },
    lower: lower_concat,
};
pub(crate) const REPEAT: ProgramDefinition = ProgramDefinition {
    descriptor: ProgramDescriptor {
        name: "repeat",
        version: 1,
        inputs: ONE_VIDEO,
        parameters: REPEAT_PARAMETERS,
        primary_parameter: Some("count"),
        output: VIDEO,
    },
    lower: lower_repeat,
};

pub(crate) static BUILTIN_PROGRAMS: [ProgramDefinition; 3] = [IMAGE, CONCAT, REPEAT];

const RESERVED_PROGRAM_NAMES: &[&str] =
    &["then", "during", "join", "timeline", "ref", "id", "clip"];

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProgramRegistry {
    definitions: &'static [ProgramDefinition],
}

impl Default for ProgramRegistry {
    fn default() -> Self {
        Self::from_definitions(&BUILTIN_PROGRAMS).expect("built-in program definitions are valid")
    }
}

impl ProgramRegistry {
    pub(crate) fn from_definitions(definitions: &'static [ProgramDefinition]) -> Result<Self> {
        validate_definitions(definitions)?;
        Ok(Self { definitions })
    }

    #[must_use]
    pub(crate) fn get(self, name: &str) -> Option<&'static ProgramDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.descriptor.name == name)
    }
}

fn validate_definitions(definitions: &[ProgramDefinition]) -> Result<()> {
    let mut programs = BTreeSet::new();
    for definition in definitions {
        let descriptor = &definition.descriptor;
        validate_definition_name("program", descriptor.name)?;
        if RESERVED_PROGRAM_NAMES.contains(&descriptor.name) {
            return Err(definition_error(format!(
                "program name `{}` is reserved by the language",
                descriptor.name
            )));
        }
        if !programs.insert(descriptor.name) {
            return Err(definition_error(format!(
                "duplicate program name `{}`",
                descriptor.name
            )));
        }

        let mut arguments = BTreeSet::new();
        let mut variadic_index = None;
        for (index, port) in descriptor.inputs.iter().enumerate() {
            validate_definition_name("input port", port.name)?;
            if !arguments.insert(port.name) {
                return Err(definition_error(format!(
                    "program `{}` has duplicate or colliding argument name `{}`",
                    descriptor.name, port.name
                )));
            }
            if matches!(port.cardinality, Cardinality::Variadic { .. })
                && variadic_index.replace(index).is_some()
            {
                return Err(definition_error(format!(
                    "program `{}` has more than one variadic input port",
                    descriptor.name
                )));
            }
        }
        for parameter in descriptor.parameters {
            validate_definition_name("parameter", parameter.name)?;
            if !arguments.insert(parameter.name) {
                return Err(definition_error(format!(
                    "program `{}` has duplicate or colliding argument name `{}`",
                    descriptor.name, parameter.name
                )));
            }
        }
        if let Some(primary) = descriptor.primary_parameter
            && !descriptor
                .parameters
                .iter()
                .any(|parameter| parameter.name == primary)
        {
            return Err(definition_error(format!(
                "program `{}` names nonexistent primary parameter `{primary}`",
                descriptor.name
            )));
        }
        if let Some(index) = variadic_index
            && index != 0
            && descriptor.inputs.len() > 1
        {
            return Err(definition_error(format!(
                "program `{}` must place its variadic input before fixed inputs",
                descriptor.name
            )));
        }
    }
    Ok(())
}

fn validate_definition_name(role: &str, name: &str) -> Result<()> {
    let mut characters = name.chars();
    let valid_start = characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_');
    let valid_rest = characters
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'));
    if valid_start && valid_rest {
        Ok(())
    } else {
        Err(definition_error(format!(
            "{role} name `{name}` must match [A-Za-z_][A-Za-z0-9_-]*"
        )))
    }
}

fn definition_error(message: String) -> Diagnostic {
    Diagnostic::new(
        "E_INVALID_PROGRAM_DEFINITION",
        message,
        SourceSpan::file_start("<program-registry>"),
    )
}

fn lower_image(call: &ResolvedCall<'_>, builder: &mut GraphBuilder<'_>) -> Result<ValueRef> {
    let (path, _) = call.string_parameter("path")?;
    let frames = if let Some((duration, span)) = call.optional_string_parameter("duration")? {
        SourceTime::parse(duration, span)?.to_frames(builder.video_spec().fps, span)?
    } else {
        call.requested_frames().ok_or_else(|| {
            Diagnostic::new(
                "E_MISSING_IMAGE_DURATION",
                "`image.duration` is required outside a context with a requested duration",
                call.origin().span.clone(),
            )
        })?
    };
    if frames == 0 {
        return Err(Diagnostic::new(
            "E_INVALID_DURATION",
            "image duration must contain at least one frame",
            call.origin().span.clone(),
        ));
    }
    let fit = if let Some((fit, span)) = call.optional_string_parameter("fit")? {
        ImageFit::parse(fit, span)?
    } else {
        ImageFit::Cover
    };
    builder.image_video(
        PathBuf::from(path),
        crate::model::FrameCount(frames),
        fit,
        call.definition().descriptor.version,
        call.origin().clone(),
    )
}

fn lower_concat(call: &ResolvedCall<'_>, builder: &mut GraphBuilder<'_>) -> Result<ValueRef> {
    builder.concat(
        call.variadic_input("videos")?.to_vec(),
        call.definition().descriptor.version,
        call.origin().clone(),
    )
}

fn lower_repeat(call: &ResolvedCall<'_>, builder: &mut GraphBuilder<'_>) -> Result<ValueRef> {
    let video = call.one_input("video")?;
    let (count, span) = call.integer_parameter("count")?;
    if count < 1 {
        return Err(Diagnostic::new(
            "E_INVALID_REPEAT_COUNT",
            "`repeat.count` must be an integer greater than or equal to one",
            span.clone(),
        ));
    }
    let count = usize::try_from(count).map_err(|_| {
        Diagnostic::new(
            "E_INVALID_REPEAT_COUNT",
            "`repeat.count` is too large",
            span.clone(),
        )
    })?;
    let mut inputs = Vec::new();
    inputs.try_reserve_exact(count).map_err(|_| {
        Diagnostic::new(
            "E_INVALID_REPEAT_COUNT",
            "`repeat.count` is too large to compile",
            span.clone(),
        )
    })?;
    inputs.resize(count, video);
    builder.concat(
        inputs,
        call.definition().descriptor.version,
        call.origin().clone_with_construct("repeat"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUPLICATE_PORTS: &[InputPort] = &[
        InputPort {
            name: "video",
            value_type: ValueType::Video,
            cardinality: Cardinality::One,
        },
        InputPort {
            name: "video",
            value_type: ValueType::Video,
            cardinality: Cardinality::One,
        },
    ];
    const COLLIDING_PARAMETER: &[ParameterDescriptor] = &[ParameterDescriptor {
        name: "video",
        parameter_type: ParameterType::String,
        required: false,
    }];
    const DUPLICATE_PARAMETERS: &[ParameterDescriptor] = &[
        ParameterDescriptor {
            name: "path",
            parameter_type: ParameterType::File,
            required: true,
        },
        ParameterDescriptor {
            name: "path",
            parameter_type: ParameterType::String,
            required: false,
        },
    ];
    const MULTIPLE_VARIADICS: &[InputPort] = &[
        InputPort {
            name: "left",
            value_type: ValueType::Video,
            cardinality: Cardinality::Variadic { min: 1 },
        },
        InputPort {
            name: "right",
            value_type: ValueType::Video,
            cardinality: Cardinality::Variadic { min: 1 },
        },
    ];
    const BAD_VARIADIC_ORDER: &[InputPort] = &[
        InputPort {
            name: "head",
            value_type: ValueType::Video,
            cardinality: Cardinality::One,
        },
        InputPort {
            name: "tail",
            value_type: ValueType::Video,
            cardinality: Cardinality::Variadic { min: 1 },
        },
    ];

    fn definition(
        name: &'static str,
        inputs: &'static [InputPort],
        parameters: &'static [ParameterDescriptor],
        primary_parameter: Option<&'static str>,
    ) -> ProgramDefinition {
        ProgramDefinition {
            descriptor: ProgramDescriptor {
                name,
                version: 1,
                inputs,
                parameters,
                primary_parameter,
                output: ValueType::Video,
            },
            lower: lower_image,
        }
    }

    #[test]
    fn rejects_duplicate_program_names() {
        let definitions = Box::leak(
            vec![
                definition("duplicate", &[], &[], None),
                definition("duplicate", &[], &[], None),
            ]
            .into_boxed_slice(),
        );
        let error = ProgramRegistry::from_definitions(definitions).expect_err("duplicate program");
        assert_eq!(error.code, "E_INVALID_PROGRAM_DEFINITION");
        assert!(error.message.contains("duplicate program"));
    }

    #[test]
    fn rejects_every_reserved_program_name() {
        for name in ["then", "during", "join", "timeline", "ref", "id", "clip"] {
            let definitions = Box::leak(vec![definition(name, &[], &[], None)].into_boxed_slice());
            let error =
                ProgramRegistry::from_definitions(definitions).expect_err("reserved program");
            assert_eq!(error.code, "E_INVALID_PROGRAM_DEFINITION");
            assert!(error.message.contains(name));
        }
    }

    #[test]
    fn rejects_invalid_descriptor_argument_layouts() {
        for invalid in [
            definition("duplicate_ports", DUPLICATE_PORTS, &[], None),
            definition(
                "collision",
                &DUPLICATE_PORTS[..1],
                COLLIDING_PARAMETER,
                None,
            ),
            definition("duplicate_parameters", &[], DUPLICATE_PARAMETERS, None),
            definition("missing_primary", &[], &[], Some("path")),
            definition("multiple_variadics", MULTIPLE_VARIADICS, &[], None),
            definition("bad_variadic", BAD_VARIADIC_ORDER, &[], None),
        ] {
            let definitions = Box::leak(vec![invalid].into_boxed_slice());
            let error =
                ProgramRegistry::from_definitions(definitions).expect_err("invalid descriptor");
            assert_eq!(error.code, "E_INVALID_PROGRAM_DEFINITION");
        }
    }
}
