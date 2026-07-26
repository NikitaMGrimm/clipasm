use crate::diagnostic::Result;
use crate::model::{FrameCount, NodeId, VideoDomain};

use super::filters::{normalize_audio, samples_for_video};
use super::recipe::{FfmpegRecipe, RecipeContext};
use super::timeline::video_segment_sample_counts;

pub(super) fn flash(
    context: &RecipeContext<'_>,
    before: NodeId,
    after: NodeId,
    frames: FrameCount,
    domain: &VideoDomain,
) -> Result<FfmpegRecipe> {
    let segment_samples = video_segment_sample_counts(
        &[before, after],
        context.nodes(),
        context.video(),
        context.audio(),
        context.span(),
    )?;
    let samples = samples_for_video(
        domain.frames(),
        context.video(),
        context.audio(),
        context.span(),
    )?;
    let mut recipe = FfmpegRecipe::new();
    recipe.args(["-i"]).artifact(before);
    recipe.args(["-i"]).artifact(after);
    let filter = format!(
        "[1:v]fade=t=in:start_frame=0:nb_frames={}:color=white[after];[0:a]{}[before_a];[1:a]{}[after_a];[0:v][before_a][after][after_a]concat=n=2:v=1:a=1[v][joined];[joined]{}[a]",
        frames.0,
        normalize_audio(
            segment_samples[0],
            context.audio(),
            context.policy().working_channel_layout(),
        ),
        normalize_audio(
            segment_samples[1],
            context.audio(),
            context.policy().working_channel_layout(),
        ),
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

mod crossfade;

pub(super) fn crossfade(
    context: &RecipeContext<'_>,
    before: NodeId,
    after: NodeId,
    frames: FrameCount,
    domain: &VideoDomain,
) -> Result<FfmpegRecipe> {
    crossfade::render(context, before, after, frames, domain)
}
