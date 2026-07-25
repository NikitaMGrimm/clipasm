use crate::model::ValueType;
use crate::program::{ParameterType, StackAccess};
use crate::source::{SourceSpan, Spanned};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceFileSyntax {
    pub(crate) version: Spanned<u32>,
    pub(crate) declarations: Vec<Declaration>,
    pub(crate) statements: Vec<Statement>,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Declaration {
    Config(ConfigDeclaration),
    Import(PathDeclaration),
    External(ExternalDeclaration),
    Input(InputDeclaration),
    Parameter(ParameterDeclaration),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExternalDeclaration {
    pub(crate) command: Option<Spanned<String>>,
    pub(crate) semantic_version: Option<Spanned<String>>,
    pub(crate) preserve: Option<Spanned<String>>,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConfigDeclaration {
    pub(crate) video: Option<VideoConfigDeclaration>,
    pub(crate) audio: Option<AudioConfigDeclaration>,
    pub(crate) output: Option<Spanned<String>>,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VideoConfigDeclaration {
    pub(crate) width: Option<Spanned<String>>,
    pub(crate) height: Option<Spanned<String>>,
    pub(crate) fps: Option<Spanned<String>>,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AudioConfigDeclaration {
    pub(crate) sample_rate: Option<Spanned<String>>,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PathDeclaration {
    pub(crate) path: Spanned<String>,
    pub(crate) alias: Spanned<String>,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InputDeclaration {
    pub(crate) name: Spanned<String>,
    pub(crate) value_type: Spanned<ValueType>,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParameterDeclaration {
    pub(crate) name: Spanned<String>,
    pub(crate) parameter_type: Spanned<ParameterType>,
    pub(crate) default: Option<Scalar>,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Scalar {
    String(Spanned<String>),
    Atom(Spanned<String>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Statement {
    pub(crate) expression: Expression,
    pub(crate) output_bindings: OutputBindings,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Expression {
    Reference(Spanned<String>),
    Invocation(Invocation),
    Block(Block),
    String(Spanned<String>),
    Atom(Spanned<String>),
}

impl Expression {
    #[must_use]
    pub(crate) const fn span(&self) -> &SourceSpan {
        match self {
            Self::Reference(value) | Self::String(value) | Self::Atom(value) => &value.span,
            Self::Invocation(invocation) => &invocation.span,
            Self::Block(block) => &block.span,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Invocation {
    pub(crate) access: Option<Spanned<StackAccess>>,
    pub(crate) name: Spanned<String>,
    pub(crate) type_argument: Option<Spanned<ValueType>>,
    pub(crate) arguments: Vec<Argument>,
    pub(crate) body: Option<Block>,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Argument {
    Positional(Expression),
    Named {
        name: Spanned<String>,
        value: Expression,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Block {
    pub(crate) access: Option<Spanned<StackAccess>>,
    pub(crate) statements: Vec<Statement>,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum OutputBindings {
    None,
    One(Spanned<String>),
    Many(Vec<Spanned<String>>, SourceSpan),
}
