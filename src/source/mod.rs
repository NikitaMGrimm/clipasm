//! Lowered, fully desugared authored `ClipAsm` programs.
//!
//! The public package types are currently opaque compiler inputs produced by
//! the native language loader. Construction remains crate-private; no stable
//! external builder API is promised.

mod location;
mod name;

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::program::{Cardinality, InputPort, ParameterType, StackAccess, ValueTypeSpec};

pub(crate) use location::Spanned;
pub use location::{SourceFile, SourceSpan};
pub(crate) use name::{PUBLIC_NAME_GRAMMAR, is_valid_public_name};

pub(crate) const SOURCE_PROGRAM_DEFAULT_STACK_ACCESS: StackAccess = StackAccess::Owned;
pub(crate) const STACK_BLOCK_DEFAULT_STACK_ACCESS: StackAccess = StackAccess::Visible;
pub(crate) const MAX_SYNTAX_NESTING: usize = 64;

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

#[derive(Clone, Debug)]
pub(crate) struct SourceUnit {
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

#[derive(Clone, Debug)]
pub(crate) struct SourceProgram {
    pub(crate) inputs: Vec<SourceInput>,
    pub(crate) parameters: Vec<SourceParameter>,
    pub(crate) implementation: SourceProgramImplementation,
    pub(crate) span: SourceSpan,
    pub(crate) stack_access: StackAccess,
}

impl SourceProgram {
    #[must_use]
    pub(crate) fn inputs(&self) -> &[SourceInput] {
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
    pub(crate) const fn span(&self) -> &SourceSpan {
        &self.span
    }

    #[must_use]
    pub(crate) const fn stack_access(&self) -> StackAccess {
        self.stack_access
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SourceInput {
    pub(crate) name: String,
    pub(crate) value_type: ValueTypeSpec,
    pub(crate) cardinality: Cardinality,
    pub(crate) declared_at: SourceSpan,
}

impl SourceInput {
    #[must_use]
    pub(crate) fn descriptor(&self) -> InputPort {
        InputPort {
            name: self.name.clone(),
            value_type: self.value_type,
            cardinality: self.cardinality,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum SourceProgramImplementation {
    Body(ProgramBody),
    External(SourceExternalImplementation),
}

#[derive(Clone, Debug)]
pub(crate) struct SourceExternalImplementation {
    pub(crate) executable: Spanned<PathBuf>,
    pub(crate) arguments: Vec<SourceExternalArgument>,
    pub(crate) semantic_version: Spanned<u32>,
    pub(crate) preserve: Spanned<String>,
}

#[derive(Clone, Debug)]
pub(crate) enum SourceExternalArgument {
    Text(Spanned<String>),
    File(Spanned<PathBuf>),
}

#[derive(Clone, Debug)]
pub(crate) struct SourceParameter {
    pub(crate) name: Spanned<String>,
    pub(crate) parameter_type: ParameterType,
    pub(crate) default: Option<ScalarExpression>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ProjectSettings {
    pub(crate) video: VideoSettings,
    pub(crate) audio: AudioSettings,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct VideoSettings {
    pub(crate) width: Option<Spanned<u32>>,
    pub(crate) height: Option<Spanned<u32>>,
    pub(crate) fps: Option<Spanned<String>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AudioSettings {
    pub(crate) sample_rate: Option<Spanned<u32>>,
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
    ScalarBinding(ScalarBinding),
    Invocation(Invocation),
    StackBlock(StackBlock),
}

#[derive(Clone, Debug)]
pub(crate) struct ScalarBinding {
    pub(crate) name: Spanned<String>,
    pub(crate) value: ScalarExpression,
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
    Scalar(ScalarExpression),
    Reference(Spanned<String>),
    Body(ProgramBody),
}

#[derive(Clone, Debug)]
pub(crate) enum ScalarExpression {
    Literal(Literal),
    Reference(Spanned<String>),
    Selector {
        root: Spanned<String>,
        path: Vec<Spanned<String>>,
        span: SourceSpan,
    },
    Unary {
        operator: ScalarUnaryOperator,
        operand: Box<Self>,
        span: SourceSpan,
    },
    Binary {
        operator: ScalarBinaryOperator,
        left: Box<Self>,
        right: Box<Self>,
        span: SourceSpan,
    },
    Postfix {
        operator: ScalarPostfixOperator,
        operand: Box<Self>,
        span: SourceSpan,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScalarUnaryOperator {
    Positive,
    Negative,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScalarBinaryOperator {
    Range,
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScalarPostfixOperator {
    Percent,
    Milliseconds,
    Seconds,
}

#[derive(Clone, Debug)]
pub(crate) enum Literal {
    String(String, SourceSpan),
    Atom(String, SourceSpan),
    Integer(i64, SourceSpan),
}

impl ArgumentValue {
    #[must_use]
    pub(crate) const fn span(&self) -> &SourceSpan {
        match self {
            Self::Scalar(expression) => expression.span(),
            Self::Reference(reference) => &reference.span,
            Self::Body(body) => &body.span,
        }
    }
}

impl ScalarExpression {
    #[must_use]
    pub(crate) const fn span(&self) -> &SourceSpan {
        match self {
            Self::Literal(literal) => literal.span(),
            Self::Reference(reference) => &reference.span,
            Self::Selector { span, .. }
            | Self::Unary { span, .. }
            | Self::Binary { span, .. }
            | Self::Postfix { span, .. } => span,
        }
    }
}

impl Literal {
    #[must_use]
    pub(crate) const fn span(&self) -> &SourceSpan {
        match self {
            Self::String(_, span) | Self::Atom(_, span) | Self::Integer(_, span) => span,
        }
    }
}
