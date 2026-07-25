mod probe;

pub(crate) use probe::decoded_audio_samples;
pub(super) use probe::{verify_audio_decodable, verify_image_decodable, verify_video_decodable};

use std::collections::BTreeSet;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::diagnostic::{Diagnostic, Result};
use crate::media_tool::{self, CapturedOutput};
use crate::source::SourceSpan;

#[derive(Serialize)]
struct ToolBuildIdentity<'a> {
    executable_content_hash: &'a str,
    version_stdout: &'a str,
    version_stderr: &'a str,
}

#[derive(Clone, Debug)]
/// One resolved external executable and the content hash verified by preflight.
pub struct ExternalToolIdentity {
    executable: PathBuf,
    content_hash: String,
}

impl ExternalToolIdentity {
    /// Return the resolved executable path used during preflight.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Return the executable content hash recorded during preflight.
    #[must_use]
    pub fn content_hash(&self) -> &str {
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
                "external executable `{}` is not executable",
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

#[derive(Clone, Debug)]
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

#[derive(Default)]
pub(super) struct FfmpegRequirements {
    encoders: BTreeSet<&'static str>,
    muxers: BTreeSet<&'static str>,
    filters: BTreeSet<&'static str>,
}

impl FfmpegRequirements {
    pub(super) fn for_render(result_has_audio: bool) -> Self {
        let mut requirements = Self::default();
        requirements.require_encoders(["libx264", "ffv1", "flac"]);
        if result_has_audio {
            requirements.require_encoders(["aac"]);
        }
        requirements.require_muxers(["mp4", "matroska"]);
        requirements
    }

    pub(super) fn require_encoders(&mut self, encoders: impl IntoIterator<Item = &'static str>) {
        self.encoders.extend(encoders);
    }

    pub(super) fn require_muxers(&mut self, muxers: impl IntoIterator<Item = &'static str>) {
        self.muxers.extend(muxers);
    }

    pub(super) fn require_filters(&mut self, filters: impl IntoIterator<Item = &'static str>) {
        self.filters.extend(filters);
    }

    #[cfg(test)]
    pub(super) fn requires_encoder(&self, encoder: &str) -> bool {
        self.encoders.contains(encoder)
    }

    #[cfg(test)]
    pub(super) fn requires_filter(&self, filter: &str) -> bool {
        self.filters.contains(filter)
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
    inspect_tool_identity(&tool, "E_FFMPEG")
}

pub(super) fn validate_ffmpeg_capabilities(
    tool: &ToolIdentity,
    requirements: &FfmpegRequirements,
) -> Result<()> {
    validate_capability_group(
        tool,
        &["-hide_banner", "-encoders"],
        "encoder",
        &requirements.encoders,
    )?;
    validate_capability_group(
        tool,
        &["-hide_banner", "-muxers"],
        "muxer",
        &requirements.muxers,
    )?;
    validate_capability_group(
        tool,
        &["-hide_banner", "-filters"],
        "filter",
        &requirements.filters,
    )
}

fn validate_capability_group(
    tool: &ToolIdentity,
    arguments: &[&str],
    role: &str,
    requirements: &BTreeSet<&'static str>,
) -> Result<()> {
    if requirements.is_empty() {
        return Ok(());
    }
    let output = tool_output(tool.executable(), arguments, "E_FFMPEG")?;
    if let Some(missing) = requirements
        .iter()
        .find(|capability| capability_missing(&output, capability))
    {
        return Err(Diagnostic::new(
            "E_FFMPEG_CAPABILITY",
            format!(
                "installed FFmpeg does not provide the required `{missing}` {role} for this prepared plan"
            ),
            SourceSpan::file_start(tool.executable()),
        ));
    }
    Ok(())
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
    #[cfg(windows)]
    {
        if is_executable_file(authored) {
            return fs::canonicalize(authored).map_err(|error| {
                Diagnostic::new(
                    code,
                    format!("could not resolve executable `{name}`: {error}"),
                    SourceSpan::file_start(authored),
                )
            });
        }
        let mut command = Command::new("where.exe");
        command.arg(name);
        let output = media_tool::capture(command, code, &SourceSpan::file_start(authored))?;
        return String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
            .find(|candidate| is_executable_file(candidate))
            .and_then(|candidate| fs::canonicalize(candidate).ok())
            .ok_or_else(|| {
                Diagnostic::new(
                    code,
                    format!("could not resolve executable `{name}` through Windows command lookup"),
                    SourceSpan::file_start(authored),
                )
            });
    }
    #[cfg(not(windows))]
    {
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
                .map(|directory| directory.join(name))
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

fn tool_command_output(
    tool: &Path,
    arguments: &[&str],
    code: &'static str,
) -> Result<CapturedOutput> {
    let mut command = Command::new(tool);
    command.args(arguments);
    media_tool::capture(command, code, &SourceSpan::file_start(tool))
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
    fn ffmpeg_discovery_does_not_require_unused_capabilities() {
        let _guard = fake_tool_test_lock();
        let (_directory, tool) = executable_script(
            "#!/bin/sh\nif [ \"$1\" = \"-version\" ]; then echo fake; else echo none; fi\n",
        );
        let identity = inspect_ffmpeg_at(&tool).expect("FFmpeg identity");
        assert_eq!(identity.version(), "fake");
    }

    #[cfg(unix)]
    #[test]
    fn ffmpeg_validation_requires_requested_encoders_and_muxers() {
        let _guard = fake_tool_test_lock();
        let (_directory, tool) = executable_script(
            "#!/bin/sh\nif [ \"$1\" = \"-version\" ]; then echo fake; elif [ \"$2\" = \"-encoders\" ]; then echo 'libx264 ffv1 flac'; elif [ \"$2\" = \"-muxers\" ]; then echo mp4; else echo none; fi\n",
        );
        let identity = inspect_ffmpeg_at(&tool).expect("FFmpeg identity");
        let requirements = FfmpegRequirements::for_render(false);
        let error = validate_ffmpeg_capabilities(&identity, &requirements)
            .expect_err("missing Matroska muxer");
        assert_eq!(error.code, "E_FFMPEG_CAPABILITY");
        assert!(error.message.contains("matroska"));
        assert!(error.message.contains("this prepared plan"));
    }

    #[cfg(unix)]
    #[test]
    fn ffmpeg_validation_ignores_unrequested_filters() {
        let _guard = fake_tool_test_lock();
        let (_directory, tool) = executable_script(
            "#!/bin/sh\nif [ \"$1\" = \"-version\" ]; then echo fake; elif [ \"$2\" = \"-encoders\" ]; then echo 'libx264 ffv1 flac'; elif [ \"$2\" = \"-muxers\" ]; then echo 'mp4 matroska'; elif [ \"$2\" = \"-filters\" ]; then echo scale; else echo none; fi\n",
        );
        let identity = inspect_ffmpeg_at(&tool).expect("FFmpeg identity");
        let mut requirements = FfmpegRequirements::for_render(false);
        requirements.require_filters(["scale"]);
        validate_ffmpeg_capabilities(&identity, &requirements)
            .expect("requested capabilities are available");

        requirements.require_filters(["blend"]);
        let error = validate_ffmpeg_capabilities(&identity, &requirements)
            .expect_err("missing requested filter");
        assert_eq!(error.code, "E_FFMPEG_CAPABILITY");
        assert!(error.message.contains("blend"));
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
        let span = SourceSpan::file_start("workflow.clipasm");
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
