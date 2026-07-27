use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::io;
use std::io::{Read as _, Write as _};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};

use serde::Serialize;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::external::EXTERNAL_PROTOCOL_VERSION;
use crate::model::{AudioDomain, AudioSpec, NodeId, ValueType, VideoDomain, VideoSpec};
use crate::preflight::tools::ExternalToolIdentity;
use crate::preflight::{PreparedExternalArgument, PreparedExternalParameterValue};
use crate::source::SourceSpan;

use super::context::RenderContext;

#[derive(Serialize)]
struct ExternalRunRequest<'a> {
    protocol_version: u32,
    inputs: BTreeMap<&'a str, ExternalRunInput<'a>>,
    parameters: BTreeMap<&'a str, ExternalRunParameter<'a>>,
    output: &'a Path,
    project: ExternalRunProject<'a>,
    tools: ExternalRunTools<'a>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum ExternalRunParameter<'a> {
    Integer(i64),
    Text(&'a str),
    File(&'a Path),
}

#[derive(Serialize)]
struct ExternalRunInput<'a> {
    path: &'a Path,
    value_type: ValueType,
    domain: Option<&'a VideoDomain>,
    audio_domain: Option<&'a AudioDomain>,
    has_audio: bool,
}

#[derive(Serialize)]
struct ExternalRunProject<'a> {
    video: &'a VideoSpec,
    audio: &'a AudioSpec,
}

#[derive(Serialize)]
struct ExternalRunTools<'a> {
    ffmpeg: &'a Path,
    ffprobe: &'a Path,
}

pub(super) fn video(
    context: &RenderContext<'_>,
    executable: &ExternalToolIdentity,
    arguments: &[PreparedExternalArgument],
    inputs: &BTreeMap<String, NodeId>,
    parameters: &BTreeMap<String, PreparedExternalParameterValue>,
) -> Result<()> {
    let inputs = inputs
        .iter()
        .map(|(name, id)| {
            let input_node = &context.nodes()[id.get() as usize];
            Ok((
                name.as_str(),
                ExternalRunInput {
                    path: context.artifact(*id)?,
                    value_type: input_node.value_type(),
                    domain: input_node.video_domain(),
                    audio_domain: input_node.audio_domain(),
                    has_audio: input_node.has_audio(),
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let parameters = parameters
        .iter()
        .map(|(name, value)| {
            let value = match value {
                PreparedExternalParameterValue::Integer(value) => {
                    ExternalRunParameter::Integer(*value)
                }
                PreparedExternalParameterValue::Keyword(value) => ExternalRunParameter::Text(value),
                PreparedExternalParameterValue::File(asset) => {
                    ExternalRunParameter::File(asset.source_path())
                }
            };
            (name.as_str(), value)
        })
        .collect();
    let request = ExternalRunRequest {
        protocol_version: EXTERNAL_PROTOCOL_VERSION,
        inputs,
        parameters,
        output: context.temporary(),
        project: ExternalRunProject {
            video: context.video(),
            audio: context.audio(),
        },
        tools: ExternalRunTools {
            ffmpeg: context.plan().ffmpeg().executable(),
            ffprobe: context.plan().ffprobe().executable(),
        },
    };
    let arguments = arguments.iter().map(|argument| match argument {
        PreparedExternalArgument::Text(value) => std::ffi::OsString::from(value),
        PreparedExternalArgument::File(asset) => asset.source_path().as_os_str().to_owned(),
    });
    run_external(executable.executable(), arguments, &request, context.span())
}

fn run_external(
    executable: &Path,
    arguments: impl IntoIterator<Item = std::ffi::OsString>,
    request: &ExternalRunRequest<'_>,
    span: &SourceSpan,
) -> Result<()> {
    const STDERR_LIMIT: usize = 64 * 1024;

    let request = serde_json::to_vec(request).map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::ExternalProtocol,
            format!("could not serialize external program request: {error}"),
            span.clone(),
        )
    })?;
    let mut command = Command::new(executable);
    command.args(arguments);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            Diagnostic::builtin(
                BuiltinDiagnostic::ExternalExecution,
                format!(
                    "could not start external program `{}`: {error}",
                    executable.display()
                ),
                span.clone(),
            )
        })?;
    let mut stderr = child.stderr.take().expect("piped external stderr");
    let stderr_reader = std::thread::spawn(move || {
        let mut retained = VecDeque::with_capacity(STDERR_LIMIT);
        let mut buffer = [0_u8; 8 * 1024];
        let mut truncated = false;
        loop {
            let read = match stderr.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => read,
            };
            retained.extend(&buffer[..read]);
            while retained.len() > STDERR_LIMIT {
                retained.pop_front();
                truncated = true;
            }
        }
        (retained.into_iter().collect::<Vec<_>>(), truncated)
    });
    if let Err(error) = child
        .stdin
        .take()
        .expect("piped external stdin")
        .write_all(&request)
    {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stderr_reader.join();
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::ExternalExecution,
            format!("could not write external program request: {error}"),
            span.clone(),
        ));
    }
    let status = wait_external_with(&mut child, Child::wait);
    let (stderr, truncated) = stderr_reader.join().unwrap_or_default();
    let status = status.map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::ExternalExecution,
            format!("could not wait for external program: {error}"),
            span.clone(),
        )
    })?;
    if status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&stderr);
    let stderr = if truncated {
        format!(
            "[stderr truncated to final {STDERR_LIMIT} bytes]\n{}",
            stderr.trim()
        )
    } else {
        stderr.trim().to_owned()
    };
    Err(Diagnostic::builtin(
        BuiltinDiagnostic::ExternalExecution,
        format!(
            "external program `{}` failed with {}\n{}",
            executable.display(),
            status,
            stderr
        ),
        span.clone(),
    ))
}

fn wait_external_with(
    child: &mut Child,
    wait: impl FnOnce(&mut Child) -> io::Result<ExitStatus>,
) -> io::Result<ExitStatus> {
    match wait(child) {
        Ok(status) => Ok(status),
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use super::*;

    #[cfg(unix)]
    #[test]
    fn wait_failure_kills_and_reaps_the_external_process() {
        let mut child = Command::new("sh")
            .args(["-c", "sleep 60"])
            .spawn()
            .expect("sleeping child");

        let error = wait_external_with(&mut child, |_| {
            Err(io::Error::other("injected wait failure"))
        })
        .expect_err("injected wait failure");

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(
            child.try_wait().expect("reaped child status").is_some(),
            "child must be reaped before returning"
        );
    }

    #[test]
    fn native_external_process_receives_protocol_and_reports_failure() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let source = directory.path().join("external-helper.rs");
        let executable = directory.path().join(if cfg!(windows) {
            "external-helper.exe"
        } else {
            "external-helper"
        });
        fs::write(
            &source,
            r#"
use std::io::Read as _;

fn main() {
    if std::env::args().nth(1).as_deref() != Some("--test-argument") {
        eprintln!("missing process argument");
        std::process::exit(21);
    }
    let mut request = String::new();
    std::io::stdin().read_to_string(&mut request).expect("request");
    if !request.contains("\"protocol_version\":1") {
        eprintln!("missing protocol version");
        std::process::exit(22);
    }
    eprintln!("native helper received request");
    std::process::exit(23);
}
"#,
        )
        .expect("helper source");
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let status = Command::new(rustc)
            .arg(&source)
            .args(["--edition", "2024", "-o"])
            .arg(&executable)
            .status()
            .expect("compile external helper");
        assert!(status.success());

        let video = VideoSpec::default();
        let audio = AudioSpec::default();
        let output = directory.path().join("output.mkv");
        let request = ExternalRunRequest {
            protocol_version: EXTERNAL_PROTOCOL_VERSION,
            inputs: BTreeMap::new(),
            parameters: BTreeMap::new(),
            output: &output,
            project: ExternalRunProject {
                video: &video,
                audio: &audio,
            },
            tools: ExternalRunTools {
                ffmpeg: Path::new("ffmpeg"),
                ffprobe: Path::new("ffprobe"),
            },
        };

        let error = run_external(
            &executable,
            [std::ffi::OsString::from("--test-argument")],
            &request,
            &SourceSpan::file_start("external-test.clipasm"),
        )
        .expect_err("external helper failure");
        assert_eq!(error.code, "E_EXTERNAL_EXECUTION");
        assert!(error.message.contains("native helper received request"));
        assert!(error.message.contains("23"));
    }
}
