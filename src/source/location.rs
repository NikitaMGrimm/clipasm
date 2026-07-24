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

    /// Return the path or virtual name displayed in diagnostics.
    #[must_use]
    pub fn display_path(&self) -> &Path {
        &self.0.display_path
    }

    /// Return the backing filesystem path when this source came from a file.
    #[must_use]
    pub fn filesystem_path(&self) -> Option<&Path> {
        self.0.filesystem_path.as_deref()
    }

    /// Return the directory used to resolve relative authored paths.
    #[must_use]
    pub fn base_directory(&self) -> Option<&Path> {
        self.0.base_directory.as_deref()
    }

    /// Return the retained authored source text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.0.text
    }
}

/// A location in an authored or generated source unit.
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

    /// Return the source unit containing this location.
    #[must_use]
    pub const fn source(&self) -> &SourceFile {
        &self.source
    }

    /// Return the path or virtual source name displayed in diagnostics.
    #[must_use]
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
