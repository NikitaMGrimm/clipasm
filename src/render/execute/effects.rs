use crate::diagnostic::Result;
use crate::model::{ExactNumber, FrameCount, NodeId, VideoDomain};

use super::filters::{normalize_audio, samples_for_video};
use super::recipe::{FfmpegRecipe, RecipeContext};

pub(super) fn zoom_in(
    context: &RecipeContext<'_>,
    input: NodeId,
    by: &ExactNumber,
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
        zoom_filter(by, domain.frames()),
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

fn zoom_filter(by: &ExactNumber, frames: FrameCount) -> String {
    let last_frame = frames.0.saturating_sub(1).max(1);
    let zoom_in = format!(
        "(1+{}*(in-1)/({}*{last_frame}))",
        by.numerator(),
        by.denominator()
    );
    let x_margin = format!("W*(1-1/{zoom_in})/2");
    let y_margin = format!("H*(1-1/{zoom_in})/2");
    format!(
        "perspective=x0='{x_margin}':y0='{y_margin}':x1='W-{x_margin}':y1='{y_margin}':x2='{x_margin}':y2='H-{y_margin}':x3='W-{x_margin}':y3='H-{y_margin}':sense=source:eval=frame:interpolation=cubic,setpts=PTS-STARTPTS"
    )
}
