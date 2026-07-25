use crate::diagnostic::{Diagnostic, Result};
use crate::model::ValueType;
use crate::program::{Cardinality, ParameterType, ProgramDefinition, ProgramOutputs, ResolvedCall};
use crate::semantic::GraphBuilder;

use super::support::{direct, exact_descriptor, input, one_output, parameter};

const DEFAULT_ZOOM_PERCENT: u16 = 8;
const DEFAULT_WOBBLE_PIXELS: u16 = 3;

pub(super) fn zoom() -> ProgramDefinition {
    direct(
        exact_descriptor(
            "zoom",
            3,
            vec![input("video", ValueType::Video, Cardinality::One)],
            vec![parameter("percent", ParameterType::Integer, false)],
            ValueType::Video,
        ),
        lower_zoom,
    )
}

pub(super) fn wobble() -> ProgramDefinition {
    direct(
        exact_descriptor(
            "wobble",
            2,
            vec![input("video", ValueType::Video, Cardinality::One)],
            vec![parameter("pixels", ParameterType::Integer, false)],
            ValueType::Video,
        ),
        lower_wobble,
    )
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
            padding < builder.video_spec().width()
                && padding < builder.video_spec().height()
                && builder.video_spec().width().checked_add(padding).is_some()
                && builder.video_spec().height().checked_add(padding).is_some()
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
