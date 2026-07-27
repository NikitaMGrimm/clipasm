use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::source::SourceSpan;

pub(super) struct FileLock {
    _file: File,
}

impl FileLock {
    pub(super) fn acquire(
        path: &Path,
        diagnostic: BuiltinDiagnostic,
        role: &str,
        span: &SourceSpan,
    ) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|error| {
                Diagnostic::builtin(
                    diagnostic,
                    format!("could not open {role} lock `{}`: {error}", path.display()),
                    span.clone(),
                )
            })?;
        file.lock().map_err(|error| {
            Diagnostic::builtin(
                diagnostic,
                format!(
                    "could not acquire {role} lock `{}`: {error}",
                    path.display()
                ),
                span.clone(),
            )
        })?;
        Ok(Self { _file: file })
    }
}

pub(super) fn sibling_lock_path(path: &Path, role: &str) -> PathBuf {
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(format!(".{role}.lock"));
    path.parent().unwrap_or_else(|| Path::new(".")).join(name)
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;

    use super::*;

    #[test]
    fn a_second_handle_cannot_acquire_an_held_lock() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("artifact.lock");
        let span = SourceSpan::file_start(&path);
        let _first = FileLock::acquire(&path, BuiltinDiagnostic::PublicationLock, "test", &span)
            .expect("first lock");
        let second = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .expect("second handle");
        assert!(second.try_lock().is_err());
    }

    #[test]
    fn sibling_lock_paths_remain_beside_the_destination() {
        assert_eq!(
            sibling_lock_path(Path::new("project/final.mp4"), "publication"),
            Path::new("project/.final.mp4.publication.lock")
        );
    }
}
