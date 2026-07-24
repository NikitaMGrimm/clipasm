use std::fs;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{AudioDomain, AudioSpec, FrameCount, VideoSpec};
use crate::source::SourceSpan;

use super::REQUIRED_FFMPEG_FILTERS;

#[derive(Serialize)]
struct ToolBuildIdentity<'a> {
    executable_content_hash: &'a str,
    version_stdout: &'a str,
    version_stderr: &'a str,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExternalToolIdentity {
    executable: PathBuf,
    content_hash: String,
}

impl ExternalToolIdentity {
    pub(crate) fn executable(&self) -> &Path {
        &self.executable
    }

    pub(crate) fn content_hash(&self) -> &str {
        &self.content_hash
    }
}

pub(crate) fn inspect_external_tool(
    authored: &Path,
    span: &SourceSpan,
) -> Result<ExternalToolIdentity> {
    let candidate = if authored.is_absolute() || authored.components().count() > 1 {
        super::assets::resolve_authored_path(authored, span)?
    } else {
        resolve_executable(
            authored.to_str().ok_or_else(|| {
                Diagnostic::new(
                    "E_EXTERNAL_EXECUTABLE",
                    "external executable name is not valid UTF-8",
                    span.clone(),
                )
            })?,
            "E_EXTERNAL_EXECUTABLE",
        )?
    };
    let executable = fs::canonicalize(&candidate).map_err(|error| {
        Diagnostic::new(
            "E_EXTERNAL_EXECUTABLE",
            format!(
                "could not resolve external executable `{}`: {error}",
                candidate.display()
            ),
            span.clone(),
        )
    })?;
    if !is_executable_file(&executable) {
        return Err(Diagnostic::new(
            "E_EXTERNAL_EXECUTABLE",
            format!(
                "external command `{}` is not executable",
                executable.display()
            ),
            span.clone(),
        ));
    }
    let content_hash = hash_tool_executable(&executable, "E_EXTERNAL_EXECUTABLE")?;
    Ok(ExternalToolIdentity {
        executable,
        content_hash,
    })
}

pub(crate) fn verify_external_tool(
    identity: &ExternalToolIdentity,
    span: &SourceSpan,
) -> Result<()> {
    let current = hash_tool_executable(identity.executable(), "E_EXTERNAL_EXECUTABLE")?;
    if current == identity.content_hash {
        return Ok(());
    }
    Err(Diagnostic::new(
        "E_EXTERNAL_CHANGED",
        format!(
            "external executable `{}` changed after preflight; prepare the program again",
            identity.executable.display()
        ),
        span.clone(),
    ))
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ToolIdentity {
    pub(super) executable: PathBuf,
    pub(super) version_summary: String,
    pub(super) build_fingerprint: String,
}

impl ToolIdentity {
    pub(crate) fn executable(&self) -> &Path {
        &self.executable
    }

    pub(crate) fn version(&self) -> &str {
        &self.version_summary
    }

    pub(crate) fn build_fingerprint(&self) -> &str {
        &self.build_fingerprint
    }
}

pub(crate) fn verify_tool_identity(tool: &ToolIdentity, role: &str) -> Result<()> {
    let current = inspect_tool_identity(tool.executable(), "E_TOOL_CHANGED")?;
    if current.build_fingerprint() == tool.build_fingerprint() {
        return Ok(());
    }
    Err(Diagnostic::new(
        "E_TOOL_CHANGED",
        format!(
            "{role} executable `{}` changed after preflight; prepare the program again",
            tool.executable().display()
        ),
        SourceSpan::file_start(tool.executable()),
    ))
}

pub(super) fn inspect_ffmpeg() -> Result<ToolIdentity> {
    inspect_ffmpeg_at(&resolve_executable("ffmpeg", "E_FFMPEG")?)
}

fn inspect_ffmpeg_at(tool: &Path) -> Result<ToolIdentity> {
    let tool = fs::canonicalize(tool).map_err(|error| {
        Diagnostic::new(
            "E_FFMPEG",
            format!(
                "could not resolve FFmpeg executable `{}`: {error}",
                tool.display()
            ),
            SourceSpan::file_start(tool),
        )
    })?;
    let identity = inspect_tool_identity(&tool, "E_FFMPEG")?;
    let encoders = tool_output(&tool, &["-hide_banner", "-encoders"], "E_FFMPEG")?;
    for encoder in ["libx264", "ffv1", "flac", "aac"] {
        if capability_missing(&encoders, encoder) {
            return Err(Diagnostic::new(
                "E_FFMPEG_CAPABILITY",
                format!("installed FFmpeg does not provide the required `{encoder}` encoder"),
                SourceSpan::file_start(&tool),
            ));
        }
    }
    let muxers = tool_output(&tool, &["-hide_banner", "-muxers"], "E_FFMPEG")?;
    for (muxer, display) in [("mp4", "MP4"), ("matroska", "Matroska")] {
        if capability_missing(&muxers, muxer) {
            return Err(Diagnostic::new(
                "E_FFMPEG_CAPABILITY",
                format!("installed FFmpeg does not provide the required {display} muxer"),
                SourceSpan::file_start(&tool),
            ));
        }
    }
    let filters = tool_output(&tool, &["-hide_banner", "-filters"], "E_FFMPEG")?;
    for filter in REQUIRED_FFMPEG_FILTERS {
        if capability_missing(&filters, filter) {
            return Err(Diagnostic::new(
                "E_FFMPEG_CAPABILITY",
                format!("installed FFmpeg does not provide the required `{filter}` filter"),
                SourceSpan::file_start(&tool),
            ));
        }
    }
    Ok(identity)
}

fn capability_missing(output: &str, capability: &str) -> bool {
    !output
        .lines()
        .any(|line| line.split_whitespace().any(|token| token == capability))
}

pub(super) fn inspect_ffprobe() -> Result<ToolIdentity> {
    let executable = resolve_executable("ffprobe", "E_FFPROBE")?;
    inspect_tool_identity(&executable, "E_FFPROBE")
}

fn resolve_executable(name: &str, code: &'static str) -> Result<PathBuf> {
    let authored = Path::new(name);
    let candidates = if authored.components().count() > 1 {
        vec![authored.to_path_buf()]
    } else {
        let path = std::env::var_os("PATH").ok_or_else(|| {
            Diagnostic::new(
                code,
                format!("could not resolve `{name}` because PATH is not set"),
                SourceSpan::file_start(authored),
            )
        })?;
        std::env::split_paths(&path)
            .flat_map(|directory| executable_candidates(&directory, name))
            .collect()
    };
    candidates
        .into_iter()
        .find(|candidate| is_executable_file(candidate))
        .and_then(|candidate| fs::canonicalize(candidate).ok())
        .ok_or_else(|| {
            Diagnostic::new(
                code,
                format!("could not resolve executable `{name}` on PATH"),
                SourceSpan::file_start(authored),
            )
        })
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn executable_candidates(directory: &Path, name: &str) -> Vec<PathBuf> {
    let candidate = directory.join(name);
    #[cfg(windows)]
    {
        let mut candidates = vec![candidate.clone()];
        if candidate.extension().is_none() {
            candidates.push(candidate.with_extension("exe"));
        }
        candidates
    }
    #[cfg(not(windows))]
    {
        vec![candidate]
    }
}

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

pub(super) fn verify_image_decodable(
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

pub(super) fn verify_video_decodable(
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

pub(super) fn verify_audio_decodable(
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
    let (duration_numerator, duration_denominator) = video_duration(stream).ok_or_else(|| {
        Diagnostic::new(
            "E_SOURCE_CONTRACT",
            format!(
                "audio `{}` does not expose a usable duration",
                path.display()
            ),
            span.clone(),
        )
    })?;
    let numerator = duration_numerator
        .checked_mul(u128::from(audio.sample_rate))
        .ok_or_else(|| audio_duration_overflow(span))?;
    let samples = numerator
        .checked_add(duration_denominator - 1)
        .ok_or_else(|| audio_duration_overflow(span))?
        / duration_denominator;
    let samples = u64::try_from(samples).map_err(|_| audio_duration_overflow(span))?;
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
    Ok(AudioDomain {
        samples,
        sample_rate: audio.sample_rate,
        channels: audio.channels,
    })
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
    FrameCount::covering_duration(available_numerator, available_denominator, video.fps, span)
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

fn inspect_tool_identity(tool: &Path, code: &'static str) -> Result<ToolIdentity> {
    let executable = fs::canonicalize(tool).map_err(|error| {
        Diagnostic::new(
            code,
            format!(
                "could not resolve executable `{}` for identity: {error}",
                tool.display()
            ),
            SourceSpan::file_start(tool),
        )
    })?;
    let version = tool_command_output(&executable, &["-version"], code)?;
    let version_stdout = normalize_tool_output(&version.stdout);
    let version_stderr = normalize_tool_output(&version.stderr);
    let version_summary = version_stdout
        .lines()
        .chain(version_stderr.lines())
        .next()
        .unwrap_or_default()
        .to_owned();
    let executable_content_hash = hash_tool_executable(&executable, code)?;
    let build_fingerprint = crate::compiler::fingerprint::hash_serializable(&ToolBuildIdentity {
        executable_content_hash: &executable_content_hash,
        version_stdout: &version_stdout,
        version_stderr: &version_stderr,
    })?;
    Ok(ToolIdentity {
        executable,
        version_summary,
        build_fingerprint,
    })
}

fn normalize_tool_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_owned()
}

fn hash_tool_executable(tool: &Path, code: &'static str) -> Result<String> {
    let file = fs::File::open(tool).map_err(|error| {
        Diagnostic::new(
            code,
            format!(
                "could not read executable `{}` for identity: {error}",
                tool.display()
            ),
            SourceSpan::file_start(tool),
        )
    })?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = reader.read(&mut buffer).map_err(|error| {
            Diagnostic::new(
                code,
                format!(
                    "could not fingerprint executable `{}`: {error}",
                    tool.display()
                ),
                SourceSpan::file_start(tool),
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn tool_output(tool: &Path, arguments: &[&str], code: &'static str) -> Result<String> {
    let output = tool_command_output(tool, arguments, code)?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr))
}

fn tool_command_output(tool: &Path, arguments: &[&str], code: &'static str) -> Result<Output> {
    const START_ATTEMPTS: usize = 5;
    let mut attempt = 0;
    let output = loop {
        attempt += 1;
        match Command::new(tool).args(arguments).output() {
            Ok(output) => break Ok(output),
            Err(error) if executable_is_temporarily_busy(&error) && attempt < START_ATTEMPTS => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => break Err(error),
        }
    }
    .map_err(|error| {
        Diagnostic::new(
            code,
            format!("could not start `{}`: {error}", tool.display()),
            SourceSpan::file_start(tool),
        )
    })?;
    if !output.status.success() {
        return Err(Diagnostic::new(
            code,
            format!(
                "`{}` exited with {}\n{}",
                tool.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            SourceSpan::file_start(tool),
        ));
    }
    Ok(output)
}

fn executable_is_temporarily_busy(error: &io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(26)
    }
    #[cfg(not(unix))]
    {
        let _ = error;
        false
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard};

    use super::*;

    static FAKE_TOOL_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn fake_tool_test_lock() -> MutexGuard<'static, ()> {
        FAKE_TOOL_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[cfg(unix)]
    fn executable_script(contents: &str) -> (tempfile::TempDir, PathBuf) {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("fake-ffmpeg");
        let staged = directory.path().join(".fake-ffmpeg-staged");
        fs::write(&staged, contents).expect("staged script");
        let mut permissions = fs::metadata(&staged).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&staged, permissions).expect("permissions");
        fs::rename(&staged, &path).expect("publish script");
        (directory, path)
    }

    #[cfg(unix)]
    #[test]
    fn ffmpeg_preflight_requires_all_render_encoders_and_muxers() {
        let _guard = fake_tool_test_lock();
        let (_directory, no_encoder) = executable_script(
            "#!/bin/sh\nif [ \"$1\" = \"-version\" ]; then echo fake; else echo none; fi\n",
        );
        let encoder_error = inspect_ffmpeg_at(&no_encoder).expect_err("missing encoder");
        assert_eq!(
            encoder_error.code, "E_FFMPEG_CAPABILITY",
            "{encoder_error:?}"
        );
        assert!(encoder_error.message.contains("libx264"));

        let (_directory, no_ffv1) = executable_script(
            "#!/bin/sh\nif [ \"$1\" = \"-version\" ]; then echo fake; elif [ \"$2\" = \"-encoders\" ]; then echo libx264; else echo none; fi\n",
        );
        let encoder_error = inspect_ffmpeg_at(&no_ffv1).expect_err("missing FFV1");
        assert_eq!(encoder_error.code, "E_FFMPEG_CAPABILITY");
        assert!(encoder_error.message.contains("ffv1"));

        let (_directory, no_matroska) = executable_script(
            "#!/bin/sh\nif [ \"$1\" = \"-version\" ]; then echo fake; elif [ \"$2\" = \"-encoders\" ]; then echo 'libx264 ffv1 flac aac'; else echo mp4; fi\n",
        );
        let container_error = inspect_ffmpeg_at(&no_matroska).expect_err("missing Matroska");
        assert_eq!(container_error.code, "E_FFMPEG_CAPABILITY");
        assert!(container_error.message.contains("Matroska"));
    }

    #[cfg(unix)]
    #[test]
    fn ffmpeg_preflight_requires_every_render_filter() {
        let _guard = fake_tool_test_lock();
        let (_directory, no_filters) = executable_script(
            "#!/bin/sh\nif [ \"$1\" = \"-version\" ]; then echo fake; elif [ \"$2\" = \"-encoders\" ]; then echo 'libx264 ffv1 flac aac'; elif [ \"$2\" = \"-muxers\" ]; then echo 'mp4 matroska'; else echo none; fi\n",
        );
        let error = inspect_ffmpeg_at(&no_filters).expect_err("missing filters");
        assert_eq!(error.code, "E_FFMPEG_CAPABILITY");
        assert!(error.message.contains("scale"));
    }

    #[cfg(unix)]
    #[test]
    fn ffmpeg_preflight_requires_the_flash_fade_filter() {
        let _guard = fake_tool_test_lock();
        let (_directory, no_fade) = executable_script(
            "#!/bin/sh\nif [ \"$1\" = \"-version\" ]; then echo fake; elif [ \"$2\" = \"-encoders\" ]; then echo 'libx264 ffv1 flac aac'; elif [ \"$2\" = \"-muxers\" ]; then echo 'mp4 matroska'; elif [ \"$2\" = \"-filters\" ]; then echo 'scale crop pad fps setsar format trim setpts tpad concat perspective aresample aformat atrim apad anullsrc color'; else echo none; fi\n",
        );
        let error = inspect_ffmpeg_at(&no_fade).expect_err("missing fade");
        assert_eq!(error.code, "E_FFMPEG_CAPABILITY");
        assert!(error.message.contains("fade"));
    }

    #[cfg(unix)]
    #[test]
    fn tool_build_identity_uses_full_version_output_not_location() {
        let _guard = fake_tool_test_lock();
        let (_first_directory, first) =
            executable_script("#!/bin/sh\nprintf 'tool 1\\nconfiguration alpha  \\r\\n'\n");
        let (_second_directory, second) =
            executable_script("#!/bin/sh\nprintf 'tool 1\\nconfiguration beta\\n'\n");
        let first_identity = inspect_tool_identity(&first, "E_TOOL").expect("first identity");
        let second_identity = inspect_tool_identity(&second, "E_TOOL").expect("second identity");
        assert_eq!(first_identity.version(), "tool 1");
        assert_ne!(
            first_identity.build_fingerprint,
            second_identity.build_fingerprint
        );

        let (_relocated_directory, relocated) =
            executable_script("#!/bin/sh\nprintf 'tool 1\\nconfiguration alpha  \\r\\n'\n");
        let relocated_identity =
            inspect_tool_identity(&relocated, "E_TOOL").expect("relocated identity");
        assert_eq!(
            first_identity.build_fingerprint,
            relocated_identity.build_fingerprint
        );
    }

    #[cfg(unix)]
    #[test]
    fn render_identity_check_rejects_a_tool_changed_after_preflight() {
        let _guard = fake_tool_test_lock();
        let (_directory, tool) = executable_script("#!/bin/sh\necho 'tool 1'\n");
        let identity = inspect_tool_identity(&tool, "E_TOOL").expect("initial identity");

        fs::write(&tool, "#!/bin/sh\necho 'tool 2'\n").expect("replace tool");
        let error = verify_tool_identity(&identity, "test tool").expect_err("changed tool");

        assert_eq!(error.code, "E_TOOL_CHANGED");
        assert!(error.message.contains("prepare the program again"));
    }

    #[test]
    fn image_contract_rejects_video_and_animated_sources() {
        if Command::new("ffmpeg").arg("-version").output().is_err()
            || Command::new("ffprobe").arg("-version").output().is_err()
        {
            return;
        }
        let directory = tempfile::tempdir().expect("temporary directory");
        let video = directory.path().join("video.mp4");
        let animated = directory.path().join("animated.gif");
        let png = directory.path().join("still.png");
        let jpeg = directory.path().join("still.jpg");
        let ppm = directory.path().join("still.ppm");
        let span = SourceSpan::file_start("workflow.yaml");
        let ffmpeg = inspect_ffmpeg().expect("FFmpeg");
        let ffprobe = inspect_ffprobe().expect("FFprobe");
        assert!(ffmpeg.executable().is_absolute());
        assert!(ffprobe.executable().is_absolute());

        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=red:size=16x16:rate=2:duration=1",
                "-c:v",
                "libx264",
            ])
            .arg(&video)
            .status()
            .expect("create video");
        assert!(status.success());
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=red:size=16x16:rate=2:duration=1",
            ])
            .arg(&animated)
            .status()
            .expect("create animation");
        assert!(status.success());

        assert!(verify_image_decodable(&video, &span, &ffmpeg, &ffprobe).is_err());
        assert!(verify_image_decodable(&animated, &span, &ffmpeg, &ffprobe).is_err());

        for still in [&png, &jpeg] {
            let status = Command::new(ffmpeg.executable())
                .args([
                    "-y",
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=red:size=16x16",
                    "-frames:v",
                    "1",
                ])
                .arg(still)
                .status()
                .expect("create still image");
            assert!(status.success());
        }
        fs::write(&ppm, b"P3\n1 1\n255\n255 0 0\n").expect("PPM");
        for still in [&png, &jpeg, &ppm] {
            verify_image_decodable(still, &span, &ffmpeg, &ffprobe).expect("supported still image");
        }
    }
}
