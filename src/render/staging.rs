use std::path::{Path, PathBuf};

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::source::SourceSpan;

const PRIVATE_DIRECTORY_RANDOM_CHARACTERS: usize = 6;

/// A private, same-filesystem directory for tool outputs and rollback backups.
pub(super) struct StagingDirectory {
    directory: tempfile::TempDir,
}

impl StagingDirectory {
    pub(super) fn beside(path: &Path, role: &str, diagnostic: BuiltinDiagnostic) -> Result<Self> {
        let parent =
            absolute_parent(path).map_err(|error| staging_error(path, role, diagnostic, &error))?;
        let prefix = staging_prefix(path, role);
        let directory = tempfile::Builder::new()
            .prefix(&prefix)
            .rand_bytes(PRIVATE_DIRECTORY_RANDOM_CHARACTERS)
            .tempdir_in(&parent)
            .map_err(|error| staging_error(path, role, diagnostic, &error))?;
        Ok(Self { directory })
    }

    pub(super) fn planned_path(
        path: &Path,
        role: &str,
        name: &str,
        diagnostic: BuiltinDiagnostic,
    ) -> Result<PathBuf> {
        let parent =
            absolute_parent(path).map_err(|error| staging_error(path, role, diagnostic, &error))?;
        let directory = format!(
            "{}{}",
            staging_prefix(path, role),
            "x".repeat(PRIVATE_DIRECTORY_RANDOM_CHARACTERS)
        );
        Ok(parent.join(directory).join(name))
    }

    pub(super) fn path(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }

    pub(super) fn keep(self) -> PathBuf {
        self.directory.keep()
    }
}

fn absolute_parent(path: &Path) -> std::io::Result<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if parent.is_absolute() {
        Ok(parent.to_path_buf())
    } else {
        std::env::current_dir().map(|current| current.join(parent))
    }
}

fn staging_prefix(path: &Path, role: &str) -> String {
    format!(
        ".{}.{role}-",
        path.file_name().unwrap_or_default().to_string_lossy()
    )
}

fn staging_error(
    path: &Path,
    role: &str,
    diagnostic: BuiltinDiagnostic,
    error: &std::io::Error,
) -> Diagnostic {
    Diagnostic::builtin(
        diagnostic,
        format!(
            "could not create private {role} staging directory beside `{}`: {error}",
            path.display()
        ),
        SourceSpan::file_start(path),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planned_path_matches_the_actual_staging_path_shape() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let destination = directory.path().join("artifact name.mkv");
        let planned = StagingDirectory::planned_path(
            &destination,
            "cache",
            "artifact.mkv",
            BuiltinDiagnostic::CacheIo,
        )
        .expect("planned path");
        let actual = StagingDirectory::beside(&destination, "cache", BuiltinDiagnostic::CacheIo)
            .expect("staging directory")
            .path("artifact.mkv");

        let encoding_shape = |path: &Path| {
            let path = path.to_string_lossy();
            (path.len(), path.encode_utf16().count())
        };
        assert_eq!(encoding_shape(&planned), encoding_shape(&actual));
    }
}
