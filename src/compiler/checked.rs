use crate::model::ValueType;
use crate::program::{ProgramId, ProgramRegistry, ResolvedSignature};

pub(super) use super::ids::{
    BodyInputId, ParameterId, ReferenceTarget, ScalarAliasId, ValueLocalId,
};
use super::stack::StackBindingPlan;

#[derive(Clone, Debug)]
pub(super) struct CheckedParameter {
    pub(super) name: String,
    pub(super) declared_at: crate::source::SourceSpan,
    pub(super) default: Option<crate::source::Spanned<crate::program::ParameterValue>>,
}

#[derive(Clone, Debug)]
pub(super) struct CheckedScalarAlias {
    pub(super) expression: CheckedScalarExpression,
}

#[derive(Clone, Debug)]
pub(super) enum CheckedInputValue {
    References(Vec<ReferenceTarget>, crate::source::SourceSpan),
    Body(Box<CheckedBody>, crate::source::SourceSpan),
}

#[derive(Clone, Debug)]
pub(super) enum CheckedParameterValue {
    Expression(CheckedScalarExpression),
}

#[derive(Clone, Debug)]
pub(super) enum CheckedScalarExpression {
    Literal(crate::source::Literal),
    Parameter {
        id: ParameterId,
        name: String,
        span: crate::source::SourceSpan,
    },
    ScalarAlias {
        id: ScalarAliasId,
        name: String,
        span: crate::source::SourceSpan,
    },
    TimelineSelector {
        root: ReferenceTarget,
        root_name: String,
        path: Vec<String>,
        contextual: bool,
        span: crate::source::SourceSpan,
    },
    Unary {
        operator: crate::source::ScalarUnaryOperator,
        operand: Box<Self>,
        span: crate::source::SourceSpan,
    },
    Binary {
        operator: crate::source::ScalarBinaryOperator,
        left: Box<Self>,
        right: Box<Self>,
        span: crate::source::SourceSpan,
    },
    Postfix {
        operator: crate::source::ScalarPostfixOperator,
        operand: Box<Self>,
        span: crate::source::SourceSpan,
    },
}

#[derive(Clone, Debug)]
pub(super) struct CheckedLocal {
    pub(super) name: String,
    pub(super) declared_at: crate::source::SourceSpan,
    pub(super) value_type: ValueType,
}

#[derive(Debug)]
pub(super) struct CheckedPackage {
    pub(super) root: crate::source::SourceUnitId,
    pub(super) registry: ProgramRegistry,
    pub(super) programs: Vec<CheckedSourceProgram>,
}

#[derive(Clone, Debug)]
pub(super) enum CheckedSourceProgram {
    ClipAsm {
        definition: ProgramId,
        program: CheckedProgram,
    },
    External {
        definition: ProgramId,
    },
}

impl CheckedSourceProgram {
    pub(super) const fn definition(&self) -> ProgramId {
        match self {
            Self::ClipAsm { definition, .. } | Self::External { definition } => *definition,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct CheckedProgramInput {
    pub(super) name: String,
    pub(super) declared_at: crate::source::SourceSpan,
    pub(super) local: ValueLocalId,
}

#[derive(Clone, Debug)]
pub(super) struct CheckedProgram {
    pub(super) span: crate::source::SourceSpan,
    pub(super) stack_access: crate::program::StackAccess,
    pub(super) inputs: Vec<CheckedProgramInput>,
    pub(super) locals: Vec<CheckedLocal>,
    pub(super) parameters: Vec<CheckedParameter>,
    pub(super) scalar_aliases: Vec<CheckedScalarAlias>,
    pub(super) body_input_count: usize,
    pub(super) body: CheckedBody,
}

#[derive(Clone, Debug)]
pub(super) struct CheckedBody {
    pub(super) items: Vec<CheckedItem>,
}

#[derive(Clone, Debug)]
pub(super) struct CheckedOutput {
    pub(super) name: Option<String>,
    pub(super) value_type: ValueType,
    pub(super) binding: Option<ValueLocalId>,
}

#[derive(Clone, Debug)]
pub(super) struct CheckedItem {
    pub(super) origin: crate::source::ItemOrigin,
    pub(super) outputs: Vec<CheckedOutput>,
    pub(super) kind: CheckedItemKind,
}

#[derive(Clone, Debug)]
pub(super) struct CheckedInvocation {
    pub(super) program: ProgramId,
    pub(super) signature: ResolvedSignature,
    pub(super) access: crate::program::StackAccess,
    pub(super) stack_plan: StackBindingPlan,
    pub(super) inputs: Vec<Option<CheckedInputValue>>,
    pub(super) parameters: Vec<Option<CheckedParameterValue>>,
    pub(super) body: Option<Box<CheckedBody>>,
    pub(super) body_input_ids: Vec<Option<BodyInputId>>,
}

#[derive(Clone, Debug)]
pub(super) struct CheckedStackBlock {
    pub(super) access: crate::program::StackAccess,
    pub(super) body: Box<CheckedBody>,
}

#[derive(Clone, Debug)]
pub(super) enum CheckedItemKind {
    Reference { target: ReferenceTarget },
    Invocation(CheckedInvocation),
    StackBlock(CheckedStackBlock),
}
