use crate::diagnostic::Result;
use crate::model::{AudioSpec, FrameCount, ImageFit, TimelineRate, VideoSpec};
use crate::source::SourceSpan;

pub(super) fn samples_for_video(
    frames: FrameCount,
    spec: &VideoSpec,
    audio: &AudioSpec,
    span: &SourceSpan,
) -> Result<u64> {
    TimelineRate::new(*spec, *audio).samples_for_frames(frames, span)
}

pub(super) fn silence_source(audio: &AudioSpec, channel_layout: &str) -> String {
    format!("anullsrc=r={}:cl={channel_layout}", audio.sample_rate())
}

pub(super) fn normalize_audio(samples: u64, audio: &AudioSpec, channel_layout: &str) -> String {
    format!(
        "aresample={},aformat=sample_rates={}:channel_layouts={channel_layout},atrim=end_sample={samples},apad=whole_len={samples},asetpts=PTS-STARTPTS",
        audio.sample_rate(),
        audio.sample_rate()
    )
}

pub(super) fn image_filter(fit: ImageFit, spec: &VideoSpec, working_pixel_format: &str) -> String {
    let width = spec.width();
    let height = spec.height();
    let geometry = match fit {
        ImageFit::Cover => format!(
            "scale={width}:{height}:force_original_aspect_ratio=increase,crop={width}:{height}"
        ),
        ImageFit::Contain => format!(
            "scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2"
        ),
        ImageFit::Stretch => format!("scale={width}:{height}"),
    };
    format!(
        "{geometry},fps={}/{},setsar=1,format={}",
        spec.fps().numerator(),
        spec.fps().denominator(),
        working_pixel_format
    )
}

pub(super) fn video_filter(
    fit: ImageFit,
    frames: FrameCount,
    spec: &VideoSpec,
    working_pixel_format: &str,
) -> String {
    format!(
        "setpts=PTS-STARTPTS,{},tpad=stop_mode=clone:stop=1,trim=end_frame={},setpts=PTS-STARTPTS",
        image_filter(fit, spec, working_pixel_format),
        frames.0
    )
}
