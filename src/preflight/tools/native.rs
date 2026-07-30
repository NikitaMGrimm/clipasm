use super::ExternalToolIdentity;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::media_tool::{self, CapturedOutput};
use crate::preflight::RenderPolicy;
use crate::source::SourceSpan;

#[derive(Serialize)]
struct ToolBuildIdentity<'a> {
    executable_content_hash: &'a str,
    version_stdout: &'a str,
    version_stderr: &'a str,
}

pub(crate) fn inspect_external_tool(
    authored: &Path,
    span: &SourceSpan,
) -> Result<ExternalToolIdentity> {
    let candidate = if authored.is_absolute() || authored.components().count() > 1 {
        super::super::assets::resolve_authored_path(authored, span)?
    } else {
        resolve_executable(
            authored.to_str().ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::ExternalExecutable,
                    "external executable name is not valid UTF-8",
                    span.clone(),
                )
            })?,
            BuiltinDiagnostic::ExternalExecutable,
        )?
    };
    #[cfg(windows)]
    let candidate = windows_command_candidate(&candidate);
    let executable = fs::canonicalize(&candidate).map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::ExternalExecutable,
            format!(
                "could not resolve external executable `{}`: {error}",
                candidate.display()
            ),
            span.clone(),
        )
    })?;
    if !is_executable_file(&executable) {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::ExternalExecutable,
            format!(
                "external executable `{}` is not executable",
                executable.display()
            ),
            span.clone(),
        ));
    }
    let content_hash = hash_tool_executable(&executable, BuiltinDiagnostic::ExternalExecutable)?;
    Ok(ExternalToolIdentity {
        executable,
        content_hash,
    })
}

pub(crate) fn verify_external_tool(
    identity: &ExternalToolIdentity,
    span: &SourceSpan,
) -> Result<()> {
    let current =
        hash_tool_executable(identity.executable(), BuiltinDiagnostic::ExternalExecutable)?;
    if current == identity.content_hash {
        return Ok(());
    }
    Err(Diagnostic::builtin(
        BuiltinDiagnostic::ExternalChanged,
        format!(
            "external executable `{}` changed after preflight; prepare the program again",
            identity.executable.display()
        ),
        span.clone(),
    ))
}

#[derive(Clone, Debug)]
pub(crate) struct ToolIdentity {
    pub(in crate::preflight) executable: PathBuf,
    pub(in crate::preflight) version_summary: String,
    pub(in crate::preflight) build_fingerprint: String,
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
pub(crate) struct FfmpegRequirements {
    encoders: BTreeSet<&'static str>,
    muxers: BTreeSet<&'static str>,
    filters: BTreeSet<&'static str>,
}

impl FfmpegRequirements {
    pub(in crate::preflight) fn for_export(
        render_policy: RenderPolicy,
        result_has_audio: bool,
    ) -> Self {
        let mut requirements = Self::default();
        requirements.require_encoders([render_policy.export_video_encoder()]);
        if result_has_audio {
            requirements.require_encoders([render_policy.export_audio_encoder()]);
        }
        requirements.require_muxers([render_policy.export_container()]);
        requirements
    }

    pub(in crate::preflight) fn require_native_video_output(
        &mut self,
        render_policy: RenderPolicy,
    ) {
        self.require_encoders([
            render_policy.native_video_encoder(),
            render_policy.native_audio_encoder(),
        ]);
        self.require_muxers([render_policy.native_container()]);
    }

    pub(in crate::preflight) fn require_native_audio_output(
        &mut self,
        render_policy: RenderPolicy,
    ) {
        self.require_encoders([render_policy.native_audio_encoder()]);
        self.require_muxers([render_policy.native_container()]);
    }

    pub(in crate::preflight) fn require_encoders(
        &mut self,
        encoders: impl IntoIterator<Item = &'static str>,
    ) {
        self.encoders.extend(encoders);
    }

    pub(in crate::preflight) fn require_muxers(
        &mut self,
        muxers: impl IntoIterator<Item = &'static str>,
    ) {
        self.muxers.extend(muxers);
    }

    pub(in crate::preflight) fn require_filters(
        &mut self,
        filters: impl IntoIterator<Item = &'static str>,
    ) {
        self.filters.extend(filters);
    }

    #[cfg(test)]
    pub(in crate::preflight) fn requires_encoder(&self, encoder: &str) -> bool {
        self.encoders.contains(encoder)
    }

    #[cfg(test)]
    pub(in crate::preflight) fn requires_filter(&self, filter: &str) -> bool {
        self.filters.contains(filter)
    }
}

pub(crate) fn verify_tool_identity(tool: &ToolIdentity, role: &str) -> Result<()> {
    let current = inspect_tool_identity(tool.executable(), BuiltinDiagnostic::ToolChanged)?;
    if current.build_fingerprint() == tool.build_fingerprint() {
        return Ok(());
    }
    Err(Diagnostic::builtin(
        BuiltinDiagnostic::ToolChanged,
        format!(
            "{role} executable `{}` changed after preflight; prepare the program again",
            tool.executable().display()
        ),
        SourceSpan::file_start(tool.executable()),
    ))
}

pub(crate) fn inspect_ffmpeg() -> Result<ToolIdentity> {
    inspect_ffmpeg_at(&resolve_executable("ffmpeg", BuiltinDiagnostic::Ffmpeg)?)
}

fn inspect_ffmpeg_at(tool: &Path) -> Result<ToolIdentity> {
    let tool = fs::canonicalize(tool).map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::Ffmpeg,
            format!(
                "could not resolve FFmpeg executable `{}`: {error}",
                tool.display()
            ),
            SourceSpan::file_start(tool),
        )
    })?;
    inspect_tool_identity(&tool, BuiltinDiagnostic::Ffmpeg)
}

pub(crate) fn validate_ffmpeg_capabilities(
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
    let output = tool_output(tool.executable(), arguments, BuiltinDiagnostic::Ffmpeg)?;
    if let Some(missing) = requirements
        .iter()
        .find(|capability| capability_missing(&output, capability))
    {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::FfmpegCapability,
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

pub(crate) fn inspect_ffprobe() -> Result<ToolIdentity> {
    let executable = resolve_executable("ffprobe", BuiltinDiagnostic::Ffprobe)?;
    inspect_tool_identity(&executable, BuiltinDiagnostic::Ffprobe)
}

fn resolve_executable(name: &str, code: BuiltinDiagnostic) -> Result<PathBuf> {
    let authored = Path::new(name);
    #[cfg(windows)]
    {
        let direct = windows_command_candidate(authored);
        if is_executable_file(&direct) {
            return fs::canonicalize(&direct).map_err(|error| {
                Diagnostic::builtin(
                    code,
                    format!("could not resolve executable `{name}`: {error}"),
                    SourceSpan::file_start(authored),
                )
            });
        }
        let mut command = Command::new("where.exe");
        command.arg(name);
        let output = media_tool::capture(command, code, &SourceSpan::file_start(authored))?;
        let candidate = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
            .map(|candidate| windows_command_candidate(&candidate))
            .find(|candidate| is_executable_file(candidate))
            .ok_or_else(|| {
                Diagnostic::builtin(
                    code,
                    format!("could not resolve executable `{name}` through Windows command lookup"),
                    SourceSpan::file_start(authored),
                )
            })?;
        return fs::canonicalize(&candidate).map_err(|error| {
            Diagnostic::builtin(
                code,
                format!(
                    "could not resolve executable candidate `{}`: {error}",
                    candidate.display()
                ),
                SourceSpan::file_start(&candidate),
            )
        });
    }
    #[cfg(not(windows))]
    {
        let candidates = if authored.components().count() > 1 {
            vec![authored.to_path_buf()]
        } else {
            let path = std::env::var_os("PATH").ok_or_else(|| {
                Diagnostic::builtin(
                    code,
                    format!("could not resolve `{name}` because PATH is not set"),
                    SourceSpan::file_start(authored),
                )
            })?;
            std::env::split_paths(&path)
                .map(|directory| directory.join(name))
                .collect()
        };
        let candidate = candidates
            .into_iter()
            .find(|candidate| is_executable_file(candidate))
            .ok_or_else(|| {
                Diagnostic::builtin(
                    code,
                    format!("could not resolve executable `{name}` on PATH"),
                    SourceSpan::file_start(authored),
                )
            })?;
        fs::canonicalize(&candidate).map_err(|error| {
            Diagnostic::builtin(
                code,
                format!(
                    "could not resolve executable candidate `{}`: {error}",
                    candidate.display()
                ),
                SourceSpan::file_start(&candidate),
            )
        })
    }
}

#[cfg(windows)]
fn windows_command_candidate(candidate: &Path) -> PathBuf {
    windows_command_candidate_with(candidate, is_executable_file)
}

#[cfg(any(windows, test))]
fn windows_command_candidate_with(
    candidate: &Path,
    mut is_file: impl FnMut(&Path) -> bool,
) -> PathBuf {
    let encoded = candidate.as_os_str().as_encoded_bytes();
    if encoded.len() >= 4 && encoded[encoded.len() - 4..].eq_ignore_ascii_case(b".exe") {
        return candidate.to_path_buf();
    }
    let mut suffixed = candidate.as_os_str().to_os_string();
    suffixed.push(".exe");
    let suffixed = PathBuf::from(suffixed);
    if is_file(&suffixed) {
        suffixed
    } else {
        candidate.to_path_buf()
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

fn inspect_tool_identity(tool: &Path, code: BuiltinDiagnostic) -> Result<ToolIdentity> {
    #[cfg(windows)]
    let tool = windows_command_candidate(tool);
    #[cfg(not(windows))]
    let tool = tool.to_path_buf();
    let executable = fs::canonicalize(&tool).map_err(|error| {
        Diagnostic::builtin(
            code,
            format!(
                "could not resolve executable `{}` for identity: {error}",
                tool.display()
            ),
            SourceSpan::file_start(&tool),
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
    let build_fingerprint = crate::identity::hash_serializable(&ToolBuildIdentity {
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

fn hash_tool_executable(tool: &Path, code: BuiltinDiagnostic) -> Result<String> {
    crate::identity::hash_file(tool).map_err(|error| {
        Diagnostic::builtin(
            code,
            format!(
                "could not fingerprint executable `{}`: {error}",
                tool.display()
            ),
            SourceSpan::file_start(tool),
        )
    })
}

fn tool_output(tool: &Path, arguments: &[&str], code: BuiltinDiagnostic) -> Result<String> {
    let output = tool_command_output(tool, arguments, code)?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr))
}

fn tool_command_output(
    tool: &Path,
    arguments: &[&str],
    code: BuiltinDiagnostic,
) -> Result<CapturedOutput> {
    let mut command = Command::new(tool);
    command.args(arguments);
    media_tool::capture(command, code, &SourceSpan::file_start(tool))
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::sync::{Mutex, MutexGuard};

    use super::super::probe::verify_image_decodable;
    use super::*;

    #[cfg(unix)]
    static FAKE_TOOL_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn windows_command_candidate_matches_rust_executable_suffix_resolution() {
        let candidate = Path::new("tools/ffmpeg");
        let resolved =
            windows_command_candidate_with(candidate, |path| path == Path::new("tools/ffmpeg.exe"));
        assert_eq!(resolved, Path::new("tools/ffmpeg.exe"));

        let explicit = Path::new("tools/ffmpeg.EXE");
        let resolved = windows_command_candidate_with(explicit, |_| {
            panic!("an explicit .exe path must not probe a sibling")
        });
        assert_eq!(resolved, explicit);

        let non_exe_extension = Path::new("tools/runner.cmd");
        let resolved = windows_command_candidate_with(non_exe_extension, |path| {
            path == Path::new("tools/runner.cmd.exe")
        });
        assert_eq!(resolved, Path::new("tools/runner.cmd.exe"));

        let extensionless = Path::new("tools/custom-tool");
        let resolved = windows_command_candidate_with(extensionless, |_| false);
        assert_eq!(resolved, extensionless);
    }

    #[cfg(unix)]
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
        let mut requirements = FfmpegRequirements::for_export(RenderPolicy::CURRENT, false);
        requirements.require_native_video_output(RenderPolicy::CURRENT);
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
        let mut requirements = FfmpegRequirements::for_export(RenderPolicy::CURRENT, false);
        requirements.require_native_video_output(RenderPolicy::CURRENT);
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
        let first_identity =
            inspect_tool_identity(&first, BuiltinDiagnostic::ToolChanged).expect("first identity");
        let second_identity = inspect_tool_identity(&second, BuiltinDiagnostic::ToolChanged)
            .expect("second identity");
        assert_eq!(first_identity.version(), "tool 1");
        assert_ne!(
            first_identity.build_fingerprint,
            second_identity.build_fingerprint
        );

        let (_relocated_directory, relocated) =
            executable_script("#!/bin/sh\nprintf 'tool 1\\nconfiguration alpha  \\r\\n'\n");
        let relocated_identity = inspect_tool_identity(&relocated, BuiltinDiagnostic::ToolChanged)
            .expect("relocated identity");
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
        let identity =
            inspect_tool_identity(&tool, BuiltinDiagnostic::ToolChanged).expect("initial identity");

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
