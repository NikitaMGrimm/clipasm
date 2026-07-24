use std::num::NonZeroU64;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, ImageFit, ValueRef, ValueType};
use crate::program::{
    Cardinality, InputPort, ParameterDescriptor, ParameterType, ProgramDefinition,
    ProgramDescriptor, ProgramImplementation, ProgramOutputs, ResolvedCall, StackAccess,
};
use crate::semantic::GraphBuilder;

const DEFAULT_ZOOM_PERCENT: u16 = 8;
const DEFAULT_WOBBLE_PIXELS: u16 = 3;
const DEFAULT_FLASH_MILLISECONDS: u64 = 160;

pub(crate) fn image() -> ProgramDefinition {
    direct(
        descriptor(
            "image",
            2,
            vec![],
            vec![
                parameter("path", ParameterType::File, true),
                parameter("duration", ParameterType::Duration, false),
                fit_parameter(),
            ],
            Some("path"),
        ),
        lower_image,
    )
}

pub(crate) fn video_source() -> ProgramDefinition {
    direct(
        descriptor(
            "video",
            3,
            vec![],
            vec![
                parameter("path", ParameterType::File, true),
                fit_parameter(),
            ],
            Some("path"),
        ),
        lower_video,
    )
}

pub(crate) fn audio_source() -> ProgramDefinition {
    direct(
        ProgramDescriptor {
            name: "audio".to_owned(),
            semantic_version: 1,
            default_stack_access: StackAccess::Owned,
            inputs: vec![],
            parameters: vec![parameter("path", ParameterType::File, true)],
            primary_parameter: Some("path".to_owned()),
            outputs: vec![ValueType::Audio],
        },
        lower_audio,
    )
}

pub(crate) fn extract_audio() -> ProgramDefinition {
    direct(
        ProgramDescriptor {
            name: "extract_audio".to_owned(),
            semantic_version: 1,
            default_stack_access: StackAccess::Owned,
            inputs: vec![InputPort {
                name: "video".to_owned(),
                value_type: ValueType::Video,
                cardinality: Cardinality::One,
            }],
            parameters: vec![],
            primary_parameter: None,
            outputs: vec![ValueType::Audio],
        },
        lower_extract_audio,
    )
}

pub(crate) fn set_audio() -> ProgramDefinition {
    direct(
        ProgramDescriptor {
            name: "set_audio".to_owned(),
            semantic_version: 1,
            default_stack_access: StackAccess::Owned,
            inputs: vec![
                InputPort {
                    name: "audio".to_owned(),
                    value_type: ValueType::Audio,
                    cardinality: Cardinality::One,
                },
                InputPort {
                    name: "video".to_owned(),
                    value_type: ValueType::Video,
                    cardinality: Cardinality::One,
                },
            ],
            parameters: vec![],
            primary_parameter: None,
            outputs: vec![ValueType::Video],
        },
        lower_set_audio,
    )
}

pub(crate) fn concat() -> ProgramDefinition {
    direct(
        descriptor(
            "concat",
            2,
            vec![input("videos", Cardinality::Variadic { min: 1 })],
            vec![],
            None,
        ),
        lower_concat,
    )
}

pub(crate) fn repeat() -> ProgramDefinition {
    direct(
        descriptor(
            "repeat",
            3,
            vec![input("video", Cardinality::One)],
            vec![parameter("count", ParameterType::Integer, true)],
            Some("count"),
        ),
        lower_repeat,
    )
}

pub(crate) fn trim() -> ProgramDefinition {
    direct(
        descriptor(
            "trim",
            2,
            vec![input("video", Cardinality::One)],
            vec![parameter("range", ParameterType::TimeRange, true)],
            Some("range"),
        ),
        lower_trim,
    )
}

pub(crate) fn zoom() -> ProgramDefinition {
    direct(
        descriptor(
            "zoom",
            3,
            vec![input("video", Cardinality::One)],
            vec![parameter("percent", ParameterType::Integer, false)],
            Some("percent"),
        ),
        lower_zoom,
    )
}

pub(crate) fn wobble() -> ProgramDefinition {
    direct(
        descriptor(
            "wobble",
            2,
            vec![input("video", Cardinality::One)],
            vec![parameter("pixels", ParameterType::Integer, false)],
            Some("pixels"),
        ),
        lower_wobble,
    )
}

pub(crate) fn flash() -> ProgramDefinition {
    direct(
        descriptor(
            "flash",
            2,
            vec![
                input("before", Cardinality::One),
                input("after", Cardinality::One),
            ],
            vec![parameter("frames", ParameterType::Integer, false)],
            Some("frames"),
        ),
        lower_flash,
    )
}

fn descriptor(
    name: &str,
    semantic_version: u32,
    inputs: Vec<InputPort>,
    parameters: Vec<ParameterDescriptor>,
    primary_parameter: Option<&str>,
) -> ProgramDescriptor {
    ProgramDescriptor {
        name: name.to_owned(),
        semantic_version,
        default_stack_access: StackAccess::Owned,
        inputs,
        parameters,
        primary_parameter: primary_parameter.map(str::to_owned),
        outputs: vec![ValueType::Video],
    }
}

fn input(name: &str, cardinality: Cardinality) -> InputPort {
    InputPort {
        name: name.to_owned(),
        value_type: ValueType::Video,
        cardinality,
    }
}

fn parameter(name: &str, parameter_type: ParameterType, required: bool) -> ParameterDescriptor {
    ParameterDescriptor {
        name: name.to_owned(),
        parameter_type,
        required,
    }
}

fn fit_parameter() -> ParameterDescriptor {
    parameter(
        "fit",
        ParameterType::Keyword(
            ["cover", "contain", "stretch"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        false,
    )
}

fn direct(
    descriptor: ProgramDescriptor,
    lower: crate::program::DirectLowerFn,
) -> ProgramDefinition {
    ProgramDefinition {
        descriptor,
        implementation: ProgramImplementation::Direct(lower),
        body_contract: None,
        postfix: None,
    }
}

fn lower_image(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    let (path, path_span) = call.file_parameter("path")?;
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
    one_output(
        builder
            .at_span(path_span.clone())
            .image_video(path.to_path_buf(), frames, fit),
    )
}

fn lower_video(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    let (path, path_span) = call.file_parameter("path")?;
    let fit = image_fit(call)?;
    one_output(
        builder
            .at_span(path_span.clone())
            .video_source(path.to_path_buf(), fit),
    )
}

fn lower_audio(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    let (path, span) = call.file_parameter("path")?;
    one_output(
        builder
            .at_span(span.clone())
            .audio_source(path.to_path_buf()),
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
        assert_eq!(video_source().descriptor.semantic_version, 3);
        assert_eq!(repeat().descriptor.semantic_version, 3);
        assert_eq!(zoom().descriptor.semantic_version, 3);
    }
}
