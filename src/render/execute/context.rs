use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{AudioSpec, NodeId, VideoSpec};
use crate::preflight::{PreparedNode, PreparedPlan};
use crate::source::SourceSpan;

static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) struct RenderContext<'a> {
    plan: &'a PreparedPlan,
    node: &'a PreparedNode,
    artifacts: &'a [PathBuf],
    destination: &'a Path,
    temporary: PathBuf,
}

impl<'a> RenderContext<'a> {
    pub(super) fn new(
        plan: &'a PreparedPlan,
        node: &'a PreparedNode,
        artifacts: &'a [PathBuf],
        destination: &'a Path,
    ) -> Self {
        let extension = match node.value_type() {
            crate::model::ValueType::Audio => "mka",
            crate::model::ValueType::Video => "mkv",
        };
        Self {
            plan,
            node,
            artifacts,
            destination,
            temporary: temporary_sibling(destination, "cache", extension),
        }
    }

    pub(super) const fn plan(&self) -> &'a PreparedPlan {
        self.plan
    }

    pub(super) fn nodes(&self) -> &'a [PreparedNode] {
        self.plan.nodes()
    }

    pub(super) fn video(&self) -> &'a VideoSpec {
        self.plan.video()
    }

    pub(super) fn audio(&self) -> &'a AudioSpec {
        self.plan.audio()
    }

    pub(super) fn temporary(&self) -> &Path {
        self.temporary.as_path()
    }

    pub(super) fn span(&self) -> &SourceSpan {
        &self.node.origin().span
    }

    pub(super) fn command(&self) -> Command {
        let mut command = Command::new(self.plan.ffmpeg().executable());
        command.args(["-y", "-v", "error"]);
        command
    }

    pub(super) fn artifact(&self, id: NodeId) -> Result<&'a Path> {
        self.artifacts
            .get(id.get() as usize)
            .map(PathBuf::as_path)
            .ok_or_else(|| {
                Diagnostic::new(
                    "E_INVALID_PLAN",
                    format!("primitive input {} is not available", id.get()),
                    self.span().clone(),
                )
            })
    }

    pub(super) fn finish_ffmpeg(&self, command: Command) -> Result<()> {
        if let Err(error) = run_command(command, "E_FFMPEG", self.span()) {
            let _ = fs::remove_file(&self.temporary);
            return Err(error);
        }
        self.commit_temporary()
    }

    pub(super) fn commit_temporary(&self) -> Result<()> {
        atomic_replace(&self.temporary, self.destination, "E_CACHE_IO")
    }
}

fn temporary_sibling(path: &Path, role: &str, extension: &str) -> PathBuf {
    let counter = TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(format!(
        ".{role}-{}-{counter}.{extension}",
        std::process::id()
    ));
    path.parent().unwrap_or_else(|| Path::new(".")).join(name)
}

pub(super) fn atomic_replace(source: &Path, destination: &Path, code: &'static str) -> Result<()> {
    fs::rename(source, destination).map_err(|error| {
        Diagnostic::new(
            code,
            format!(
                "could not atomically replace `{}` from `{}`: {error}",
                destination.display(),
                source.display()
            ),
            SourceSpan::file_start(destination),
        )
    })
}

pub(super) fn run_command(command: Command, code: &'static str, span: &SourceSpan) -> Result<()> {
    run_output(command, code, span).map(|_| ())
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
