use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::diagnostic::{Diagnostic, Result};
use crate::model::ValueType;
use crate::program::{
    Cardinality, InputPort, InputSlot, ParameterDescriptor, ParameterType, ParameterValue,
    ProgramDefinition, ProgramDescriptor, ProgramImplementation, ResolvedCall, ResolvedInput,
    StackAccess,
};
use crate::source::{SourceFile, SourceSpan, Spanned};

pub(crate) const EXTERNAL_PROTOCOL_VERSION: u32 = 1;
const EXTERNAL_MANIFEST_FORMAT_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub(crate) struct ExternalProgramId(u32);

impl ExternalProgramId {
    #[must_use]
    pub(crate) const fn new(value: u32) -> Self {
        Self(value)
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ExternalInvocation {
    pub(crate) command: Spanned<PathBuf>,
    pub(crate) preserve_input: String,
    pub(crate) inputs: std::collections::BTreeMap<String, crate::model::ValueRef>,
    pub(crate) parameters: std::collections::BTreeMap<String, ExternalParameterValue>,
}

#[derive(Clone, Debug)]
pub(crate) struct ExternalProgram {
    semantic_version: u32,
    inputs: Vec<InputPort>,
    parameters: Vec<ParameterDescriptor>,
    runtime: ExternalRuntime,
}

#[derive(Clone, Debug)]
pub(crate) struct ExternalRuntime {
    command: Spanned<PathBuf>,
    preserve_input: InputSlot,
}

impl ExternalProgram {
    pub(crate) fn descriptor(&self, name: String) -> ProgramDescriptor {
        ProgramDescriptor {
            name,
            semantic_version: self.semantic_version,
            default_stack_access: StackAccess::Owned,
            inputs: self.inputs.clone(),
            parameters: self.parameters.clone(),
            outputs: vec![ValueType::Video.into()],
        }
    }

    pub(crate) fn definition(&self, name: String) -> ProgramDefinition {
        ProgramDefinition {
            descriptor: self.descriptor(name),
            implementation: ProgramImplementation::External(self.runtime.clone()),
        }
    }
}

impl ExternalRuntime {
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
            .filter_map(|(descriptor, value)| value.map(|value| (descriptor, value)))
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format_version: u32,
    protocol_version: u32,
    semantic_version: u32,
    command: PathBuf,
    #[serde(default)]
    inputs: Vec<ManifestInput>,
    #[serde(default)]
    parameters: Vec<ManifestParameter>,
    output: ManifestOutput,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestInput {
    name: String,
    #[serde(rename = "type")]
    value_type: ManifestValueType,
}

#[derive(Clone, Copy, Deserialize)]
enum ManifestValueType {
    Video,
    Audio,
}

impl From<ManifestValueType> for ValueType {
    fn from(value: ManifestValueType) -> Self {
        match value {
            ManifestValueType::Video => Self::Video,
            ManifestValueType::Audio => Self::Audio,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestParameter {
    name: String,
    #[serde(rename = "type")]
    parameter_type: ManifestParameterType,
    #[serde(default)]
    required: bool,
    values: Option<Vec<String>>,
}

#[derive(Clone, Copy, Deserialize)]
enum ManifestParameterType {
    Integer,
    Keyword,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestOutput {
    #[serde(rename = "type")]
    value_type: ManifestValueType,
    preserve: String,
}

#[allow(clippy::too_many_lines)]
pub(crate) fn load_manifest(path: &Path) -> Result<ExternalProgram> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| Diagnostic::io("E_EXTERNAL_MANIFEST_IO", path, &error))?;
    let text = fs::read_to_string(&canonical)
        .map_err(|error| Diagnostic::io("E_EXTERNAL_MANIFEST_IO", &canonical, &error))?;
    let source = SourceFile::new(canonical, text.clone());
    let span = SourceSpan::source_start(source);
    let manifest: Manifest = serde_json::from_str(&text).map_err(|error| {
        Diagnostic::new(
            "E_EXTERNAL_MANIFEST_SYNTAX",
            format!("invalid external program manifest: {error}"),
            SourceSpan::at(span.source().clone(), error.line(), error.column()),
        )
    })?;

    if manifest.format_version != EXTERNAL_MANIFEST_FORMAT_VERSION {
        return Err(Diagnostic::new(
            "E_EXTERNAL_MANIFEST_VERSION",
            format!(
                "unsupported external manifest format {}; expected {}",
                manifest.format_version, EXTERNAL_MANIFEST_FORMAT_VERSION
            ),
            span,
        ));
    }
    if manifest.protocol_version != EXTERNAL_PROTOCOL_VERSION {
        return Err(Diagnostic::new(
            "E_EXTERNAL_PROTOCOL_VERSION",
            format!(
                "unsupported external protocol {}; expected {}",
                manifest.protocol_version, EXTERNAL_PROTOCOL_VERSION
            ),
            span,
        ));
    }
    if manifest.semantic_version == 0 {
        return Err(Diagnostic::new(
            "E_INVALID_EXTERNAL_PROGRAM",
            "external `semantic_version` must be greater than zero",
            span,
        ));
    }
    if manifest.command.as_os_str().is_empty() {
        return Err(Diagnostic::new(
            "E_INVALID_EXTERNAL_PROGRAM",
            "external `command` must not be empty",
            span,
        ));
    }

    let inputs = manifest
        .inputs
        .into_iter()
        .map(|input| InputPort {
            name: input.name,
            value_type: ValueType::from(input.value_type).into(),
            cardinality: Cardinality::One,
        })
        .collect::<Vec<_>>();
    let parameters = manifest
        .parameters
        .into_iter()
        .map(|parameter| {
            let parameter_type = match parameter.parameter_type {
                ManifestParameterType::Integer => {
                    if parameter.values.is_some() {
                        return Err(Diagnostic::new(
                            "E_INVALID_EXTERNAL_PROGRAM",
                            format!(
                                "external Integer parameter `{}` cannot declare `values`",
                                parameter.name
                            ),
                            span.clone(),
                        ));
                    }
                    ParameterType::Integer
                }
                ManifestParameterType::Keyword => {
                    let values = parameter
                        .values
                        .filter(|values| !values.is_empty())
                        .ok_or_else(|| {
                            Diagnostic::new(
                                "E_INVALID_EXTERNAL_PROGRAM",
                                format!(
                                    "external Keyword parameter `{}` requires nonempty `values`",
                                    parameter.name
                                ),
                                span.clone(),
                            )
                        })?;
                    ParameterType::Keyword(values)
                }
            };
            Ok(ParameterDescriptor {
                name: parameter.name,
                parameter_type,
                required: parameter.required,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    if ValueType::from(manifest.output.value_type) != ValueType::Video {
        return Err(Diagnostic::new(
            "E_INVALID_EXTERNAL_PROGRAM",
            "the initial external protocol supports exactly one Video output",
            span,
        ));
    }
    let Some(preserved) = inputs
        .iter()
        .find(|input| input.name == manifest.output.preserve)
    else {
        return Err(Diagnostic::new(
            "E_INVALID_EXTERNAL_PROGRAM",
            format!(
                "external output preserves unknown input `{}`",
                manifest.output.preserve
            ),
            span,
        ));
    };
    if preserved.value_type.exact() != Some(ValueType::Video) {
        return Err(Diagnostic::new(
            "E_INVALID_EXTERNAL_PROGRAM",
            format!(
                "external output preserve input `{}` must be Video",
                manifest.output.preserve
            ),
            span,
        ));
    }

    let preserve_input = inputs
        .iter()
        .position(|input| input.name == manifest.output.preserve)
        .map(InputSlot::new)
        .expect("preserved input was validated above");
    let program = ExternalProgram {
        semantic_version: manifest.semantic_version,
        inputs,
        parameters,
        runtime: ExternalRuntime {
            command: Spanned::new(manifest.command, span.clone()),
            preserve_input,
        },
    };
    let validation = program.definition("external".to_owned());
    crate::program::ProgramRegistry::from_definitions(vec![validation])?;
    Ok(program)
}
