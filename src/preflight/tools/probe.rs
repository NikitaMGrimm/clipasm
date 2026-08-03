use std::path::Path;
#[cfg(feature = "native")]
use std::process::Command;

use serde::Deserialize;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
#[cfg(feature = "native")]
use crate::media_tool;
#[cfg(feature = "native")]
use crate::model::{AudioDomain, AudioSpec};
use crate::model::{ColorSpec, FrameCount, VideoSpec};
use crate::preflight::PreparedSourceColor;
use crate::source::SourceSpan;

#[cfg(feature = "native")]
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
    pix_fmt: Option<String>,
    color_primaries: Option<String>,
    color_transfer: Option<String>,
    color_space: Option<String>,
    color_range: Option<String>,
    #[serde(default)]
    side_data_list: Vec<ProbeSideData>,
}

#[derive(Deserialize)]
struct ProbeSideData {
    side_data_type: Option<String>,
}

#[cfg(feature = "native")]
pub(crate) fn verify_image_decodable(
    path: &Path,
    span: &SourceSpan,
    ffmpeg: &ToolIdentity,
    ffprobe: &ToolIdentity,
) -> Result<PreparedSourceColor> {
    let mut probe = Command::new(ffprobe.executable());
    probe
        .args([
            "-v",
            "error",
            "-count_frames",
            "-show_entries",
            "stream=codec_type,nb_read_frames,pix_fmt,color_primaries,color_transfer,color_space,color_range:stream_side_data=side_data_type",
            "-of",
            "json",
            "-f",
            "image2",
            "-pattern_type",
            "none",
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
    let document = parse_image_probe(path, span, &output.stdout)?;
    let color = validate_image_document(path, span, &document)?;
    decode_image_frame(path, span, ffmpeg)?;
    Ok(color)
}

pub(crate) fn validate_image_probe_json(
    path: &Path,
    span: &SourceSpan,
    probe_json: &str,
) -> Result<PreparedSourceColor> {
    let document = parse_image_probe(path, span, probe_json.as_bytes())?;
    validate_image_document(path, span, &document)
}

fn parse_image_probe(path: &Path, span: &SourceSpan, bytes: &[u8]) -> Result<ImageProbeDocument> {
    serde_json::from_slice(bytes).map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::SourceDecodability,
            format!(
                "FFprobe returned invalid image metadata for `{}`: {error}",
                path.display()
            ),
            span.clone(),
        )
    })
}

fn validate_image_document(
    path: &Path,
    span: &SourceSpan,
    document: &ImageProbeDocument,
) -> Result<PreparedSourceColor> {
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
    if videos.len() == 1 && audio_count == 0 && frame_count == Some(1) {
        return validate_image_color(path, span, videos[0]);
    }
    Err(Diagnostic::builtin(
        BuiltinDiagnostic::SourceContract,
        format!(
            "image `{}` must contain exactly one video stream, no audio, and one decoded frame; found {} video stream(s), {audio_count} audio stream(s), and {frame_count:?} decoded frame(s)",
            path.display(),
            videos.len()
        ),
        span.clone(),
    ))
}

fn validate_image_color(
    path: &Path,
    span: &SourceSpan,
    stream: &ImageProbeStream,
) -> Result<PreparedSourceColor> {
    if stream.side_data_list.iter().any(|side_data| {
        side_data
            .side_data_type
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("ICC profile"))
    }) {
        return Err(source_color_error(
            path,
            span,
            "embedded ICC profiles are not supported yet",
        ));
    }
    let Some(pixel_format) = stream.pix_fmt.as_deref() else {
        return Err(source_color_error(
            path,
            span,
            "FFprobe did not report a pixel format",
        ));
    };
    if matches!(
        pixel_format,
        "rgb24" | "bgr24" | "rgb48le" | "rgb48be" | "bgr48le" | "bgr48be"
    ) {
        validate_image_tags(path, span, stream, &["gbr", "rgb"])?;
        return Ok(PreparedSourceColor::image_srgb_rgb(pixel_format.to_owned()));
    }
    if matches!(
        pixel_format,
        "yuvj420p" | "yuvj422p" | "yuvj444p" | "yuvj440p"
    ) {
        validate_image_tags(path, span, stream, &["bt470bg", "smpte170m"])?;
        let chroma_location = is_subsampled(pixel_format).then(|| "center".to_owned());
        return Ok(PreparedSourceColor::image_srgb_yuv(
            pixel_format.to_owned(),
            chroma_location,
        ));
    }
    Err(source_color_error(
        path,
        span,
        &format!(
            "the opaque sRGB image contract requires RGB or JPEG Y'CbCr samples without alpha; found `{pixel_format}`"
        ),
    ))
}

fn validate_image_tags(
    path: &Path,
    span: &SourceSpan,
    stream: &ImageProbeStream,
    accepted_matrices: &[&str],
) -> Result<()> {
    require_unknown_or(
        path,
        span,
        "primaries",
        stream.color_primaries.as_deref(),
        &["bt709"],
    )?;
    require_unknown_or(
        path,
        span,
        "transfer",
        stream.color_transfer.as_deref(),
        &["iec61966-2-1"],
    )?;
    require_unknown_or(
        path,
        span,
        "matrix",
        stream.color_space.as_deref(),
        accepted_matrices,
    )?;
    require_unknown_or(path, span, "range", stream.color_range.as_deref(), &["pc"])
}

#[cfg(feature = "native")]
fn decode_image_frame(path: &Path, span: &SourceSpan, ffmpeg: &ToolIdentity) -> Result<()> {
    let mut decode = Command::new(ffmpeg.executable());
    decode
        .args([
            "-v",
            "error",
            "-f",
            "image2",
            "-loop",
            "1",
            "-pattern_type",
            "none",
            "-i",
        ])
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

#[cfg(feature = "native")]
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
    pix_fmt: Option<String>,
    color_primaries: Option<String>,
    color_transfer: Option<String>,
    color_space: Option<String>,
    color_range: Option<String>,
    chroma_location: Option<String>,
    #[serde(default)]
    side_data_list: Vec<ProbeSideData>,
    #[cfg(feature = "native")]
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

#[cfg(feature = "native")]
pub(crate) fn verify_video_decodable(
    path: &Path,
    video: &VideoSpec,
    span: &SourceSpan,
    ffmpeg: &ToolIdentity,
    ffprobe: &ToolIdentity,
) -> Result<(FrameCount, bool, PreparedSourceColor)> {
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
) -> Result<(FrameCount, bool, PreparedSourceColor)> {
    let document = parse_video_probe(path, span, probe_json.as_bytes())?;
    validate_video_document(path, video, span, &document)
}

#[cfg(feature = "native")]
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

#[cfg(feature = "native")]
fn audio_duration_overflow(span: &SourceSpan) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::AudioDurationOverflow,
        "audio duration exceeds the supported range",
        span.clone(),
    )
}

#[cfg(feature = "native")]
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
            "stream=codec_type,nb_read_frames,duration_ts,time_base,avg_frame_rate,sample_rate,pix_fmt,color_primaries,color_transfer,color_space,color_range,chroma_location:stream_side_data=side_data_type",
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
) -> Result<(FrameCount, bool, PreparedSourceColor)> {
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
    let source_color = validate_video_color(path, span, stream)?;
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
    Ok((frames, has_audio, source_color))
}

fn validate_video_color(
    path: &Path,
    span: &SourceSpan,
    stream: &VideoProbeStream,
) -> Result<PreparedSourceColor> {
    let transfer = stream.color_transfer.as_deref();
    if matches!(transfer, Some("smpte2084" | "arib-std-b67")) {
        return Err(source_color_error(
            path,
            span,
            &format!(
                "HDR transfer `{}` is not supported by the `sdr_bt709` project profile; explicit tone mapping is not implemented",
                transfer.expect("matched transfer")
            ),
        ));
    }
    if stream.side_data_list.iter().any(|side_data| {
        side_data.side_data_type.as_deref().is_some_and(|kind| {
            kind.contains("Mastering display") || kind.contains("Content light")
        })
    }) {
        return Err(source_color_error(
            path,
            span,
            "HDR mastering metadata is not supported by the `sdr_bt709` project profile",
        ));
    }
    let tuple = (
        stream.color_primaries.as_deref(),
        transfer,
        stream.color_space.as_deref(),
        stream.color_range.as_deref(),
    );
    let color = match tuple {
        (Some("bt709"), Some("bt709"), Some("bt709"), Some("tv")) => ColorSpec::SDR_BT709,
        (Some("bt709"), Some("bt709"), Some("bt709"), Some("pc")) => ColorSpec::BT709_FULL,
        _ => {
            return Err(source_color_error(
                path,
                span,
                &format!(
                    "video color metadata must explicitly be BT.709 primaries/transfer/matrix with `tv` or `pc` range; found primaries={:?}, transfer={:?}, matrix={:?}, range={:?}",
                    tuple.0, tuple.1, tuple.2, tuple.3
                ),
            ));
        }
    };
    let Some(pixel_format) = stream.pix_fmt.as_deref() else {
        return Err(source_color_error(
            path,
            span,
            "FFprobe did not report a pixel format",
        ));
    };
    if pixel_format.contains('a')
        && (pixel_format.starts_with("yuva") || pixel_format.starts_with("gbrap"))
    {
        return Err(source_color_error(
            path,
            span,
            "video sources with alpha are outside the opaque Video contract",
        ));
    }
    let chroma_location = stream
        .chroma_location
        .as_deref()
        .filter(|value| !is_unknown(value))
        .map(str::to_owned);
    if is_subsampled(pixel_format) && chroma_location.is_none() {
        return Err(source_color_error(
            path,
            span,
            &format!(
                "subsampled pixel format `{pixel_format}` requires an explicit chroma location"
            ),
        ));
    }
    Ok(PreparedSourceColor::explicit_video(
        color,
        pixel_format.to_owned(),
        chroma_location,
    ))
}

fn require_unknown_or(
    path: &Path,
    span: &SourceSpan,
    field: &str,
    actual: Option<&str>,
    accepted: &[&str],
) -> Result<()> {
    if actual.is_none_or(is_unknown) || actual.is_some_and(|value| accepted.contains(&value)) {
        return Ok(());
    }
    Err(source_color_error(
        path,
        span,
        &format!("image {field} metadata {actual:?} conflicts with the sRGB image contract"),
    ))
}

fn is_unknown(value: &str) -> bool {
    matches!(value, "unknown" | "unspecified" | "reserved")
}

fn is_subsampled(pixel_format: &str) -> bool {
    ["420", "422", "411", "410"]
        .iter()
        .any(|marker| pixel_format.contains(marker))
}

fn source_color_error(path: &Path, span: &SourceSpan, detail: &str) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::SourceContract,
        format!(
            "source `{}` has unsupported color: {detail}",
            path.display()
        ),
        span.clone(),
    )
}

#[cfg(feature = "native")]
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

#[cfg(feature = "native")]
fn tool_context(mut error: Diagnostic, mut context: String) -> Diagnostic {
    context.push('\n');
    context.push_str(&error.message);
    error.message = context;
    error
}

#[cfg(feature = "native")]
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

#[cfg(all(test, unix, feature = "native"))]
mod tests {
    use super::*;

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

    #[test]
    fn source_color_requires_explicit_sdr_video_metadata() {
        let path = Path::new("source.mkv");
        let span = SourceSpan::file_start(path);
        let video = VideoSpec::default();
        let untagged = r#"{"streams":[{"codec_type":"video","nb_read_frames":"1","avg_frame_rate":"30/1","pix_fmt":"yuv420p"}]}"#;
        let error = validate_video_probe_json(path, &video, &span, untagged)
            .expect_err("untagged video must be ambiguous");
        assert_eq!(error.code, "E_SOURCE_CONTRACT");
        assert!(error.message.contains("explicitly be BT.709"));

        let tagged = r#"{"streams":[{"codec_type":"video","nb_read_frames":"1","avg_frame_rate":"30/1","pix_fmt":"yuv420p","color_primaries":"bt709","color_transfer":"bt709","color_space":"bt709","color_range":"tv","chroma_location":"left"}]}"#;
        let (_, _, color) =
            validate_video_probe_json(path, &video, &span, tagged).expect("complete SDR metadata");
        assert_eq!(color.color(), ColorSpec::SDR_BT709);
        assert_eq!(color.chroma_location(), Some("left"));
    }

    #[test]
    fn hdr_transfer_is_rejected_without_silent_tone_mapping() {
        let path = Path::new("hdr.mkv");
        let span = SourceSpan::file_start(path);
        for transfer in ["smpte2084", "arib-std-b67"] {
            let probe = format!(
                r#"{{"streams":[{{"codec_type":"video","nb_read_frames":"1","avg_frame_rate":"30/1","pix_fmt":"yuv420p10le","color_primaries":"bt2020","color_transfer":"{transfer}","color_space":"bt2020nc","color_range":"tv","chroma_location":"left"}}]}}"#
            );
            let error = validate_video_probe_json(path, &VideoSpec::default(), &span, &probe)
                .expect_err("HDR is unsupported in the SDR profile");
            assert!(error.message.contains("HDR transfer"));
            assert!(error.message.contains("tone mapping"));
        }
    }

    #[test]
    fn untagged_rgb_stills_have_a_deliberate_srgb_convention() {
        let path = Path::new("still.ppm");
        let span = SourceSpan::file_start(path);
        let probe = r#"{"streams":[{"codec_type":"video","nb_read_frames":"1","pix_fmt":"rgb24","color_primaries":"unknown","color_transfer":"unknown","color_space":"gbr","color_range":"pc"}]}"#;
        let color =
            validate_image_probe_json(path, &span, probe).expect("untagged RGB still is sRGB");
        assert_eq!(color.color(), ColorSpec::SRGB_RGB);
        assert_eq!(
            color.convention(),
            crate::preflight::SourceColorConvention::ImageSrgb
        );

        let alpha = r#"{"streams":[{"codec_type":"video","nb_read_frames":"1","pix_fmt":"rgba","color_primaries":"unknown","color_transfer":"unknown","color_space":"gbr","color_range":"pc"}]}"#;
        let error = validate_image_probe_json(path, &span, alpha)
            .expect_err("alpha is outside the opaque contract");
        assert!(error.message.contains("without alpha"));
    }
}
