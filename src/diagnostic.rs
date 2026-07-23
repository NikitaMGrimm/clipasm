//! Structured, source-located failures shared by every pipeline phase.
//!
//! Public operations return [`Result`], allowing embedding applications to
//! inspect stable diagnostic codes, human-readable messages, locations, and
//! supplemental notes without parsing display text.
//!
//! ```
//! use std::path::Path;
//!
//! let error = clipasm::syntax::parse_str(
//!     Path::new("workflow.yaml"),
//!     "version: 1\ntimeline: not-a-sequence\n",
//! )
//! .expect_err("invalid workflow");
//!
//! assert_eq!(error.code, "E_EXPECTED_SEQUENCE");
//! assert_eq!(error.span.line, 2);
//! ```

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A location in the workflow source.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceSpan {
    /// Path associated with the authored input or generated context.
    pub file: PathBuf,
    /// One-based source line.
    pub line: usize,
    /// One-based source column.
    pub column: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Spanned<T> {
    pub(crate) value: T,
    pub(crate) span: SourceSpan,
}

impl<T> Spanned<T> {
    #[must_use]
    pub(crate) fn new(value: T, span: SourceSpan) -> Self {
        Self { value, span }
    }
}

impl SourceSpan {
    /// Construct a source location from a path and one-based coordinates.
    #[must_use]
    pub fn new(file: impl Into<PathBuf>, line: usize, column: usize) -> Self {
        Self {
            file: file.into(),
            line,
            column,
        }
    }

    /// Construct the first source position in a file.
    #[must_use]
    pub fn file_start(file: impl Into<PathBuf>) -> Self {
        Self::new(file, 1, 1)
    }
}

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

    /// Append a supplemental note.
    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Construct a path-located diagnostic from an I/O error.
    #[must_use]
    pub fn io(code: &'static str, path: &Path, error: &std::io::Error) -> Self {
        Self::new(
            code,
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
            self.span.file.display(),
            self.span.line,
            self.span.column,
            self.code,
            self.message
        )?;
        for note in &self.notes {
            write!(f, "\n\nnote: {note}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}

/// Result type returned by `ClipAsm` parsing, compilation, preparation, and rendering.
pub type Result<T> = std::result::Result<T, Diagnostic>;
