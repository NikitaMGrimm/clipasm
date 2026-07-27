use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{FrameCount, ImageFit, ValueType};
use crate::program::{
    ParameterType, ProgramDefinition, ProgramOutputs, RequestedVideoExtent, ResolvedCall,
};
use crate::semantic::GraphBuilder;

use super::DEFAULT_FIT;
use super::support::{direct, exact_descriptor, one_output, parameter};

pub(super) fn image() -> ProgramDefinition {
    direct(
        exact_descriptor(
            "image",
            2,
            vec![],
            vec![
                parameter("path", ParameterType::File, true),
                parameter("duration", ParameterType::Duration, false),
                fit_parameter(),
            ],
            ValueType::Video,
        ),
        lower_image,
    )
}

pub(super) fn video() -> ProgramDefinition {
    direct(
        exact_descriptor(
            "video",
            3,
            vec![],
            vec![
                parameter("path", ParameterType::File, true),
                fit_parameter(),
            ],
            ValueType::Video,
        ),
        lower_video,
    )
}

pub(super) fn audio() -> ProgramDefinition {
    direct(
        exact_descriptor(
            "audio",
            1,
            vec![],
            vec![parameter("path", ParameterType::File, true)],
            ValueType::Audio,
        ),
        lower_audio,
    )
}

fn fit_parameter() -> crate::program::ParameterDescriptor {
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

fn lower_image(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<ProgramOutputs> {
    let (path, path_span) = call.file_parameter("path")?;
    let frames = if let Some((duration, span)) = call.optional_duration_parameter("duration")? {
        FrameCount(duration.to_frames(builder.video_spec().fps(), span)?)
    } else {
        match call.requested_extent() {
            Some(RequestedVideoExtent::Concrete(frames)) => *frames,
            Some(RequestedVideoExtent::Deferred(extent)) => {
                let fit = image_fit(call)?;
                return one_output(builder.at_span(path_span.clone()).deferred_image_video(
                    path.to_path_buf(),
                    extent.clone(),
                    fit,
                ));
            }
            None => {
                return Err(Diagnostic::builtin(
                    BuiltinDiagnostic::MissingImageDuration,
                    "`image.duration` is required outside a context with a requested duration",
                    call.origin().span.clone(),
                ));
            }
        }
    };
    if frames.0 == 0 {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidDuration,
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

fn image_fit(call: &ResolvedCall) -> Result<ImageFit> {
    let fit = call
        .optional_keyword_parameter("fit")?
        .map_or_else(|| DEFAULT_FIT.keyword(), |(fit, _)| fit);
    Ok(match fit {
        "cover" => ImageFit::Cover,
        "contain" => ImageFit::Contain,
        "stretch" => ImageFit::Stretch,
        _ => unreachable!("fit keyword and built-in default were validated by the catalog"),
    })
}
