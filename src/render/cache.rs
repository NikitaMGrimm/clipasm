use std::fs;
use std::io::{BufReader, Read as _};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::source::SourceSpan;

const ENTRY_FORMAT_VERSION: u32 = 1;
const MAX_METADATA_BYTES: u64 = 4 * 1024;

#[derive(Deserialize, Serialize)]
struct CacheEntryDocument<'a> {
    format_version: u32,
    fingerprint: &'a str,
    content_hash: String,
}

pub(super) fn metadata_path(artifact: &Path) -> PathBuf {
    let mut name = artifact.file_name().unwrap_or_default().to_os_string();
    name.push(".cache.json");
    artifact
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(name)
}

pub(super) fn verify_entry(artifact: &Path, fingerprint: &str) -> Result<()> {
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
            format!("cache metadata is not valid JSON: {error}"),
        )
    })?;
    if document.format_version != ENTRY_FORMAT_VERSION || document.fingerprint != fingerprint {
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

pub(super) fn commit_verified(
    artifact: &Path,
    staged_metadata: &Path,
    destination: &Path,
    fingerprint: &str,
) -> Result<()> {
    let content_hash = hash_file(artifact)?;
    let document = CacheEntryDocument {
        format_version: ENTRY_FORMAT_VERSION,
        fingerprint,
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
    let file = fs::File::open(path)
        .map_err(|error| cache_error(path, format!("could not hash cache artifact: {error}")))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = reader.read(&mut buffer).map_err(|error| {
            cache_error(path, format!("could not hash cache artifact: {error}"))
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
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
    use super::*;

    #[test]
    fn committed_metadata_detects_artifact_changes() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let staged = directory.path().join("staged.mkv");
        let staged_metadata = directory.path().join("staged.json");
        let destination = directory.path().join("artifact.mkv");
        fs::write(&staged, b"verified artifact").expect("staged artifact");
        commit_verified(&staged, &staged_metadata, &destination, "fingerprint")
            .expect("commit cache entry");
        verify_entry(&destination, "fingerprint").expect("verified entry");

        fs::write(&destination, b"substituted artifact").expect("substitute artifact");
        let error = verify_entry(&destination, "fingerprint").expect_err("content mismatch");
        assert!(error.message.contains("recorded hash"));
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
        let error = verify_entry(&artifact, "fingerprint").expect_err("symlink");
        assert!(error.message.contains("symlink"));
    }
}
