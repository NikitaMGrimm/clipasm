use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A location in the workflow source.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceSpan {
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Spanned<T> {
    pub value: T,
    pub span: SourceSpan,
}

impl<T> Spanned<T> {
    #[must_use]
    pub fn new(value: T, span: SourceSpan) -> Self {
        Self { value, span }
    }
}

impl SourceSpan {
    #[must_use]
    pub fn new(file: impl Into<PathBuf>, line: usize, column: usize) -> Self {
        Self {
            file: file.into(),
            line,
            column,
        }
    }

    #[must_use]
    pub fn file_start(file: impl Into<PathBuf>) -> Self {
        Self::new(file, 1, 1)
    }
}

/// A structured, source-located user diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: SourceSpan,
    pub notes: Vec<String>,
}

impl Diagnostic {
    #[must_use]
    pub fn new(code: &'static str, message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            code,
            message: message.into(),
            span,
            notes: Vec::new(),
        }
    }

    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

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

pub type Result<T> = std::result::Result<T, Diagnostic>;
