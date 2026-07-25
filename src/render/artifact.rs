#![allow(clippy::trivially_copy_pass_by_ref)]

use std::path::Path;
use std::process::{Command, Output};

use serde::Deserialize;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{AudioDomain, AudioSpec, VideoDomain};
use crate::preflight::tools::decoded_audio_samples;
use crate::preflight::{PreparedNode, PreparedNodeMedia};
use crate::source::SourceSpan;

#[derive(Deserialize)]
struct ProbeDocument {
    streams: Vec<ProbeStream>,
}

#[derive(Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    pix_fmt: Option<String>,
    r_frame_rate: Option<String>,
    nb_read_frames: Option<String>,
    start_time: Option<String>,
    sample_aspect_ratio: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u8>,
}

pub(super) fn verify_prepared_artifact(
    ffprobe: &Path,
    path: &Path,
    node: &PreparedNode,
    audio: &AudioSpec,
    pixel_format: &str,
) -> Result<()> {
    match node.media() {
        PreparedNodeMedia::Video { domain, .. } => {
            verify_video_artifact(ffprobe, path, domain, audio, true, true, pixel_format)
        }
        PreparedNodeMedia::Audio { domain, .. } => {
            verify_audio_artifact(ffprobe, path, domain, audio)
        }
    }
}

fn probe_artifact(ffprobe: &Path, path: &Path) -> Result<ProbeDocument> {
    let mut command = Command::new(ffprobe);
    command
        .args([
            "-v",
            "error",
            "-count_frames",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(path);
    let output = run_output(command, "E_FFPROBE", &SourceSpan::file_start(path))?;
    serde_json::from_slice(&output.stdout).map_err(|error| {
        Diagnostic::new(
            "E_ARTIFACT_CONTRACT",
            format!(
                "FFprobe returned invalid JSON for `{}`: {error}",
                path.display()
            ),
            SourceSpan::file_start(path),
        )
    })
}

#[allow(clippy::too_many_lines)]
pub(super) fn verify_video_artifact(
    ffprobe: &Path,
    path: &Path,
    domain: &VideoDomain,
    audio: &AudioSpec,
    expect_audio: bool,
    exact_audio_samples: bool,
    pixel_format: &str,
) -> Result<()> {
    let document = probe_artifact(ffprobe, path)?;
    let videos = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("video"))
        .collect::<Vec<_>>();
    let audios = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
        .collect::<Vec<_>>();
    let expected_audio_count = usize::from(expect_audio);
    if videos.len() != 1 || audios.len() != expected_audio_count {
        return Err(contract_error(
            path,
            &format!(
                "expected one video stream and {expected_audio_count} audio stream(s), found {} video and {} audio streams",
                videos.len(),
                audios.len()
            ),
        ));
    }
    let video = videos[0];
    if video.width != Some(domain.width()) || video.height != Some(domain.height()) {
        return Err(contract_error(
            path,
            &format!(
                "expected {}x{}, found {:?}x{:?}",
                domain.width(),
                domain.height(),
                video.width,
                video.height
            ),
        ));
    }
    if video.pix_fmt.as_deref() != Some(pixel_format) {
        return Err(contract_error(
            path,
            &format!("expected {pixel_format}, found {:?}", video.pix_fmt),
        ));
    }
    let expected_rate = format!(
        "{}/{}",
        domain.frame_rate().numerator(),
        domain.frame_rate().denominator()
    );
    if domain.frames().0 > 1 && video.r_frame_rate.as_deref() != Some(expected_rate.as_str()) {
        return Err(contract_error(
            path,
            &format!(
                "expected frame rate {expected_rate}, found {:?}",
                video.r_frame_rate
            ),
        ));
    }
    let actual_frames = video
        .nb_read_frames
        .as_deref()
        .and_then(|value| value.parse::<u64>().ok());
    if actual_frames != Some(domain.frames().0) {
        return Err(contract_error(
            path,
            &format!(
                "expected {} frames, FFprobe counted {:?}",
                domain.frames().0,
                actual_frames
            ),
        ));
    }
    verify_zero_start(path, video)?;
    if video.sample_aspect_ratio.as_deref() != Some("1:1") {
        return Err(contract_error(
            path,
            &format!(
                "expected square pixels (1:1), found {:?}",
                video.sample_aspect_ratio
            ),
        ));
    }
    if let Some(audio_stream) = audios.first() {
        verify_audio_stream(path, audio_stream, audio)?;
        if exact_audio_samples {
            let expected_samples = audio.samples_for_frames(
                domain.frames(),
                domain.frame_rate(),
                &SourceSpan::file_start(path),
            )?;
            verify_audio_samples(ffprobe, path, expected_samples)?;
        }
    }
    Ok(())
}

fn verify_audio_artifact(
    ffprobe: &Path,
    path: &Path,
    domain: &AudioDomain,
    audio: &AudioSpec,
) -> Result<()> {
    let document = probe_artifact(ffprobe, path)?;
    let videos = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("video"))
        .count();
    let audios = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
        .collect::<Vec<_>>();
    if videos != 0 || audios.len() != 1 {
        return Err(contract_error(
            path,
            &format!(
                "expected one audio stream and no video, found {videos} video and {} audio streams",
                audios.len()
            ),
        ));
    }
    verify_audio_stream(path, audios[0], audio)?;
    verify_audio_samples(ffprobe, path, domain.samples())
}

fn verify_audio_samples(ffprobe: &Path, path: &Path, expected: u64) -> Result<()> {
    let actual = decoded_audio_samples(
        ffprobe,
        path,
        &SourceSpan::file_start(path),
        "E_ARTIFACT_CONTRACT",
    )?;
    if actual != expected {
        return Err(contract_error(
            path,
            &format!("expected {expected} audio samples, FFprobe decoded {actual}"),
        ));
    }
    Ok(())
}

fn verify_audio_stream(path: &Path, stream: &ProbeStream, audio: &AudioSpec) -> Result<()> {
    let expected_sample_rate = audio.sample_rate().to_string();
    if stream.sample_rate.as_deref() != Some(expected_sample_rate.as_str()) {
        return Err(contract_error(
            path,
            &format!(
                "expected audio sample rate {}, found {:?}",
                audio.sample_rate(),
                stream.sample_rate
            ),
        ));
    }
    if stream.channels != Some(audio.channels()) {
        return Err(contract_error(
            path,
            &format!(
                "expected {} audio channels, found {:?}",
                audio.channels(),
                stream.channels
            ),
        ));
    }
    verify_zero_start(path, stream)
}

fn verify_zero_start(path: &Path, stream: &ProbeStream) -> Result<()> {
    let start = stream
        .start_time
        .as_deref()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0);
    if start.abs() > 0.000_001 {
        return Err(contract_error(
            path,
            &format!("timestamps must begin at zero, found {start}"),
        ));
    }
    Ok(())
}

fn contract_error(path: &Path, message: &str) -> Diagnostic {
    Diagnostic::new(
        "E_ARTIFACT_CONTRACT",
        format!(
            "artifact `{}` violates its contract: {message}",
            path.display()
        ),
        SourceSpan::file_start(path),
    )
}

fn run_output(mut command: Command, code: &'static str, span: &SourceSpan) -> Result<Output> {
    let debug = format!("{command:?}");
    let output = command.output().map_err(|error| {
        Diagnostic::new(
            code,
            format!("could not start external tool: {error}"),
            span.clone(),
        )
        .note(debug.clone())
    })?;
    if !output.status.success() {
        return Err(Diagnostic::new(
            code,
            format!(
                "external tool exited with {}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            span.clone(),
        )
        .note(debug));
    }
    Ok(output)
}
