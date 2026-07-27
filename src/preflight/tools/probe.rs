use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::media_tool;
use crate::model::{AudioDomain, AudioSpec, FrameCount, VideoSpec};
use crate::source::SourceSpan;

use super::ToolIdentity;

#[derive(Deserialize)]
struct ImageProbeDocument {
    #[serde(default)]
    streams: Vec<ImageProbeStream>,
}

#[derive(Deserialize)]
struct ImageProbeStream {
    codec_type: Option<String>,
    nb_read_frames: Option<String>,
}

pub(crate) fn verify_image_decodable(
    path: &Path,
    span: &SourceSpan,
    ffmpeg: &ToolIdentity,
    ffprobe: &ToolIdentity,
) -> Result<()> {
    let mut probe = Command::new(ffprobe.executable());
    probe
        .args([
            "-v",
            "error",
            "-count_frames",
            "-show_entries",
            "stream=codec_type,nb_read_frames",
            "-of",
            "json",
        ])
        .arg(path);
    let output = media_tool::capture(probe, BuiltinDiagnostic::SourceDecodability, span).map_err(
        |error| {
            tool_context(
                error,
                format!("could not inspect image `{}`", path.display()),
            )
        },
    )?;
    let document: ImageProbeDocument = serde_json::from_slice(&output.stdout).map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::SourceDecodability,
            format!(
                "FFprobe returned invalid image metadata for `{}`: {error}",
                path.display()
            ),
            span.clone(),
        )
    })?;
    let videos = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("video"))
        .collect::<Vec<_>>();
    let audio_count = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
        .count();
    let frame_count = videos
        .first()
        .and_then(|stream| stream.nb_read_frames.as_deref())
        .and_then(|frames| frames.parse::<u64>().ok());
    if videos.len() != 1 || audio_count != 0 || frame_count != Some(1) {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::SourceContract,
            format!(
                "image `{}` must contain exactly one video stream, no audio, and one decoded frame; found {} video stream(s), {audio_count} audio stream(s), and {frame_count:?} decoded frame(s)",
                path.display(),
                videos.len()
            ),
            span.clone(),
        ));
    }
    let mut decode = Command::new(ffmpeg.executable());
    decode
        .args(["-v", "error", "-loop", "1", "-i"])
        .arg(path)
        .args(["-map", "0:v:0", "-frames:v", "1", "-an", "-f", "null", "-"]);
    media_tool::run(decode, BuiltinDiagnostic::SourceDecodability, span).map_err(|error| {
        tool_context(
            error,
            format!(
                "image `{}` is not compatible with the renderer's still-image input mode",
                path.display()
            ),
        )
    })
}

pub(crate) fn decoded_audio_samples(
    ffprobe: &Path,
    path: &Path,
    span: &SourceSpan,
    contract_code: BuiltinDiagnostic,
) -> Result<u64> {
    let mut command = Command::new(ffprobe);
    command
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_frames",
            "-show_entries",
            "frame=nb_samples",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path);
    let mut samples = 0_u64;
    media_tool::stream_stdout_lines(command, 64, contract_code, span, |line| {
        let line = std::str::from_utf8(line).map_err(|error| {
            Diagnostic::builtin(
        contract_code,
                format!(
                    "FFprobe returned non-UTF-8 audio frame metadata for `{}`: {error}",
                    path.display()
                ),
                span.clone(),
            )
        })?;
        let line = line.trim();
        if line.is_empty() {
            return Ok(());
        }
        let count = line.parse::<u64>().map_err(|error| {
            Diagnostic::builtin(
        contract_code,
                format!(
                    "FFprobe returned an invalid decoded audio sample count `{line}` for `{}`: {error}",
                    path.display()
                ),
                span.clone(),
            )
        })?;
        samples = samples
            .checked_add(count)
            .ok_or_else(|| audio_duration_overflow(span))?;
        Ok(())
    })
    .map_err(|error| {
        tool_context(
            error,
            format!(
                "could not count decoded audio samples in `{}`",
                path.display()
            ),
        )
    })?;
    if samples == 0 {
        return Err(Diagnostic::builtin(
            contract_code,
            format!("audio `{}` contains no decoded samples", path.display()),
            span.clone(),
        ));
    }
    Ok(samples)
}

#[derive(Deserialize)]
struct VideoProbeDocument {
    #[serde(default)]
    streams: Vec<VideoProbeStream>,
}

#[derive(Deserialize)]
struct VideoProbeStream {
    codec_type: Option<String>,
    nb_read_frames: Option<String>,
    duration_ts: Option<ProbeInteger>,
    time_base: Option<String>,
    avg_frame_rate: Option<String>,
    sample_rate: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ProbeInteger {
    Number(u64),
    String(String),
}

impl ProbeInteger {
    fn get(&self) -> Option<u128> {
        match self {
            Self::Number(value) => Some(u128::from(*value)),
            Self::String(value) => value.parse().ok(),
        }
    }
}

pub(crate) fn verify_video_decodable(
    path: &Path,
    video: &VideoSpec,
    span: &SourceSpan,
    ffmpeg: &ToolIdentity,
    ffprobe: &ToolIdentity,
) -> Result<(FrameCount, bool)> {
    let document = probe_video(path, span, ffprobe)?;
    let result = validate_video_document(path, video, span, &document)?;
    decode_video_frame(path, span, ffmpeg)?;
    Ok(result)
}

pub(crate) fn validate_video_probe_json(
    path: &Path,
    video: &VideoSpec,
    span: &SourceSpan,
    probe_json: &str,
) -> Result<(FrameCount, bool)> {
    let document = parse_video_probe(path, span, probe_json.as_bytes())?;
    validate_video_document(path, video, span, &document)
}

pub(crate) fn verify_audio_decodable(
    path: &Path,
    audio: AudioSpec,
    span: &SourceSpan,
    ffmpeg: &ToolIdentity,
    ffprobe: &ToolIdentity,
) -> Result<AudioDomain> {
    let document = probe_video(path, span, ffprobe)?;
    let stream = document
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("audio"))
        .ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::SourceContract,
                format!("audio `{}` contains no audio stream", path.display()),
                span.clone(),
            )
        })?;
    let decoded_samples = decoded_audio_samples(
        ffprobe.executable(),
        path,
        span,
        BuiltinDiagnostic::SourceContract,
    )?;
    let (duration_numerator, duration_denominator) = audio_duration(stream, decoded_samples)
        .ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::SourceContract,
                format!(
                    "audio `{}` does not expose a usable stream duration or sample rate",
                    path.display()
                ),
                span.clone(),
            )
        })?;
    let mut decode = Command::new(ffmpeg.executable());
    decode
        .args(["-v", "error", "-xerror", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-frames:a", "1", "-f", "null", "-"]);
    media_tool::run(decode, BuiltinDiagnostic::SourceDecodability, span).map_err(|error| {
        tool_context(
            error,
            format!("audio `{}` is not decodable by FFmpeg", path.display()),
        )
    })?;
    AudioDomain::covering_duration(duration_numerator, duration_denominator, audio, span)
}

fn audio_duration_overflow(span: &SourceSpan) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::AudioDurationOverflow,
        "audio duration exceeds the supported range",
        span.clone(),
    )
}

fn probe_video(
    path: &Path,
    span: &SourceSpan,
    ffprobe: &ToolIdentity,
) -> Result<VideoProbeDocument> {
    let mut command = Command::new(ffprobe.executable());
    command
        .args([
            "-v",
            "error",
            "-count_frames",
            "-show_entries",
            "stream=codec_type,nb_read_frames,duration_ts,time_base,avg_frame_rate,sample_rate",
            "-of",
            "json",
        ])
        .arg(path);
    let output = media_tool::capture(command, BuiltinDiagnostic::SourceDecodability, span)
        .map_err(|error| {
            tool_context(
                error,
                format!("could not inspect video `{}`", path.display()),
            )
        })?;
    parse_video_probe(path, span, &output.stdout)
}

fn parse_video_probe(
    path: &Path,
    span: &SourceSpan,
    document: &[u8],
) -> Result<VideoProbeDocument> {
    serde_json::from_slice(document).map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::SourceDecodability,
            format!(
                "FFprobe returned invalid video metadata for `{}`: {error}",
                path.display()
            ),
            span.clone(),
        )
    })
}

fn validate_video_document(
    path: &Path,
    video: &VideoSpec,
    span: &SourceSpan,
    document: &VideoProbeDocument,
) -> Result<(FrameCount, bool)> {
    let videos = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("video"))
        .collect::<Vec<_>>();
    if videos.len() != 1 {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::SourceContract,
            format!(
                "video `{}` must contain exactly one video stream; found {}",
                path.display(),
                videos.len()
            ),
            span.clone(),
        ));
    }
    let stream = videos[0];
    let decoded_frames = stream
        .nb_read_frames
        .as_deref()
        .and_then(|frames| frames.parse::<u64>().ok());
    if decoded_frames.is_none_or(|frames| frames == 0) {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::SourceContract,
            format!(
                "video `{}` must contain at least one decodable frame; FFprobe counted {decoded_frames:?}",
                path.display()
            ),
            span.clone(),
        ));
    }
    let Some((available_numerator, available_denominator)) = video_duration(stream) else {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::SourceContract,
            format!(
                "video `{}` does not expose a usable stream duration",
                path.display()
            ),
            span.clone(),
        ));
    };
    let frames = FrameCount::covering_duration(
        available_numerator,
        available_denominator,
        video.fps(),
        span,
    )?;
    let has_audio = document
        .streams
        .iter()
        .any(|stream| stream.codec_type.as_deref() == Some("audio"));
    Ok((frames, has_audio))
}

fn decode_video_frame(path: &Path, span: &SourceSpan, ffmpeg: &ToolIdentity) -> Result<()> {
    let mut decode = Command::new(ffmpeg.executable());
    decode
        .args(["-v", "error", "-xerror", "-i"])
        .arg(path)
        .args(["-map", "0:v:0", "-frames:v", "1", "-an", "-f", "null", "-"]);
    media_tool::run(decode, BuiltinDiagnostic::SourceDecodability, span).map_err(|error| {
        tool_context(
            error,
            format!(
                "video `{}` is not compatible with the renderer's video input mode",
                path.display()
            ),
        )
    })
}

fn tool_context(mut error: Diagnostic, mut context: String) -> Diagnostic {
    context.push('\n');
    context.push_str(&error.message);
    error.message = context;
    error
}

fn audio_duration(stream: &VideoProbeStream, decoded_samples: u64) -> Option<(u128, u128)> {
    stream_duration(stream).or_else(|| {
        let sample_rate = stream.sample_rate.as_deref()?.parse::<u128>().ok()?;
        (sample_rate > 0).then_some((u128::from(decoded_samples), sample_rate))
    })
}

fn stream_duration(stream: &VideoProbeStream) -> Option<(u128, u128)> {
    stream
        .duration_ts
        .as_ref()
        .and_then(ProbeInteger::get)
        .filter(|duration| *duration > 0)
        .zip(stream.time_base.as_deref().and_then(parse_positive_ratio))
        .and_then(|(duration, (time_numerator, time_denominator))| {
            duration
                .checked_mul(time_numerator)
                .map(|numerator| (numerator, time_denominator))
        })
}

fn video_duration(stream: &VideoProbeStream) -> Option<(u128, u128)> {
    stream_duration(stream).or_else(|| {
        let frames = stream.nb_read_frames.as_deref()?.parse::<u128>().ok()?;
        let (rate_numerator, rate_denominator) =
            parse_positive_ratio(stream.avg_frame_rate.as_deref()?)?;
        frames
            .checked_mul(rate_denominator)
            .map(|numerator| (numerator, rate_numerator))
    })
}

fn parse_positive_ratio(value: &str) -> Option<(u128, u128)> {
    let (numerator, denominator) = value.split_once('/')?;
    let numerator = numerator.parse::<u128>().ok()?;
    let denominator = denominator.parse::<u128>().ok()?;
    (numerator > 0 && denominator > 0).then_some((numerator, denominator))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn exact_sample_count_streams_many_frame_records() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().expect("temporary directory");
        let ffprobe = directory.path().join("ffprobe");
        fs::write(
            &ffprobe,
            "#!/bin/sh\ni=0\nwhile [ \"$i\" -lt 50000 ]; do\n  printf '1024\\n'\n  i=$((i + 1))\ndone\n",
        )
        .expect("fake FFprobe");
        let mut permissions = fs::metadata(&ffprobe).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&ffprobe, permissions).expect("executable permissions");
        let source = directory.path().join("long.mka");
        let samples = decoded_audio_samples(
            &ffprobe,
            &source,
            &SourceSpan::file_start(&source),
            BuiltinDiagnostic::SourceContract,
        )
        .expect("streamed sample count");
        assert_eq!(samples, 50_000 * 1_024);
    }
}
