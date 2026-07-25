use std::fmt::Write as _;

use crate::diagnostic::Result;
use crate::model::{AudioDomain, NodeId};
use crate::preflight::PreparedAsset;

use super::context::RenderContext;
use super::filters::normalize_audio;

pub(super) fn source(
    context: &RenderContext<'_>,
    asset: &PreparedAsset,
    domain: &AudioDomain,
) -> Result<()> {
    let mut command = context.command();
    command.arg("-i").arg(asset.execution_path());
    let filter = format!(
        "[0:a:0]{}[a]",
        normalize_audio(
            domain.samples(),
            context.audio(),
            context.policy().working_channel_layout(),
        )
    );
    command.args(["-filter_complex", &filter, "-map", "[a]"]);
    context.append_audio_output(&mut command);
    context.finish_ffmpeg(command)
}

pub(super) fn slice(
    context: &RenderContext<'_>,
    input: NodeId,
    start: u64,
    end: u64,
) -> Result<()> {
    let mut command = context.command();
    command.arg("-i").arg(context.artifact(input)?);
    let filter =
        format!("[0:a]atrim=start_sample={start}:end_sample={end},asetpts=PTS-STARTPTS[a]");
    command.args(["-filter_complex", &filter, "-map", "[a]"]);
    context.append_audio_output(&mut command);
    context.finish_ffmpeg(command)
}

pub(super) fn repeat(
    context: &RenderContext<'_>,
    input: NodeId,
    count: u64,
    domain: &AudioDomain,
) -> Result<()> {
    let mut command = context.command();
    command
        .args(["-stream_loop", &(count - 1).to_string(), "-i"])
        .arg(context.artifact(input)?);
    let filter = format!(
        "[0:a]{}[a]",
        normalize_audio(
            domain.samples(),
            context.audio(),
            context.policy().working_channel_layout(),
        )
    );
    command.args(["-filter_complex", &filter, "-map", "[a]"]);
    context.append_audio_output(&mut command);
    context.finish_ffmpeg(command)
}

pub(super) fn concat(
    context: &RenderContext<'_>,
    inputs: &[NodeId],
    domain: &AudioDomain,
) -> Result<()> {
    let mut command = context.command();
    for input in inputs {
        command.arg("-i").arg(context.artifact(*input)?);
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
            context.policy().working_channel_layout(),
        )
    );
    command.args(["-filter_complex", &filter, "-map", "[a]"]);
    context.append_audio_output(&mut command);
    context.finish_ffmpeg(command)
}

pub(super) fn extract(
    context: &RenderContext<'_>,
    video: NodeId,
    domain: &AudioDomain,
) -> Result<()> {
    let mut command = context.command();
    command.arg("-i").arg(context.artifact(video)?);
    let filter = format!(
        "[0:a]{}[a]",
        normalize_audio(
            domain.samples(),
            context.audio(),
            context.policy().working_channel_layout(),
        )
    );
    command.args(["-filter_complex", &filter, "-map", "[a]"]);
    context.append_audio_output(&mut command);
    context.finish_ffmpeg(command)
}
