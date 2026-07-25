mod builtins;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, SourceTime, SourceTimeRange, ValueRef, ValueType};
use crate::semantic::{GraphBuilder, SourceOrigin};
use crate::source::{SourceSpan, SourceUnitId, Spanned};

pub(crate) use builtins::builtin_programs;

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
pub(crate) struct ResolvedInputPort {
    pub(crate) name: String,
    pub(crate) value_type: ValueType,
    pub(crate) cardinality: Cardinality,
    pub(crate) allow_adaptation: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedSignature {
    pub(crate) generic: Option<ValueType>,
    pub(crate) inputs: Vec<ResolvedInputPort>,
    pub(crate) outputs: Vec<ValueType>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParameterType {
    Integer,
    File,
    Duration,
    TimeRange,
    Keyword(Vec<String>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParameterValue {
    Integer(i64),
    File(PathBuf),
    Duration(SourceTime),
    TimeRange(SourceTimeRange),
    Keyword(String),
}

pub(crate) type BoundParameters = BTreeMap<String, Spanned<ParameterValue>>;

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
    pub(crate) type_selector: Option<String>,
    pub(crate) outputs: Vec<ValueTypeSpec>,
}

impl ProgramDescriptor {
    pub(crate) fn resolve_signature(&self, generic: Option<ValueType>) -> ResolvedSignature {
        let resolve = |spec: ValueTypeSpec| match spec {
            ValueTypeSpec::Exact(value_type) => value_type,
            ValueTypeSpec::Generic => generic.expect("generic descriptor has a resolved type"),
        };
        ResolvedSignature {
            generic,
            inputs: self
                .inputs
                .iter()
                .map(|port| ResolvedInputPort {
                    name: port.name.clone(),
                    value_type: resolve(port.value_type),
                    cardinality: port.cardinality,
                    allow_adaptation: matches!(port.value_type, ValueTypeSpec::Exact(_)),
                })
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

impl BodyContract {
    pub(crate) fn resolve(&self, generic: Option<ValueType>) -> ResolvedBodyContract {
        let resolve = |spec: ValueTypeSpec| match spec {
            ValueTypeSpec::Exact(value_type) => value_type,
            ValueTypeSpec::Generic => generic.expect("generic body contract has a resolved type"),
        };
        ResolvedBodyContract {
            initial_values: self.initial_values.iter().copied().map(resolve).collect(),
            outputs: match &self.outputs {
                BodyOutputConstraint::Exactly(outputs) => ResolvedBodyOutputConstraint::Exactly(
                    outputs.iter().copied().map(resolve).collect(),
                ),
                BodyOutputConstraint::Variadic { value_type, min } => {
                    ResolvedBodyOutputConstraint::Variadic {
                        value_type: resolve(*value_type),
                        min: *min,
                    }
                }
            },
        }
    }

    #[must_use]
    pub(crate) fn exact_initial_values(&self) -> Option<Vec<ValueType>> {
        self.initial_values
            .iter()
            .copied()
            .map(ValueTypeSpec::exact)
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BodyOutputConstraint {
    Exactly(Vec<ValueTypeSpec>),
    Variadic {
        value_type: ValueTypeSpec,
        min: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedBodyContract {
    pub(crate) initial_values: Vec<ValueType>,
    pub(crate) outputs: ResolvedBodyOutputConstraint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResolvedBodyOutputConstraint {
    Exactly(Vec<ValueType>),
    Variadic { value_type: ValueType, min: usize },
}

pub(crate) type ProgramOutputs = Vec<ValueRef>;
pub(crate) type DirectLowerFn =
    for<'graph> fn(&ResolvedCall, &mut GraphBuilder<'graph>) -> Result<ProgramOutputs>;
pub(crate) type BodyPrepareFn =
    for<'graph> fn(&ResolvedCall, &mut GraphBuilder<'graph>) -> Result<BodyPlan>;

#[derive(Clone)]
pub(crate) enum ProgramImplementation {
    Direct(DirectLowerFn),
    Body {
        prepare: BodyPrepareFn,
        contract: BodyContract,
    },
    Authored(SourceUnitId),
    External(crate::external::ExternalRuntime),
}

impl std::fmt::Debug for ProgramImplementation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Direct(_) => "Direct",
            Self::Body { .. } => "Body",
            Self::Authored(_) => "Authored",
            Self::External(_) => "External",
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ProgramDefinition {
    pub(crate) descriptor: ProgramDescriptor,
    pub(crate) implementation: ProgramImplementation,
}

impl ProgramDefinition {
    #[must_use]
    pub(crate) const fn is_body(&self) -> bool {
        matches!(self.implementation, ProgramImplementation::Body { .. })
    }

    #[must_use]
    pub(crate) const fn body_contract(&self) -> Option<&BodyContract> {
        match &self.implementation {
            ProgramImplementation::Body { contract, .. } => Some(contract),
            ProgramImplementation::Direct(_)
            | ProgramImplementation::Authored(_)
            | ProgramImplementation::External(_) => None,
        }
    }
}

pub(crate) struct BodyPlan {
    pub(crate) initial_values: Vec<ValueRef>,
    pub(crate) requested_frames: Option<FrameCount>,
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
pub(crate) struct ResolvedCall {
    program_name: String,
    inputs: BTreeMap<String, Vec<ValueRef>>,
    parameters: BoundParameters,
    requested_frames: Option<FrameCount>,
    origin: SourceOrigin,
}

impl ResolvedCall {
    pub(crate) fn new(
        program_name: String,
        inputs: BTreeMap<String, Vec<ValueRef>>,
        parameters: BoundParameters,
        requested_frames: Option<FrameCount>,
        origin: SourceOrigin,
    ) -> Self {
        Self {
            program_name,
            inputs,
            parameters,
            requested_frames,
            origin,
        }
    }

    #[must_use]
    pub(crate) fn program_name(&self) -> &str {
        &self.program_name
    }

    #[must_use]
    pub(crate) const fn requested_frames(&self) -> Option<FrameCount> {
        self.requested_frames
    }

    #[must_use]
    pub(crate) const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }

    #[must_use]
    pub(crate) fn inputs(&self) -> &BTreeMap<String, Vec<ValueRef>> {
        &self.inputs
    }

    #[must_use]
    pub(crate) fn parameters(&self) -> &BoundParameters {
        &self.parameters
    }

    pub(crate) fn one_input(&self, name: &str) -> Result<ValueRef> {
        self.inputs
            .get(name)
            .and_then(|values| match values.as_slice() {
                [value] => Some(*value),
                _ => None,
            })
            .ok_or_else(|| self.binding_error(name))
    }

    pub(crate) fn variadic_input(&self, name: &str) -> Result<&[ValueRef]> {
        self.inputs
            .get(name)
            .map(Vec::as_slice)
            .ok_or_else(|| self.binding_error(name))
    }

    pub(crate) fn integer_parameter(&self, name: &str) -> Result<(i64, &SourceSpan)> {
        let parameter = self.parameter(name)?;
        match &parameter.value {
            ParameterValue::Integer(value) => Ok((*value, &parameter.span)),
            _ => Err(self.parameter_type_error(name, "integer")),
        }
    }

    pub(crate) fn optional_integer_parameter(
        &self,
        name: &str,
    ) -> Result<Option<(i64, &SourceSpan)>> {
        let Some(parameter) = self.parameters.get(name) else {
            return Ok(None);
        };
        match &parameter.value {
            ParameterValue::Integer(value) => Ok(Some((*value, &parameter.span))),
            _ => Err(self.parameter_type_error(name, "integer")),
        }
    }

    pub(crate) fn file_parameter(&self, name: &str) -> Result<(&Path, &SourceSpan)> {
        let parameter = self.parameter(name)?;
        match &parameter.value {
            ParameterValue::File(value) => Ok((value.as_path(), &parameter.span)),
            _ => Err(self.parameter_type_error(name, "file")),
        }
    }

    pub(crate) fn optional_duration_parameter(
        &self,
        name: &str,
    ) -> Result<Option<(SourceTime, &SourceSpan)>> {
        let Some(parameter) = self.parameters.get(name) else {
            return Ok(None);
        };
        match &parameter.value {
            ParameterValue::Duration(value) => Ok(Some((*value, &parameter.span))),
            _ => Err(self.parameter_type_error(name, "duration")),
        }
    }

    pub(crate) fn time_range_parameter(
        &self,
        name: &str,
    ) -> Result<(SourceTimeRange, &SourceSpan)> {
        let parameter = self.parameter(name)?;
        match &parameter.value {
            ParameterValue::TimeRange(value) => Ok((*value, &parameter.span)),
            _ => Err(self.parameter_type_error(name, "time range")),
        }
    }

    pub(crate) fn optional_keyword_parameter(
        &self,
        name: &str,
    ) -> Result<Option<(&str, &SourceSpan)>> {
        let Some(parameter) = self.parameters.get(name) else {
            return Ok(None);
        };
        match &parameter.value {
            ParameterValue::Keyword(value) => Ok(Some((value, &parameter.span))),
            _ => Err(self.parameter_type_error(name, "keyword")),
        }
    }

    fn parameter(&self, name: &str) -> Result<&Spanned<ParameterValue>> {
        self.parameters
            .get(name)
            .ok_or_else(|| self.binding_error(name))
    }

    fn binding_error(&self, name: &str) -> Diagnostic {
        Diagnostic::new(
            "E_INTERNAL_BINDING",
            format!("resolved call has an invalid or missing binding for `{name}`"),
            self.origin.span.clone(),
        )
    }

    fn parameter_type_error(&self, name: &str, expected: &str) -> Diagnostic {
        Diagnostic::new(
            "E_INTERNAL_BINDING",
            format!("resolved parameter `{name}` is not a {expected}"),
            self.origin.span.clone(),
        )
    }
}

#[derive(Debug)]
struct ProgramCatalogData {
    definitions: Vec<ProgramDefinition>,
    names: BTreeMap<String, ProgramId>,
    source_programs: BTreeMap<SourceUnitId, ProgramId>,
}

#[derive(Clone, Debug)]
pub(crate) struct ProgramRegistry {
    data: Arc<ProgramCatalogData>,
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
            data: Arc::new(ProgramCatalogData {
                definitions,
                names,
                source_programs: BTreeMap::new(),
            }),
        })
    }

    pub(crate) fn from_linked(
        definitions: Vec<ProgramDefinition>,
        builtin_count: usize,
        source_programs: BTreeMap<SourceUnitId, ProgramId>,
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
            data: Arc::new(ProgramCatalogData {
                definitions,
                names,
                source_programs,
            }),
        })
    }

    #[must_use]
    pub(crate) fn id(&self, name: &str) -> Option<ProgramId> {
        self.data.names.get(name).copied()
    }

    #[must_use]
    pub(crate) fn get(&self, name: &str) -> Option<&ProgramDefinition> {
        self.id(name).map(|id| self.definition(id))
    }

    #[must_use]
    pub(crate) fn definition(&self, id: ProgramId) -> &ProgramDefinition {
        &self.data.definitions[id.index()]
    }

    #[must_use]
    pub(crate) fn definitions(&self) -> &[ProgramDefinition] {
        &self.data.definitions
    }

    #[must_use]
    pub(crate) fn source_program(&self, unit: SourceUnitId) -> Option<&ProgramDefinition> {
        self.data
            .source_programs
            .get(&unit)
            .map(|id| self.definition(*id))
    }
}

#[allow(clippy::too_many_lines)]
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
        let has_generic = descriptor
            .inputs
            .iter()
            .any(|port| matches!(port.value_type, ValueTypeSpec::Generic))
            || descriptor
                .outputs
                .iter()
                .any(|output| matches!(output, ValueTypeSpec::Generic));
        match &descriptor.type_selector {
            Some(type_selector) => {
                if !has_generic {
                    return Err(definition_error(format!(
                        "program `{}` declares a type parameter without generic inputs or outputs",
                        descriptor.name
                    )));
                }
                let Some(selector) = descriptor
                    .parameters
                    .iter()
                    .find(|parameter| parameter.name == *type_selector)
                else {
                    return Err(definition_error(format!(
                        "program `{}` names nonexistent type selector `{}`",
                        descriptor.name, type_selector
                    )));
                };
                if selector.required
                    || !matches!(
                        &selector.parameter_type,
                        ParameterType::Keyword(values)
                            if values == &["Video".to_owned(), "Audio".to_owned()]
                    )
                {
                    return Err(definition_error(format!(
                        "program `{}` type selector `{}` must be an optional Video/Audio Keyword",
                        descriptor.name, type_selector
                    )));
                }
            }
            None if has_generic => {
                return Err(definition_error(format!(
                    "program `{}` uses generic value types without a type parameter",
                    descriptor.name
                )));
            }
            None => {}
        }

        if let ProgramImplementation::Body { contract, .. } = &definition.implementation
            && matches!(
                contract.outputs,
                BodyOutputConstraint::Variadic { min: 0, .. }
            )
        {
            return Err(definition_error(format!(
                "body program `{}` has a variadic body output minimum of zero",
                descriptor.name
            )));
        }
    }
    Ok(())
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

    fn definition(
        name: &str,
        inputs: Vec<InputPort>,
        parameters: Vec<ParameterDescriptor>,
        _primary_parameter: Option<&str>,
        implementation: ProgramImplementation,
        _postfix: Option<()>,
    ) -> ProgramDefinition {
        ProgramDefinition {
            descriptor: ProgramDescriptor {
                name: name.to_owned(),
                semantic_version: 1,
                default_stack_access: StackAccess::Owned,
                inputs,
                parameters,
                type_selector: None,
                outputs: vec![ValueType::Video.into()],
            },
            implementation,
        }
    }

    #[test]
    fn rejects_duplicate_program_names() {
        let definitions = vec![
            definition(
                "duplicate",
                vec![],
                vec![],
                None,
                ProgramImplementation::Direct(direct_stub),
                None,
            ),
            definition(
                "duplicate",
                vec![],
                vec![],
                None,
                ProgramImplementation::Direct(direct_stub),
                None,
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
                None,
                ProgramImplementation::Direct(direct_stub),
                None,
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
            None,
            ProgramImplementation::Direct(direct_stub),
            None,
        )];
        ProgramRegistry::from_definitions(definitions).expect_err("mixed cardinalities");
    }
}
