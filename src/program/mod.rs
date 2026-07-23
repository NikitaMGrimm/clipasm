use std::path::PathBuf;

use crate::compiler::{GraphBuilder, ResolvedCall};
use crate::diagnostic::{Diagnostic, Result};
use crate::model::{ImageFit, SourceTime, ValueRef, ValueType};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cardinality {
    One,
    Variadic { min: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputPort {
    pub name: &'static str,
    pub value_type: ValueType,
    pub cardinality: Cardinality,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParameterType {
    String,
    Integer,
    File,
    Duration,
    Enum(&'static [&'static str]),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParameterDescriptor {
    pub name: &'static str,
    pub parameter_type: ParameterType,
    pub required: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgramDescriptor {
    pub name: &'static str,
    pub version: u32,
    pub inputs: &'static [InputPort],
    pub parameters: &'static [ParameterDescriptor],
    pub primary_parameter: Option<&'static str>,
    pub output: ValueType,
}

pub type LowerFn =
    for<'call, 'graph> fn(&ResolvedCall<'call>, &mut GraphBuilder<'graph>) -> Result<ValueRef>;

#[derive(Clone, Copy)]
pub struct ProgramDefinition {
    pub descriptor: ProgramDescriptor,
    pub lower: LowerFn,
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

pub const IMAGE: ProgramDefinition = ProgramDefinition {
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
pub const CONCAT: ProgramDefinition = ProgramDefinition {
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
pub const REPEAT: ProgramDefinition = ProgramDefinition {
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

pub static BUILTIN_PROGRAMS: [ProgramDefinition; 3] = [IMAGE, CONCAT, REPEAT];

#[derive(Clone, Copy)]
pub struct ProgramRegistry {
    definitions: &'static [ProgramDefinition],
}

impl Default for ProgramRegistry {
    fn default() -> Self {
        Self {
            definitions: &BUILTIN_PROGRAMS,
        }
    }
}

impl ProgramRegistry {
    #[must_use]
    pub const fn from_definitions(definitions: &'static [ProgramDefinition]) -> Self {
        Self { definitions }
    }

    #[must_use]
    pub fn get(self, name: &str) -> Option<&'static ProgramDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.descriptor.name == name)
    }

    #[must_use]
    pub const fn definitions(self) -> &'static [ProgramDefinition] {
        self.definitions
    }
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
