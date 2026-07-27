use std::path::{Path, PathBuf};

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::source::SourceSpan;

/// A private, same-filesystem directory for tool outputs and rollback backups.
pub(super) struct StagingDirectory {
    directory: tempfile::TempDir,
}

impl StagingDirectory {
    pub(super) fn beside(path: &Path, role: &str, diagnostic: BuiltinDiagnostic) -> Result<Self> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let prefix = format!(
            ".{}.{role}-",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        let directory = tempfile::Builder::new()
            .prefix(&prefix)
            .tempdir_in(parent)
            .map_err(|error| {
                Diagnostic::builtin(
                    diagnostic,
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

    pub(super) fn keep(self) -> PathBuf {
        self.directory.keep()
    }
}
