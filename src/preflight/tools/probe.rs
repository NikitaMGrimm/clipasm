use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use crate::diagnostic::{Diagnostic, Result};
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
    let output = Command::new(ffprobe.executable())
        .args([
            "-v",
            "error",
            "-count_frames",
            "-show_entries",
            "stream=codec_type,nb_read_frames",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|error| {
            Diagnostic::new(
                "E_FFPROBE",
                format!("could not inspect image `{}`: {error}", path.display()),
                span.clone(),
            )
        })?;
    if !output.status.success() {
        return Err(Diagnostic::new(
            "E_SOURCE_DECODABILITY",
            format!(
                "image `{}` is not decodable by FFprobe\n{}",
                path.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            span.clone(),
        ));
    }
    let document: ImageProbeDocument = serde_json::from_slice(&output.stdout).map_err(|error| {
        Diagnostic::new(
            "E_SOURCE_DECODABILITY",
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
        return Err(Diagnostic::new(
            "E_SOURCE_CONTRACT",
            format!(
                "image `{}` must contain exactly one video stream, no audio, and one decoded frame; found {} video stream(s), {audio_count} audio stream(s), and {frame_count:?} decoded frame(s)",
                path.display(),
                videos.len()
            ),
            span.clone(),
        ));
    }
    let decode = Command::new(ffmpeg.executable())
        .args(["-v", "error", "-loop", "1", "-i"])
        .arg(path)
        .args(["-map", "0:v:0", "-frames:v", "1", "-an", "-f", "null", "-"])
        .output()
        .map_err(|error| {
            Diagnostic::new(
                "E_FFMPEG",
                format!("could not decode image `{}`: {error}", path.display()),
                span.clone(),
            )
        })?;
    if !decode.status.success() {
        return Err(Diagnostic::new(
            "E_SOURCE_DECODABILITY",
            format!(
                "image `{}` is not compatible with the renderer's still-image input mode\n{}",
                path.display(),
                String::from_utf8_lossy(&decode.stderr).trim()
            ),
            span.clone(),
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
struct AudioFrameProbeDocument {
    #[serde(default)]
    frames: Vec<AudioFrameProbe>,
}

#[derive(Deserialize)]
struct AudioFrameProbe {
    nb_samples: Option<u64>,
}

pub(crate) fn decoded_audio_samples(
    ffprobe: &Path,
    path: &Path,
    span: &SourceSpan,
    contract_code: &'static str,
) -> Result<u64> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_frames",
            "-show_entries",
            "frame=nb_samples",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|error| {
            Diagnostic::new(
                "E_FFPROBE",
                format!(
                    "could not count decoded audio samples in `{}`: {error}",
                    path.display()
                ),
                span.clone(),
            )
        })?;
    if !output.status.success() {
        return Err(Diagnostic::new(
            contract_code,
            format!(
                "FFprobe could not count decoded audio samples in `{}`\n{}",
                path.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            span.clone(),
        ));
    }
    let document: AudioFrameProbeDocument =
        serde_json::from_slice(&output.stdout).map_err(|error| {
            Diagnostic::new(
                contract_code,
                format!(
                    "FFprobe returned invalid audio frame metadata for `{}`: {error}",
                    path.display()
                ),
                span.clone(),
            )
        })?;
    let mut samples = 0_u64;
    for frame in document.frames {
        let count = frame.nb_samples.ok_or_else(|| {
            Diagnostic::new(
                contract_code,
                format!(
                    "FFprobe omitted a decoded audio sample count for `{}`",
                    path.display()
                ),
                span.clone(),
            )
        })?;
        samples = samples
            .checked_add(count)
            .ok_or_else(|| audio_duration_overflow(span))?;
    }
    if samples == 0 {
        return Err(Diagnostic::new(
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
    let frames = validate_video_contract(path, video, span, &document)?;
    decode_video_frame(path, span, ffmpeg)?;
    let has_audio = document
        .streams
        .iter()
        .any(|stream| stream.codec_type.as_deref() == Some("audio"));
    Ok((frames, has_audio))
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
            Diagnostic::new(
                "E_SOURCE_CONTRACT",
                format!("audio `{}` contains no audio stream", path.display()),
                span.clone(),
            )
        })?;
    let _ = stream;
    let samples = decoded_audio_samples(ffprobe.executable(), path, span, "E_SOURCE_CONTRACT")?;
    let decode = Command::new(ffmpeg.executable())
        .args(["-v", "error", "-xerror", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-frames:a", "1", "-f", "null", "-"])
        .output()
        .map_err(|error| {
            Diagnostic::new(
                "E_FFMPEG",
                format!("could not decode audio `{}`: {error}", path.display()),
                span.clone(),
            )
        })?;
    if !decode.status.success() {
        return Err(Diagnostic::new(
            "E_SOURCE_DECODABILITY",
            format!(
                "audio `{}` is not decodable by FFmpeg\n{}",
                path.display(),
                String::from_utf8_lossy(&decode.stderr).trim()
            ),
            span.clone(),
        ));
    }
    Ok(AudioDomain::new(samples, audio))
}

fn audio_duration_overflow(span: &SourceSpan) -> Diagnostic {
    Diagnostic::new(
        "E_AUDIO_DURATION_OVERFLOW",
        "audio duration exceeds the supported range",
        span.clone(),
    )
}

fn probe_video(
    path: &Path,
    span: &SourceSpan,
    ffprobe: &ToolIdentity,
) -> Result<VideoProbeDocument> {
    let output = Command::new(ffprobe.executable())
        .args([
            "-v",
            "error",
            "-count_frames",
            "-show_entries",
            "stream=codec_type,nb_read_frames,duration_ts,time_base,avg_frame_rate",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|error| {
            Diagnostic::new(
                "E_FFPROBE",
                format!("could not inspect video `{}`: {error}", path.display()),
                span.clone(),
            )
        })?;
    if !output.status.success() {
        return Err(Diagnostic::new(
            "E_SOURCE_DECODABILITY",
            format!(
                "video `{}` is not decodable by FFprobe\n{}",
                path.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            span.clone(),
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| {
        Diagnostic::new(
            "E_SOURCE_DECODABILITY",
            format!(
                "FFprobe returned invalid video metadata for `{}`: {error}",
                path.display()
            ),
            span.clone(),
        )
    })
}

fn validate_video_contract(
    path: &Path,
    video: &VideoSpec,
    span: &SourceSpan,
    document: &VideoProbeDocument,
) -> Result<FrameCount> {
    let videos = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("video"))
        .collect::<Vec<_>>();
    if videos.len() != 1 {
        return Err(Diagnostic::new(
            "E_SOURCE_CONTRACT",
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
        return Err(Diagnostic::new(
            "E_SOURCE_CONTRACT",
            format!(
                "video `{}` must contain at least one decodable frame; FFprobe counted {decoded_frames:?}",
                path.display()
            ),
            span.clone(),
        ));
    }
    let Some((available_numerator, available_denominator)) = video_duration(stream) else {
        return Err(Diagnostic::new(
            "E_SOURCE_CONTRACT",
            format!(
                "video `{}` does not expose a usable stream duration",
                path.display()
            ),
            span.clone(),
        ));
    };
    FrameCount::covering_duration(
        available_numerator,
        available_denominator,
        video.fps(),
        span,
    )
}

fn decode_video_frame(path: &Path, span: &SourceSpan, ffmpeg: &ToolIdentity) -> Result<()> {
    let decode = Command::new(ffmpeg.executable())
        .args(["-v", "error", "-xerror", "-i"])
        .arg(path)
        .args(["-map", "0:v:0", "-frames:v", "1", "-an", "-f", "null", "-"])
        .output()
        .map_err(|error| {
            Diagnostic::new(
                "E_FFMPEG",
                format!("could not decode video `{}`: {error}", path.display()),
                span.clone(),
            )
        })?;
    if !decode.status.success() {
        return Err(Diagnostic::new(
            "E_SOURCE_DECODABILITY",
            format!(
                "video `{}` is not compatible with the renderer's video input mode\n{}",
                path.display(),
                String::from_utf8_lossy(&decode.stderr).trim()
            ),
            span.clone(),
        ));
    }
    Ok(())
}

fn video_duration(stream: &VideoProbeStream) -> Option<(u128, u128)> {
    stream
        .duration_ts
        .as_ref()
        .and_then(ProbeInteger::get)
        .zip(stream.time_base.as_deref().and_then(parse_positive_ratio))
        .and_then(|(duration, (time_numerator, time_denominator))| {
            duration
                .checked_mul(time_numerator)
                .map(|numerator| (numerator, time_denominator))
        })
        .or_else(|| {
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
