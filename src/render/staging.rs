use std::path::{Path, PathBuf};

use crate::diagnostic::{Diagnostic, Result};
use crate::source::SourceSpan;

/// A private, same-filesystem directory for tool outputs and rollback backups.
pub(super) struct StagingDirectory {
    directory: tempfile::TempDir,
}

impl StagingDirectory {
    pub(super) fn beside(path: &Path, role: &str, code: &'static str) -> Result<Self> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let prefix = format!(
            ".{}.{role}-",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        let directory = tempfile::Builder::new()
            .prefix(&prefix)
            .tempdir_in(parent)
            .map_err(|error| {
                Diagnostic::new(
                    code,
                    format!(
                        "could not create private {role} staging directory beside `{}`: {error}",
                        path.display()
                    ),
                    SourceSpan::file_start(path),
                )
            })?;
        Ok(Self { directory })
    }

    pub(super) fn path(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }
}
