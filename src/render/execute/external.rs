use std::collections::BTreeMap;
use std::io;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread::JoinHandle;

use serde::Serialize;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{AudioDomain, AudioSpec, NodeId, ValueType, VideoDomain, VideoSpec};
use crate::preflight::tools::ExternalToolIdentity;
use crate::preflight::{PreparedExternalArgument, PreparedExternalParameterValue};
use crate::process::{self as child_process, ReaderError, RetainedOutput};
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
        protocol_version: crate::contracts::EXTERNAL_PROGRAM_PROTOCOL_VERSION,
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
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = child_process::spawn(command).map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::ExternalExecution,
            format!(
                "could not start external program `{}`: {error}",
                executable.display()
            ),
            span.clone(),
        )
    })?;
    let stderr = child.stderr.take().expect("piped external stderr");
    let stderr_reader = std::thread::spawn(move || child_process::read_tail(stderr, STDERR_LIMIT));
    if let Err(error) = child
        .stdin
        .take()
        .expect("piped external stdin")
        .write_all(&request)
    {
        child_process::terminate(&mut child);
        let _ = child_process::join_reader(stderr_reader);
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::ExternalExecution,
            format!("could not write external program request: {error}"),
            span.clone(),
        ));
    }
    let status = child_process::wait(&mut child);
    let stderr = join_external_stderr(stderr_reader, span);
    let status = status.map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::ExternalExecution,
            format!("could not wait for external program: {error}"),
            span.clone(),
        )
    })?;
    let stderr = stderr?;
    if status.success() {
        return Ok(());
    }
    let stderr_text = String::from_utf8_lossy(&stderr.bytes);
    let stderr_text = if stderr.truncated {
        format!(
            "[stderr truncated to final {STDERR_LIMIT} bytes]\n{}",
            stderr_text.trim()
        )
    } else {
        stderr_text.trim().to_owned()
    };
    Err(Diagnostic::builtin(
        BuiltinDiagnostic::ExternalExecution,
        format!(
            "external program `{}` failed with {}\n{}",
            executable.display(),
            status,
            stderr_text
        ),
        span.clone(),
    ))
}

fn join_external_stderr(
    reader: JoinHandle<io::Result<RetainedOutput>>,
    span: &SourceSpan,
) -> Result<RetainedOutput> {
    child_process::join_reader(reader).map_err(|error| {
        let message = match error {
            ReaderError::Panicked => "external program stderr reader panicked".to_owned(),
            ReaderError::Io(error) => {
                format!("could not read external program stderr: {error}")
            }
        };
        Diagnostic::builtin(BuiltinDiagnostic::ExternalExecution, message, span.clone())
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use super::*;

    #[test]
    fn stderr_reader_failures_are_reported() {
        let span = SourceSpan::file_start("effect.clipasm");
        let read_failure = std::thread::spawn(|| -> io::Result<RetainedOutput> {
            Err(io::Error::other("injected stderr read failure"))
        });
        let error = join_external_stderr(read_failure, &span).expect_err("stderr read failure");
        assert!(
            error
                .message
                .contains("could not read external program stderr")
        );

        let panic = std::thread::spawn(|| -> io::Result<RetainedOutput> {
            panic!("injected stderr reader panic")
        });
        let error = join_external_stderr(panic, &span).expect_err("stderr reader panic");
        assert!(error.message.contains("stderr reader panicked"));
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
            protocol_version: crate::contracts::EXTERNAL_PROGRAM_PROTOCOL_VERSION,
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
