use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::media_tool;
use crate::model::{AudioDomain, AudioSpec, VideoDomain};
use crate::preflight::WorkingArtifactContract;
use crate::preflight::tools::decoded_audio_samples;
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
    contract: &WorkingArtifactContract,
    pixel_format: &str,
) -> Result<()> {
    match contract {
        WorkingArtifactContract::Video { video, audio } => verify_video_artifact(
            ffprobe,
            path,
            video,
            audio.audio_spec(),
            true,
            Some(audio.samples()),
            pixel_format,
        ),
        WorkingArtifactContract::Audio { audio } => {
            verify_audio_artifact(ffprobe, path, audio, audio.audio_spec())
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
    let output = media_tool::capture(
        command,
        BuiltinDiagnostic::Ffprobe,
        &SourceSpan::file_start(path),
    )?;
    serde_json::from_slice(&output.stdout).map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::ArtifactContract,
            format!(
                "FFprobe returned invalid JSON for `{}`: {error}",
                path.display()
            ),
            SourceSpan::file_start(path),
        )
    })
}

pub(super) fn verify_video_artifact(
    ffprobe: &Path,
    path: &Path,
    domain: &VideoDomain,
    audio: AudioSpec,
    expect_audio: bool,
    expected_audio_samples: Option<u64>,
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
        if let Some(expected_samples) = expected_audio_samples {
            verify_audio_samples(ffprobe, path, expected_samples)?;
        }
    }
    Ok(())
}

fn verify_audio_artifact(
    ffprobe: &Path,
    path: &Path,
    domain: &AudioDomain,
    audio: AudioSpec,
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
        BuiltinDiagnostic::ArtifactContract,
    )?;
    if actual != expected {
        return Err(contract_error(
            path,
            &format!("expected {expected} audio samples, FFprobe decoded {actual}"),
        ));
    }
    Ok(())
}

fn verify_audio_stream(path: &Path, stream: &ProbeStream, audio: AudioSpec) -> Result<()> {
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
    let Some(encoded) = stream.start_time.as_deref() else {
        return Err(contract_error(path, "stream start timestamp is missing"));
    };
    let Ok(start) = encoded.parse::<f64>() else {
        return Err(contract_error(
            path,
            &format!("stream start timestamp is not numeric: {encoded:?}"),
        ));
    };
    if !start.is_finite() {
        return Err(contract_error(
            path,
            &format!("stream start timestamp is not finite: {encoded:?}"),
        ));
    }
    if start.abs() > 0.000_001 {
        return Err(contract_error(
            path,
            &format!("timestamps must begin at zero, found {start}"),
        ));
    }
    Ok(())
}

fn contract_error(path: &Path, message: &str) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::ArtifactContract,
        format!(
            "artifact `{}` violates its contract: {message}",
            path.display()
        ),
        SourceSpan::file_start(path),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream_with_start(start_time: Option<&str>) -> ProbeStream {
        ProbeStream {
            codec_type: None,
            width: None,
            height: None,
            pix_fmt: None,
            r_frame_rate: None,
            nb_read_frames: None,
            start_time: start_time.map(str::to_owned),
            sample_aspect_ratio: None,
            sample_rate: None,
            channels: None,
        }
    }

    #[test]
    fn zero_start_requires_present_finite_numeric_metadata() {
        let path = Path::new("artifact.mkv");
        for start in [None, Some("N/A"), Some("NaN"), Some("inf")] {
            let error = verify_zero_start(path, &stream_with_start(start))
                .expect_err("invalid start time must be rejected");
            assert_eq!(error.code, "E_ARTIFACT_CONTRACT");
        }
    }

    #[test]
    fn zero_start_accepts_only_values_within_tolerance() {
        let path = Path::new("artifact.mkv");
        for start in ["0", "-0.000001", "0.000001"] {
            verify_zero_start(path, &stream_with_start(Some(start)))
                .expect("zero start within tolerance");
        }
        verify_zero_start(path, &stream_with_start(Some("0.000002")))
            .expect_err("nonzero start must be rejected");
    }
}
