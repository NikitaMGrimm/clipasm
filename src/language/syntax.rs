use crate::model::ValueType;
use crate::program::StackAccess;
use crate::source::{SourceSpan, Spanned};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceFileSyntax {
    pub(crate) version: Spanned<u32>,
    pub(crate) statements: Vec<Statement>,
    pub(crate) span: SourceSpan,
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
