use crate::diagnostic::Result;
use crate::model::{AudioSpec, FrameCount, ImageFit, TimelineRate, VideoSpec};
use crate::preflight::{AudioEncoding, PreparedSourceColor, VideoEncoding};
use crate::source::SourceSpan;

use super::color::{linear_rgb_to_encoding, source_to_linear_rgb};

pub(super) fn samples_for_video(
    frames: FrameCount,
    spec: &VideoSpec,
    audio: AudioSpec,
    span: &SourceSpan,
) -> Result<u64> {
    TimelineRate::new(*spec, audio).samples_for_frames(frames, span)
}

pub(super) fn silence_source(audio: AudioSpec, encoding: AudioEncoding) -> String {
    format!(
        "anullsrc=r={}:cl={}",
        audio.sample_rate(),
        encoding.channel_layout()
    )
}

pub(super) fn normalize_audio(samples: u64, audio: AudioSpec, encoding: AudioEncoding) -> String {
    format!(
        "aresample={},aformat=sample_fmts={}:sample_rates={}:channel_layouts={},atrim=end_sample={samples},apad=whole_len={samples},asetpts=PTS-STARTPTS",
        audio.sample_rate(),
        encoding.sample_format(),
        audio.sample_rate(),
        encoding.channel_layout(),
    )
}

pub(super) fn image_filter(
    fit: ImageFit,
    spec: &VideoSpec,
    source: &PreparedSourceColor,
    working: VideoEncoding,
) -> String {
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
        "{},{geometry},fps={}/{},setsar=1,{}",
        source_to_linear_rgb(source),
        spec.fps().numerator(),
        spec.fps().denominator(),
        linear_rgb_to_encoding(working),
    )
}

pub(super) fn video_filter(
    fit: ImageFit,
    frames: FrameCount,
    spec: &VideoSpec,
    source: &PreparedSourceColor,
    working: VideoEncoding,
) -> String {
    format!(
        "setpts=PTS-STARTPTS,{},tpad=stop_mode=clone:stop=1,trim=end_frame={},setpts=PTS-STARTPTS",
        image_filter(fit, spec, source, working),
        frames.0
    )
}
