//! Sanitized public access to the built-in diagnostic catalog.

pub use crate::diagnostic::catalog::{
    DiagnosticCategory, DiagnosticReference, RelatedReference, RetryGuidance,
};

/// Return every built-in diagnostic reference in code order.
#[must_use]
pub fn diagnostics() -> &'static [DiagnosticReference] {
    crate::diagnostic::catalog::references()
}

/// Return the reference for one exact built-in diagnostic code.
///
/// Custom codes created by embedding applications are not part of this
/// catalog.
#[must_use]
pub fn diagnostic(code: &str) -> Option<&'static DiagnosticReference> {
    crate::diagnostic::catalog::reference(code)
}
