use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::media_tool;
use crate::model::{AudioDomain, AudioSpec, VideoDomain};
use crate::preflight::tools::decoded_audio_samples;
use crate::preflight::{AudioEncoding, VideoEncoding, WorkingArtifactContract};
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
    channel_layout: Option<String>,
    sample_fmt: Option<String>,
    bits_per_raw_sample: Option<String>,
    color_primaries: Option<String>,
    color_transfer: Option<String>,
    color_space: Option<String>,
    color_range: Option<String>,
    chroma_location: Option<String>,
}

#[derive(Clone, Copy)]
enum CountEvidence {
    Decoded,
    TrustedNativeRecipe,
}

pub(super) fn verify_prepared_artifact(
    ffprobe: &Path,
    path: &Path,
    contract: &WorkingArtifactContract,
    video_encoding: VideoEncoding,
    audio_encoding: AudioEncoding,
) -> Result<()> {
    verify_prepared_artifact_with(
        ffprobe,
        path,
        contract,
        video_encoding,
        audio_encoding,
        CountEvidence::Decoded,
    )
}

pub(super) fn verify_native_transient_artifact(
    ffprobe: &Path,
    path: &Path,
    contract: &WorkingArtifactContract,
    video_encoding: VideoEncoding,
    audio_encoding: AudioEncoding,
) -> Result<()> {
    verify_prepared_artifact_with(
        ffprobe,
        path,
        contract,
        video_encoding,
        audio_encoding,
        CountEvidence::TrustedNativeRecipe,
    )
}

pub(super) fn verify_native_result_artifact(
    ffprobe: &Path,
    path: &Path,
    contract: &WorkingArtifactContract,
    video_encoding: VideoEncoding,
    audio_encoding: AudioEncoding,
) -> Result<()> {
    verify_native_transient_artifact(ffprobe, path, contract, video_encoding, audio_encoding)?;
    let audio = match contract {
        WorkingArtifactContract::Video { audio, .. } | WorkingArtifactContract::Audio { audio } => {
            audio
        }
    };
    verify_audio_samples(ffprobe, path, audio.samples())
}

fn verify_prepared_artifact_with(
    ffprobe: &Path,
    path: &Path,
    contract: &WorkingArtifactContract,
    video_encoding: VideoEncoding,
    audio_encoding: AudioEncoding,
    count_evidence: CountEvidence,
) -> Result<()> {
    match contract {
        WorkingArtifactContract::Video { video, audio } => verify_video_artifact_with(
            ffprobe,
            path,
            video,
            audio.audio_spec(),
            true,
            Some(audio.samples()),
            video_encoding,
            Some(audio_encoding),
            count_evidence,
        ),
        WorkingArtifactContract::Audio { audio } => verify_audio_artifact(
            ffprobe,
            path,
            audio,
            audio.audio_spec(),
            audio_encoding,
            count_evidence,
        ),
    }
}

fn probe_artifact(
    ffprobe: &Path,
    path: &Path,
    count_evidence: CountEvidence,
) -> Result<ProbeDocument> {
    let mut command = Command::new(ffprobe);
    command.args(["-v", "error"]);
    if matches!(count_evidence, CountEvidence::Decoded) {
        command.arg("-count_frames");
    }
    command.args(["-show_streams", "-of", "json"]).arg(path);
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

#[expect(
    clippy::too_many_arguments,
    reason = "artifact verification keeps every independent physical stream expectation explicit"
)]
pub(super) fn verify_video_artifact(
    ffprobe: &Path,
    path: &Path,
    domain: &VideoDomain,
    audio: AudioSpec,
    expect_audio: bool,
    expected_audio_samples: Option<u64>,
    encoding: VideoEncoding,
    audio_encoding: Option<AudioEncoding>,
) -> Result<()> {
    verify_video_artifact_with(
        ffprobe,
        path,
        domain,
        audio,
        expect_audio,
        expected_audio_samples,
        encoding,
        audio_encoding,
        CountEvidence::Decoded,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "artifact verification keeps every independent physical stream expectation explicit"
)]
fn verify_video_artifact_with(
    ffprobe: &Path,
    path: &Path,
    domain: &VideoDomain,
    audio: AudioSpec,
    expect_audio: bool,
    expected_audio_samples: Option<u64>,
    encoding: VideoEncoding,
    audio_encoding: Option<AudioEncoding>,
    count_evidence: CountEvidence,
) -> Result<()> {
    let document = probe_artifact(ffprobe, path, count_evidence)?;
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
    verify_video_format(path, video, domain, encoding)?;
    verify_video_timing(path, video, domain, count_evidence)?;
    if let Some(audio_stream) = audios.first() {
        verify_audio_stream(path, audio_stream, audio, audio_encoding)?;
        if matches!(count_evidence, CountEvidence::Decoded)
            && let Some(expected_samples) = expected_audio_samples
        {
            verify_audio_samples(ffprobe, path, expected_samples)?;
        }
    }
    Ok(())
}

fn verify_video_format(
    path: &Path,
    video: &ProbeStream,
    domain: &VideoDomain,
    encoding: VideoEncoding,
) -> Result<()> {
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
    if video.pix_fmt.as_deref() != Some(encoding.pixel_format()) {
        return Err(contract_error(
            path,
            &format!(
                "expected pixel format {}, found {:?}",
                encoding.pixel_format(),
                video.pix_fmt
            ),
        ));
    }
    let actual_bits = video
        .bits_per_raw_sample
        .as_deref()
        .and_then(|value| value.parse::<u8>().ok());
    if actual_bits != Some(encoding.component_bits()) {
        return Err(contract_error(
            path,
            &format!(
                "expected {}-bit components, found {:?}",
                encoding.component_bits(),
                video.bits_per_raw_sample
            ),
        ));
    }
    verify_video_color(path, video, encoding)?;
    if video.sample_aspect_ratio.as_deref() != Some("1:1") {
        return Err(contract_error(
            path,
            &format!(
                "expected square pixels (1:1), found {:?}",
                video.sample_aspect_ratio
            ),
        ));
    }
    Ok(())
}

fn verify_video_timing(
    path: &Path,
    video: &ProbeStream,
    domain: &VideoDomain,
    count_evidence: CountEvidence,
) -> Result<()> {
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
    if matches!(count_evidence, CountEvidence::Decoded) {
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
    }
    verify_zero_start(path, video)?;
    Ok(())
}

fn verify_video_color(path: &Path, stream: &ProbeStream, encoding: VideoEncoding) -> Result<()> {
    let expected = super::color::metadata(encoding.color());
    let actual = (
        stream.color_primaries.as_deref(),
        stream.color_transfer.as_deref(),
        stream.color_space.as_deref(),
        stream.color_range.as_deref(),
    );
    if actual
        != (
            Some(expected.primaries),
            Some(expected.transfer),
            Some(expected.matrix),
            Some(expected.range),
        )
    {
        return Err(contract_error(
            path,
            &format!(
                "expected color primaries={0}, transfer={1}, matrix={2}, range={3}; found primaries={4:?}, transfer={5:?}, matrix={6:?}, range={7:?}",
                expected.primaries,
                expected.transfer,
                expected.matrix,
                expected.range,
                actual.0,
                actual.1,
                actual.2,
                actual.3
            ),
        ));
    }
    if let Some(expected_location) = encoding.chroma_location()
        && stream.chroma_location.as_deref() != Some(expected_location.ffmpeg_name())
    {
        return Err(contract_error(
            path,
            &format!(
                "expected chroma location {}, found {:?}",
                expected_location.ffmpeg_name(),
                stream.chroma_location
            ),
        ));
    }
    Ok(())
}

fn verify_audio_artifact(
    ffprobe: &Path,
    path: &Path,
    domain: &AudioDomain,
    audio: AudioSpec,
    encoding: AudioEncoding,
    count_evidence: CountEvidence,
) -> Result<()> {
    let document = probe_artifact(ffprobe, path, count_evidence)?;
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
    verify_audio_stream(path, audios[0], audio, Some(encoding))?;
    if matches!(count_evidence, CountEvidence::Decoded) {
        verify_audio_samples(ffprobe, path, domain.samples())
    } else {
        Ok(())
    }
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

fn verify_audio_stream(
    path: &Path,
    stream: &ProbeStream,
    audio: AudioSpec,
    encoding: Option<AudioEncoding>,
) -> Result<()> {
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
    if let Some(encoding) = encoding {
        if stream.channel_layout.as_deref() != Some(encoding.channel_layout()) {
            return Err(contract_error(
                path,
                &format!(
                    "expected audio channel layout {}, found {:?}",
                    encoding.channel_layout(),
                    stream.channel_layout
                ),
            ));
        }
        if stream.sample_fmt.as_deref() != Some(encoding.sample_format()) {
            return Err(contract_error(
                path,
                &format!(
                    "expected audio sample format {}, found {:?}",
                    encoding.sample_format(),
                    stream.sample_fmt
                ),
            ));
        }
        let actual_bits = stream
            .bits_per_raw_sample
            .as_deref()
            .and_then(|value| value.parse::<u8>().ok());
        if actual_bits != Some(encoding.component_bits()) {
            return Err(contract_error(
                path,
                &format!(
                    "expected {}-bit audio samples, found {:?}",
                    encoding.component_bits(),
                    stream.bits_per_raw_sample
                ),
            ));
        }
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
            channel_layout: None,
            sample_fmt: None,
            bits_per_raw_sample: None,
            color_primaries: None,
            color_transfer: None,
            color_space: None,
            color_range: None,
            chroma_location: None,
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

    #[test]
    fn artifact_color_tags_are_part_of_the_physical_contract() {
        let path = Path::new("artifact.mkv");
        let mut stream = stream_with_start(Some("0"));
        stream.color_primaries = Some("bt709".to_owned());
        stream.color_transfer = Some("smpte2084".to_owned());
        stream.color_space = Some("bt709".to_owned());
        stream.color_range = Some("tv".to_owned());
        let error = verify_video_color(
            path,
            &stream,
            crate::preflight::RenderPolicy::CURRENT.working_video_encoding(),
        )
        .expect_err("wrong transfer tag must invalidate an artifact");
        assert_eq!(error.code, "E_ARTIFACT_CONTRACT");
        assert!(error.message.contains("transfer=bt709"));
        assert!(error.message.contains("smpte2084"));
    }

    #[test]
    fn artifact_audio_encoding_is_part_of_the_physical_contract() {
        let path = Path::new("artifact.mka");
        let mut stream = stream_with_start(Some("0"));
        stream.sample_rate = Some("48000".to_owned());
        stream.channels = Some(2);
        stream.channel_layout = Some("stereo".to_owned());
        stream.sample_fmt = Some("s32".to_owned());
        stream.bits_per_raw_sample = Some("24".to_owned());
        let error = verify_audio_stream(
            path,
            &stream,
            AudioSpec::default(),
            Some(crate::preflight::RenderPolicy::CURRENT.working_audio_encoding()),
        )
        .expect_err("wrong sample representation must invalidate an artifact");
        assert_eq!(error.code, "E_ARTIFACT_CONTRACT");
        assert!(error.message.contains("sample format s16"));
    }

    #[test]
    fn artifact_audio_channel_layout_is_part_of_the_physical_contract() {
        let path = Path::new("artifact.mka");
        let mut stream = stream_with_start(Some("0"));
        stream.sample_rate = Some("48000".to_owned());
        stream.channels = Some(2);
        stream.channel_layout = Some("2 channels".to_owned());
        stream.sample_fmt = Some("s16".to_owned());
        stream.bits_per_raw_sample = Some("16".to_owned());
        let error = verify_audio_stream(
            path,
            &stream,
            AudioSpec::default(),
            Some(crate::preflight::RenderPolicy::CURRENT.working_audio_encoding()),
        )
        .expect_err("wrong channel layout must invalidate an artifact");
        assert_eq!(error.code, "E_ARTIFACT_CONTRACT");
        assert!(error.message.contains("channel layout stereo"));
    }
}
