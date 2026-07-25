use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{ValueRef, VideoSpec};
use crate::program::{
    BoundParameters, ParameterType, ProgramImplementation, ProgramRegistry, ResolvedCall,
};
use crate::semantic::{DraftNode, GraphBuilder, SourceOrigin};
use crate::source::{
    ArgumentValue, Invocation, Item, ItemKind, Literal, OutputBindings, ProgramBody, SourcePackage,
    SourceSpan, Spanned,
};

use super::stack::EvaluationStack;

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
    package: &SourcePackage,
    registry: &ProgramRegistry,
    bindings: &EntrypointBindings,
    mut resolve_video: impl FnMut(&VideoInputBinding) -> Result<Vec<ValueRef>>,
) -> Result<ResolvedCall> {
    let program = package.root().program();
    let Some(definition) = registry.source_program(package.root).cloned() else {
        debug_assert!(bindings.video_inputs.is_empty());
        debug_assert!(bindings.parameters.is_empty());
        return Ok(ResolvedCall::new(
            "root".to_owned(),
            BTreeMap::new(),
            BoundParameters::new(),
            None,
            SourceOrigin::new("root program", program.span().clone()),
        ));
    };
    let mut arguments = BTreeMap::new();
    for (name, binding) in &bindings.video_inputs {
        arguments.insert(name.clone(), ArgumentValue::Body(video_input_body(binding)));
    }
    for (name, binding) in &bindings.parameters {
        let parameter_type = program
            .parameters()
            .iter()
            .find(|parameter| parameter.name.value == *name)
            .map(|parameter| &parameter.parameter_type);
        let literal = match parameter_type {
            Some(ParameterType::Integer) => binding.value.parse::<i64>().map_or_else(
                |_| Literal::String(binding.value.clone(), binding.span.clone()),
                |value| Literal::Integer(value, binding.span.clone()),
            ),
            _ => Literal::String(binding.value.clone(), binding.span.clone()),
        };
        arguments.insert(name.clone(), ArgumentValue::Literal(literal));
    }
    let invocation = Invocation {
        program: Spanned::new("root".to_owned(), program.span().clone()),
        stack_access: None,
        arguments,
        body: None,
    };
    let signature = definition.descriptor.resolve_signature(None);
    let (mut stack, mut frame) =
        EvaluationStack::isolated("root program call", program.span().clone());
    super::bind::bind_call(
        &definition,
        &signature,
        &invocation,
        super::bind::BindContext {
            stack: &mut stack,
            frame: &mut frame,
            access: definition.descriptor.default_stack_access,
            requested_frames: None,
            origin: SourceOrigin::new("root program", program.span().clone()),
            stack_plan: None,
        },
        |_value, port| {
            let binding = bindings.video_inputs.get(&port.name).ok_or_else(|| {
                Diagnostic::new(
                    "E_MISSING_REQUIRED_INPUT",
                    format!("root program is missing input `{}`", port.name),
                    program.span().clone(),
                )
            })?;
            resolve_video(binding)
        },
        |_reference, _descriptor| unreachable!("entrypoint scalar bindings do not use references"),
    )
}

pub(super) fn lower_video_binding(
    registry: &ProgramRegistry,
    binding: &VideoInputBinding,
    nodes: &mut Vec<DraftNode>,
    video: &VideoSpec,
) -> Result<Vec<ValueRef>> {
    let program = registry
        .id("video")
        .expect("native video program is registered");
    let definition = registry.definition(program).clone();
    let signature = definition.descriptor.resolve_signature(None);
    let span = binding.span.clone();
    let invocation = Invocation {
        program: Spanned::new("video".to_owned(), span.clone()),
        stack_access: None,
        arguments: BTreeMap::from([(
            "path".to_owned(),
            ArgumentValue::Literal(Literal::File(binding.path.clone(), span.clone())),
        )]),
        body: None,
    };
    let (mut stack, mut frame) = EvaluationStack::isolated("entrypoint Video input", span.clone());
    let call = super::bind::bind_call(
        &definition,
        &signature,
        &invocation,
        super::bind::BindContext {
            stack: &mut stack,
            frame: &mut frame,
            access: definition.descriptor.default_stack_access,
            requested_frames: None,
            origin: SourceOrigin::new("video", span.clone()),
            stack_plan: None,
        },
        |_value, _port| unreachable!("video has no graph inputs"),
        |_reference, _descriptor| unreachable!("video parameters use literals"),
    )?;
    let ProgramImplementation::Direct(lower) = definition.implementation else {
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

fn video_input_body(binding: &VideoInputBinding) -> ProgramBody {
    let span = binding.span.clone();
    ProgramBody {
        items: vec![Item {
            kind: ItemKind::Invocation(Invocation {
                program: Spanned::new("video".to_owned(), span.clone()),
                stack_access: None,
                arguments: BTreeMap::from([(
                    "path".to_owned(),
                    ArgumentValue::Literal(Literal::File(binding.path.clone(), span.clone())),
                )]),
                body: None,
            }),
            output_bindings: OutputBindings::None,
            span: span.clone(),
        }],
        span,
    }
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
