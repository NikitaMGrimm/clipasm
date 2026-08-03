use std::fmt::Write as _;

use crate::model::{AudioDomain, NodeId};
use crate::preflight::PreparedAsset;

use super::filters::normalize_audio;
use super::recipe::{FfmpegRecipe, RecipeContext};

pub(super) fn source(
    context: &RecipeContext<'_>,
    asset: &PreparedAsset,
    domain: &AudioDomain,
) -> FfmpegRecipe {
    let mut recipe = FfmpegRecipe::new();
    recipe.args(["-i"]).asset(asset.source_path());
    let filter = format!(
        "[0:a:0]{}[a]",
        normalize_audio(
            domain.samples(),
            context.audio(),
            context.policy().working_audio_encoding(),
        )
    );
    recipe.args(["-filter_complex", &filter, "-map", "[a]"]);
    context.append_audio_output(&mut recipe);
    recipe
}

pub(super) fn slice(
    context: &RecipeContext<'_>,
    input: NodeId,
    start: u64,
    end: u64,
) -> FfmpegRecipe {
    let mut recipe = FfmpegRecipe::new();
    recipe.args(["-i"]).artifact(input);
    let filter =
        format!("[0:a]atrim=start_sample={start}:end_sample={end},asetpts=PTS-STARTPTS[a]");
    recipe.args(["-filter_complex", &filter, "-map", "[a]"]);
    context.append_audio_output(&mut recipe);
    recipe
}

pub(super) fn repeat(
    context: &RecipeContext<'_>,
    input: NodeId,
    count: u64,
    domain: &AudioDomain,
) -> FfmpegRecipe {
    let mut recipe = FfmpegRecipe::new();
    recipe
        .args(["-stream_loop", &(count - 1).to_string(), "-i"])
        .artifact(input);
    let filter = format!(
        "[0:a]{}[a]",
        normalize_audio(
            domain.samples(),
            context.audio(),
            context.policy().working_audio_encoding(),
        )
    );
    recipe.args(["-filter_complex", &filter, "-map", "[a]"]);
    context.append_audio_output(&mut recipe);
    recipe
}

pub(super) fn concat(
    context: &RecipeContext<'_>,
    inputs: &[NodeId],
    domain: &AudioDomain,
) -> FfmpegRecipe {
    let mut recipe = FfmpegRecipe::new();
    for input in inputs {
        recipe.args(["-i"]).artifact(*input);
    }
    let labels = (0..inputs.len()).fold(String::new(), |mut output, index| {
        let _ = write!(output, "[{index}:a]");
        output
    });
    let filter = format!(
        "{labels}concat=n={}:v=0:a=1[joined];[joined]{}[a]",
        inputs.len(),
        normalize_audio(
            domain.samples(),
            context.audio(),
            context.policy().working_audio_encoding(),
        )
    );
    recipe.args(["-filter_complex", &filter, "-map", "[a]"]);
    context.append_audio_output(&mut recipe);
    recipe
}

pub(super) fn extract(
    context: &RecipeContext<'_>,
    video: NodeId,
    domain: &AudioDomain,
) -> FfmpegRecipe {
    let mut recipe = FfmpegRecipe::new();
    recipe.args(["-i"]).artifact(video);
    let filter = format!(
        "[0:a]{}[a]",
        normalize_audio(
            domain.samples(),
            context.audio(),
            context.policy().working_audio_encoding(),
        )
    );
    recipe.args(["-filter_complex", &filter, "-map", "[a]"]);
    context.append_audio_output(&mut recipe);
    recipe
}
