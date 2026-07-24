//! Structured, source-located failures shared by every pipeline phase.
//!
//! Public operations return [`Result`], allowing embedding applications to
//! inspect stable diagnostic codes, human-readable messages, locations, and
//! supplemental notes without parsing display text.
//!
//! ```
//! use std::path::Path;
//!
//! let error = clipasm::frontend::yaml::parse_str(
//!     Path::new("program.yaml"),
//!     "- program:\n    version: 1\n\n- glue:\n    body: not-a-sequence\n",
//! )
//! .expect_err("invalid source program");
//!
//! assert_eq!(error.code, "E_EXPECTED_SEQUENCE");
//! assert_eq!(error.span.line, 5);
//! ```

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::de::Deserializer;
use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};

/// One authored or generated source unit.
///
/// The display path is used in diagnostics, while the optional base directory
/// controls resolution of relative authored paths. Source text is retained so
/// richer diagnostics can be added without changing the compiler-facing span
/// model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceFile(Arc<SourceFileData>);

#[derive(Debug, Eq, PartialEq)]
struct SourceFileData {
    display_path: PathBuf,
    filesystem_path: Option<PathBuf>,
    base_directory: Option<PathBuf>,
    text: Arc<str>,
}

impl SourceFile {
    /// Construct a filesystem-backed or path-associated source unit.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, text: impl Into<Arc<str>>) -> Self {
        let display_path = path.into();
        let base_directory = display_path.parent().map(Path::to_path_buf);
        Self(Arc::new(SourceFileData {
            filesystem_path: Some(display_path.clone()),
            display_path,
            base_directory,
            text: text.into(),
        }))
    }

    /// Construct a source unit with an explicit relative-path base.
    #[must_use]
    pub fn with_base(
        display_path: impl Into<PathBuf>,
        base_directory: Option<PathBuf>,
        text: impl Into<Arc<str>>,
    ) -> Self {
        Self(Arc::new(SourceFileData {
            display_path: display_path.into(),
            filesystem_path: None,
            base_directory,
            text: text.into(),
        }))
    }

    #[must_use]
    /// Return the path or virtual name displayed in diagnostics.
    pub fn display_path(&self) -> &Path {
        &self.0.display_path
    }

    #[must_use]
    /// Return the backing filesystem path when this source came from a file.
    pub fn filesystem_path(&self) -> Option<&Path> {
        self.0.filesystem_path.as_deref()
    }

    #[must_use]
    /// Return the directory used to resolve relative authored paths.
    pub fn base_directory(&self) -> Option<&Path> {
        self.0.base_directory.as_deref()
    }

    #[must_use]
    /// Return the retained authored source text.
    pub fn text(&self) -> &str {
        &self.0.text
    }
}

/// A location in the source program.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    source: SourceFile,
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
        Self::at(SourceFile::new(file, Arc::<str>::from("")), line, column)
    }

    /// Construct a location within an existing source unit.
    #[must_use]
    pub const fn at(source: SourceFile, line: usize, column: usize) -> Self {
        Self {
            source,
            line,
            column,
        }
    }

    /// Construct the first source position in a file.
    #[must_use]
    pub fn file_start(file: impl Into<PathBuf>) -> Self {
        Self::new(file, 1, 1)
    }

    /// Construct the first position in an existing source unit.
    #[must_use]
    pub const fn source_start(source: SourceFile) -> Self {
        Self::at(source, 1, 1)
    }

    #[must_use]
    /// Return the source unit containing this location.
    pub const fn source(&self) -> &SourceFile {
        &self.source
    }

    #[must_use]
    /// Return the path or virtual source name displayed in diagnostics.
    pub fn file(&self) -> &Path {
        self.source.display_path()
    }
}

impl Serialize for SourceSpan {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("SourceSpan", 3)?;
        state.serialize_field("file", self.file())?;
        state.serialize_field("line", &self.line)?;
        state.serialize_field("column", &self.column)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for SourceSpan {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct SerializedSpan {
            file: PathBuf,
            line: usize,
            column: usize,
        }

        let span = SerializedSpan::deserialize(deserializer)?;
        Ok(Self::new(span.file, span.line, span.column))
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
            self.span.file().display(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_files_keep_display_and_relative_path_context_separate() {
        let source = SourceFile::with_base(
            "<editor-buffer>",
            Some(PathBuf::from("/project/effects")),
            "- image: card.png\n",
        );
        let span = SourceSpan::at(source, 1, 3);

        assert_eq!(span.file(), Path::new("<editor-buffer>"));
        assert_eq!(span.source().filesystem_path(), None);
        assert_eq!(
            span.source().base_directory(),
            Some(Path::new("/project/effects"))
        );
        assert_eq!(span.source().text(), "- image: card.png\n");
    }
}
