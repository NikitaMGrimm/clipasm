use crate::diagnostic::Result;
use crate::model::{ValueRef, ValueType};
use crate::program::{
    Cardinality, InputPort, ParameterDescriptor, ParameterType, ProgramDefinition,
    ProgramDescriptor, ProgramImplementation, ProgramOutputs, StackAccess, ValueTypeSpec,
};

pub(super) fn direct(
    descriptor: ProgramDescriptor,
    lower: crate::program::DirectLowerFn,
) -> ProgramDefinition {
    ProgramDefinition {
        descriptor,
        implementation: ProgramImplementation::Direct(lower),
    }
}

pub(super) fn exact_descriptor(
    name: &str,
    semantic_version: u32,
    inputs: Vec<InputPort>,
    parameters: Vec<ParameterDescriptor>,
    output: ValueType,
) -> ProgramDescriptor {
    ProgramDescriptor {
        name: name.to_owned(),
        semantic_version,
        default_stack_access: StackAccess::Owned,
        inputs,
        parameters,
        outputs: vec![output.into()],
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn generic_descriptor(
    name: &str,
    semantic_version: u32,
    input_name: &str,
    cardinality: Cardinality,
    parameters: Vec<ParameterDescriptor>,
    has_output: bool,
) -> ProgramDescriptor {
    ProgramDescriptor {
        name: name.to_owned(),
        semantic_version,
        default_stack_access: StackAccess::Owned,
        inputs: vec![InputPort {
            name: input_name.to_owned(),
            value_type: ValueTypeSpec::Generic,
            cardinality,
        }],
        parameters,
        outputs: has_output
            .then_some(ValueTypeSpec::Generic)
            .into_iter()
            .collect(),
    }
}

pub(super) fn input(name: &str, value_type: ValueType, cardinality: Cardinality) -> InputPort {
    InputPort {
        name: name.to_owned(),
        value_type: value_type.into(),
        cardinality,
    }
}

pub(super) fn parameter(
    name: &str,
    parameter_type: ParameterType,
    required: bool,
) -> ParameterDescriptor {
    ParameterDescriptor {
        name: name.to_owned(),
        parameter_type,
        required,
    }
}

pub(super) fn one_output(output: Result<ValueRef>) -> Result<ProgramOutputs> {
    output.map(|value| vec![value])
}
