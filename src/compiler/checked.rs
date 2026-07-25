use std::collections::BTreeMap;

use crate::model::ValueType;
use crate::program::{ParameterType, ProgramId, ProgramRegistry, ResolvedSignature};

pub(super) use super::ids::{BodyInputId, ParameterId, ReferenceTarget, ValueLocalId};
use super::stack::StackBindingPlan;

#[derive(Clone, Debug)]
pub(super) struct CheckedParameter {
    pub(super) name: String,
    pub(super) parameter_type: ParameterType,
    pub(super) declared_at: crate::source::SourceSpan,
    pub(super) default: Option<crate::source::Spanned<crate::program::ParameterValue>>,
}

#[derive(Clone, Debug)]
pub(super) enum CheckedInputValue {
    References(Vec<ReferenceTarget>, crate::source::SourceSpan),
    Body(Box<CheckedBody>, crate::source::SourceSpan),
}

#[derive(Clone, Debug)]
pub(super) enum CheckedParameterValue {
    Literal(crate::source::Spanned<crate::program::ParameterValue>),
    Reference(ParameterId),
}

#[derive(Clone, Debug)]
pub(super) struct CheckedLocal {
    pub(super) name: String,
    pub(super) declared_at: crate::source::SourceSpan,
    pub(super) value_type: ValueType,
}

#[derive(Clone, Debug)]
pub(super) struct CheckedPackage {
    pub(super) root: crate::source::SourceUnitId,
    pub(super) registry: ProgramRegistry,
    pub(super) programs: Vec<CheckedProgram>,
}

#[derive(Clone, Debug)]
pub(super) struct CheckedProgramInput {
    pub(super) name: String,
    pub(super) value_type: ValueType,
}

#[derive(Clone, Debug)]
pub(super) struct CheckedProgram {
    pub(super) span: crate::source::SourceSpan,
    pub(super) stack_access: crate::program::StackAccess,
    pub(super) inputs: Vec<CheckedProgramInput>,
    pub(super) locals: Vec<CheckedLocal>,
    pub(super) parameters: Vec<CheckedParameter>,
    pub(super) body_input_count: usize,
    pub(super) clips: Vec<CheckedClip>,
    pub(super) body: CheckedBody,
}

#[derive(Clone, Debug)]
pub(super) struct CheckedClip {
    pub(super) name: String,
    pub(super) span: crate::source::SourceSpan,
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
    pub(super) span: crate::source::SourceSpan,
    pub(super) construct: String,
    pub(super) outputs: Vec<CheckedOutput>,
    pub(super) kind: CheckedItemKind,
}

#[derive(Clone, Debug)]
pub(super) enum CheckedItemKind {
    Reference {
        target: ReferenceTarget,
    },
    Invocation {
        program: ProgramId,
        signature: ResolvedSignature,
        access: crate::program::StackAccess,
        stack_plan: StackBindingPlan,
        inputs: Vec<Option<CheckedInputValue>>,
        parameters: Vec<Option<CheckedParameterValue>>,
        body: Option<Box<CheckedBody>>,
        body_input_ids: BTreeMap<String, BodyInputId>,
    },
}
