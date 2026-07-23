use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::diagnostic::{SourceSpan, Spanned};

/// A parser-owned, syntax-valid workflow.
///
/// Fields are intentionally private so compilation cannot receive a workflow
/// that bypassed syntax validation.
///
/// ```compile_fail
/// use clipasm::syntax::Workflow;
///
/// let invalid = Workflow {
///     version: 999,
///     ..todo!()
/// };
/// ```
#[derive(Clone, Debug)]
pub struct Workflow {
    pub(super) source_path: PathBuf,
    pub(super) version: u64,
    pub(super) video: VideoSettings,
    pub(super) clips: Vec<NamedClip>,
    pub(super) root: Item,
    pub(super) output: Option<Spanned<PathBuf>>,
}

impl Workflow {
    #[must_use]
    pub(crate) fn source_path(&self) -> &Path {
        &self.source_path
    }

    #[must_use]
    pub(crate) const fn version(&self) -> u64 {
        self.version
    }

    #[must_use]
    pub(crate) const fn video(&self) -> &VideoSettings {
        &self.video
    }

    #[must_use]
    pub(crate) fn clips(&self) -> &[NamedClip] {
        &self.clips
    }

    #[must_use]
    pub(crate) const fn root(&self) -> &Item {
        &self.root
    }

    #[must_use]
    pub(crate) const fn output(&self) -> Option<&Spanned<PathBuf>> {
        self.output.as_ref()
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct VideoSettings {
    pub(crate) width: Option<Spanned<u32>>,
    pub(crate) height: Option<Spanned<u32>>,
    pub(crate) fps: Option<Spanned<String>>,
}

#[derive(Clone, Debug)]
pub(crate) struct NamedClip {
    pub(crate) name: String,
    pub(crate) body: ProgramBody,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(crate) struct ProgramBody {
    pub(crate) items: Vec<Item>,
}

#[derive(Clone, Debug)]
pub(crate) struct Item {
    pub(crate) kind: ItemKind,
    pub(crate) id: Option<Spanned<String>>,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(crate) enum ItemKind {
    Reference(Reference),
    Invocation(Invocation),
}

#[derive(Clone, Debug)]
pub(crate) struct Reference {
    pub(crate) name: Spanned<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct Invocation {
    pub(crate) program: Spanned<String>,
    pub(crate) arguments: BTreeMap<String, Argument>,
    pub(crate) body: Option<ProgramBody>,
}

#[derive(Clone, Debug)]
pub(crate) enum Argument {
    Reference(String, SourceSpan),
    String(String, SourceSpan),
    Integer(i64, SourceSpan),
    List(Vec<Argument>, SourceSpan),
}

impl Argument {
    #[must_use]
    pub(crate) fn span(&self) -> &SourceSpan {
        match self {
            Self::Reference(_, span)
            | Self::String(_, span)
            | Self::Integer(_, span)
            | Self::List(_, span) => span,
        }
    }
}
