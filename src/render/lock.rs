use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use crate::diagnostic::{Diagnostic, Result};
use crate::source::SourceSpan;

pub(super) struct FileLock {
    _file: File,
}

impl FileLock {
    pub(super) fn acquire(
        path: &Path,
        code: &'static str,
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
                Diagnostic::new(
                    code,
                    format!("could not open {role} lock `{}`: {error}", path.display()),
                    span.clone(),
                )
            })?;
        file.lock().map_err(|error| {
            Diagnostic::new(
                code,
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
