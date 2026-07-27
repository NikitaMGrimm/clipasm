use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{AudioSpec, ValueRef, ValueType, VideoSpec};
use crate::program::{
    ParameterValue, ProgramDefinition, ProgramImplementation, ProgramRegistry, ResolvedCall,
    ResolvedInput,
};
use crate::semantic::{DraftNode, GraphBuilder, SourceOrigin};
use crate::source::{SourceProgram, SourceSpan, Spanned};

#[derive(Clone, Debug)]
pub(super) struct MediaInputBinding {
    pub(super) path: PathBuf,
    pub(super) span: SourceSpan,
    pub(super) value_type: ValueType,
}

#[derive(Clone, Debug)]
pub(super) struct ParameterBinding {
    pub(super) value: String,
    pub(super) span: SourceSpan,
}

/// External values supplied when compiling a root source program.
///
/// Video and Audio inputs and scalar parameters are matched by name against the root
/// program's declared interface. Relative file paths resolve from the
/// [`SourceSpan`] supplied with each binding, allowing callers such as the CLI
/// to use their own working directory without rewriting authored source.
#[derive(Clone, Debug, Default)]
pub struct EntrypointBindings {
    pub(super) media_inputs: BTreeMap<String, MediaInputBinding>,
    pub(super) parameters: BTreeMap<String, ParameterBinding>,
    pub(super) output: Option<Spanned<PathBuf>>,
}

impl EntrypointBindings {
    /// Construct an empty set of root-program bindings.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            media_inputs: BTreeMap::new(),
            parameters: BTreeMap::new(),
            output: None,
        }
    }

    /// Bind one declared root `Video` input to a video-file path.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when the same input name was already supplied.
    pub fn bind_video_input(
        &mut self,
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        span: SourceSpan,
    ) -> Result<()> {
        self.bind_media_input(name, path, span, ValueType::Video)
    }

    /// Bind one declared root `Audio` input to an audio-file path.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when the same input name was already supplied.
    pub fn bind_audio_input(
        &mut self,
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        span: SourceSpan,
    ) -> Result<()> {
        self.bind_media_input(name, path, span, ValueType::Audio)
    }

    fn bind_media_input(
        &mut self,
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        span: SourceSpan,
        value_type: ValueType,
    ) -> Result<()> {
        let name = name.into();
        if let Some(previous) = self.media_inputs.get(&name) {
            return Err(duplicate_binding("input", &name, span, &previous.span));
        }
        self.media_inputs.insert(
            name,
            MediaInputBinding {
                path: path.into(),
                span,
                value_type,
            },
        );
        Ok(())
    }

    /// Bind one declared root scalar parameter from its authored text form.
    ///
    /// The value is converted according to the root program's declared
    /// parameter type during compilation.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when the same parameter name was already supplied.
    pub fn bind_parameter(
        &mut self,
        name: impl Into<String>,
        value: impl Into<String>,
        span: SourceSpan,
    ) -> Result<()> {
        let name = name.into();
        if let Some(previous) = self.parameters.get(&name) {
            return Err(duplicate_binding("parameter", &name, span, &previous.span));
        }
        self.parameters.insert(
            name,
            ParameterBinding {
                value: value.into(),
                span,
            },
        );
        Ok(())
    }

    /// Override the root program's publication path for this compilation.
    pub fn set_output(&mut self, path: impl Into<PathBuf>, span: SourceSpan) {
        self.output = Some(Spanned::new(path.into(), span));
    }
}

pub(super) fn bind_root_call<'a>(
    definition: &'a ProgramDefinition,
    program: &SourceProgram,
    registry: &'a ProgramRegistry,
    bindings: &EntrypointBindings,
    nodes: &mut Vec<DraftNode>,
    video: &VideoSpec,
    audio: AudioSpec,
) -> Result<ResolvedCall<'a>> {
    debug_assert_eq!(definition.descriptor.inputs.len(), program.inputs().len());
    for (name, binding) in &bindings.media_inputs {
        let Some(input) = definition
            .descriptor
            .inputs
            .iter()
            .find(|input| input.name == *name)
        else {
            return Err(unknown_binding(name, &binding.span));
        };
        let input_type = input
            .value_type
            .exact()
            .expect("root source inputs are concrete");
        if input_type != binding.value_type {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidArgumentType,
                format!(
                    "root input `{name}` is {input_type}, but the supplied binding is {}",
                    binding.value_type
                ),
                binding.span.clone(),
            ));
        }
        if let Some(parameter) = bindings.parameters.get(name) {
            return Err(duplicate_binding(
                "argument",
                name,
                parameter.span.clone(),
                &binding.span,
            ));
        }
    }
    for (name, binding) in &bindings.parameters {
        if !definition
            .descriptor
            .parameters
            .iter()
            .any(|parameter| parameter.name == *name)
        {
            return Err(unknown_binding(name, &binding.span));
        }
        if let Some(input) = bindings.media_inputs.get(name) {
            return Err(duplicate_binding(
                "argument",
                name,
                binding.span.clone(),
                &input.span,
            ));
        }
    }

    let signature = definition.descriptor.resolve_signature(None);
    let inputs = bind_root_inputs(definition, program, registry, bindings, nodes, video, audio)?;

    let parameters = definition
        .descriptor
        .parameters
        .iter()
        .map(|parameter| {
            bindings
                .parameters
                .get(&parameter.name)
                .map(|binding| {
                    Ok(Spanned::new(
                        super::parameter::from_text(
                            "root",
                            &parameter.name,
                            &parameter.parameter_type,
                            &binding.value,
                            &binding.span,
                        )?,
                        binding.span.clone(),
                    ))
                })
                .transpose()
        })
        .collect::<Result<Vec<_>>>()?;

    ResolvedCall::new(
        &definition.descriptor,
        &signature,
        inputs,
        parameters,
        None,
        SourceOrigin::new("root program", program.span().clone()),
    )
}

fn bind_root_inputs(
    definition: &ProgramDefinition,
    program: &SourceProgram,
    registry: &ProgramRegistry,
    bindings: &EntrypointBindings,
    nodes: &mut Vec<DraftNode>,
    video: &VideoSpec,
    audio: AudioSpec,
) -> Result<Vec<ResolvedInput>> {
    definition
        .descriptor
        .inputs
        .iter()
        .zip(program.inputs())
        .map(|(input, source_input)| {
            let binding = bindings.media_inputs.get(&input.name).ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::MissingRequiredInput,
                    format!("root program is missing input `{}`", input.name),
                    source_input.declared_at.clone(),
                )
            })?;
            Ok(ResolvedInput::One(lower_media_binding(
                registry, binding, nodes, video, audio,
            )?))
        })
        .collect()
}

fn lower_media_binding(
    registry: &ProgramRegistry,
    binding: &MediaInputBinding,
    nodes: &mut Vec<DraftNode>,
    video: &VideoSpec,
    audio: AudioSpec,
) -> Result<ValueRef> {
    let program_name = match binding.value_type {
        ValueType::Video => "video",
        ValueType::Audio => "audio",
    };
    let program = registry
        .id(program_name)
        .expect("native media source program is registered");
    let definition = registry.definition(program);
    let signature = definition.descriptor.resolve_signature(None);
    let span = binding.span.clone();
    let parameters = definition
        .descriptor
        .parameters
        .iter()
        .map(|parameter| {
            (parameter.name == "path")
                .then(|| Spanned::new(ParameterValue::File(binding.path.clone()), span.clone()))
        })
        .collect();
    let call = ResolvedCall::new(
        &definition.descriptor,
        &signature,
        Vec::new(),
        parameters,
        None,
        SourceOrigin::new(program_name, span.clone()),
    )?;
    let ProgramImplementation::Direct(lower) = &definition.implementation else {
        unreachable!("native media sources are direct programs")
    };
    let mut builder = GraphBuilder::for_program(
        nodes,
        video,
        audio,
        definition.descriptor.semantic_version,
        SourceOrigin::new(program_name, span.clone()),
    );
    let outputs = lower(&call, &mut builder)?;
    let [output] = outputs.as_slice() else {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::ProgramOutputType,
            "native media input adapter returned outputs outside its declared signature",
            span,
        ));
    };
    if signature.outputs != [output.value_type()] {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::ProgramOutputType,
            "native media input adapter returned outputs outside its declared signature",
            span,
        ));
    }
    Ok(*output)
}

fn unknown_binding(name: &str, span: &SourceSpan) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::UnknownProgramArgument,
        format!("unknown argument `{name}` for root program"),
        span.clone(),
    )
}

fn duplicate_binding(
    role: &str,
    name: &str,
    span: SourceSpan,
    previous: &SourceSpan,
) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::DuplicateArgument,
        format!("root {role} `{name}` was supplied more than once"),
        span,
    )
    .note(format!(
        "the previous binding was supplied at {}:{}:{}",
        previous.file().display(),
        previous.line,
        previous.column
    ))
}
