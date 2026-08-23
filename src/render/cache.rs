use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::source::SourceSpan;

use super::staging::StagingDirectory;

const ENTRY_FORMAT_VERSION: u32 = 2;
const MAX_METADATA_BYTES: u64 = 4 * 1024;

#[derive(Deserialize, Serialize)]
struct CacheEntryDocument<'a> {
    format_version: u32,
    execution_namespace: &'a str,
    fingerprint: &'a str,
    content_hash: String,
}

#[derive(Clone, Copy)]
pub(super) struct CacheEntryIdentity<'a> {
    execution_namespace: &'a str,
    fingerprint: &'a str,
}

impl<'a> CacheEntryIdentity<'a> {
    pub(super) const fn new(execution_namespace: &'a str, fingerprint: &'a str) -> Self {
        Self {
            execution_namespace,
            fingerprint,
        }
    }
}

pub(super) struct StagedArtifact {
    _staging: StagingDirectory,
    path: PathBuf,
    metadata: PathBuf,
    destination: PathBuf,
}

pub(super) struct VerifiedArtifact(StagedArtifact);

impl StagedArtifact {
    pub(super) fn planned_path(destination: &Path, extension: &str) -> Result<PathBuf> {
        StagingDirectory::planned_path(
            destination,
            "cache",
            &format!("artifact.{extension}"),
            BuiltinDiagnostic::CacheIo,
        )
    }

    pub(super) fn new(destination: &Path, extension: &str) -> Result<Self> {
        let staging = StagingDirectory::beside(destination, "cache", BuiltinDiagnostic::CacheIo)?;
        Ok(Self {
            path: staging.path(&format!("artifact.{extension}")),
            metadata: staging.path("artifact.cache.json"),
            destination: destination.to_path_buf(),
            _staging: staging,
        })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn verify(
        self,
        verifier: impl FnOnce(&Path) -> Result<()>,
    ) -> Result<VerifiedArtifact> {
        require_regular_file(&self.path, "staged cache artifact")?;
        verifier(&self.path)?;
        Ok(VerifiedArtifact(self))
    }
}

impl VerifiedArtifact {
    pub(super) fn commit(self, identity: CacheEntryIdentity<'_>) -> Result<()> {
        let staged = self.0;
        commit_verified(
            &staged.path,
            &staged.metadata,
            &staged.destination,
            identity,
        )
    }
}

pub(super) fn metadata_path(artifact: &Path) -> PathBuf {
    let mut name = artifact.file_name().unwrap_or_default().to_os_string();
    name.push(".cache.json");
    artifact
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(name)
}

pub(super) fn verify_entry(artifact: &Path, identity: CacheEntryIdentity<'_>) -> Result<()> {
    require_regular_file(artifact, "cache artifact")?;
    let metadata_path = metadata_path(artifact);
    let metadata = require_regular_file(&metadata_path, "cache metadata")?;
    if metadata.len() > MAX_METADATA_BYTES {
        return Err(cache_error(
            &metadata_path,
            format!(
                "cache metadata is too large: {} bytes exceeds {MAX_METADATA_BYTES}",
                metadata.len()
            ),
        ));
    }
    let bytes = fs::read(&metadata_path).map_err(|error| {
        cache_error(
            &metadata_path,
            format!("could not read cache metadata: {error}"),
        )
    })?;
    let document: CacheEntryDocument<'_> = serde_json::from_slice(&bytes).map_err(|error| {
        cache_error(
            &metadata_path,
            format!("cache metadata does not match the current schema: {error}"),
        )
    })?;
    if document.format_version != ENTRY_FORMAT_VERSION
        || document.execution_namespace != identity.execution_namespace
        || document.fingerprint != identity.fingerprint
    {
        return Err(cache_error(
            &metadata_path,
            "cache metadata does not identify this prepared node",
        ));
    }
    let actual = hash_file(artifact)?;
    if actual != document.content_hash {
        return Err(cache_error(
            artifact,
            "cache artifact content does not match its recorded hash",
        ));
    }
    Ok(())
}

fn commit_verified(
    artifact: &Path,
    staged_metadata: &Path,
    destination: &Path,
    identity: CacheEntryIdentity<'_>,
) -> Result<()> {
    require_regular_file(artifact, "staged cache artifact")?;
    let content_hash = hash_file(artifact)?;
    let document = CacheEntryDocument {
        format_version: ENTRY_FORMAT_VERSION,
        execution_namespace: identity.execution_namespace,
        fingerprint: identity.fingerprint,
        content_hash,
    };
    let bytes = serde_json::to_vec(&document).map_err(|error| {
        cache_error(
            staged_metadata,
            format!("could not serialize cache metadata: {error}"),
        )
    })?;
    fs::write(staged_metadata, bytes).map_err(|error| {
        cache_error(
            staged_metadata,
            format!("could not write staged cache metadata: {error}"),
        )
    })?;

    let destination_metadata = metadata_path(destination);
    fs::rename(staged_metadata, &destination_metadata).map_err(|error| {
        cache_error(
            &destination_metadata,
            format!("could not commit cache metadata: {error}"),
        )
    })?;
    if let Err(error) = fs::rename(artifact, destination) {
        let cleanup = fs::remove_file(&destination_metadata);
        let diagnostic = cache_error(
            destination,
            format!("could not commit verified cache artifact: {error}"),
        );
        return Err(match cleanup {
            Ok(()) => diagnostic,
            Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => {
                diagnostic
            }
            Err(cleanup_error) => diagnostic.note(format!(
                "could not remove committed cache metadata `{}` after artifact failure: {cleanup_error}",
                destination_metadata.display()
            )),
        });
    }
    Ok(())
}

pub(super) fn remove_entry(artifact: &Path) -> Result<()> {
    remove_if_present(artifact)?;
    remove_if_present(&metadata_path(artifact))
}

fn require_regular_file(path: &Path, role: &str) -> Result<fs::Metadata> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(cache_error(path, format!("{role} is a symlink")))
        }
        Ok(metadata) if metadata.is_file() => Ok(metadata),
        Ok(_) => Err(cache_error(path, format!("{role} is not a regular file"))),
        Err(error) => Err(cache_error(
            path,
            format!("could not inspect {role}: {error}"),
        )),
    }
}

fn remove_if_present(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => fs::remove_file(path).map_err(|error| {
            cache_error(
                path,
                format!("could not remove invalid cache entry: {error}"),
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(cache_error(
            path,
            format!("could not inspect invalid cache entry: {error}"),
        )),
    }
}

fn hash_file(path: &Path) -> Result<String> {
    crate::identity::hash_file(path)
        .map_err(|error| cache_error(path, format!("could not hash cache artifact: {error}")))
}

fn cache_error(path: &Path, message: impl Into<String>) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::CacheIo,
        message,
        SourceSpan::file_start(path),
    )
}

#[cfg(test)]
mod tests {
    use sha2::Digest as _;

    use super::*;

    fn identity<'a>() -> CacheEntryIdentity<'a> {
        CacheEntryIdentity::new("namespace", "fingerprint")
    }

    #[test]
    fn unverified_staging_is_removed_on_drop() {
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
    fn only_verified_staging_can_be_committed() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let destination = directory.path().join("artifact.mkv");
        let staged = StagedArtifact::new(&destination, "mkv").expect("staging");
        let staging_parent = staged
            .path()
            .parent()
            .expect("staging parent")
            .to_path_buf();
        fs::write(staged.path(), b"verified artifact").expect("staged bytes");
        staged
            .verify(|_| Ok(()))
            .expect("verification")
            .commit(identity())
            .expect("commit");
        assert_eq!(
            fs::read(&destination).expect("artifact"),
            b"verified artifact"
        );
        assert!(!staging_parent.exists());
        verify_entry(&destination, identity()).expect("cache metadata");
    }

    #[test]
    fn committed_metadata_detects_artifact_changes() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let staged = directory.path().join("staged.mkv");
        let staged_metadata = directory.path().join("staged.json");
        let destination = directory.path().join("artifact.mkv");
        fs::write(&staged, b"verified artifact").expect("staged artifact");
        commit_verified(&staged, &staged_metadata, &destination, identity())
            .expect("commit cache entry");
        verify_entry(&destination, identity()).expect("verified entry");

        fs::write(&destination, b"substituted artifact").expect("substitute artifact");
        let error = verify_entry(&destination, identity()).expect_err("content mismatch");
        assert!(error.message.contains("recorded hash"));
    }

    #[test]
    fn metadata_must_match_the_execution_namespace() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let staged = directory.path().join("staged.mkv");
        let staged_metadata = directory.path().join("staged.json");
        let destination = directory.path().join("artifact.mkv");
        fs::write(&staged, b"verified artifact").expect("staged artifact");
        commit_verified(&staged, &staged_metadata, &destination, identity())
            .expect("commit cache entry");

        let wrong = CacheEntryIdentity::new("other-namespace", "fingerprint");
        let error = verify_entry(&destination, wrong).expect_err("namespace mismatch");
        assert!(error.message.contains("does not identify"));
    }

    #[test]
    fn legacy_metadata_is_not_reusable() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let artifact = directory.path().join("artifact.mkv");
        fs::write(&artifact, b"verified artifact").expect("artifact");
        fs::write(
            metadata_path(&artifact),
            serde_json::to_vec(&serde_json::json!({
                "format_version": 1,
                "fingerprint": "fingerprint",
                "content_hash": hex::encode(sha2::Sha256::digest(b"verified artifact")),
            }))
            .expect("legacy metadata"),
        )
        .expect("write legacy metadata");

        let error = verify_entry(&artifact, identity()).expect_err("legacy metadata");
        assert!(error.message.contains("current schema"));
    }

    #[cfg(unix)]
    #[test]
    fn cache_entries_reject_symlinks() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().expect("temporary directory");
        let target = directory.path().join("target.mkv");
        let artifact = directory.path().join("artifact.mkv");
        fs::write(&target, b"target").expect("target");
        symlink(&target, &artifact).expect("artifact symlink");
        let error = verify_entry(&artifact, identity()).expect_err("symlink");
        assert!(error.message.contains("symlink"));
    }

    #[cfg(unix)]
    #[test]
    fn cache_admission_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().expect("temporary directory");
        let destination = directory.path().join("artifact.mkv");
        let target = directory.path().join("target.mkv");
        fs::write(&target, b"target").expect("target");
        let staged = StagedArtifact::new(&destination, "mkv").expect("staging");
        symlink(&target, staged.path()).expect("staged symlink");

        let Err(error) = staged.verify(|_| Ok(())) else {
            panic!("symlink admission must fail");
        };
        assert!(error.message.contains("staged cache artifact is a symlink"));
        assert!(!destination.exists());
    }

    #[cfg(unix)]
    #[test]
    fn cache_commit_rechecks_staged_file_type() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().expect("temporary directory");
        let target = directory.path().join("target.mkv");
        let staged = directory.path().join("staged.mkv");
        let staged_metadata = directory.path().join("staged.json");
        let destination = directory.path().join("artifact.mkv");
        fs::write(&target, b"target").expect("target");
        symlink(&target, &staged).expect("staged symlink");

        let error = commit_verified(&staged, &staged_metadata, &destination, identity())
            .expect_err("symlink commit");
        assert!(error.message.contains("staged cache artifact is a symlink"));
        assert!(!staged_metadata.exists());
        assert!(!destination.exists());
    }
}
