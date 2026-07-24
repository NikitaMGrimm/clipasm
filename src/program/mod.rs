mod builtins;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::diagnostic::{Diagnostic, Result, SourceSpan, Spanned};
use crate::model::{FrameCount, SourceTime, SourceTimeRange, ValueRef, ValueType};
use crate::semantic::{GraphBuilder, SourceOrigin};

pub(crate) use builtins::BUILTIN_PROGRAMS;

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
pub(crate) struct InputPort {
    pub(crate) name: &'static str,
    pub(crate) value_type: ValueType,
    pub(crate) cardinality: Cardinality,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ParameterType {
    Integer,
    File,
    Duration,
    TimeRange,
    Keyword(&'static [&'static str]),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParameterValue {
    Integer(i64),
    File(PathBuf),
    Duration(SourceTime),
    TimeRange(SourceTimeRange),
    Keyword(&'static str),
}

pub(crate) type BoundParameters = BTreeMap<&'static str, Spanned<ParameterValue>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ParameterDescriptor {
    pub(crate) name: &'static str,
    pub(crate) parameter_type: ParameterType,
    pub(crate) required: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProgramDescriptor {
    pub(crate) name: &'static str,
    pub(crate) semantic_version: u32,
    pub(crate) default_stack_access: StackAccess,
    pub(crate) inputs: &'static [InputPort],
    pub(crate) parameters: &'static [ParameterDescriptor],
    pub(crate) primary_parameter: Option<&'static str>,
    pub(crate) output: ValueType,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PostfixSyntax {
    pub(crate) parameter: &'static str,
}

pub(crate) type DirectLowerFn =
    for<'graph> fn(&ResolvedCall, &mut GraphBuilder<'graph>) -> Result<ValueRef>;
pub(crate) type BodyPrepareFn =
    for<'graph> fn(&ResolvedCall, &mut GraphBuilder<'graph>) -> Result<BodyPlan>;

#[derive(Clone, Copy)]
pub(crate) enum ProgramImplementation {
    Direct(DirectLowerFn),
    Body(BodyPrepareFn),
}

impl std::fmt::Debug for ProgramImplementation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Direct(_) => "Direct",
            Self::Body(_) => "Body",
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProgramDefinition {
    pub(crate) descriptor: ProgramDescriptor,
    pub(crate) implementation: ProgramImplementation,
    pub(crate) postfix: Option<PostfixSyntax>,
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
    ) -> Result<ValueRef>;
}

#[derive(Debug)]
pub(crate) struct ResolvedCall {
    definition: &'static ProgramDefinition,
    inputs: BTreeMap<&'static str, Vec<ValueRef>>,
    parameters: BoundParameters,
    requested_frames: Option<FrameCount>,
    origin: SourceOrigin,
}

impl ResolvedCall {
    pub(crate) fn new(
        definition: &'static ProgramDefinition,
        inputs: BTreeMap<&'static str, Vec<ValueRef>>,
        parameters: BoundParameters,
        requested_frames: Option<FrameCount>,
        origin: SourceOrigin,
    ) -> Self {
        Self {
            definition,
            inputs,
            parameters,
            requested_frames,
            origin,
        }
    }

    #[must_use]
    pub(crate) const fn definition(&self) -> &'static ProgramDefinition {
        self.definition
    }

    #[must_use]
    pub(crate) const fn requested_frames(&self) -> Option<FrameCount> {
        self.requested_frames
    }

    #[must_use]
    pub(crate) const fn origin(&self) -> &SourceOrigin {
        &self.origin
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
    ) -> Result<Option<(&'static str, &SourceSpan)>> {
        let Some(parameter) = self.parameters.get(name) else {
            return Ok(None);
        };
        match &parameter.value {
            ParameterValue::Keyword(value) => Ok(Some((*value, &parameter.span))),
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProgramRegistry {
    definitions: &'static [ProgramDefinition],
}

impl Default for ProgramRegistry {
    fn default() -> Self {
        Self::from_definitions(BUILTIN_PROGRAMS).expect("built-in program definitions are valid")
    }
}

impl ProgramRegistry {
    pub(crate) fn from_definitions(definitions: &'static [ProgramDefinition]) -> Result<Self> {
        validate_definitions(definitions)?;
        Ok(Self { definitions })
    }

    #[must_use]
    pub(crate) fn get(self, name: &str) -> Option<&'static ProgramDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.descriptor.name == name)
    }

    #[must_use]
    pub(crate) const fn definitions(self) -> &'static [ProgramDefinition] {
        self.definitions
    }
}

fn validate_definitions(definitions: &[ProgramDefinition]) -> Result<()> {
    let mut programs = BTreeSet::new();
    for definition in definitions {
        let descriptor = &definition.descriptor;
        validate_definition_name("program", descriptor.name)?;
        if !programs.insert(descriptor.name) {
            return Err(definition_error(format!(
                "duplicate program name `{}`",
                descriptor.name
            )));
        }

        let mut arguments = BTreeSet::new();
        let mut fixed = false;
        let mut variadic = false;
        for port in descriptor.inputs {
            validate_definition_name("input port", port.name)?;
            if !arguments.insert(port.name) {
                return Err(collision_error(descriptor.name, port.name));
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

        for parameter in descriptor.parameters {
            validate_definition_name("parameter", parameter.name)?;
            if !arguments.insert(parameter.name) {
                return Err(collision_error(descriptor.name, parameter.name));
            }
        }
        if let Some(primary) = descriptor.primary_parameter
            && !descriptor
                .parameters
                .iter()
                .any(|parameter| parameter.name == primary)
        {
            return Err(definition_error(format!(
                "program `{}` names nonexistent primary parameter `{primary}`",
                descriptor.name
            )));
        }

        match (definition.implementation, definition.postfix) {
            (ProgramImplementation::Direct(_), Some(_)) => {
                return Err(definition_error(format!(
                    "direct program `{}` cannot declare postfix syntax",
                    descriptor.name
                )));
            }
            (ProgramImplementation::Body(_), Some(postfix)) => {
                let Some(_) = descriptor
                    .parameters
                    .iter()
                    .find(|parameter| parameter.name == postfix.parameter)
                else {
                    return Err(definition_error(format!(
                        "program `{}` names nonexistent postfix parameter `{}`",
                        descriptor.name, postfix.parameter
                    )));
                };
            }
            _ => {}
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
    let mut characters = name.chars();
    let valid_start = characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_');
    let valid_rest = characters
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'));
    if valid_start && valid_rest {
        Ok(())
    } else {
        Err(definition_error(format!(
            "{role} name `{name}` must match [A-Za-z_][A-Za-z0-9_-]*"
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

    fn direct_stub(_call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<ValueRef> {
        unreachable!("validation does not execute programs")
    }

    fn body_stub(_call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
        unreachable!("validation does not execute programs")
    }

    fn definition(
        name: &'static str,
        inputs: &'static [InputPort],
        parameters: &'static [ParameterDescriptor],
        primary_parameter: Option<&'static str>,
        implementation: ProgramImplementation,
        postfix: Option<PostfixSyntax>,
    ) -> ProgramDefinition {
        ProgramDefinition {
            descriptor: ProgramDescriptor {
                name,
                semantic_version: 1,
                default_stack_access: StackAccess::Owned,
                inputs,
                parameters,
                primary_parameter,
                output: ValueType::Video,
            },
            implementation,
            postfix,
        }
    }

    #[test]
    fn rejects_duplicate_program_names() {
        let definitions = Box::leak(
            vec![
                definition(
                    "duplicate",
                    &[],
                    &[],
                    None,
                    ProgramImplementation::Direct(direct_stub),
                    None,
                ),
                definition(
                    "duplicate",
                    &[],
                    &[],
                    None,
                    ProgramImplementation::Direct(direct_stub),
                    None,
                ),
            ]
            .into_boxed_slice(),
        );
        let error = ProgramRegistry::from_definitions(definitions).expect_err("duplicate");
        assert_eq!(error.code, "E_INVALID_PROGRAM_DEFINITION");
    }

    #[test]
    fn allows_ref_and_clip_program_names() {
        for name in ["ref", "clip"] {
            let definitions = Box::leak(
                vec![definition(
                    name,
                    &[],
                    &[],
                    None,
                    ProgramImplementation::Direct(direct_stub),
                    None,
                )]
                .into_boxed_slice(),
            );
            ProgramRegistry::from_definitions(definitions).expect("ordinary program name");
        }
    }

    #[test]
    fn rejects_mixed_fixed_and_variadic_inputs() {
        let ports = Box::leak(
            vec![
                InputPort {
                    name: "head",
                    value_type: ValueType::Video,
                    cardinality: Cardinality::One,
                },
                InputPort {
                    name: "tail",
                    value_type: ValueType::Video,
                    cardinality: Cardinality::Variadic { min: 1 },
                },
            ]
            .into_boxed_slice(),
        );
        let definitions = Box::leak(
            vec![definition(
                "mixed",
                ports,
                &[],
                None,
                ProgramImplementation::Direct(direct_stub),
                None,
            )]
            .into_boxed_slice(),
        );
        ProgramRegistry::from_definitions(definitions).expect_err("mixed cardinalities");
    }

    #[test]
    fn validates_primary_and_postfix_targets() {
        let definitions = Box::leak(
            vec![definition(
                "missing_primary",
                &[],
                &[],
                Some("value"),
                ProgramImplementation::Direct(direct_stub),
                None,
            )]
            .into_boxed_slice(),
        );
        ProgramRegistry::from_definitions(definitions).expect_err("missing primary target");

        let definitions = Box::leak(
            vec![definition(
                "bad_postfix",
                &[],
                &[],
                None,
                ProgramImplementation::Body(body_stub),
                Some(PostfixSyntax { parameter: "range" }),
            )]
            .into_boxed_slice(),
        );
        ProgramRegistry::from_definitions(definitions).expect_err("missing postfix target");
    }
}
