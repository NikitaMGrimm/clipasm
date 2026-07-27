use std::path::Path;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{ExactNumber, SourceTime, ValueRef};
use crate::semantic::SourceOrigin;
use crate::source::{SourceSpan, Spanned};

use super::{
    Cardinality, InputPort, InputSlot, ParameterDescriptor, ParameterSlot, ParameterType,
    ParameterValue, ProgramDescriptor, RequestedVideoExtent, ResolvedSignature, TimeRangeValue,
};

#[derive(Clone, Debug)]
pub(crate) enum ResolvedInput {
    One(ValueRef),
    Variadic(Vec<ValueRef>),
}

impl ResolvedInput {
    #[must_use]
    pub(crate) fn values(&self) -> &[ValueRef] {
        match self {
            Self::One(value) => std::slice::from_ref(value),
            Self::Variadic(values) => values,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ResolvedCall<'a> {
    descriptor: &'a ProgramDescriptor,
    inputs: Vec<ResolvedInput>,
    parameters: Vec<Option<Spanned<ParameterValue>>>,
    requested_extent: Option<RequestedVideoExtent>,
    origin: SourceOrigin,
}

impl<'a> ResolvedCall<'a> {
    pub(crate) fn new(
        descriptor: &'a ProgramDescriptor,
        signature: &ResolvedSignature,
        inputs: Vec<ResolvedInput>,
        parameters: Vec<Option<Spanned<ParameterValue>>>,
        requested_extent: Option<RequestedVideoExtent>,
        origin: SourceOrigin,
    ) -> Result<Self> {
        if inputs.len() != descriptor.inputs.len() || inputs.len() != signature.inputs.len() {
            return Err(binding_error(
                &origin,
                "resolved call input slots do not match its descriptor",
            ));
        }
        if parameters.len() != descriptor.parameters.len() {
            return Err(binding_error(
                &origin,
                "resolved call parameter slots do not match its descriptor",
            ));
        }

        for (index, ((port, expected_type), input)) in descriptor
            .inputs
            .iter()
            .zip(&signature.inputs)
            .zip(&inputs)
            .enumerate()
        {
            let cardinality_valid = match (port.cardinality, input) {
                (Cardinality::One, ResolvedInput::One(_)) => true,
                (Cardinality::Variadic { min }, ResolvedInput::Variadic(values)) => {
                    values.len() >= min
                }
                _ => false,
            };
            if !cardinality_valid {
                return Err(binding_error(
                    &origin,
                    &format!(
                        "resolved input `{}` has invalid cardinality at slot {index}",
                        port.name
                    ),
                ));
            }
            if input
                .values()
                .iter()
                .any(|value| value.value_type() != *expected_type)
            {
                return Err(binding_error(
                    &origin,
                    &format!(
                        "resolved input `{}` contains a value outside its resolved type",
                        port.name
                    ),
                ));
            }
        }

        for (descriptor, parameter) in descriptor.parameters.iter().zip(&parameters) {
            if let Some(parameter) = parameter
                && !parameter_matches(&descriptor.parameter_type, &parameter.value)
            {
                return Err(binding_error(
                    &origin,
                    &format!(
                        "resolved parameter `{}` has a value outside its descriptor type",
                        descriptor.name
                    ),
                ));
            }
        }

        Ok(Self {
            descriptor,
            inputs,
            parameters,
            requested_extent,
            origin,
        })
    }

    #[must_use]
    pub(crate) fn program_name(&self) -> &str {
        &self.descriptor.name
    }

    #[must_use]
    pub(crate) const fn requested_extent(&self) -> Option<&RequestedVideoExtent> {
        self.requested_extent.as_ref()
    }

    #[must_use]
    pub(crate) const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }

    #[must_use]
    pub(crate) fn input_at(&self, slot: InputSlot) -> &ResolvedInput {
        &self.inputs[slot.index()]
    }

    #[must_use]
    pub(crate) fn input_binding(&self, slot: InputSlot) -> (&InputPort, &ResolvedInput) {
        (self.descriptor.input(slot), &self.inputs[slot.index()])
    }

    #[must_use]
    pub(crate) fn parameter_at(&self, slot: ParameterSlot) -> Option<&Spanned<ParameterValue>> {
        self.parameters[slot.index()].as_ref()
    }

    pub(crate) fn inputs(&self) -> impl ExactSizeIterator<Item = (&InputPort, &ResolvedInput)> {
        self.descriptor.inputs.iter().zip(&self.inputs)
    }

    pub(crate) fn parameters(
        &self,
    ) -> impl ExactSizeIterator<Item = (&ParameterDescriptor, Option<&Spanned<ParameterValue>>)>
    {
        self.descriptor
            .parameters
            .iter()
            .zip(self.parameters.iter().map(Option::as_ref))
    }

    pub(crate) fn one_input(&self, name: &str) -> Result<ValueRef> {
        let slot = self
            .descriptor
            .input_slot(name)
            .ok_or_else(|| self.invalid_binding(name))?;
        match self.input_at(slot) {
            ResolvedInput::One(value) => Ok(*value),
            ResolvedInput::Variadic(_) => Err(self.invalid_binding(name)),
        }
    }

    pub(crate) fn variadic_input(&self, name: &str) -> Result<&[ValueRef]> {
        let slot = self
            .descriptor
            .input_slot(name)
            .ok_or_else(|| self.invalid_binding(name))?;
        match self.input_at(slot) {
            ResolvedInput::One(_) => Err(self.invalid_binding(name)),
            ResolvedInput::Variadic(values) => Ok(values),
        }
    }

    pub(crate) fn integer_parameter(&self, name: &str) -> Result<(i64, &SourceSpan)> {
        let parameter = self.parameter(name)?;
        match &parameter.value {
            ParameterValue::Integer(value) => Ok((*value, &parameter.span)),
            _ => Err(self.parameter_type_error(name, "integer")),
        }
    }

    pub(crate) fn optional_number_parameter(
        &self,
        name: &str,
    ) -> Result<Option<(&ExactNumber, &SourceSpan)>> {
        let Some(parameter) = self.optional_parameter(name)? else {
            return Ok(None);
        };
        match &parameter.value {
            ParameterValue::Number(value) => Ok(Some((value, &parameter.span))),
            _ => Err(self.parameter_type_error(name, "number")),
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
        let Some(parameter) = self.optional_parameter(name)? else {
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
    ) -> Result<(&TimeRangeValue, &SourceSpan)> {
        let parameter = self.parameter(name)?;
        match &parameter.value {
            ParameterValue::TimeRange(value) => Ok((value, &parameter.span)),
            _ => Err(self.parameter_type_error(name, "time range")),
        }
    }

    pub(crate) fn optional_keyword_parameter(
        &self,
        name: &str,
    ) -> Result<Option<(&str, &SourceSpan)>> {
        let Some(parameter) = self.optional_parameter(name)? else {
            return Ok(None);
        };
        match &parameter.value {
            ParameterValue::Keyword(value) => Ok(Some((value, &parameter.span))),
            _ => Err(self.parameter_type_error(name, "keyword")),
        }
    }

    fn parameter(&self, name: &str) -> Result<&Spanned<ParameterValue>> {
        self.optional_parameter(name)?
            .ok_or_else(|| self.invalid_binding(name))
    }

    fn optional_parameter(&self, name: &str) -> Result<Option<&Spanned<ParameterValue>>> {
        let slot = self
            .descriptor
            .parameter_slot(name)
            .ok_or_else(|| self.invalid_binding(name))?;
        Ok(self.parameter_at(slot))
    }

    fn invalid_binding(&self, name: &str) -> Diagnostic {
        binding_error(
            &self.origin,
            &format!("resolved call has an invalid or missing binding for `{name}`"),
        )
    }

    fn parameter_type_error(&self, name: &str, expected: &str) -> Diagnostic {
        binding_error(
            &self.origin,
            &format!("resolved parameter `{name}` is not a {expected}"),
        )
    }
}

fn parameter_matches(parameter_type: &ParameterType, value: &ParameterValue) -> bool {
    match (parameter_type, value) {
        (ParameterType::Number, ParameterValue::Number(_))
        | (ParameterType::Integer, ParameterValue::Integer(_))
        | (ParameterType::File, ParameterValue::File(_))
        | (ParameterType::Duration, ParameterValue::Duration(_))
        | (ParameterType::TimeRange, ParameterValue::TimeRange(_)) => true,
        (ParameterType::Keyword(allowed), ParameterValue::Keyword(value)) => {
            allowed.iter().any(|candidate| candidate == value)
        }
        _ => false,
    }
}

fn binding_error(origin: &SourceOrigin, message: &str) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::InternalBinding,
        message,
        origin.span.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ValueType;
    use crate::program::{InputPort, ParameterDescriptor, ProgramDescriptor, StackAccess};
    use crate::semantic::SourceOrigin;

    fn descriptor() -> ProgramDescriptor {
        ProgramDescriptor {
            name: "test".to_owned(),
            semantic_version: 1,
            default_stack_access: StackAccess::Owned,
            inputs: vec![
                InputPort {
                    name: "video".to_owned(),
                    value_type: ValueType::Video.into(),
                    cardinality: Cardinality::One,
                },
                InputPort {
                    name: "values".to_owned(),
                    value_type: ValueType::Audio.into(),
                    cardinality: Cardinality::Variadic { min: 1 },
                },
            ],
            parameters: vec![ParameterDescriptor {
                name: "count".to_owned(),
                parameter_type: ParameterType::Integer,
                required: false,
            }],
            outputs: vec![ValueType::Video.into()],
        }
    }

    fn value(value_type: ValueType, id: u32) -> ValueRef {
        ValueRef::new(crate::model::ValueId::new(id), value_type)
    }

    #[test]
    fn validates_descriptor_aligned_bindings() {
        let descriptor = descriptor();
        let signature = descriptor.resolve_signature(None);
        let origin = SourceOrigin::new("test", SourceSpan::file_start("test.clipasm"));
        let call = ResolvedCall::new(
            &descriptor,
            &signature,
            vec![
                ResolvedInput::One(value(ValueType::Video, 0)),
                ResolvedInput::Variadic(vec![value(ValueType::Audio, 1)]),
            ],
            vec![Some(Spanned::new(
                ParameterValue::Integer(2),
                SourceSpan::file_start("test.clipasm"),
            ))],
            None,
            origin,
        )
        .expect("valid call");
        assert_eq!(
            call.one_input("video").expect("video").value_type(),
            ValueType::Video
        );
        assert_eq!(call.variadic_input("values").expect("values").len(), 1);
        assert_eq!(call.integer_parameter("count").expect("count").0, 2);
    }

    #[test]
    fn rejects_misaligned_cardinality_and_types() {
        let descriptor = descriptor();
        let signature = descriptor.resolve_signature(None);
        let origin = SourceOrigin::new("test", SourceSpan::file_start("test.clipasm"));
        let error = ResolvedCall::new(
            &descriptor,
            &signature,
            vec![
                ResolvedInput::Variadic(vec![value(ValueType::Video, 0)]),
                ResolvedInput::Variadic(vec![value(ValueType::Video, 1)]),
            ],
            vec![None],
            None,
            origin,
        )
        .expect_err("invalid call");
        assert_eq!(error.code, "E_INTERNAL_BINDING");
    }
}
