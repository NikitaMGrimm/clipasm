use std::path::PathBuf;

use serde::Serialize;

use crate::diagnostic::{Diagnostic, Result};
use crate::program::{InputSlot, ParameterValue, ResolvedCall, ResolvedInput};
use crate::source::Spanned;

pub(crate) const EXTERNAL_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ExternalInvocation {
    pub(crate) command: Spanned<PathBuf>,
    pub(crate) preserve_input: String,
    pub(crate) inputs: std::collections::BTreeMap<String, crate::model::ValueRef>,
    pub(crate) parameters: std::collections::BTreeMap<String, ExternalParameterValue>,
}

#[derive(Clone, Debug)]
pub(crate) struct ExternalRuntime {
    command: Spanned<PathBuf>,
    preserve_input: InputSlot,
    parameter_defaults: Vec<Option<Spanned<ParameterValue>>>,
}

impl ExternalRuntime {
    pub(crate) fn new(
        command: Spanned<PathBuf>,
        preserve_input: InputSlot,
        parameter_defaults: Vec<Option<Spanned<ParameterValue>>>,
    ) -> Self {
        Self {
            command,
            preserve_input,
            parameter_defaults,
        }
    }

    pub(crate) fn invocation(&self, call: &ResolvedCall) -> Result<ExternalInvocation> {
        let inputs = call
            .inputs()
            .map(|(input, binding)| match binding {
                ResolvedInput::One(value) => Ok((input.name.clone(), *value)),
                ResolvedInput::Variadic(_) => Err(Diagnostic::new(
                    "E_INVALID_EXTERNAL_PROGRAM",
                    format!(
                        "external input `{}` unexpectedly became variadic",
                        input.name
                    ),
                    call.origin().span.clone(),
                )),
            })
            .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
        let parameters = call
            .parameters()
            .enumerate()
            .filter_map(|(index, (descriptor, value))| {
                value
                    .or_else(|| self.parameter_defaults[index].as_ref())
                    .map(|value| (descriptor, value))
            })
            .map(|(descriptor, value)| {
                let parameter = match &value.value {
                    ParameterValue::Integer(value) => ExternalParameterValue::Integer(*value),
                    ParameterValue::Keyword(value) => {
                        ExternalParameterValue::Keyword(value.clone())
                    }
                    ParameterValue::File(_)
                    | ParameterValue::Duration(_)
                    | ParameterValue::TimeRange(_) => {
                        return Err(Diagnostic::new(
                            "E_INVALID_EXTERNAL_PROGRAM",
                            format!(
                                "external parameter `{}` uses an unsupported runtime type",
                                descriptor.name
                            ),
                            value.span.clone(),
                        ));
                    }
                };
                Ok((descriptor.name.clone(), parameter))
            })
            .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
        let (preserved, _) = call.input_binding(self.preserve_input);
        Ok(ExternalInvocation {
            command: self.command.clone(),
            preserve_input: preserved.name.clone(),
            inputs,
            parameters,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
/// One scalar value passed to an external program process.
pub enum ExternalParameterValue {
    /// Signed integer parameter.
    Integer(i64),
    /// One value from a manifest-declared keyword set.
    Keyword(String),
}
