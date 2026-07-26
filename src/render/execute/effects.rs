use crate::diagnostic::Result;
use crate::model::{FrameCount, NodeId, VideoDomain, VideoSpec};

use super::filters::{normalize_audio, samples_for_video};
use super::recipe::{FfmpegRecipe, RecipeContext};

const WOBBLE_FREQUENCY_NUMERATOR: u32 = 13;
const WOBBLE_FREQUENCY_DENOMINATOR: u32 = 2;

pub(super) fn zoom(
    context: &RecipeContext<'_>,
    input: NodeId,
    percent: u32,
    domain: &VideoDomain,
) -> Result<FfmpegRecipe> {
    let samples = samples_for_video(
        domain.frames(),
        context.video(),
        context.audio(),
        context.span(),
    )?;
    let mut recipe = FfmpegRecipe::new();
    recipe.args(["-i"]).artifact(input);
    let filter = format!(
        "[0:v]{}[v];[0:a]{}[a]",
        zoom_filter(percent, domain.frames()),
        normalize_audio(
            samples,
            context.audio(),
            context.policy().working_channel_layout(),
        )
    );
    recipe.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
    context.append_video_output(&mut recipe);
    Ok(recipe)
}

pub(super) fn wobble(
    context: &RecipeContext<'_>,
    input: NodeId,
    pixels: u32,
    domain: &VideoDomain,
) -> Result<FfmpegRecipe> {
    let samples = samples_for_video(
        domain.frames(),
        context.video(),
        context.audio(),
        context.span(),
    )?;
    let mut recipe = FfmpegRecipe::new();
    recipe.args(["-i"]).artifact(input);
    let filter = format!(
        "[0:v]{}[v];[0:a]{}[a]",
        wobble_filter(pixels, context.video()),
        normalize_audio(
            samples,
            context.audio(),
            context.policy().working_channel_layout(),
        )
    );
    recipe.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
    context.append_video_output(&mut recipe);
    Ok(recipe)
}

fn zoom_filter(percent: u32, frames: FrameCount) -> String {
    let last_frame = frames.0.saturating_sub(1).max(1);
    let zoom = format!("(1+{percent}*(in-1)/(100*{last_frame}))");
    let x_margin = format!("W*(1-1/{zoom})/2");
    let y_margin = format!("H*(1-1/{zoom})/2");
    format!(
        "perspective=x0='{x_margin}':y0='{y_margin}':x1='W-{x_margin}':y1='{y_margin}':x2='{x_margin}':y2='H-{y_margin}':x3='W-{x_margin}':y3='H-{y_margin}':sense=source:eval=frame:interpolation=cubic,setpts=PTS-STARTPTS"
    )
}

fn wobble_filter(pixels: u32, spec: &VideoSpec) -> String {
    let padding = pixels * 2;
    let scaled_width = spec
        .width()
        .checked_add(padding)
        .expect("wobble dimensions were validated during compilation");
    let scaled_height = spec
        .height()
        .checked_add(padding)
        .expect("wobble dimensions were validated during compilation");
    let phase = format!(
        "2*PI*{}*n*{}/({}*{})",
        WOBBLE_FREQUENCY_NUMERATOR,
        spec.fps().denominator(),
        WOBBLE_FREQUENCY_DENOMINATOR,
        spec.fps().numerator()
    );
    format!(
        "scale={scaled_width}:{scaled_height},setsar=1,crop={}:{}:x='{pixels}*(1+sin({phase}))':y='{pixels}*(1+sin({phase}+PI/2))',setpts=PTS-STARTPTS",
        spec.width(),
        spec.height()
    )
}
