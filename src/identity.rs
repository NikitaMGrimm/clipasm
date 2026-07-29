//! Canonical hashing for private semantic and execution identities.
//!
//! Identity owners define explicit serializable documents in their own phase.
//! This module owns only the deterministic JSON-to-SHA-256 mechanism and its
//! common diagnostic, keeping later phases from depending on compiler internals.

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::source::SourceSpan;

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
