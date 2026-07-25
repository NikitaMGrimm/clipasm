use std::collections::BTreeMap;

use crate::diagnostic::{Diagnostic, Result};
use crate::program::{
    Cardinality, ProgramDefinition, ProgramId, ProgramImplementation, StackAccess,
};
use crate::source::{
    ArgumentValue, ItemKind, Literal, OutputBindings, ProgramBody, SourceProgram, SourceSpan,
    Spanned,
};

const MAX_BODY_NESTING: usize = 256;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct InvocationId(pub(super) usize);

#[derive(Clone, Debug)]
pub(super) struct DraftProgram {
    pub(super) span: SourceSpan,
    pub(super) clips: Vec<DraftClip>,
    pub(super) body: DraftBody,
    pub(super) invocation_count: usize,
}

#[derive(Clone, Debug)]
pub(super) struct DraftClip {
    pub(super) name: String,
    pub(super) span: SourceSpan,
    pub(super) body: DraftBody,
}

#[derive(Clone, Debug)]
pub(super) struct DraftBody {
    pub(super) span: SourceSpan,
    pub(super) items: Vec<DraftItem>,
}

#[derive(Clone, Debug)]
pub(super) struct DraftItem {
    pub(super) span: SourceSpan,
    pub(super) output_bindings: OutputBindings,
    pub(super) kind: DraftItemKind,
}

#[derive(Clone, Debug)]
pub(super) enum DraftItemKind {
    Reference(Spanned<String>),
    Invocation(DraftInvocation),
}

#[derive(Clone, Debug)]
pub(super) struct DraftInvocation {
    pub(super) id: InvocationId,
    pub(super) name: Spanned<String>,
    pub(super) program: ProgramId,
    pub(super) access: StackAccess,
    pub(super) inputs: Vec<Option<DraftInput>>,
    pub(super) parameters: Vec<Option<DraftParameter>>,
    pub(super) body: Option<Box<DraftBody>>,
}

#[derive(Clone, Debug)]
pub(super) enum DraftInput {
    Reference(Spanned<String>),
    References(Vec<Spanned<String>>, SourceSpan),
    Body(Box<DraftBody>),
}

impl DraftInput {
    pub(super) const fn span(&self) -> &SourceSpan {
        match self {
            Self::Reference(reference) => &reference.span,
            Self::References(_, span) => span,
            Self::Body(body) => &body.span,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum DraftParameter {
    Literal(Literal),
    Reference(Spanned<String>),
}

impl DraftProgram {
    pub(super) fn build(
        source: &SourceProgram,
        definitions: &[ProgramDefinition],
        builtins: &BTreeMap<String, ProgramId>,
        namespace: &BTreeMap<String, ProgramId>,
    ) -> Result<Self> {
        let mut invocation_count = 0;
        let clips = source
            .clips()
            .iter()
            .map(|clip| {
                Ok(DraftClip {
                    name: clip.name.clone(),
                    span: clip.span.clone(),
                    body: DraftBody::build(
                        &clip.body,
                        definitions,
                        builtins,
                        namespace,
                        0,
                        &mut invocation_count,
                    )?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let body = DraftBody::build(
            source.body(),
            definitions,
            builtins,
            namespace,
            0,
            &mut invocation_count,
        )?;
        Ok(Self {
            span: source.span().clone(),
            clips,
            body,
            invocation_count,
        })
    }
}

impl DraftBody {
    fn build(
        source: &ProgramBody,
        definitions: &[ProgramDefinition],
        builtins: &BTreeMap<String, ProgramId>,
        namespace: &BTreeMap<String, ProgramId>,
        depth: usize,
        invocation_count: &mut usize,
    ) -> Result<Self> {
        if depth > MAX_BODY_NESTING {
            return Err(Diagnostic::new(
                "E_BODY_NESTING_DEPTH",
                format!("program body nesting exceeds the supported depth of {MAX_BODY_NESTING}"),
                source.span.clone(),
            ));
        }
        Ok(Self {
            span: source.span.clone(),
            items: source
                .items
                .iter()
                .map(|item| {
                    let kind = match &item.kind {
                        ItemKind::Reference(reference) => {
                            DraftItemKind::Reference(reference.name.clone())
                        }
                        ItemKind::Invocation(invocation) => {
                            DraftItemKind::Invocation(DraftInvocation::build(
                                invocation,
                                definitions,
                                builtins,
                                namespace,
                                depth,
                                invocation_count,
                            )?)
                        }
                    };
                    Ok(DraftItem {
                        span: item.span.clone(),
                        output_bindings: item.output_bindings.clone(),
                        kind,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl DraftInvocation {
    #[allow(clippy::too_many_lines)]
    fn build(
        source: &crate::source::Invocation,
        definitions: &[ProgramDefinition],
        builtins: &BTreeMap<String, ProgramId>,
        namespace: &BTreeMap<String, ProgramId>,
        depth: usize,
        invocation_count: &mut usize,
    ) -> Result<Self> {
        let id = InvocationId(*invocation_count);
        *invocation_count = invocation_count
            .checked_add(1)
            .expect("draft invocation count fits in usize");
        let program = program_id_for(
            &source.program.value,
            builtins,
            namespace,
            &source.program.span,
        )?;
        let definition = &definitions[program.index()];
        let access = source
            .stack_access
            .as_ref()
            .map_or(definition.descriptor.default_stack_access, |access| {
                access.value
            });
        let mut inputs = vec![None; definition.descriptor.inputs.len()];
        let mut parameters = vec![None; definition.descriptor.parameters.len()];

        for (name, argument) in &source.arguments {
            if let Some(slot) = definition.descriptor.input_slot(name) {
                let port = definition.descriptor.input(slot);
                let input = match argument {
                    ArgumentValue::Reference(reference) => DraftInput::Reference(reference.clone()),
                    ArgumentValue::References(references, span) => {
                        DraftInput::References(references.clone(), span.clone())
                    }
                    ArgumentValue::Body(body) => {
                        if matches!(port.cardinality, Cardinality::Variadic { .. }) {
                            return Err(Diagnostic::new(
                                "E_INVALID_ARGUMENT_TYPE",
                                format!(
                                    "explicit variadic input `{}.{}` must use `$name` references",
                                    source.program.value, port.name
                                ),
                                body.span.clone(),
                            ));
                        }
                        DraftInput::Body(Box::new(DraftBody::build(
                            body,
                            definitions,
                            builtins,
                            namespace,
                            depth + 1,
                            invocation_count,
                        )?))
                    }
                    ArgumentValue::Literal(_) => {
                        return Err(Diagnostic::new(
                            "E_INVALID_ARGUMENT_TYPE",
                            format!(
                                "input `{}.{}` requires a graph value",
                                source.program.value, port.name
                            ),
                            argument.span().clone(),
                        ));
                    }
                };
                let count = match &input {
                    DraftInput::Reference(_) | DraftInput::Body(_) => 1,
                    DraftInput::References(references, _) => references.len(),
                };
                match port.cardinality {
                    Cardinality::One if count != 1 => {
                        return Err(Diagnostic::new(
                            "E_INVALID_ARGUMENT_TYPE",
                            format!(
                                "input `{}.{}` requires exactly one value",
                                source.program.value, port.name
                            ),
                            input.span().clone(),
                        ));
                    }
                    Cardinality::Variadic { min } if count < min => {
                        return Err(Diagnostic::new(
                            "E_MISSING_REQUIRED_INPUT",
                            format!(
                                "input `{}.{}` requires at least {min} value(s)",
                                source.program.value, port.name
                            ),
                            input.span().clone(),
                        ));
                    }
                    _ => {}
                }
                inputs[slot.index()] = Some(input);
            } else if let Some(slot) = definition.descriptor.parameter_slot(name) {
                parameters[slot.index()] = Some(match argument {
                    ArgumentValue::Literal(literal) => DraftParameter::Literal(literal.clone()),
                    ArgumentValue::Reference(reference) => {
                        DraftParameter::Reference(reference.clone())
                    }
                    ArgumentValue::References(_, _) | ArgumentValue::Body(_) => {
                        return Err(Diagnostic::new(
                            "E_INVALID_ARGUMENT_TYPE",
                            format!(
                                "parameter `{}.{name}` requires a scalar value",
                                source.program.value
                            ),
                            argument.span().clone(),
                        ));
                    }
                });
            } else {
                return Err(Diagnostic::new(
                    "E_UNKNOWN_PROGRAM_ARGUMENT",
                    format!(
                        "unknown argument `{name}` for program `{}`",
                        source.program.value
                    ),
                    argument.span().clone(),
                ));
            }
        }

        for (descriptor, argument) in definition.descriptor.parameters.iter().zip(&parameters) {
            if descriptor.required && argument.is_none() {
                return Err(Diagnostic::new(
                    "E_MISSING_ARGUMENT",
                    format!(
                        "missing required parameter `{}.{}`",
                        source.program.value, descriptor.name
                    ),
                    source.program.span.clone(),
                ));
            }
        }

        let body = match definition.implementation {
            ProgramImplementation::Direct(_)
            | ProgramImplementation::Authored(_)
            | ProgramImplementation::External(_) => {
                if source.body.is_some() {
                    return Err(Diagnostic::new(
                        "E_UNEXPECTED_PROGRAM_BODY",
                        format!(
                            "program `{}` does not accept a caller-supplied body",
                            source.program.value
                        ),
                        source.program.span.clone(),
                    ));
                }
                None
            }
            ProgramImplementation::Body { .. } => {
                let body = source.body.as_ref().ok_or_else(|| {
                    Diagnostic::new(
                        "E_MISSING_PROGRAM_BODY",
                        format!("body program `{}` requires a `body`", source.program.value),
                        source.program.span.clone(),
                    )
                })?;
                Some(Box::new(DraftBody::build(
                    body,
                    definitions,
                    builtins,
                    namespace,
                    depth + 1,
                    invocation_count,
                )?))
            }
        };

        Ok(Self {
            id,
            name: source.program.clone(),
            program,
            access,
            inputs,
            parameters,
            body,
        })
    }
}

fn program_id_for(
    name: &str,
    builtins: &BTreeMap<String, ProgramId>,
    namespace: &BTreeMap<String, ProgramId>,
    span: &SourceSpan,
) -> Result<ProgramId> {
    builtins
        .get(name)
        .or_else(|| namespace.get(name))
        .copied()
        .ok_or_else(|| {
            Diagnostic::new(
                "E_UNKNOWN_PROGRAM",
                format!("unknown program `{name}`"),
                span.clone(),
            )
        })
}
