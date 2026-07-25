use std::collections::HashMap;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{BufReader, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest as _, Sha256};

use crate::diagnostic::{Diagnostic, Result};
use crate::source::SourceSpan;

#[derive(Clone)]
pub(super) struct SnapshotGuard {
    root: Arc<SnapshotRoot>,
}

#[derive(Debug)]
struct SnapshotRoot {
    directory: tempfile::TempDir,
}

impl SnapshotGuard {
    fn path(&self) -> &Path {
        self.root.directory.path()
    }
}

impl fmt::Debug for SnapshotGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SnapshotGuard")
    }
}

#[derive(Debug)]
pub(super) struct AssetSnapshotStore {
    guard: SnapshotGuard,
    materialized: HashMap<SnapshotKey, PathBuf>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SnapshotKey {
    digest: String,
    extension: Option<OsString>,
}

#[derive(Debug)]
pub(super) struct AssetSnapshot {
    pub(super) path: PathBuf,
    pub(super) digest: String,
}

impl AssetSnapshotStore {
    pub(super) fn new(span: &SourceSpan) -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("clipasm-assets-")
            .tempdir()
            .map_err(|error| {
                Diagnostic::new(
                    "E_ASSET_SNAPSHOT",
                    format!("could not create private asset snapshot storage: {error}"),
                    span.clone(),
                )
            })?;
        Ok(Self {
            guard: SnapshotGuard {
                root: Arc::new(SnapshotRoot { directory }),
            },
            materialized: HashMap::new(),
        })
    }

    pub(super) fn guard(&self) -> SnapshotGuard {
        self.guard.clone()
    }

    pub(super) fn materialize(
        &mut self,
        source: &Path,
        span: &SourceSpan,
    ) -> Result<AssetSnapshot> {
        let file = fs::File::open(source).map_err(|error| {
            snapshot_error(
                source,
                span,
                format!("could not open asset for snapshotting: {error}"),
            )
        })?;
        if !file
            .metadata()
            .map_err(|error| {
                snapshot_error(
                    source,
                    span,
                    format!("could not inspect opened asset: {error}"),
                )
            })?
            .is_file()
        {
            return Err(snapshot_error(
                source,
                span,
                "opened asset is not a regular file",
            ));
        }
        let mut reader = BufReader::new(file);
        let mut staged = tempfile::NamedTempFile::new_in(self.guard.path()).map_err(|error| {
            snapshot_error(
                source,
                span,
                format!("could not create staged asset snapshot: {error}"),
            )
        })?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
        loop {
            let read = reader.read(&mut buffer).map_err(|error| {
                snapshot_error(
                    source,
                    span,
                    format!("could not read asset while snapshotting: {error}"),
                )
            })?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            staged.write_all(&buffer[..read]).map_err(|error| {
                snapshot_error(
                    source,
                    span,
                    format!("could not write staged asset snapshot: {error}"),
                )
            })?;
        }
        staged.flush().map_err(|error| {
            snapshot_error(
                source,
                span,
                format!("could not flush staged asset snapshot: {error}"),
            )
        })?;

        let digest = hex::encode(hasher.finalize());
        let key = SnapshotKey {
            digest: digest.clone(),
            extension: source.extension().map(OsString::from),
        };
        if let Some(path) = self.materialized.get(&key) {
            return Ok(AssetSnapshot {
                path: path.clone(),
                digest,
            });
        }

        let destination = self.guard.path().join(snapshot_name(&key));
        staged.persist(&destination).map_err(|error| {
            snapshot_error(
                source,
                span,
                format!("could not install asset snapshot: {}", error.error),
            )
        })?;
        self.materialized.insert(key, destination.clone());
        Ok(AssetSnapshot {
            path: destination,
            digest,
        })
    }
}

fn snapshot_name(key: &SnapshotKey) -> OsString {
    let mut name = OsString::from(&key.digest);
    if let Some(extension) = &key.extension {
        name.push(".");
        name.push(extension);
    }
    name
}

fn snapshot_error(source: &Path, span: &SourceSpan, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(
        "E_ASSET_SNAPSHOT",
        format!("{} `{}`", message.into(), source.display()),
        span.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_content_and_extension_reuses_one_snapshot() {
        let source = tempfile::tempdir().expect("source directory");
        let first = source.path().join("first.bin");
        let second = source.path().join("second.bin");
        fs::write(&first, b"same bytes").expect("first asset");
        fs::write(&second, b"same bytes").expect("second asset");
        let span = SourceSpan::file_start(&first);
        let mut store = AssetSnapshotStore::new(&span).expect("snapshot store");

        let first = store.materialize(&first, &span).expect("first snapshot");
        let second = store.materialize(&second, &span).expect("second snapshot");

        assert_eq!(first.digest, second.digest);
        assert_eq!(first.path, second.path);
        assert_eq!(fs::read(first.path).expect("snapshot bytes"), b"same bytes");
    }

    #[test]
    fn snapshot_survives_authored_file_changes_until_guard_drops() {
        let source = tempfile::tempdir().expect("source directory");
        let asset = source.path().join("asset.txt");
        fs::write(&asset, b"original").expect("asset");
        let span = SourceSpan::file_start(&asset);
        let mut store = AssetSnapshotStore::new(&span).expect("snapshot store");
        let snapshot = store.materialize(&asset, &span).expect("snapshot");
        let guard = store.guard();
        drop(store);
        fs::write(&asset, b"changed").expect("changed asset");

        assert_eq!(
            fs::read(&snapshot.path).expect("snapshot bytes"),
            b"original"
        );
        drop(guard);
        assert!(!snapshot.path.exists());
    }
}
