use std::num::NonZeroU64;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, ImageFit, ValueRef, ValueType};
use crate::program::{
    Cardinality, InputPort, ParameterDescriptor, ParameterType, ProgramDefinition,
    ProgramDescriptor, ProgramImplementation, ProgramOutputs, ResolvedCall, StackAccess,
};
use crate::semantic::GraphBuilder;

const VIDEO: ValueType = ValueType::Video;
const VIDEO_OUTPUTS: &[ValueType] = &[VIDEO];
const NO_INPUTS: &[InputPort] = &[];
const ONE_VIDEO: &[InputPort] = &[InputPort {
    name: "video",
    value_type: VIDEO,
    cardinality: Cardinality::One,
}];
const TWO_VIDEOS: &[InputPort] = &[
    InputPort {
        name: "before",
        value_type: VIDEO,
        cardinality: Cardinality::One,
    },
    InputPort {
        name: "after",
        value_type: VIDEO,
        cardinality: Cardinality::One,
    },
];
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
const TRIM_PARAMETERS: &[ParameterDescriptor] = &[ParameterDescriptor {
    name: "range",
    parameter_type: ParameterType::TimeRange,
    required: true,
}];
const ZOOM_PARAMETERS: &[ParameterDescriptor] = &[ParameterDescriptor {
    name: "percent",
    parameter_type: ParameterType::Integer,
    required: false,
}];
const WOBBLE_PARAMETERS: &[ParameterDescriptor] = &[ParameterDescriptor {
    name: "pixels",
    parameter_type: ParameterType::Integer,
    required: false,
}];
const FLASH_PARAMETERS: &[ParameterDescriptor] = &[ParameterDescriptor {
    name: "frames",
    parameter_type: ParameterType::Integer,
    required: false,
}];
const DEFAULT_ZOOM_PERCENT: u16 = 8;
const DEFAULT_WOBBLE_PIXELS: u16 = 3;
const DEFAULT_FLASH_MILLISECONDS: u64 = 160;

pub(crate) const IMAGE: ProgramDefinition = direct(
    ProgramDescriptor {
        name: "image",
        semantic_version: 1,
        default_stack_access: StackAccess::Owned,
        inputs: NO_INPUTS,
        parameters: IMAGE_PARAMETERS,
        primary_parameter: Some("path"),
        outputs: VIDEO_OUTPUTS,
    },
    lower_image,
);

pub(crate) const VIDEO_SOURCE: ProgramDefinition = direct(
    ProgramDescriptor {
        name: "video",
        semantic_version: 2,
        default_stack_access: StackAccess::Owned,
        inputs: NO_INPUTS,
        parameters: VIDEO_PARAMETERS,
        primary_parameter: Some("path"),
        outputs: VIDEO_OUTPUTS,
    },
    lower_video,
);

pub(crate) const CONCAT: ProgramDefinition = direct(
    ProgramDescriptor {
        name: "concat",
        semantic_version: 1,
        default_stack_access: StackAccess::Owned,
        inputs: VIDEOS,
        parameters: &[],
        primary_parameter: None,
        outputs: VIDEO_OUTPUTS,
    },
    lower_concat,
);

pub(crate) const REPEAT: ProgramDefinition = direct(
    ProgramDescriptor {
        name: "repeat",
        semantic_version: 2,
        default_stack_access: StackAccess::Owned,
        inputs: ONE_VIDEO,
        parameters: REPEAT_PARAMETERS,
        primary_parameter: Some("count"),
        outputs: VIDEO_OUTPUTS,
    },
    lower_repeat,
);

pub(crate) const TRIM: ProgramDefinition = direct(
    ProgramDescriptor {
        name: "trim",
        semantic_version: 1,
        default_stack_access: StackAccess::Owned,
        inputs: ONE_VIDEO,
        parameters: TRIM_PARAMETERS,
        primary_parameter: Some("range"),
        outputs: VIDEO_OUTPUTS,
    },
    lower_trim,
);

pub(crate) const ZOOM: ProgramDefinition = direct(
    ProgramDescriptor {
        name: "zoom",
        semantic_version: 2,
        default_stack_access: StackAccess::Owned,
        inputs: ONE_VIDEO,
        parameters: ZOOM_PARAMETERS,
        primary_parameter: Some("percent"),
        outputs: VIDEO_OUTPUTS,
    },
    lower_zoom,
);

pub(crate) const WOBBLE: ProgramDefinition = direct(
    ProgramDescriptor {
        name: "wobble",
        semantic_version: 1,
        default_stack_access: StackAccess::Owned,
        inputs: ONE_VIDEO,
        parameters: WOBBLE_PARAMETERS,
        primary_parameter: Some("pixels"),
        outputs: VIDEO_OUTPUTS,
    },
    lower_wobble,
);

pub(crate) const FLASH: ProgramDefinition = direct(
    ProgramDescriptor {
        name: "flash",
        semantic_version: 1,
        default_stack_access: StackAccess::Owned,
        inputs: TWO_VIDEOS,
        parameters: FLASH_PARAMETERS,
        primary_parameter: Some("frames"),
        outputs: VIDEO_OUTPUTS,
    },
    lower_flash,
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

fn lower_image(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
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
    one_output(builder.image_video(path.to_path_buf(), frames, fit))
}

fn lower_video(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    let (path, _) = call.file_parameter("path")?;
    one_output(builder.video_source(path.to_path_buf(), image_fit(call)?))
}

fn image_fit(call: &ResolvedCall) -> Result<ImageFit> {
    Ok(match call.optional_keyword_parameter("fit")? {
        None | Some(("cover", _)) => ImageFit::Cover,
        Some(("contain", _)) => ImageFit::Contain,
        Some(("stretch", _)) => ImageFit::Stretch,
        Some((_, _)) => unreachable!("fit keyword was validated by the binder"),
    })
}

fn lower_concat(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    one_output(builder.concat(call.variadic_input("videos")?.to_vec()))
}

fn lower_repeat(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    let video = call.one_input("video")?;
    let (count, span) = call.integer_parameter("count")?;
    let count = u64::try_from(count)
        .ok()
        .and_then(NonZeroU64::new)
        .ok_or_else(|| {
            Diagnostic::new(
                "E_INVALID_REPEAT_COUNT",
                "`repeat.count` must be an integer greater than or equal to one",
                span.clone(),
            )
        })?;
    one_output(builder.repeat(video, count))
}

fn lower_trim(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    let video = call.one_input("video")?;
    let (range, span) = call.time_range_parameter("range")?;
    let range = range.to_frames(builder.video_spec().fps, span)?;
    one_output(builder.at_span(span.clone()).slice(video, range))
}

fn lower_zoom(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    let video = call.one_input("video")?;
    let percent = match call.optional_integer_parameter("percent")? {
        Some((percent, span)) => u32::try_from(percent)
            .ok()
            .filter(|percent| *percent > 0)
            .ok_or_else(|| {
                Diagnostic::new(
                    "E_INVALID_ZOOM_PERCENT",
                    "`zoom.percent` must be a positive integer representable as `u32`",
                    span.clone(),
                )
            })?,
        None => u32::from(DEFAULT_ZOOM_PERCENT),
    };
    one_output(builder.zoom(video, percent))
}

fn lower_wobble(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    let video = call.one_input("video")?;
    let (pixels, span) = match call.optional_integer_parameter("pixels")? {
        Some((pixels, span)) => (pixels, span.clone()),
        None => (i64::from(DEFAULT_WOBBLE_PIXELS), call.origin().span.clone()),
    };
    let pixels = u32::try_from(pixels)
        .ok()
        .filter(|pixels| *pixels > 0)
        .filter(|pixels| {
            let Some(padding) = pixels.checked_mul(2) else {
                return false;
            };
            padding < builder.video_spec().width
                && padding < builder.video_spec().height
                && builder.video_spec().width.checked_add(padding).is_some()
                && builder.video_spec().height.checked_add(padding).is_some()
        })
        .ok_or_else(|| {
            Diagnostic::new(
                "E_INVALID_WOBBLE_PIXELS",
                "`wobble.pixels` must be positive and small enough to fit twice within both project dimensions without overflow",
                span,
            )
        })?;
    one_output(builder.wobble(video, pixels))
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
            builder.video_spec().fps,
            &call.origin().span,
        )?,
    };
    one_output(builder.flash_join(before, after, frames))
}

fn one_output(output: Result<ValueRef>) -> Result<ProgramOutputs> {
    output.map(|value| vec![value])
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::semantic::SemanticNodeKind;

    fn compile_repeat(count: u64) -> crate::compiler::CompiledProgram {
        let workflow = crate::frontend::yaml::parse_str(
            Path::new("repeat.yaml"),
            &format!(
                "- program:\n    version: 1\n    project:\n      video: {{fps: 10}}\n\n- image: {{path: card.png, duration: 1s}}\n- repeat: {count}\n"
            ),
        )
        .expect("workflow");
        crate::compiler::compile(&workflow).expect("compile")
    }

    #[test]
    fn repeat_one_aliases_while_two_emits_one_compact_node() {
        let once = compile_repeat(1);
        assert_eq!(once.value_count(), 1);

        let twice = compile_repeat(2);
        assert_eq!(twice.value_count(), 2);
        assert!(matches!(
            twice.nodes()[1].kind(),
            SemanticNodeKind::Repeat { count, .. } if count.get() == 2
        ));
    }

    #[test]
    fn a_million_repeats_have_bounded_graph_and_json_size() {
        let compiled = compile_repeat(1_000_000);
        let json = compiled.canonical_json().expect("compiled JSON");

        assert_eq!(compiled.value_count(), 2);
        assert!(json.len() < 10_000, "compact plan was {} bytes", json.len());
    }

    #[test]
    fn source_program_versions_cover_duration_and_repeat_semantics() {
        assert_eq!(VIDEO_SOURCE.descriptor.semantic_version, 2);
        assert_eq!(REPEAT.descriptor.semantic_version, 2);
        assert_eq!(ZOOM.descriptor.semantic_version, 2);
    }
}
