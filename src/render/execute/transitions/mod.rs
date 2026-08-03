use crate::diagnostic::Result;
use crate::model::{AudioDomain, FrameCount, NodeId, VideoDomain};

use super::color::{linear_rgb_to_encoding, working_to_linear_rgb};
use super::filters::{normalize_audio, samples_for_video};
use super::recipe::{FfmpegRecipe, RecipeContext};
use super::timeline::video_segment_sample_counts;

pub(super) fn flash_cut(
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
        "[1:v]{},fade=t=in:start_frame=0:nb_frames={}:color=white,{}[after];[0:a]{}[before_a];[1:a]{}[after_a];[0:v][before_a][after][after_a]concat=n=2:v=1:a=1[v][joined];[joined]{}[a]",
        working_to_linear_rgb(),
        frames.0,
        linear_rgb_to_encoding(context.policy().working_video_encoding()),
        normalize_audio(
            segment_samples[0],
            context.audio(),
            context.policy().working_audio_encoding(),
        ),
        normalize_audio(
            segment_samples[1],
            context.audio(),
            context.policy().working_audio_encoding(),
        ),
        normalize_audio(
            samples,
            context.audio(),
            context.policy().working_audio_encoding(),
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

pub(super) fn audio_crossfade(
    context: &RecipeContext<'_>,
    before: NodeId,
    after: NodeId,
    samples: u64,
    domain: &AudioDomain,
) -> Result<FfmpegRecipe> {
    crossfade::render_audio(context, before, after, samples, domain)
}
