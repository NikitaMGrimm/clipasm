mod builtins;
mod call;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{
    ExactNumber, FrameCount, FrameRange, NativeRange, SampleRange, SourceTime, SourceTimeRange,
    TimelineRangeExpression, TimelineViewId, ValueRef, ValueType, exact_seconds_to_frames,
    exact_seconds_to_samples,
};
use crate::semantic::GraphBuilder;
use crate::source::{SourceSpan, SourceUnitId};

pub(crate) use builtins::{
    BuiltinBodyInitialValue, BuiltinDefault, BuiltinProgram, builtin_programs, builtin_references,
};
pub(crate) use call::{ResolvedCall, ResolvedInput};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ProgramId(u32);

impl ProgramId {
    #[must_use]
    pub(crate) const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

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
pub(crate) enum ParameterValue {
    Number(ExactNumber),
    Integer(i64),
    File(PathBuf),
    Duration(SourceTime),
    TimeRange(TimeRangeValue),
    Keyword(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TimeRangeValue {
    Absolute(SourceTimeRange),
    Marker {
        owner: TimelineViewId,
        range: TimelineRangeExpression,
    },
}

impl TimeRangeValue {
    pub(crate) fn to_native_range(
        &self,
        value_type: ValueType,
        fps: crate::model::FrameRate,
        sample_rate: u32,
        span: &SourceSpan,
    ) -> Result<NativeTimeRange> {
        match value_type {
            ValueType::Video => self.to_frame_range(fps, span),
            ValueType::Audio => self.to_sample_range(sample_rate, span),
        }
    }

    fn to_frame_range(
        &self,
        fps: crate::model::FrameRate,
        span: &SourceSpan,
    ) -> Result<NativeTimeRange> {
        match self {
            Self::Absolute(range) => range
                .to_frames(fps, span)
                .map(|range| NativeTimeRange::Concrete(NativeRange::Frames(range))),
            Self::Marker { range, .. } => {
                let (Some(start), Some(end)) =
                    (range.start.constant_value(), range.end.constant_value())
                else {
                    return Ok(NativeTimeRange::Deferred(range.clone()));
                };
                let start = exact_seconds_to_frames(start, fps, span)?;
                let end = exact_seconds_to_frames(end, fps, span)?;
                FrameRange::new(start, end)
                    .map(|range| NativeTimeRange::Concrete(NativeRange::Frames(range)))
                    .ok_or_else(|| invalid_timeline_range(span))
            }
        }
    }

    fn to_sample_range(&self, sample_rate: u32, span: &SourceSpan) -> Result<NativeTimeRange> {
        match self {
            Self::Absolute(range) => range
                .to_samples(sample_rate, span)
                .map(|range| NativeTimeRange::Concrete(NativeRange::Samples(range))),
            Self::Marker { range, .. } => {
                let (Some(start), Some(end)) =
                    (range.start.constant_value(), range.end.constant_value())
                else {
                    return Ok(NativeTimeRange::Deferred(range.clone()));
                };
                let start = exact_seconds_to_samples(start, sample_rate, span)?;
                let end = exact_seconds_to_samples(end, sample_rate, span)?;
                SampleRange::new(start, end)
                    .map(|range| NativeTimeRange::Concrete(NativeRange::Samples(range)))
                    .ok_or_else(|| invalid_timeline_range(span))
            }
        }
    }

    pub(crate) const fn marker_owner(&self) -> Option<TimelineViewId> {
        match self {
            Self::Absolute(_) => None,
            Self::Marker { owner, .. } => Some(*owner),
        }
    }
}

fn invalid_timeline_range(span: &SourceSpan) -> Diagnostic {
    Diagnostic::new(
        "E_INVALID_TIME_RANGE",
        "timeline-range start must be earlier than its end",
        span.clone(),
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NativeTimeRange {
    Concrete(NativeRange),
    Deferred(TimelineRangeExpression),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RequestedVideoExtent {
    Concrete(FrameCount),
    Deferred(crate::model::TimelineExpression),
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BodyContract {
    pub(crate) initial_values: Vec<ValueTypeSpec>,
    pub(crate) outputs: BodyOutputConstraint,
    pub(crate) count_error_code: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BodyOutputConstraint {
    Exactly(Vec<ValueTypeSpec>),
    Variadic {
        value_type: ValueTypeSpec,
        min: usize,
    },
}

pub(crate) type ProgramOutputs = Vec<ValueRef>;
pub(crate) type DirectLowerFn = for<'call, 'graph> fn(
    &ResolvedCall<'call>,
    &mut GraphBuilder<'graph>,
) -> Result<ProgramOutputs>;
pub(crate) type BodyPrepareFn =
    for<'call, 'graph> fn(&ResolvedCall<'call>, &mut GraphBuilder<'graph>) -> Result<BodyPlan>;

#[derive(Clone)]
pub(crate) enum ProgramImplementation {
    Direct(DirectLowerFn),
    Body {
        prepare: BodyPrepareFn,
        contract: BodyContract,
    },
    ClipAsm(SourceUnitId),
    External(crate::external::ExternalRuntime),
}

impl std::fmt::Debug for ProgramImplementation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Direct(_) => "Direct",
            Self::Body { .. } => "Body",
            Self::ClipAsm(_) => "ClipAsm",
            Self::External(_) => "External",
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ProgramDefinition {
    pub(crate) descriptor: ProgramDescriptor,
    pub(crate) implementation: ProgramImplementation,
    pub(crate) timeline_behavior: TimelineBehavior,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TimelineBehavior {
    Fresh,
    Identity { input: InputSlot },
    Repeat { input: InputSlot },
    Concat { input: InputSlot },
    BodyConcat { inputs: &'static [InputSlot] },
    Crop { input: InputSlot },
    Replace { base: InputSlot },
    FlashCut { before: InputSlot, after: InputSlot },
    Crossfade { before: InputSlot, after: InputSlot },
}

pub(crate) struct BodyPlan {
    pub(crate) initial_values: Vec<ValueRef>,
    pub(crate) requested_extent: Option<RequestedVideoExtent>,
    pub(crate) finalizer: Box<dyn BodyFinalizer>,
}

pub(crate) trait BodyFinalizer {
    fn finish(
        self: Box<Self>,
        values: Vec<ValueRef>,
        builder: &mut GraphBuilder<'_>,
    ) -> Result<ProgramOutputs>;
}

#[derive(Debug)]
struct ProgramCatalogData {
    definitions: Vec<ProgramDefinition>,
    names: BTreeMap<String, ProgramId>,
}

#[derive(Debug)]
pub(crate) struct ProgramRegistry {
    data: ProgramCatalogData,
}

impl Default for ProgramRegistry {
    fn default() -> Self {
        Self::from_definitions(builtin_programs()).expect("built-in program definitions are valid")
    }
}

impl ProgramRegistry {
    pub(crate) fn from_definitions(definitions: Vec<ProgramDefinition>) -> Result<Self> {
        validate_definitions(&definitions)?;
        let names = definitions
            .iter()
            .enumerate()
            .map(|(index, definition)| {
                (
                    definition.descriptor.name.clone(),
                    ProgramId::new(u32::try_from(index).expect("program catalog fits in u32")),
                )
            })
            .collect();
        Ok(Self {
            data: ProgramCatalogData { definitions, names },
        })
    }

    pub(crate) fn from_linked(
        definitions: Vec<ProgramDefinition>,
        builtin_count: usize,
    ) -> Result<Self> {
        validate_definitions(&definitions)?;
        let names = definitions[..builtin_count]
            .iter()
            .enumerate()
            .map(|(index, definition)| {
                (
                    definition.descriptor.name.clone(),
                    ProgramId::new(u32::try_from(index).expect("program catalog fits in u32")),
                )
            })
            .collect();
        Ok(Self {
            data: ProgramCatalogData { definitions, names },
        })
    }

    #[must_use]
    pub(crate) fn id(&self, name: &str) -> Option<ProgramId> {
        self.data.names.get(name).copied()
    }

    #[must_use]
    pub(crate) fn definition(&self, id: ProgramId) -> &ProgramDefinition {
        &self.data.definitions[id.index()]
    }

    #[must_use]
    pub(crate) fn definitions(&self) -> &[ProgramDefinition] {
        &self.data.definitions
    }
}

fn validate_definitions(definitions: &[ProgramDefinition]) -> Result<()> {
    let mut programs = BTreeSet::new();
    for definition in definitions {
        let descriptor = &definition.descriptor;
        validate_definition_name("program", &descriptor.name)?;
        if !programs.insert(descriptor.name.as_str()) {
            return Err(definition_error(format!(
                "duplicate program name `{}`",
                descriptor.name
            )));
        }

        let mut arguments = BTreeSet::new();
        let mut fixed = false;
        let mut variadic = false;
        for port in &descriptor.inputs {
            validate_definition_name("input port", &port.name)?;
            if !arguments.insert(port.name.as_str()) {
                return Err(collision_error(&descriptor.name, &port.name));
            }
            match port.cardinality {
                Cardinality::One => fixed = true,
                Cardinality::Variadic { min: 0 } => {
                    return Err(definition_error(format!(
                        "program `{}` has a variadic input with minimum zero",
                        descriptor.name
                    )));
                }
                Cardinality::Variadic { .. } if variadic => {
                    return Err(definition_error(format!(
                        "program `{}` has more than one variadic input port",
                        descriptor.name
                    )));
                }
                Cardinality::Variadic { .. } => variadic = true,
            }
        }
        if fixed && variadic {
            return Err(definition_error(format!(
                "program `{}` combines fixed and variadic input ports",
                descriptor.name
            )));
        }

        for parameter in &descriptor.parameters {
            validate_definition_name("parameter", &parameter.name)?;
            if !arguments.insert(parameter.name.as_str()) {
                return Err(collision_error(&descriptor.name, &parameter.name));
            }
        }
        if let ProgramImplementation::Body { contract, .. } = &definition.implementation {
            if descriptor
                .inputs
                .iter()
                .any(|port| matches!(port.cardinality, Cardinality::Variadic { .. }))
            {
                return Err(definition_error(format!(
                    "body program `{}` has a variadic input; body lexical bindings require fixed inputs",
                    descriptor.name
                )));
            }
            if matches!(
                contract.outputs,
                BodyOutputConstraint::Variadic { min: 0, .. }
            ) {
                return Err(definition_error(format!(
                    "body program `{}` has a variadic body output minimum of zero",
                    descriptor.name
                )));
            }
            if body_contract_uses_generic(contract) && !descriptor.is_generic() {
                return Err(definition_error(format!(
                    "body program `{}` uses generic body values without a generic descriptor",
                    descriptor.name
                )));
            }
        }
        validate_timeline_behavior(definition)?;
    }
    Ok(())
}

fn validate_timeline_behavior(definition: &ProgramDefinition) -> Result<()> {
    let descriptor = &definition.descriptor;
    if !matches!(definition.timeline_behavior, TimelineBehavior::Fresh)
        && descriptor.outputs.len() != 1
    {
        return Err(definition_error(format!(
            "program `{}` timeline behavior requires exactly one declared output",
            descriptor.name
        )));
    }

    match definition.timeline_behavior {
        TimelineBehavior::Fresh => Ok(()),
        TimelineBehavior::Identity { input }
        | TimelineBehavior::Repeat { input }
        | TimelineBehavior::Crop { input } => {
            let input = fixed_timeline_input(descriptor, input, "source")?;
            require_timeline_output_type(descriptor, input.value_type)
        }
        TimelineBehavior::Concat { input } => {
            let input = timeline_input(descriptor, input, "concat")?;
            require_timeline_output_type(descriptor, input.value_type)
        }
        TimelineBehavior::BodyConcat { inputs } => {
            let ProgramImplementation::Body { contract, .. } = &definition.implementation else {
                return Err(definition_error(format!(
                    "program `{}` uses body-concat timeline behavior without a body implementation",
                    descriptor.name
                )));
            };
            if inputs.len() != contract.initial_values.len() {
                return Err(definition_error(format!(
                    "program `{}` body-concat timeline behavior maps {} input(s), but its body contract declares {} initial value(s)",
                    descriptor.name,
                    inputs.len(),
                    contract.initial_values.len()
                )));
            }
            for (slot, expected) in inputs.iter().zip(&contract.initial_values) {
                let input = fixed_timeline_input(descriptor, *slot, "body")?;
                if input.value_type != *expected {
                    return Err(definition_error(format!(
                        "program `{}` body-concat input `{}` does not match its initial body value type",
                        descriptor.name, input.name
                    )));
                }
            }
            let output_type = homogeneous_body_output_type(contract).ok_or_else(|| {
                definition_error(format!(
                    "program `{}` body-concat behavior requires homogeneous nonempty body outputs",
                    descriptor.name
                ))
            })?;
            require_timeline_output_type(descriptor, output_type)
        }
        TimelineBehavior::Replace { base } => {
            let base = fixed_timeline_input(descriptor, base, "base")?;
            let ProgramImplementation::Body { contract, .. } = &definition.implementation else {
                return Err(definition_error(format!(
                    "program `{}` uses replacement timeline behavior without a body implementation",
                    descriptor.name
                )));
            };
            if contract.initial_values.as_slice() != [base.value_type]
                || !matches!(
                    &contract.outputs,
                    BodyOutputConstraint::Exactly(outputs)
                        if outputs.as_slice() == [base.value_type]
                )
            {
                return Err(definition_error(format!(
                    "program `{}` replacement timeline behavior requires one initial body value and one body output matching its base type",
                    descriptor.name
                )));
            }
            require_timeline_output_type(descriptor, base.value_type)
        }
        TimelineBehavior::FlashCut { before, after }
        | TimelineBehavior::Crossfade { before, after } => {
            let before = fixed_timeline_input(descriptor, before, "before")?;
            let after = fixed_timeline_input(descriptor, after, "after")?;
            let video = ValueTypeSpec::Exact(ValueType::Video);
            if before.value_type != video || after.value_type != video {
                return Err(definition_error(format!(
                    "program `{}` transition timeline behavior requires Video inputs",
                    descriptor.name
                )));
            }
            require_timeline_output_type(descriptor, video)
        }
    }
}

fn timeline_input<'a>(
    descriptor: &'a ProgramDescriptor,
    slot: InputSlot,
    role: &str,
) -> Result<&'a InputPort> {
    descriptor.inputs.get(slot.index()).ok_or_else(|| {
        definition_error(format!(
            "program `{}` timeline behavior maps missing {role} input slot {}",
            descriptor.name,
            slot.index()
        ))
    })
}

fn fixed_timeline_input<'a>(
    descriptor: &'a ProgramDescriptor,
    slot: InputSlot,
    role: &str,
) -> Result<&'a InputPort> {
    let input = timeline_input(descriptor, slot, role)?;
    if !matches!(input.cardinality, Cardinality::One) {
        return Err(definition_error(format!(
            "program `{}` timeline behavior requires `{}` to be a fixed input",
            descriptor.name, input.name
        )));
    }
    Ok(input)
}

fn require_timeline_output_type(
    descriptor: &ProgramDescriptor,
    expected: ValueTypeSpec,
) -> Result<()> {
    if descriptor.outputs.as_slice() == [expected] {
        Ok(())
    } else {
        Err(definition_error(format!(
            "program `{}` timeline output must match its mapped input type",
            descriptor.name
        )))
    }
}

fn homogeneous_body_output_type(contract: &BodyContract) -> Option<ValueTypeSpec> {
    match &contract.outputs {
        BodyOutputConstraint::Exactly(outputs) => {
            let first = *outputs.first()?;
            outputs
                .iter()
                .all(|output| *output == first)
                .then_some(first)
        }
        BodyOutputConstraint::Variadic { value_type, min } => (*min > 0).then_some(*value_type),
    }
}

fn body_contract_uses_generic(contract: &BodyContract) -> bool {
    contract
        .initial_values
        .iter()
        .any(|value| matches!(value, ValueTypeSpec::Generic))
        || match &contract.outputs {
            BodyOutputConstraint::Exactly(values) => values
                .iter()
                .any(|value| matches!(value, ValueTypeSpec::Generic)),
            BodyOutputConstraint::Variadic { value_type, .. } => {
                matches!(value_type, ValueTypeSpec::Generic)
            }
        }
}

fn collision_error(program: &str, name: &str) -> Diagnostic {
    definition_error(format!(
        "program `{program}` has duplicate or colliding argument name `{name}`"
    ))
}

fn validate_definition_name(role: &str, name: &str) -> Result<()> {
    if crate::source::is_valid_public_name(name) {
        Ok(())
    } else {
        Err(definition_error(format!(
            "{role} name `{name}` must match {}",
            crate::source::PUBLIC_NAME_GRAMMAR
        )))
    }
}

pub(crate) fn definition_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(
        "E_INVALID_PROGRAM_DEFINITION",
        message,
        SourceSpan::file_start("<program-registry>"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direct_stub(_call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<Vec<ValueRef>> {
        unreachable!("validation does not execute programs")
    }

    fn body_stub(_call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
        unreachable!("validation does not execute programs")
    }

    fn definition(
        name: &str,
        inputs: Vec<InputPort>,
        parameters: Vec<ParameterDescriptor>,
        implementation: ProgramImplementation,
    ) -> ProgramDefinition {
        ProgramDefinition {
            descriptor: ProgramDescriptor {
                name: name.to_owned(),
                semantic_version: 1,
                default_stack_access: StackAccess::Owned,
                inputs,
                parameters,
                outputs: vec![ValueType::Video.into()],
            },
            implementation,
            timeline_behavior: TimelineBehavior::Fresh,
        }
    }

    #[test]
    fn rejects_duplicate_program_names() {
        let definitions = vec![
            definition(
                "duplicate",
                vec![],
                vec![],
                ProgramImplementation::Direct(direct_stub),
            ),
            definition(
                "duplicate",
                vec![],
                vec![],
                ProgramImplementation::Direct(direct_stub),
            ),
        ];
        let error = ProgramRegistry::from_definitions(definitions).expect_err("duplicate");
        assert_eq!(error.code, "E_INVALID_PROGRAM_DEFINITION");
    }

    #[test]
    fn allows_ref_and_clip_program_names() {
        for name in ["ref", "clip"] {
            let definitions = vec![definition(
                name,
                vec![],
                vec![],
                ProgramImplementation::Direct(direct_stub),
            )];
            ProgramRegistry::from_definitions(definitions).expect("ordinary program name");
        }
    }

    #[test]
    fn rejects_mixed_fixed_and_variadic_inputs() {
        let ports = vec![
            InputPort {
                name: "head".to_owned(),
                value_type: ValueType::Video.into(),
                cardinality: Cardinality::One,
            },
            InputPort {
                name: "tail".to_owned(),
                value_type: ValueType::Video.into(),
                cardinality: Cardinality::Variadic { min: 1 },
            },
        ];
        let definitions = vec![definition(
            "mixed",
            ports,
            vec![],
            ProgramImplementation::Direct(direct_stub),
        )];
        ProgramRegistry::from_definitions(definitions).expect_err("mixed cardinalities");
    }

    #[test]
    fn rejects_generic_body_initial_values_without_descriptor_generic() {
        let mut definition = definition(
            "bad-body-generic",
            vec![],
            vec![],
            ProgramImplementation::Body {
                prepare: body_stub,
                contract: BodyContract {
                    initial_values: vec![ValueTypeSpec::Generic],
                    outputs: BodyOutputConstraint::Exactly(vec![ValueType::Video.into()]),
                    count_error_code: "E_TEST",
                },
            },
        );
        definition.descriptor.outputs = vec![ValueType::Video.into()];

        let error = ProgramRegistry::from_definitions(vec![definition])
            .expect_err("generic body contract requires a generic descriptor");
        assert_eq!(error.code, "E_INVALID_PROGRAM_DEFINITION");
    }

    #[test]
    fn rejects_generic_body_outputs_without_descriptor_generic() {
        let definition = definition(
            "bad-body-output",
            vec![],
            vec![],
            ProgramImplementation::Body {
                prepare: body_stub,
                contract: BodyContract {
                    initial_values: vec![],
                    outputs: BodyOutputConstraint::Exactly(vec![ValueTypeSpec::Generic]),
                    count_error_code: "E_TEST",
                },
            },
        );

        let error = ProgramRegistry::from_definitions(vec![definition])
            .expect_err("generic body output requires a generic descriptor");
        assert_eq!(error.code, "E_INVALID_PROGRAM_DEFINITION");
    }

    #[test]
    fn rejects_variadic_inputs_on_body_programs() {
        let definition = definition(
            "bad-variadic-body",
            vec![InputPort {
                name: "items".to_owned(),
                value_type: ValueType::Video.into(),
                cardinality: Cardinality::Variadic { min: 1 },
            }],
            vec![],
            ProgramImplementation::Body {
                prepare: body_stub,
                contract: BodyContract {
                    initial_values: vec![],
                    outputs: BodyOutputConstraint::Exactly(vec![ValueType::Video.into()]),
                    count_error_code: "E_TEST",
                },
            },
        );

        let error = ProgramRegistry::from_definitions(vec![definition])
            .expect_err("variadic body input has no lexical binding semantics");
        assert_eq!(error.code, "E_INVALID_PROGRAM_DEFINITION");
    }
    #[test]
    fn rejects_timeline_behavior_that_maps_a_missing_input() {
        let mut definition = definition(
            "bad-timeline-input",
            vec![],
            vec![],
            ProgramImplementation::Direct(direct_stub),
        );
        definition.timeline_behavior = TimelineBehavior::Identity {
            input: InputSlot::new(0),
        };

        let error = ProgramRegistry::from_definitions(vec![definition])
            .expect_err("timeline behavior input must exist");
        assert_eq!(error.code, "E_INVALID_PROGRAM_DEFINITION");
        assert!(error.message.contains("missing"));
    }

    #[test]
    fn rejects_replacement_behavior_without_its_exact_body_shape() {
        let mut definition = definition(
            "bad-replacement-body",
            vec![InputPort {
                name: "timeline".to_owned(),
                value_type: ValueType::Video.into(),
                cardinality: Cardinality::One,
            }],
            vec![],
            ProgramImplementation::Body {
                prepare: body_stub,
                contract: BodyContract {
                    initial_values: vec![],
                    outputs: BodyOutputConstraint::Exactly(vec![ValueType::Video.into()]),
                    count_error_code: "E_TEST",
                },
            },
        );
        definition.timeline_behavior = TimelineBehavior::Replace {
            base: InputSlot::new(0),
        };

        let error = ProgramRegistry::from_definitions(vec![definition])
            .expect_err("replacement body shape must match its timeline behavior");
        assert_eq!(error.code, "E_INVALID_PROGRAM_DEFINITION");
        assert!(error.message.contains("one initial body value"));
    }
    #[test]
    fn rejects_timeline_behavior_with_a_mismatched_output_type() {
        let mut definition = definition(
            "bad-timeline-output",
            vec![InputPort {
                name: "audio".to_owned(),
                value_type: ValueType::Audio.into(),
                cardinality: Cardinality::One,
            }],
            vec![],
            ProgramImplementation::Direct(direct_stub),
        );
        definition.timeline_behavior = TimelineBehavior::Identity {
            input: InputSlot::new(0),
        };

        let error = ProgramRegistry::from_definitions(vec![definition])
            .expect_err("timeline output type must match its source");
        assert_eq!(error.code, "E_INVALID_PROGRAM_DEFINITION");
        assert!(error.message.contains("output"));
    }
    #[test]
    fn rejects_transition_timeline_behavior_for_audio() {
        let mut definition = definition(
            "bad-audio-transition",
            vec![
                InputPort {
                    name: "before".to_owned(),
                    value_type: ValueType::Audio.into(),
                    cardinality: Cardinality::One,
                },
                InputPort {
                    name: "after".to_owned(),
                    value_type: ValueType::Audio.into(),
                    cardinality: Cardinality::One,
                },
            ],
            vec![],
            ProgramImplementation::Direct(direct_stub),
        );
        definition.descriptor.outputs = vec![ValueType::Audio.into()];
        definition.timeline_behavior = TimelineBehavior::FlashCut {
            before: InputSlot::new(0),
            after: InputSlot::new(1),
        };

        let error = ProgramRegistry::from_definitions(vec![definition])
            .expect_err("transition mapping is Video-only");
        assert_eq!(error.code, "E_INVALID_PROGRAM_DEFINITION");
        assert!(error.message.contains("Video"));
    }
}
