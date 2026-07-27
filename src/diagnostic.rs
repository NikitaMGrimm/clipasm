//! Structured, source-located failures shared by every pipeline phase.
//!
//! Public operations return [`Result`], allowing embedding applications to
//! inspect stable diagnostic codes, human-readable messages, locations, and
//! supplemental notes without parsing display text.
//!
//! ```
//! use std::path::Path;
//!
//! let error = clipasm::language::parse_str(
//!     Path::new("program.clipasm"),
//!     "clipasm 1\nimage(unknown=1)\n",
//! )
//! .expect_err("invalid source program");
//!
//! assert_eq!(error.code, "E_UNKNOWN_PROGRAM_ARGUMENT");
//! assert_eq!(error.span.line, 2);
//! ```

use std::fmt;
use std::path::Path;

use crate::source::SourceSpan;

pub(crate) mod catalog;

pub use catalog::BuiltinDiagnostic;

/// A structured, source-located user diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    /// Stable machine-readable error code such as `E_INVALID_ARGUMENT_TYPE`.
    pub code: &'static str,
    /// Concise human-readable explanation of the failure.
    pub message: String,
    /// Most relevant authored or generated source location.
    pub span: SourceSpan,
    /// Additional context that does not replace the primary message.
    pub notes: Vec<String>,
}

impl Diagnostic {
    /// Construct a diagnostic with no supplemental notes.
    #[must_use]
    pub fn new(code: &'static str, message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            code,
            message: message.into(),
            span,
            notes: Vec::new(),
        }
    }

    /// Construct a catalog-backed built-in diagnostic with no supplemental notes.
    ///
    /// Embedding applications can continue to use [`Diagnostic::new`] for
    /// application-defined codes.
    #[must_use]
    pub fn builtin(
        diagnostic: BuiltinDiagnostic,
        message: impl Into<String>,
        span: SourceSpan,
    ) -> Self {
        Self {
            code: diagnostic.code(),
            message: message.into(),
            span,
            notes: Vec::new(),
        }
    }

    /// Append a supplemental note.
    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Construct a path-located diagnostic from an I/O error.
    #[must_use]
    pub fn io(code: &'static str, path: &Path, error: &std::io::Error) -> Self {
        Self {
            code,
            message: format!("could not access `{}`: {error}", path.display()),
            span: SourceSpan::file_start(path),
            notes: Vec::new(),
        }
    }

    /// Construct a path-located built-in diagnostic from an I/O error.
    #[must_use]
    pub fn builtin_io(diagnostic: BuiltinDiagnostic, path: &Path, error: &std::io::Error) -> Self {
        Self::builtin(
            diagnostic,
            format!("could not access `{}`: {error}", path.display()),
            SourceSpan::file_start(path),
        )
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{} [{}]\n\n{}",
            self.span.file().display(),
            self.span.line,
            self.span.column,
            self.code,
            self.message
        )?;
        if let Some(line) = self
            .span
            .source()
            .text()
            .lines()
            .nth(self.span.line.saturating_sub(1))
        {
            write!(
                f,
                "\n\n{line}\n{}^",
                " ".repeat(self.span.column.saturating_sub(1))
            )?;
        }
        for note in &self.notes {
            write!(f, "\n\nnote: {note}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}

/// Result type returned by `ClipAsm` parsing, compilation, preparation, and rendering.
pub type Result<T> = std::result::Result<T, Diagnostic>;

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::Path;

    use super::{BuiltinDiagnostic, Diagnostic};
    use crate::source::SourceSpan;

    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "the test verifies the custom diagnostic constructor retained for embedding applications"
    )]
    fn custom_and_builtin_construction_preserve_the_public_shape() {
        let span = SourceSpan::file_start("program.clipasm");
        let custom = Diagnostic::new("E_EMBEDDER_TEST", "custom failure", span.clone());
        assert_eq!(custom.code, "E_EMBEDDER_TEST");
        assert_eq!(custom.message, "custom failure");
        assert_eq!(custom.span, span);
        assert!(custom.notes.is_empty());

        let builtin = Diagnostic::builtin(
            BuiltinDiagnostic::UnknownProgram,
            "unknown program `missing`",
            span,
        )
        .note("check the import");
        assert_eq!(builtin.code, "E_UNKNOWN_PROGRAM");
        assert_eq!(builtin.message, "unknown program `missing`");
        assert_eq!(builtin.notes, ["check the import"]);
    }

    #[test]
    fn builtin_io_uses_the_typed_code_and_path_location() {
        let path = Path::new("missing.clipasm");
        let error = io::Error::new(io::ErrorKind::NotFound, "not found");
        let diagnostic = Diagnostic::builtin_io(BuiltinDiagnostic::SourceIo, path, &error);

        assert_eq!(diagnostic.code, "E_SOURCE_IO");
        assert_eq!(diagnostic.span.file(), path);
        assert!(diagnostic.message.contains("missing.clipasm"));
        assert!(diagnostic.message.contains("not found"));
    }
}
