//! Canonical hashing primitives for private semantic and execution identities.
//!
//! Identity owners define explicit serializable documents in their own phase.
//! This module owns deterministic JSON hashing, native-path encoding for those
//! documents, and streaming SHA-256 for file content. Callers retain ownership
//! of phase-specific diagnostics and identity structure.

#[cfg(feature = "native")]
use std::fs;
#[cfg(feature = "native")]
use std::io::{self, BufReader, Read as _};
use std::path::Path;

use serde::ser::SerializeMap as _;
use serde::{Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::source::SourceSpan;

#[derive(Clone, Copy, Debug)]
pub(crate) struct PathIdentity<'a> {
    path: &'a Path,
}

impl<'a> PathIdentity<'a> {
    pub(crate) const fn new(path: &'a Path) -> Self {
        Self { path }
    }
}

impl Serialize for PathIdentity<'_> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.path.to_str() {
            Some(path) => serializer.serialize_str(path),
            None => serialize_native_path(self.path, serializer),
        }
    }
}

#[cfg(unix)]
fn serialize_native_path<S>(path: &Path, serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    use std::os::unix::ffi::OsStrExt as _;

    let mut map = serializer.serialize_map(Some(2))?;
    map.serialize_entry("encoding", "unix_bytes")?;
    map.serialize_entry("hex", &hex::encode(path.as_os_str().as_bytes()))?;
    map.end()
}

#[cfg(windows)]
fn serialize_native_path<S>(path: &Path, serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    use std::os::windows::ffi::OsStrExt as _;

    let units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    let mut map = serializer.serialize_map(Some(2))?;
    map.serialize_entry("encoding", "windows_wide")?;
    map.serialize_entry("units", &units)?;
    map.end()
}

#[cfg(not(any(unix, windows)))]
fn serialize_native_path<S>(path: &Path, serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut map = serializer.serialize_map(Some(2))?;
    map.serialize_entry("encoding", "platform_bytes")?;
    map.serialize_entry("hex", &hex::encode(path.as_os_str().as_encoded_bytes()))?;
    map.end()
}

pub(crate) fn hash_serializable(value: &impl Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::Fingerprint,
            format!("could not serialize identity: {error}"),
            SourceSpan::file_start("<fingerprint>"),
        )
    })?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

#[cfg(feature = "native")]
pub(crate) fn hash_file(path: &Path) -> io::Result<String> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_path_identity_preserves_the_existing_json_shape() {
        let path = Path::new("assets/card.png");
        assert_eq!(
            serde_json::to_vec(&PathIdentity::new(path)).expect("path identity"),
            serde_json::to_vec(path).expect("ordinary path")
        );
    }

    #[cfg(unix)]
    #[test]
    fn native_path_identity_distinguishes_non_utf8_bytes() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt as _;
        use std::path::PathBuf;

        let first = PathBuf::from(OsString::from_vec(b"asset-\xff.png".to_vec()));
        let second = PathBuf::from(OsString::from_vec(b"asset-\xfe.png".to_vec()));
        let first = serde_json::to_value(PathIdentity::new(&first)).expect("first identity");
        let second = serde_json::to_value(PathIdentity::new(&second)).expect("second identity");

        assert_eq!(first["encoding"], "unix_bytes");
        assert_ne!(first, second);
    }

    #[test]
    #[cfg(feature = "native")]
    fn file_hash_is_streaming_sha256() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("input.bin");
        fs::write(&path, b"abc").expect("input");

        assert_eq!(
            hash_file(&path).expect("hash"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
