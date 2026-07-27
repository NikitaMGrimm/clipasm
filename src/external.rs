use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::program::{InputSlot, ParameterValue, ResolvedCall, ResolvedInput};
use crate::source::Spanned;

pub(crate) const EXTERNAL_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ExternalInvocation {
    pub(crate) executable: Spanned<PathBuf>,
    pub(crate) arguments: Vec<ExternalArgumentValue>,
    pub(crate) preserve_input: String,
    pub(crate) inputs: BTreeMap<String, crate::model::ValueRef>,
    pub(crate) parameters: BTreeMap<String, ExternalParameterValue>,
}

#[derive(Clone, Debug)]
pub(crate) struct ExternalRuntime {
    executable: Spanned<PathBuf>,
    arguments: Vec<ExternalArgumentValue>,
    preserve_input: InputSlot,
    parameter_defaults: Vec<Option<Spanned<ParameterValue>>>,
}

impl ExternalRuntime {
    pub(crate) fn new(
        executable: Spanned<PathBuf>,
        arguments: Vec<ExternalArgumentValue>,
        preserve_input: InputSlot,
        parameter_defaults: Vec<Option<Spanned<ParameterValue>>>,
    ) -> Self {
        Self {
            executable,
            arguments,
            preserve_input,
            parameter_defaults,
        }
    }

    pub(crate) fn invocation(&self, call: &ResolvedCall) -> Result<ExternalInvocation> {
        let inputs = call
            .inputs()
            .map(|(input, binding)| match binding {
                ResolvedInput::One(value) => Ok((input.name.clone(), *value)),
                ResolvedInput::Variadic(_) => Err(Diagnostic::builtin(
                    BuiltinDiagnostic::InvalidExternalProgram,
                    format!(
                        "external input `{}` unexpectedly became variadic",
                        input.name
                    ),
                    call.origin().span.clone(),
                )),
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
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
                    ParameterValue::File(path) => {
                        ExternalParameterValue::File(Spanned::new(path.clone(), value.span.clone()))
                    }
                    ParameterValue::Number(_)
                    | ParameterValue::Duration(_)
                    | ParameterValue::TimeRange(_) => {
                        return Err(Diagnostic::builtin(
                            BuiltinDiagnostic::InvalidExternalProgram,
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
            .collect::<Result<BTreeMap<_, _>>>()?;
        let (preserved, _) = call.input_binding(self.preserve_input);
        Ok(ExternalInvocation {
            executable: self.executable.clone(),
            arguments: self.arguments.clone(),
            preserve_input: preserved.name.clone(),
            inputs,
            parameters,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ExternalArgumentValue {
    Text { value: String },
    File { path: Spanned<PathBuf> },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub(crate) enum ExternalParameterValue {
    Integer(i64),
    Keyword(String),
    File(Spanned<PathBuf>),
}
