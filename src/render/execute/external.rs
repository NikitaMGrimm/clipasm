use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use serde::Serialize;

use crate::diagnostic::{Diagnostic, Result};
use crate::external::{EXTERNAL_PROTOCOL_VERSION, ExternalParameterValue};
use crate::model::{AudioDomain, AudioSpec, NodeId, ValueType, VideoDomain, VideoSpec};
use crate::preflight::tools::ExternalToolIdentity;
use crate::source::SourceSpan;

use super::context::RenderContext;

#[derive(Serialize)]
struct ExternalRunRequest<'a> {
    protocol_version: u32,
    inputs: BTreeMap<&'a str, ExternalRunInput<'a>>,
    parameters: &'a BTreeMap<String, ExternalParameterValue>,
    output: &'a Path,
    project: ExternalRunProject<'a>,
    tools: ExternalRunTools<'a>,
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
    inputs: &BTreeMap<String, NodeId>,
    parameters: &BTreeMap<String, ExternalParameterValue>,
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
    run_external(executable.executable(), &request, context.span())?;
    context.commit_temporary()
}

fn run_external(
    executable: &Path,
    request: &ExternalRunRequest<'_>,
    span: &SourceSpan,
) -> Result<()> {
    let mut child = Command::new(executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            Diagnostic::new(
                "E_EXTERNAL_EXECUTION",
                format!(
                    "could not start external program `{}`: {error}",
                    executable.display()
                ),
                span.clone(),
            )
        })?;
    let request = serde_json::to_vec(request).map_err(|error| {
        Diagnostic::new(
            "E_EXTERNAL_PROTOCOL",
            format!("could not serialize external program request: {error}"),
            span.clone(),
        )
    })?;
    child
        .stdin
        .take()
        .expect("piped external stdin")
        .write_all(&request)
        .map_err(|error| {
            Diagnostic::new(
                "E_EXTERNAL_EXECUTION",
                format!("could not write external program request: {error}"),
                span.clone(),
            )
        })?;
    let output = child.wait_with_output().map_err(|error| {
        Diagnostic::new(
            "E_EXTERNAL_EXECUTION",
            format!("could not wait for external program: {error}"),
            span.clone(),
        )
    })?;
    if output.status.success() {
        return Ok(());
    }
    Err(Diagnostic::new(
        "E_EXTERNAL_EXECUTION",
        format!(
            "external program `{}` failed with {}\n{}",
            executable.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        span.clone(),
    ))
}
