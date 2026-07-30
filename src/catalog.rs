//! Shared vocabulary for authored and registered program signatures.
//!
//! These types describe programs without depending on source-package or
//! semantic-graph implementations. Keeping them neutral lets both phases use
//! the same vocabulary without introducing a dependency cycle.

use crate::model::ValueType;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct InputSlot(usize);

impl InputSlot {
    #[must_use]
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }

    #[must_use]
    pub(crate) const fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ParameterSlot(usize);

impl ParameterSlot {
    #[must_use]
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }

    #[must_use]
    pub(crate) const fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Cardinality {
    One,
    Variadic { min: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StackAccess {
    Owned,
    Visible,
}

impl StackAccess {
    #[must_use]
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Owned => "owned",
            Self::Visible => "visible",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ValueTypeSpec {
    Exact(ValueType),
    Generic,
}

impl ValueTypeSpec {
    #[must_use]
    pub(crate) const fn exact(self) -> Option<ValueType> {
        match self {
            Self::Exact(value_type) => Some(value_type),
            Self::Generic => None,
        }
    }
}

impl From<ValueType> for ValueTypeSpec {
    fn from(value_type: ValueType) -> Self {
        Self::Exact(value_type)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InputPort {
    pub(crate) name: String,
    pub(crate) value_type: ValueTypeSpec,
    pub(crate) cardinality: Cardinality,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedSignature {
    pub(crate) inputs: Vec<ValueType>,
    pub(crate) outputs: Vec<ValueType>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParameterType {
    Number,
    Integer,
    File,
    Duration,
    TimeRange,
    Keyword(Vec<String>),
}

impl ParameterType {
    #[must_use]
    pub(crate) fn from_source_name(
        name: &str,
        keyword_values: Option<Vec<String>>,
    ) -> Option<Self> {
        match name {
            "Number" => Some(Self::Number),
            "Integer" => Some(Self::Integer),
            "File" => Some(Self::File),
            "Duration" => Some(Self::Duration),
            "TimeRange" => Some(Self::TimeRange),
            "Keyword" => keyword_values.map(Self::Keyword),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParameterDescriptor {
    pub(crate) name: String,
    pub(crate) parameter_type: ParameterType,
    pub(crate) required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProgramDescriptor {
    pub(crate) name: String,
    pub(crate) semantic_version: u32,
    pub(crate) default_stack_access: StackAccess,
    pub(crate) inputs: Vec<InputPort>,
    pub(crate) parameters: Vec<ParameterDescriptor>,
    pub(crate) outputs: Vec<ValueTypeSpec>,
}

impl ProgramDescriptor {
    #[must_use]
    pub(crate) fn input(&self, slot: InputSlot) -> &InputPort {
        &self.inputs[slot.index()]
    }

    #[must_use]
    pub(crate) fn input_slot(&self, name: &str) -> Option<InputSlot> {
        self.inputs
            .iter()
            .position(|input| input.name == name)
            .map(InputSlot::new)
    }

    #[must_use]
    pub(crate) fn parameter_slot(&self, name: &str) -> Option<ParameterSlot> {
        self.parameters
            .iter()
            .position(|parameter| parameter.name == name)
            .map(ParameterSlot::new)
    }

    #[must_use]
    pub(crate) fn is_generic(&self) -> bool {
        self.inputs
            .iter()
            .any(|port| matches!(port.value_type, ValueTypeSpec::Generic))
            || self
                .outputs
                .iter()
                .any(|output| matches!(output, ValueTypeSpec::Generic))
    }

    pub(crate) fn resolve_signature(&self, generic: Option<ValueType>) -> ResolvedSignature {
        let resolve = |spec: ValueTypeSpec| match spec {
            ValueTypeSpec::Exact(value_type) => value_type,
            ValueTypeSpec::Generic => generic.expect("generic descriptor has a resolved type"),
        };
        ResolvedSignature {
            inputs: self
                .inputs
                .iter()
                .map(|port| resolve(port.value_type))
                .collect(),
            outputs: self.outputs.iter().copied().map(resolve).collect(),
        }
    }
}
