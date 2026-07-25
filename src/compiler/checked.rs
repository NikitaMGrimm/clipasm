use std::collections::BTreeMap;
use std::sync::Arc;

use crate::model::ValueType;
use crate::program::{InputPort, ProgramId, ProgramRegistry, ResolvedSignature};

use super::stack::StackBindingPlan;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct ValueLocalId(pub(super) u32);

impl ValueLocalId {
    pub(super) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct BodyInputId(pub(super) u32);

impl BodyInputId {
    pub(super) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct ParameterId(pub(super) u32);

impl ParameterId {
    pub(super) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug)]
pub(super) struct CheckedParameter {
    pub(super) name: String,
    pub(super) declared_at: crate::source::SourceSpan,
    pub(super) default: Option<crate::source::Spanned<crate::program::ParameterValue>>,
}

#[derive(Clone, Debug)]
pub(super) enum CheckedInputValue {
    References(Vec<CheckedReferenceTarget>, crate::source::SourceSpan),
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
    pub(super) registry: ProgramRegistry,
    pub(super) programs: Vec<Arc<CheckedProgram>>,
}

#[derive(Clone, Debug)]
pub(super) struct CheckedProgram {
    pub(super) span: crate::source::SourceSpan,
    pub(super) stack_access: crate::program::StackAccess,
    pub(super) inputs: Vec<InputPort>,
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
pub(super) struct CheckedItem {
    pub(super) span: crate::source::SourceSpan,
    pub(super) construct: String,
    pub(super) output_names: Vec<Option<String>>,
    pub(super) output_types: Vec<ValueType>,
    pub(super) output_bindings: Vec<Option<ValueLocalId>>,
    pub(super) kind: CheckedItemKind,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum CheckedReferenceTarget {
    Local(ValueLocalId),
    BodyInput(BodyInputId),
}

#[derive(Clone, Debug)]
pub(super) enum CheckedItemKind {
    Reference {
        target: Option<CheckedReferenceTarget>,
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
