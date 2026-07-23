use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, ImageFit, ValueRef, ValueType};
use crate::program::{
    Cardinality, InputPort, ParameterDescriptor, ParameterType, ProgramDefinition,
    ProgramDescriptor, ProgramImplementation, ResolvedCall,
};
use crate::semantic::GraphBuilder;

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
        parameter_type: ParameterType::Keyword(&["cover", "contain", "stretch"]),
        required: false,
    },
];
const VIDEO_PARAMETERS: &[ParameterDescriptor] = &[
    ParameterDescriptor {
        name: "path",
        parameter_type: ParameterType::File,
        required: true,
    },
    ParameterDescriptor {
        name: "fit",
        parameter_type: ParameterType::Keyword(&["cover", "contain", "stretch"]),
        required: false,
    },
];
const REPEAT_PARAMETERS: &[ParameterDescriptor] = &[ParameterDescriptor {
    name: "count",
    parameter_type: ParameterType::Integer,
    required: true,
}];

pub(crate) const IMAGE: ProgramDefinition = direct(
    ProgramDescriptor {
        name: "image",
        semantic_version: 1,
        inputs: NO_INPUTS,
        parameters: IMAGE_PARAMETERS,
        primary_parameter: Some("path"),
        output: VIDEO,
    },
    lower_image,
);

pub(crate) const VIDEO_SOURCE: ProgramDefinition = direct(
    ProgramDescriptor {
        name: "video",
        semantic_version: 1,
        inputs: NO_INPUTS,
        parameters: VIDEO_PARAMETERS,
        primary_parameter: Some("path"),
        output: VIDEO,
    },
    lower_video,
);

pub(crate) const CONCAT: ProgramDefinition = direct(
    ProgramDescriptor {
        name: "concat",
        semantic_version: 1,
        inputs: VIDEOS,
        parameters: &[],
        primary_parameter: None,
        output: VIDEO,
    },
    lower_concat,
);

pub(crate) const REPEAT: ProgramDefinition = direct(
    ProgramDescriptor {
        name: "repeat",
        semantic_version: 1,
        inputs: ONE_VIDEO,
        parameters: REPEAT_PARAMETERS,
        primary_parameter: Some("count"),
        output: VIDEO,
    },
    lower_repeat,
);

const fn direct(
    descriptor: ProgramDescriptor,
    lower: crate::program::DirectLowerFn,
) -> ProgramDefinition {
    ProgramDefinition {
        descriptor,
        implementation: ProgramImplementation::Direct(lower),
        postfix: None,
    }
}

fn lower_image(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ValueRef> {
    let (path, _) = call.file_parameter("path")?;
    let frames = if let Some((duration, span)) = call.optional_duration_parameter("duration")? {
        FrameCount(duration.to_frames(builder.video_spec().fps, span)?)
    } else {
        call.requested_frames().ok_or_else(|| {
            Diagnostic::new(
                "E_MISSING_IMAGE_DURATION",
                "`image.duration` is required outside a context with a requested duration",
                call.origin().span.clone(),
            )
        })?
    };
    if frames.0 == 0 {
        return Err(Diagnostic::new(
            "E_INVALID_DURATION",
            "image duration must contain at least one frame",
            call.origin().span.clone(),
        ));
    }
    let fit = image_fit(call)?;
    builder.image_video(path.to_path_buf(), frames, fit)
}

fn lower_video(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ValueRef> {
    let (path, _) = call.file_parameter("path")?;
    builder.video_source(path.to_path_buf(), image_fit(call)?)
}

fn image_fit(call: &ResolvedCall) -> Result<ImageFit> {
    Ok(match call.optional_keyword_parameter("fit")? {
        None | Some(("cover", _)) => ImageFit::Cover,
        Some(("contain", _)) => ImageFit::Contain,
        Some(("stretch", _)) => ImageFit::Stretch,
        Some((_, _)) => unreachable!("fit keyword was validated by the binder"),
    })
}

fn lower_concat(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ValueRef> {
    builder.concat(call.variadic_input("videos")?.to_vec())
}

fn lower_repeat(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ValueRef> {
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
    builder.concat(inputs)
}
