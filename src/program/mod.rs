mod builtins;
mod call;
mod external;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub(crate) use crate::catalog::{
    Cardinality, InputPort, InputSlot, ParameterDescriptor, ParameterSlot, ParameterType,
    ProgramDescriptor, ResolvedSignature, StackAccess, ValueTypeSpec,
};
use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{
    DurationValue, ExactNumber, FrameCount, FrameRange, NativeRange, SampleRange, SourceTimeRange,
    TimelineRangeExpression, TimelineViewId, ValueRef, ValueType,
};
use crate::semantic::GraphBuilder;
use crate::source::{SourceSpan, SourceUnitId};

pub(crate) use builtins::{
    BuiltinBodyInitialValue, BuiltinCategory, BuiltinDefault, BuiltinProgram, builtin_catalog,
    builtin_programs,
};
pub(crate) use call::{ResolvedCall, ResolvedInput};
pub(crate) use external::ExternalRuntime;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParameterValue {
    Number(ExactNumber),
    Integer(i64),
    File(PathBuf),
    Duration(DurationValue),
    TimeRange(TimeRangeValue),
    Keyword(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TimeRangeValue {
    WallClock(SourceTimeRange),
    ProjectFrames(FrameRange),
    Marker {
        owner: TimelineViewId,
        range: Box<TimelineRangeExpression>,
    },
}

impl TimeRangeValue {
    pub(crate) fn to_native_range(
        &self,
        value_type: ValueType,
        video: crate::model::VideoSpec,
        audio: crate::model::AudioSpec,
        span: &SourceSpan,
    ) -> Result<NativeTimeRange> {
        match value_type {
            ValueType::Video => self.to_frame_range(video.fps(), span),
            ValueType::Audio => self.to_sample_range(video, audio, span),
        }
    }

    fn to_frame_range(
        &self,
        fps: crate::model::FrameRate,
        span: &SourceSpan,
    ) -> Result<NativeTimeRange> {
        match self {
            Self::WallClock(range) => range
                .to_frames(fps, span)
                .map(|range| NativeTimeRange::Concrete(NativeRange::Frames(range))),
            Self::ProjectFrames(range) => {
                Ok(NativeTimeRange::Concrete(NativeRange::Frames(*range)))
            }
            Self::Marker { range, .. } => {
                if !range.start.terms().is_empty() || !range.end.terms().is_empty() {
                    return Ok(NativeTimeRange::Deferred(range.clone()));
                }
                let start = range.start.resolve_frame_boundary(
                    fps,
                    |_| unreachable!("constant range has no terms"),
                    span,
                )?;
                let end = range.end.resolve_frame_boundary(
                    fps,
                    |_| unreachable!("constant range has no terms"),
                    span,
                )?;
                FrameRange::new(start, end)
                    .map(|range| NativeTimeRange::Concrete(NativeRange::Frames(range)))
                    .ok_or_else(|| invalid_timeline_range(span))
            }
        }
    }

    fn to_sample_range(
        &self,
        video: crate::model::VideoSpec,
        audio: crate::model::AudioSpec,
        span: &SourceSpan,
    ) -> Result<NativeTimeRange> {
        match self {
            Self::WallClock(range) => range
                .to_samples(audio.sample_rate(), span)
                .map(|range| NativeTimeRange::Concrete(NativeRange::Samples(range))),
            Self::ProjectFrames(range) => {
                let timeline = crate::model::TimelineRate::new(video, audio);
                let start = timeline.sample_boundary(range.start(), span)?;
                let end = timeline.sample_boundary(range.end(), span)?;
                SampleRange::new(start, end)
                    .map(|range| NativeTimeRange::Concrete(NativeRange::Samples(range)))
                    .ok_or_else(|| invalid_timeline_range(span))
            }
            Self::Marker { range, .. } => {
                if !range.start.terms().is_empty() || !range.end.terms().is_empty() {
                    return Ok(NativeTimeRange::Deferred(range.clone()));
                }
                let start = range.start.resolve_sample_boundary(
                    video,
                    audio,
                    |_| unreachable!("constant range has no terms"),
                    span,
                )?;
                let end = range.end.resolve_sample_boundary(
                    video,
                    audio,
                    |_| unreachable!("constant range has no terms"),
                    span,
                )?;
                SampleRange::new(start, end)
                    .map(|range| NativeTimeRange::Concrete(NativeRange::Samples(range)))
                    .ok_or_else(|| invalid_timeline_range(span))
            }
        }
    }

    pub(crate) const fn marker_owner(&self) -> Option<TimelineViewId> {
        match self {
            Self::WallClock(_) | Self::ProjectFrames(_) => None,
            Self::Marker { owner, .. } => Some(*owner),
        }
    }
}

fn invalid_timeline_range(span: &SourceSpan) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::InvalidTimeRange,
        "timeline-range start must be earlier than its end",
        span.clone(),
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NativeTimeRange {
    Concrete(NativeRange),
    Deferred(Box<TimelineRangeExpression>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RequestedVideoExtent {
    Concrete(FrameCount),
    Deferred(crate::model::TimelineExpression),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BodyContract {
    pub(crate) initial_values: Vec<ValueTypeSpec>,
    pub(crate) outputs: BodyOutputConstraint,
    pub(crate) count_diagnostic: BodyCountDiagnostic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BodyCountDiagnostic {
    Builtin(BuiltinDiagnostic),
    #[cfg(test)]
    Custom(&'static str),
}

impl BodyCountDiagnostic {
    pub(crate) fn build(self, message: impl Into<String>, span: SourceSpan) -> Diagnostic {
        match self {
            Self::Builtin(diagnostic) => Diagnostic::builtin(diagnostic, message, span),
            #[cfg(test)]
            Self::Custom(code) => custom_body_count_diagnostic(code, message, span),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "test program definitions intentionally exercise application-defined diagnostic codes"
)]
fn custom_body_count_diagnostic(
    code: &'static str,
    message: impl Into<String>,
    span: SourceSpan,
) -> Diagnostic {
    Diagnostic::new(code, message, span)
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
    External(ExternalRuntime),
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
    Diagnostic::builtin(
        BuiltinDiagnostic::InvalidProgramDefinition,
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
                    count_diagnostic: BodyCountDiagnostic::Custom("E_TEST"),
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
                    count_diagnostic: BodyCountDiagnostic::Custom("E_TEST"),
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
                    count_diagnostic: BodyCountDiagnostic::Custom("E_TEST"),
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
                    count_diagnostic: BodyCountDiagnostic::Custom("E_TEST"),
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
