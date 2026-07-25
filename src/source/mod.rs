//! Lowered, fully desugared authored `ClipAsm` programs.
//!
//! The public package types are currently opaque compiler inputs produced by
//! the native language loader. Construction remains crate-private; no stable
//! alternate-frontend or external builder API is promised.

mod location;
mod name;

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::external::{ExternalProgram, ExternalProgramId};
use crate::program::{InputPort, ParameterType, StackAccess};

pub(crate) use location::Spanned;
pub use location::{SourceFile, SourceSpan};
pub(crate) use name::{PUBLIC_NAME_GRAMMAR, is_valid_public_name};

pub(crate) const SOURCE_PROGRAM_DEFAULT_STACK_ACCESS: StackAccess = StackAccess::Owned;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct SourceUnitId(pub(crate) usize);

impl SourceUnitId {
    #[must_use]
    pub(crate) const fn index(self) -> usize {
        self.0
    }
}

/// One opaque linked collection of authored source programs.
///
/// Obtain a package from the native language loader.
#[derive(Clone, Debug)]
pub struct SourcePackage {
    pub(crate) root: SourceUnitId,
    pub(crate) units: Vec<SourceUnit>,
    pub(crate) external_programs: Vec<ExternalProgram>,
}

impl SourcePackage {
    #[must_use]
    pub(crate) fn root(&self) -> &SourceUnit {
        &self.units[self.root.index()]
    }

    #[must_use]
    pub(crate) fn units(&self) -> &[SourceUnit] {
        &self.units
    }

    #[must_use]
    pub(crate) fn external_programs(&self) -> &[ExternalProgram] {
        &self.external_programs
    }
}

/// One opaque linked authored source unit.
#[derive(Clone, Debug)]
pub struct SourceUnit {
    pub(crate) source: SourceFile,
    pub(crate) imports: Vec<ResolvedImport>,
    pub(crate) externals: Vec<ResolvedExternalImport>,
    pub(crate) project: Option<Spanned<ProjectSettings>>,
    pub(crate) program: SourceProgram,
    pub(crate) output: Option<Spanned<PathBuf>>,
}

#[derive(Clone, Debug)]
pub(crate) struct UnlinkedSourceUnit {
    pub(crate) source: SourceFile,
    pub(crate) imports: Vec<SourceImport>,
    pub(crate) externals: Vec<SourceExternalImport>,
    pub(crate) project: Option<Spanned<ProjectSettings>>,
    pub(crate) program: SourceProgram,
    pub(crate) output: Option<Spanned<PathBuf>>,
}

impl SourceUnit {
    #[must_use]
    pub(crate) const fn source(&self) -> &SourceFile {
        &self.source
    }

    #[must_use]
    pub(crate) const fn project(&self) -> Option<&Spanned<ProjectSettings>> {
        self.project.as_ref()
    }

    #[must_use]
    pub(crate) fn program(&self) -> &SourceProgram {
        &self.program
    }

    #[must_use]
    pub(crate) const fn output(&self) -> Option<&Spanned<PathBuf>> {
        self.output.as_ref()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SourceImport {
    pub(crate) alias: Spanned<String>,
    pub(crate) path: Spanned<PathBuf>,
}

#[derive(Clone, Debug)]
pub(crate) struct SourceExternalImport {
    pub(crate) alias: Spanned<String>,
    pub(crate) path: Spanned<PathBuf>,
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedExternalImport {
    pub(crate) alias: Spanned<String>,
    pub(crate) target: ExternalProgramId,
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedImport {
    pub(crate) alias: Spanned<String>,
    pub(crate) target: SourceUnitId,
}

/// One opaque callable authored `ClipAsm` stack program.
#[derive(Clone, Debug)]
pub struct SourceProgram {
    pub(crate) inputs: Vec<InputPort>,
    pub(crate) parameters: Vec<SourceParameter>,
    pub(crate) body: ProgramBody,
    pub(crate) span: SourceSpan,
    pub(crate) stack_access: StackAccess,
}

impl SourceProgram {
    #[must_use]
    pub(crate) fn inputs(&self) -> &[InputPort] {
        &self.inputs
    }

    #[must_use]
    pub(crate) fn parameters(&self) -> &[SourceParameter] {
        &self.parameters
    }

    #[must_use]
    pub(crate) const fn body(&self) -> &ProgramBody {
        &self.body
    }

    #[must_use]
    pub(crate) const fn span(&self) -> &SourceSpan {
        &self.span
    }

    #[must_use]
    pub(crate) const fn stack_access(&self) -> StackAccess {
        self.stack_access
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SourceParameter {
    pub(crate) name: Spanned<String>,
    pub(crate) parameter_type: ParameterType,
    pub(crate) default: Option<Literal>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ProjectSettings {
    pub(crate) video: VideoSettings,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct VideoSettings {
    pub(crate) width: Option<Spanned<u32>>,
    pub(crate) height: Option<Spanned<u32>>,
    pub(crate) fps: Option<Spanned<String>>,
}

#[derive(Clone, Debug)]
pub(crate) struct ProgramBody {
    pub(crate) items: Vec<Item>,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(crate) struct Item {
    pub(crate) kind: ItemKind,
    pub(crate) output_bindings: OutputBindings,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(crate) enum OutputBindings {
    None,
    One(Spanned<String>),
    Many(Vec<Spanned<String>>, SourceSpan),
}

#[derive(Clone, Debug)]
pub(crate) enum ItemKind {
    Reference(Reference),
    Invocation(Invocation),
    StackBlock(StackBlock),
}

#[derive(Clone, Debug)]
pub(crate) struct StackBlock {
    pub(crate) stack_access: StackAccess,
    pub(crate) body: ProgramBody,
}

#[derive(Clone, Debug)]
pub(crate) struct Reference {
    pub(crate) name: Spanned<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct Invocation {
    pub(crate) program: Spanned<String>,
    pub(crate) type_argument: Option<Spanned<crate::model::ValueType>>,
    pub(crate) stack_access: Option<Spanned<StackAccess>>,
    pub(crate) arguments: BTreeMap<String, ArgumentValue>,
    pub(crate) body: Option<ProgramBody>,
}

#[derive(Clone, Debug)]
pub(crate) enum ArgumentValue {
    Literal(Literal),
    Reference(Spanned<String>),
    References(Vec<Spanned<String>>, SourceSpan),
    Body(ProgramBody),
}

#[derive(Clone, Debug)]
pub(crate) enum Literal {
    String(String, SourceSpan),
    Integer(i64, SourceSpan),
}

impl ArgumentValue {
    #[must_use]
    pub(crate) const fn span(&self) -> &SourceSpan {
        match self {
            Self::Literal(literal) => literal.span(),
            Self::Reference(reference) => &reference.span,
            Self::References(_, span) => span,
            Self::Body(body) => &body.span,
        }
    }
}

impl Literal {
    #[must_use]
    pub(crate) const fn span(&self) -> &SourceSpan {
        match self {
            Self::String(_, span) | Self::Integer(_, span) => span,
        }
    }
}
