use crate::diagnostic::Result;
use crate::model::{FrameCount, NodeId, VideoDomain};

use super::context::RenderContext;
use super::filters::{append_video_output, normalize_audio, samples_for_video};
use super::timeline::video_segment_sample_counts;

pub(super) fn flash(
    context: &RenderContext<'_>,
    before: NodeId,
    after: NodeId,
    frames: FrameCount,
    domain: &VideoDomain,
) -> Result<()> {
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
    let mut command = context.command();
    command.arg("-i").arg(context.artifact(before)?);
    command.arg("-i").arg(context.artifact(after)?);
    let filter = format!(
        "[1:v]fade=t=in:start_frame=0:nb_frames={}:color=white[after];[0:a]{}[before_a];[1:a]{}[after_a];[0:v][before_a][after][after_a]concat=n=2:v=1:a=1[v][joined];[joined]{}[a]",
        frames.0,
        normalize_audio(segment_samples[0], context.audio()),
        normalize_audio(segment_samples[1], context.audio()),
        normalize_audio(samples, context.audio())
    );
    command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
    append_video_output(
        &mut command,
        context.video(),
        context.audio(),
        context.temporary(),
    );
    context.finish_ffmpeg(command)
}

mod crossfade;

pub(super) fn crossfade(
    context: &RenderContext<'_>,
    before: NodeId,
    after: NodeId,
    frames: FrameCount,
    domain: &VideoDomain,
) -> Result<()> {
    crossfade::render(context, before, after, frames, domain)
}
