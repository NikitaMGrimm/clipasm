use std::path::{Path, PathBuf};
use std::process::Command;

use crate::diagnostic::{Diagnostic, Result};
use crate::media_tool;
use crate::model::{AudioSpec, NodeId, VideoSpec};
use crate::preflight::{PreparedNode, PreparedPlan, RenderPolicy};
use crate::source::SourceSpan;

use super::super::{cache, staging::StagingDirectory};

pub(super) struct RenderContext<'a> {
    plan: &'a PreparedPlan,
    node: &'a PreparedNode,
    artifacts: &'a [Option<PathBuf>],
    temporary: &'a Path,
}

impl<'a> RenderContext<'a> {
    pub(super) const fn new(
        plan: &'a PreparedPlan,
        node: &'a PreparedNode,
        artifacts: &'a [Option<PathBuf>],
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

    pub(super) const fn policy(&self) -> RenderPolicy {
        self.plan.render_policy()
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

    pub(super) fn append_video_output(&self, command: &mut Command) {
        let policy = self.policy();
        command
            .args(["-c:v", policy.native_video_encoder()])
            .args([
                "-level",
                &policy.native_video_level().to_string(),
                "-pix_fmt",
                policy.working_pixel_format(),
                "-r",
            ])
            .arg(format!(
                "{}/{}",
                self.video().fps().numerator(),
                self.video().fps().denominator()
            ))
            .args([
                "-c:a",
                policy.native_audio_encoder(),
                "-ar",
                &self.audio().sample_rate().to_string(),
                "-ac",
                &self.audio().channels().to_string(),
                "-f",
                policy.native_container(),
            ])
            .arg(self.temporary());
    }

    pub(super) fn append_audio_output(&self, command: &mut Command) {
        let policy = self.policy();
        command
            .args([
                "-c:a",
                policy.native_audio_encoder(),
                "-ar",
                &self.audio().sample_rate().to_string(),
                "-ac",
                &self.audio().channels().to_string(),
                "-f",
                policy.native_container(),
            ])
            .arg(self.temporary());
    }

    pub(super) fn artifact(&self, id: NodeId) -> Result<&'a Path> {
        self.artifacts
            .get(id.get() as usize)
            .and_then(Option::as_deref)
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
    metadata: PathBuf,
    destination: PathBuf,
}

impl StagedArtifact {
    pub(super) fn new(destination: &Path, extension: &str) -> Result<Self> {
        let staging = StagingDirectory::beside(destination, "cache", "E_CACHE_IO")?;
        Ok(Self {
            path: staging.path(&format!("artifact.{extension}")),
            metadata: staging.path("artifact.cache.json"),
            destination: destination.to_path_buf(),
            _staging: staging,
        })
    }

    pub(in crate::render) fn path(&self) -> &Path {
        &self.path
    }

    pub(in crate::render) fn commit(self, fingerprint: &str) -> Result<()> {
        cache::commit_verified(&self.path, &self.metadata, &self.destination, fingerprint)
    }
}

pub(super) fn run_command(command: Command, code: &'static str, span: &SourceSpan) -> Result<()> {
    media_tool::run(command, code, span)
}

#[cfg(test)]
mod tests {
    use std::fs;

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
        staged.commit("fingerprint").expect("commit");
        assert_eq!(
            fs::read(&destination).expect("artifact"),
            b"verified artifact"
        );
        assert!(!staging_parent.exists());
        cache::verify_entry(&destination, "fingerprint").expect("cache metadata");
    }
}
