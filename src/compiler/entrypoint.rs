use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{ValueRef, ValueType, VideoSpec};
use crate::program::{
    BoundParameters, ParameterValue, ProgramImplementation, ProgramRegistry, ResolvedCall,
};
use crate::semantic::{DraftNode, GraphBuilder, SourceOrigin};
use crate::source::{SourceSpan, Spanned};

use super::checked::CheckedProgram;

#[derive(Clone, Debug)]
pub(super) struct VideoInputBinding {
    pub(super) path: PathBuf,
    pub(super) span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(super) struct ParameterBinding {
    pub(super) value: String,
    pub(super) span: SourceSpan,
}

/// External values supplied when compiling a root source program.
///
/// Video inputs and scalar parameters are matched by name against the root
/// program's declared interface. Relative file paths resolve from the
/// [`SourceSpan`] supplied with each binding, allowing callers such as the CLI
/// to use their own working directory without rewriting authored YAML.
#[derive(Clone, Debug, Default)]
pub struct EntrypointBindings {
    pub(super) video_inputs: BTreeMap<String, VideoInputBinding>,
    pub(super) parameters: BTreeMap<String, ParameterBinding>,
    pub(super) output: Option<Spanned<PathBuf>>,
}

impl EntrypointBindings {
    /// Construct an empty set of root-program bindings.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            video_inputs: BTreeMap::new(),
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
        let name = name.into();
        if let Some(previous) = self.video_inputs.get(&name) {
            return Err(duplicate_binding("input", &name, span, &previous.span));
        }
        self.video_inputs.insert(
            name,
            VideoInputBinding {
                path: path.into(),
                span,
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

pub(super) fn bind_root_call(
    program: &CheckedProgram,
    registry: &ProgramRegistry,
    bindings: &EntrypointBindings,
    nodes: &mut Vec<DraftNode>,
    video: &VideoSpec,
) -> Result<ResolvedCall> {
    for (name, binding) in &bindings.video_inputs {
        let Some(input) = program.inputs.iter().find(|input| input.name == *name) else {
            return Err(unknown_binding(name, &binding.span));
        };
        if input.value_type != ValueType::Video {
            return Err(Diagnostic::new(
                "E_INVALID_ARGUMENT_TYPE",
                format!(
                    "root input `{name}` is {}, but `bind_video_input` supplies Video",
                    input.value_type
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
        if !program
            .parameters
            .iter()
            .any(|parameter| parameter.name == *name)
        {
            return Err(unknown_binding(name, &binding.span));
        }
        if let Some(input) = bindings.video_inputs.get(name) {
            return Err(duplicate_binding(
                "argument",
                name,
                binding.span.clone(),
                &input.span,
            ));
        }
    }

    let mut inputs = BTreeMap::new();
    for input in &program.inputs {
        let binding = bindings.video_inputs.get(&input.name).ok_or_else(|| {
            Diagnostic::new(
                "E_MISSING_REQUIRED_INPUT",
                format!("root program is missing input `{}`", input.name),
                program.span.clone(),
            )
        })?;
        inputs.insert(
            input.name.clone(),
            lower_video_binding(registry, binding, nodes, video)?,
        );
    }

    let mut parameters = BoundParameters::new();
    for parameter in &program.parameters {
        if let Some(binding) = bindings.parameters.get(&parameter.name) {
            parameters.insert(
                parameter.name.clone(),
                Spanned::new(
                    super::parameter::from_text(
                        "root",
                        &parameter.name,
                        &parameter.parameter_type,
                        &binding.value,
                        &binding.span,
                    )?,
                    binding.span.clone(),
                ),
            );
        }
    }

    Ok(ResolvedCall::new(
        "root".to_owned(),
        inputs,
        parameters,
        None,
        SourceOrigin::new("root program", program.span.clone()),
    ))
}

fn lower_video_binding(
    registry: &ProgramRegistry,
    binding: &VideoInputBinding,
    nodes: &mut Vec<DraftNode>,
    video: &VideoSpec,
) -> Result<Vec<ValueRef>> {
    let program = registry
        .id("video")
        .expect("native video program is registered");
    let definition = registry.definition(program);
    let signature = definition.descriptor.resolve_signature(None);
    let span = binding.span.clone();
    let call = ResolvedCall::new(
        definition.descriptor.name.clone(),
        BTreeMap::new(),
        BTreeMap::from([(
            "path".to_owned(),
            Spanned::new(ParameterValue::File(binding.path.clone()), span.clone()),
        )]),
        None,
        SourceOrigin::new("video", span.clone()),
    );
    let ProgramImplementation::Direct(lower) = &definition.implementation else {
        unreachable!("video is a direct program")
    };
    let mut builder = GraphBuilder::for_program(
        nodes,
        video,
        definition.descriptor.semantic_version,
        SourceOrigin::new("video", span.clone()),
    );
    let outputs = lower(&call, &mut builder)?;
    if outputs.len() != signature.outputs.len()
        || outputs
            .iter()
            .zip(&signature.outputs)
            .any(|(output, expected)| output.value_type() != *expected)
    {
        return Err(Diagnostic::new(
            "E_PROGRAM_OUTPUT_TYPE",
            "native video input adapter returned outputs outside its declared signature",
            span,
        ));
    }
    Ok(outputs)
}

fn unknown_binding(name: &str, span: &SourceSpan) -> Diagnostic {
    Diagnostic::new(
        "E_UNKNOWN_PROGRAM_ARGUMENT",
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
    Diagnostic::new(
        "E_DUPLICATE_ARGUMENT",
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
