//! Lowered, fully desugared authored `ClipAsm` programs.
//!
//! The public package types are currently opaque compiler inputs produced by
//! the native language loader. Construction remains crate-private; no stable
//! external builder API is promised.

mod location;
mod name;

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::program::{InputPort, ParameterType, StackAccess};

pub(crate) use location::Spanned;
pub use location::{SourceFile, SourceSpan};
pub(crate) use name::{PUBLIC_NAME_GRAMMAR, is_valid_public_name};

pub(crate) const SOURCE_PROGRAM_DEFAULT_STACK_ACCESS: StackAccess = StackAccess::Owned;
pub(crate) const STACK_BLOCK_DEFAULT_STACK_ACCESS: StackAccess = StackAccess::Visible;

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
}

/// One opaque linked authored source unit.
#[derive(Clone, Debug)]
pub struct SourceUnit {
    pub(crate) source: SourceFile,
    pub(crate) imports: Vec<ResolvedImport>,
    pub(crate) project: Option<Spanned<ProjectSettings>>,
    pub(crate) program: SourceProgram,
    pub(crate) output: Option<Spanned<PathBuf>>,
}

#[derive(Clone, Debug)]
pub(crate) struct UnlinkedSourceUnit {
    pub(crate) source: SourceFile,
    pub(crate) imports: Vec<SourceImport>,
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
    pub(crate) implementation: SourceProgramImplementation,
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
    pub(crate) const fn implementation(&self) -> &SourceProgramImplementation {
        &self.implementation
    }

    #[must_use]
    pub(crate) fn body(&self) -> &ProgramBody {
        let SourceProgramImplementation::Body(body) = &self.implementation else {
            panic!("external source programs do not have ClipAsm bodies");
        };
        body
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
pub(crate) enum SourceProgramImplementation {
    Body(ProgramBody),
    External(SourceExternalImplementation),
}

#[derive(Clone, Debug)]
pub(crate) struct SourceExternalImplementation {
    pub(crate) command: Spanned<PathBuf>,
    pub(crate) semantic_version: Spanned<u32>,
    pub(crate) preserve: Spanned<String>,
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
    pub(crate) origin: ItemOrigin,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ItemOrigin {
    pub(crate) construct: String,
    pub(crate) span: SourceSpan,
    pub(crate) expansion: Vec<ExpansionFrame>,
    pub(crate) visibility: SurfaceVisibility,
}

impl ItemOrigin {
    #[must_use]
    pub(crate) fn authored(construct: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            construct: construct.into(),
            span,
            expansion: Vec::new(),
            visibility: SurfaceVisibility::Visible,
        }
    }

    #[must_use]
    pub(crate) fn expanded(
        &self,
        sugar: impl Into<String>,
        role: impl Into<String>,
        visibility: SurfaceVisibility,
    ) -> Self {
        let mut expansion = self.expansion.clone();
        expansion.push(ExpansionFrame {
            sugar: sugar.into(),
            role: role.into(),
            span: self.span.clone(),
        });
        Self {
            construct: self.construct.clone(),
            span: self.span.clone(),
            expansion,
            visibility,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExpansionFrame {
    pub(crate) sugar: String,
    pub(crate) role: String,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SurfaceVisibility {
    Visible,
    Hidden,
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
