use std::path::Path;
use std::process::Command;

use crate::diagnostic::Result;
use crate::model::{AudioSpec, FrameCount, ImageFit, TimelineRate, VideoSpec};
use crate::preflight::WORKING_PIXEL_FORMAT;
use crate::source::SourceSpan;

pub(super) fn append_video_output(
    command: &mut Command,
    spec: &VideoSpec,
    audio: &AudioSpec,
    destination: &Path,
) {
    command
        .args(["-c:v", "ffv1"])
        .args(["-level", "3", "-pix_fmt", WORKING_PIXEL_FORMAT, "-r"])
        .arg(format!(
            "{}/{}",
            spec.fps().numerator(),
            spec.fps().denominator()
        ))
        .args([
            "-c:a",
            "flac",
            "-ar",
            &audio.sample_rate().to_string(),
            "-ac",
            &audio.channels().to_string(),
        ])
        .arg(destination);
}

pub(super) fn append_audio_output(command: &mut Command, audio: &AudioSpec, destination: &Path) {
    command
        .args([
            "-c:a",
            "flac",
            "-ar",
            &audio.sample_rate().to_string(),
            "-ac",
            &audio.channels().to_string(),
            "-f",
            "matroska",
        ])
        .arg(destination);
}

pub(super) fn samples_for_video(
    frames: FrameCount,
    spec: &VideoSpec,
    audio: &AudioSpec,
    span: &SourceSpan,
) -> Result<u64> {
    TimelineRate::new(*spec, *audio).samples_for_frames(frames, span)
}

pub(super) fn silence_source(audio: &AudioSpec) -> String {
    format!("anullsrc=r={}:cl=stereo", audio.sample_rate())
}

pub(super) fn normalize_audio(samples: u64, audio: &AudioSpec) -> String {
    format!(
        "aresample={},aformat=sample_rates={}:channel_layouts=stereo,atrim=end_sample={samples},apad=whole_len={samples},asetpts=PTS-STARTPTS",
        audio.sample_rate(),
        audio.sample_rate()
    )
}

pub(super) fn image_filter(fit: ImageFit, spec: &VideoSpec) -> String {
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
        WORKING_PIXEL_FORMAT
    )
}

pub(super) fn video_filter(fit: ImageFit, frames: FrameCount, spec: &VideoSpec) -> String {
    format!(
        "setpts=PTS-STARTPTS,{},tpad=stop_mode=clone:stop=1,trim=end_frame={},setpts=PTS-STARTPTS",
        image_filter(fit, spec),
        frames.0
    )
}
