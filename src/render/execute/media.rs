use crate::diagnostic::Result;
use crate::model::{FrameCount, ImageFit, NodeId, VideoDomain};
use crate::preflight::{PreparedAsset, PreparedSourceColor};

use super::color::{linear_rgb_to_encoding, to_linear_rgb};
use super::filters::{
    image_filter, normalize_audio, samples_for_video, silence_source, video_filter,
};
use super::recipe::{FfmpegRecipe, RecipeContext};

pub(super) fn image(
    context: &RecipeContext<'_>,
    asset: &PreparedAsset,
    color: &PreparedSourceColor,
    fit: ImageFit,
    frames: FrameCount,
) -> Result<FfmpegRecipe> {
    let samples = samples_for_video(frames, context.video(), context.audio(), context.span())?;
    let mut recipe = FfmpegRecipe::new();
    recipe
        .args(["-f", "image2", "-loop", "1", "-pattern_type", "none", "-i"])
        .asset(asset.source_path());
    recipe.args(["-f", "lavfi", "-i"]).arg(silence_source(
        context.audio(),
        context.policy().working_audio_encoding(),
    ));
    let filter = format!(
        "[0:v]{},trim=end_frame={},setpts=PTS-STARTPTS[v];[1:a]{}[a]",
        image_filter(
            fit,
            context.video(),
            color,
            context.policy().working_video_encoding(),
        ),
        frames.0,
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

pub(super) fn video_source(
    context: &RecipeContext<'_>,
    asset: &PreparedAsset,
    color: &PreparedSourceColor,
    fit: ImageFit,
    frames: FrameCount,
    has_audio: bool,
) -> Result<FfmpegRecipe> {
    let samples = samples_for_video(frames, context.video(), context.audio(), context.span())?;
    let mut recipe = FfmpegRecipe::new();
    recipe.args(["-i"]).asset(asset.source_path());
    let audio_input = if has_audio {
        "[0:a:0]".to_owned()
    } else {
        recipe.args(["-f", "lavfi", "-i"]).arg(silence_source(
            context.audio(),
            context.policy().working_audio_encoding(),
        ));
        "[1:a]".to_owned()
    };
    let filter = format!(
        "[0:v]{}[v];{audio_input}{}[a]",
        video_filter(
            fit,
            frames,
            context.video(),
            color,
            context.policy().working_video_encoding(),
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

pub(super) fn set_audio(
    context: &RecipeContext<'_>,
    audio_node: NodeId,
    video: NodeId,
    domain: &VideoDomain,
) -> Result<FfmpegRecipe> {
    let samples = samples_for_video(
        domain.frames(),
        context.video(),
        context.audio(),
        context.span(),
    )?;
    let mut recipe = FfmpegRecipe::new();
    recipe.args(["-i"]).artifact(audio_node);
    recipe.args(["-i"]).artifact(video);
    let filter = format!(
        "[1:v]trim=end_frame={},setpts=PTS-STARTPTS[v];[0:a]{}[a]",
        domain.frames().0,
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

pub(super) fn audio_on_black(
    context: &RecipeContext<'_>,
    audio_node: NodeId,
    domain: &VideoDomain,
) -> Result<FfmpegRecipe> {
    let samples = samples_for_video(
        domain.frames(),
        context.video(),
        context.audio(),
        context.span(),
    )?;
    let mut recipe = FfmpegRecipe::new();
    recipe.args(["-f", "lavfi", "-i"]).arg(format!(
        "color=c=black:s={}x{}:r={}/{}",
        context.video().width(),
        context.video().height(),
        context.video().fps().numerator(),
        context.video().fps().denominator()
    ));
    recipe.args(["-i"]).artifact(audio_node);
    let filter = format!(
        "[0:v]trim=end_frame={},setpts=PTS-STARTPTS,{},{}[v];[1:a]{}[a]",
        domain.frames().0,
        to_linear_rgb(crate::model::ColorSpec::SDR_BT709, None),
        linear_rgb_to_encoding(context.policy().working_video_encoding()),
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
