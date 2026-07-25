use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{AudioSpec, NodeId, VideoSpec};
use crate::preflight::{PreparedNode, PreparedPlan};
use crate::source::SourceSpan;

use super::super::staging::StagingDirectory;

pub(super) struct RenderContext<'a> {
    plan: &'a PreparedPlan,
    node: &'a PreparedNode,
    artifacts: &'a [PathBuf],
    temporary: &'a Path,
}

impl<'a> RenderContext<'a> {
    pub(super) const fn new(
        plan: &'a PreparedPlan,
        node: &'a PreparedNode,
        artifacts: &'a [PathBuf],
        temporary: &'a Path,
    ) -> Self {
        Self {
            plan,
            node,
            artifacts,
            temporary,
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
        self.temporary
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
        run_command(command, "E_FFMPEG", self.span())
    }
}

pub(in crate::render) struct StagedArtifact {
    _staging: StagingDirectory,
    path: PathBuf,
    destination: PathBuf,
}

impl StagedArtifact {
    pub(super) fn new(destination: &Path, extension: &str) -> Result<Self> {
        let staging = StagingDirectory::beside(destination, "cache", "E_CACHE_IO")?;
        Ok(Self {
            path: staging.path(&format!("artifact.{extension}")),
            destination: destination.to_path_buf(),
            _staging: staging,
        })
    }

    pub(in crate::render) fn path(&self) -> &Path {
        &self.path
    }

    pub(in crate::render) fn commit(self) -> Result<()> {
        atomic_replace(&self.path, &self.destination, "E_CACHE_IO")
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncommitted_cache_staging_is_removed_on_drop() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let destination = directory.path().join("artifact.mkv");
        let staged_path = {
            let staged = StagedArtifact::new(&destination, "mkv").expect("staging");
            fs::write(staged.path(), b"invalid artifact").expect("staged bytes");
            staged.path().to_path_buf()
        };
        assert!(!staged_path.exists());
        assert!(!destination.exists());
    }

    #[test]
    fn committed_cache_staging_moves_only_the_staged_file() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let destination = directory.path().join("artifact.mkv");
        let staged = StagedArtifact::new(&destination, "mkv").expect("staging");
        let staging_parent = staged
            .path()
            .parent()
            .expect("staging parent")
            .to_path_buf();
        fs::write(staged.path(), b"verified artifact").expect("staged bytes");
        staged.commit().expect("commit");
        assert_eq!(
            fs::read(&destination).expect("artifact"),
            b"verified artifact"
        );
        assert!(!staging_parent.exists());
    }
}
