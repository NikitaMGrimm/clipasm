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
