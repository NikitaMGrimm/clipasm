use crate::diagnostic::Result;
use crate::model::{FrameCount, ImageFit, NodeId, VideoDomain};
use crate::preflight::{PreparedAsset, WORKING_PIXEL_FORMAT};

use super::context::RenderContext;
use super::filters::{
    append_video_output, image_filter, normalize_audio, samples_for_video, silence_source,
    video_filter,
};

pub(super) fn image(
    context: &RenderContext<'_>,
    asset: &PreparedAsset,
    fit: ImageFit,
    frames: FrameCount,
) -> Result<()> {
    let samples = samples_for_video(frames, context.video(), context.audio(), context.span())?;
    let mut command = context.command();
    command.args(["-loop", "1", "-i"]).arg(asset.source_path());
    command
        .args(["-f", "lavfi", "-i"])
        .arg(silence_source(context.audio()));
    let filter = format!(
        "[0:v]{},trim=end_frame={},setpts=PTS-STARTPTS[v];[1:a]{}[a]",
        image_filter(fit, context.video()),
        frames.0,
        normalize_audio(samples, context.audio())
    );
    command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
    append_video_output(
        &mut command,
        frames,
        context.video(),
        context.audio(),
        context.temporary(),
    );
    context.finish_ffmpeg(command)
}

pub(super) fn video_source(
    context: &RenderContext<'_>,
    asset: &PreparedAsset,
    fit: ImageFit,
    frames: FrameCount,
    has_audio: bool,
) -> Result<()> {
    let samples = samples_for_video(frames, context.video(), context.audio(), context.span())?;
    let mut command = context.command();
    command.arg("-i").arg(asset.source_path());
    let audio_input = if has_audio {
        "[0:a:0]".to_owned()
    } else {
        command
            .args(["-f", "lavfi", "-i"])
            .arg(silence_source(context.audio()));
        "[1:a]".to_owned()
    };
    let filter = format!(
        "[0:v]{}[v];{audio_input}{}[a]",
        video_filter(fit, frames, context.video()),
        normalize_audio(samples, context.audio())
    );
    command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
    append_video_output(
        &mut command,
        frames,
        context.video(),
        context.audio(),
        context.temporary(),
    );
    context.finish_ffmpeg(command)
}

pub(super) fn set_audio(
    context: &RenderContext<'_>,
    audio_node: NodeId,
    video: NodeId,
    domain: &VideoDomain,
) -> Result<()> {
    let samples = samples_for_video(
        domain.frames(),
        context.video(),
        context.audio(),
        context.span(),
    )?;
    let mut command = context.command();
    command.arg("-i").arg(context.artifact(audio_node)?);
    command.arg("-i").arg(context.artifact(video)?);
    let filter = format!(
        "[1:v]trim=end_frame={},setpts=PTS-STARTPTS[v];[0:a]{}[a]",
        domain.frames().0,
        normalize_audio(samples, context.audio())
    );
    command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
    append_video_output(
        &mut command,
        domain.frames(),
        context.video(),
        context.audio(),
        context.temporary(),
    );
    context.finish_ffmpeg(command)
}

pub(super) fn audio_on_black(
    context: &RenderContext<'_>,
    audio_node: NodeId,
    domain: &VideoDomain,
) -> Result<()> {
    let samples = samples_for_video(
        domain.frames(),
        context.video(),
        context.audio(),
        context.span(),
    )?;
    let mut command = context.command();
    command.args(["-f", "lavfi", "-i"]).arg(format!(
        "color=c=black:s={}x{}:r={}/{}",
        context.video().width(),
        context.video().height(),
        context.video().fps().numerator(),
        context.video().fps().denominator()
    ));
    command.arg("-i").arg(context.artifact(audio_node)?);
    let filter = format!(
        "[0:v]trim=end_frame={},setpts=PTS-STARTPTS,format={WORKING_PIXEL_FORMAT}[v];[1:a]{}[a]",
        domain.frames().0,
        normalize_audio(samples, context.audio())
    );
    command.args(["-filter_complex", &filter, "-map", "[v]", "-map", "[a]"]);
    append_video_output(
        &mut command,
        domain.frames(),
        context.video(),
        context.audio(),
        context.temporary(),
    );
    context.finish_ffmpeg(command)
}
