use std::collections::BTreeMap;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::ValueType;
use crate::program::{Cardinality, ParameterDescriptor, ProgramDefinition, ProgramImplementation};
use crate::source::OutputBindings;

use super::super::draft::{
    BodyId, DraftBody, DraftInput, DraftInvocation, DraftItemKind, DraftParameter, IdTable,
    InvocationId, StackBlockId,
};
use super::{
    BodyInputId, CheckedBody, CheckedInputValue, CheckedInvocation, CheckedItem, CheckedItemKind,
    CheckedOutput, CheckedParameterValue, CheckedStackBlock, LocalType, ScalarAliasChecker,
    ValueLocalId, resolve_value_target, value_local,
};

pub(super) struct CheckedMaterializer<'a> {
    local_types: &'a BTreeMap<String, LocalType>,
    local_ids: &'a BTreeMap<String, ValueLocalId>,
    alias_checker: &'a ScalarAliasChecker<'a>,
    definitions: &'a [ProgramDefinition],
    invocations: IdTable<InvocationId, super::super::typecheck::ResolvedInvocation>,
    stack_blocks: IdTable<StackBlockId, Vec<ValueType>>,
    body_input_count: usize,
}

#[derive(Clone, Copy)]
pub(super) struct BodyBinding {
    pub(super) value_type: ValueType,
    pub(super) id: BodyInputId,
}

struct MaterializedArguments {
    inputs: Vec<Option<CheckedInputValue>>,
    parameters: Vec<Option<CheckedParameterValue>>,
}

impl<'a> CheckedMaterializer<'a> {
    pub(super) fn new(
        local_types: &'a BTreeMap<String, LocalType>,
        local_ids: &'a BTreeMap<String, ValueLocalId>,
        alias_checker: &'a ScalarAliasChecker<'a>,
        definitions: &'a [ProgramDefinition],
        invocations: IdTable<InvocationId, super::super::typecheck::ResolvedInvocation>,
        stack_blocks: IdTable<StackBlockId, Vec<ValueType>>,
    ) -> Self {
        Self {
            local_types,
            local_ids,
            alias_checker,
            definitions,
            invocations,
            stack_blocks,
            body_input_count: 0,
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one body materialization pass owns lexical bindings, checked arguments, and ordered output construction"
    )]
    pub(super) fn body(
        &mut self,
        body: DraftBody,
        lexical: &BTreeMap<String, BodyBinding>,
    ) -> Result<CheckedBody> {
        let scope = body.id;
        self.alias_checker.check_body(scope, lexical)?;
        let mut checked_items = Vec::with_capacity(body.items.len());
        for item in body.items {
            if matches!(item.kind, DraftItemKind::ScalarBinding { .. }) {
                continue;
            }
            let checked = match item.kind {
                DraftItemKind::Reference(reference) => {
                    let output = resolved_value_type(
                        self.local_types,
                        lexical,
                        &reference.value,
                        &reference.span,
                    )?;
                    let target = resolve_value_target(
                        &reference.value,
                        &reference.span,
                        self.local_ids,
                        lexical,
                    )?;
                    CheckedItem {
                        origin: item.origin,
                        outputs: checked_outputs(item.output_bindings, &[output], self.local_ids)?,
                        kind: CheckedItemKind::Reference { target },
                    }
                }
                DraftItemKind::ScalarBinding { .. } => {
                    unreachable!("scalar bindings are removed before checked item materialization")
                }
                DraftItemKind::Invocation(invocation) => {
                    let DraftInvocation {
                        id,
                        name,
                        program,
                        access,
                        type_argument: _,
                        inputs,
                        parameters,
                        body,
                    } = invocation;
                    let definition = &self.definitions[program.index()];
                    let resolved = self.invocations.take(id).ok_or_else(|| {
                        Diagnostic::builtin(
                            BuiltinDiagnostic::InternalTypeResolution,
                            format!("invocation {} was consumed more than once", id.0),
                            item.origin.span.clone(),
                        )
                    })?;
                    let validated = self.explicit_arguments(
                        &name.value,
                        inputs,
                        parameters,
                        definition,
                        scope,
                        lexical,
                    )?;
                    let mut body_input_ids = vec![None; definition.descriptor.inputs.len()];
                    let checked_body = match definition.implementation {
                        ProgramImplementation::Direct(_)
                        | ProgramImplementation::ClipAsm(_)
                        | ProgramImplementation::External(_) => None,
                        ProgramImplementation::Body { .. } => {
                            let body = body.expect("draft body program");
                            let mut body_lexical = lexical.clone();
                            for (index, (port, value_type)) in definition
                                .descriptor
                                .inputs
                                .iter()
                                .zip(&resolved.signature.inputs)
                                .enumerate()
                            {
                                if !matches!(port.cardinality, Cardinality::One) {
                                    continue;
                                }
                                let id = self.allocate_body_input(&item.origin.span)?;
                                body_input_ids[index] = Some(id);
                                body_lexical.insert(
                                    port.name.clone(),
                                    BodyBinding {
                                        value_type: *value_type,
                                        id,
                                    },
                                );
                            }
                            Some(Box::new(self.body(*body, &body_lexical)?))
                        }
                    };
                    let outputs = checked_outputs(
                        item.output_bindings,
                        &resolved.signature.outputs,
                        self.local_ids,
                    )?;
                    CheckedItem {
                        origin: item.origin,
                        outputs,
                        kind: CheckedItemKind::Invocation(CheckedInvocation {
                            program,
                            signature: resolved.signature,
                            access,
                            stack_plan: resolved.stack_plan,
                            inputs: validated.inputs,
                            parameters: validated.parameters,
                            body: checked_body,
                            body_input_ids,
                        }),
                    }
                }
                DraftItemKind::StackBlock(block) => {
                    let output_types = self.stack_blocks.take(block.id).ok_or_else(|| {
                        Diagnostic::builtin(
                            BuiltinDiagnostic::InternalTypeResolution,
                            format!("stack block {} was consumed more than once", block.id.0),
                            item.origin.span.clone(),
                        )
                    })?;
                    CheckedItem {
                        origin: item.origin,
                        outputs: checked_outputs(
                            item.output_bindings,
                            &output_types,
                            self.local_ids,
                        )?,
                        kind: CheckedItemKind::StackBlock(CheckedStackBlock {
                            access: block.access,
                            body: Box::new(self.body(*block.body, lexical)?),
                        }),
                    }
                }
            };
            checked_items.push(checked);
        }
        Ok(CheckedBody {
            items: checked_items,
        })
    }

    pub(super) fn ensure_consumed(&self, span: &crate::source::SourceSpan) -> Result<()> {
        if let Some(index) = self.invocations.first_present() {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InternalTypeResolution,
                format!("invocation {index} was resolved but not materialized"),
                span.clone(),
            ));
        }
        if let Some(index) = self.stack_blocks.first_present() {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InternalTypeResolution,
                format!("stack block {index} was resolved but not materialized"),
                span.clone(),
            ));
        }
        Ok(())
    }

    pub(super) fn body_input_count(&self) -> usize {
        self.body_input_count
    }

    fn allocate_body_input(&mut self, span: &crate::source::SourceSpan) -> Result<BodyInputId> {
        let id = BodyInputId(u32::try_from(self.body_input_count).map_err(|_| {
            Diagnostic::builtin(
                BuiltinDiagnostic::GraphTooLarge,
                "too many lexical body inputs were declared",
                span.clone(),
            )
        })?);
        self.body_input_count = self
            .body_input_count
            .checked_add(1)
            .expect("body input count fits in usize");
        Ok(id)
    }

    fn explicit_arguments(
        &mut self,
        program_name: &str,
        inputs: Vec<Option<DraftInput>>,
        parameters: Vec<Option<DraftParameter>>,
        definition: &ProgramDefinition,
        scope: BodyId,
        lexical: &BTreeMap<String, BodyBinding>,
    ) -> Result<MaterializedArguments> {
        let inputs = inputs
            .into_iter()
            .map(|argument| {
                argument
                    .map(|argument| self.input_argument(argument, lexical))
                    .transpose()
            })
            .collect::<Result<Vec<_>>>()?;
        let parameters = definition
            .descriptor
            .parameters
            .iter()
            .zip(parameters)
            .map(|(parameter, argument)| {
                argument
                    .map(|argument| {
                        check_parameter_argument(
                            program_name,
                            parameter,
                            argument,
                            self.alias_checker,
                            scope,
                            lexical,
                        )
                    })
                    .transpose()
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(MaterializedArguments { inputs, parameters })
    }

    fn input_argument(
        &mut self,
        argument: DraftInput,
        lexical: &BTreeMap<String, BodyBinding>,
    ) -> Result<CheckedInputValue> {
        match argument {
            DraftInput::Reference(reference) => {
                let target = resolve_value_target(
                    &reference.value,
                    &reference.span,
                    self.local_ids,
                    lexical,
                )?;
                Ok(CheckedInputValue::References(vec![target], reference.span))
            }
            DraftInput::Body(body) => {
                let span = body.span.clone();
                Ok(CheckedInputValue::Body(
                    Box::new(self.body(*body, lexical)?),
                    span,
                ))
            }
        }
    }
}

fn checked_outputs(
    bindings: OutputBindings,
    types: &[ValueType],
    local_ids: &BTreeMap<String, ValueLocalId>,
) -> Result<Vec<CheckedOutput>> {
    let names = match bindings {
        OutputBindings::None => vec![None; types.len()],
        OutputBindings::One(name) => vec![Some(name.value)],
        OutputBindings::Many(names, _) => names.into_iter().map(|name| Some(name.value)).collect(),
    };
    debug_assert_eq!(names.len(), types.len());
    names
        .into_iter()
        .zip(types.iter().copied())
        .map(|(name, value_type)| {
            let binding = name
                .as_ref()
                .map(|name| {
                    local_ids.get(name).copied().ok_or_else(|| {
                        Diagnostic::builtin(
                            BuiltinDiagnostic::InternalBinding,
                            format!("checked output `{name}` has no local identity"),
                            crate::source::SourceSpan::file_start("<checked-source>"),
                        )
                    })
                })
                .transpose()?;
            Ok(CheckedOutput {
                name,
                value_type,
                binding,
            })
        })
        .collect()
}

fn check_parameter_argument(
    program: &str,
    parameter: &ParameterDescriptor,
    argument: DraftParameter,
    aliases: &ScalarAliasChecker<'_>,
    scope: BodyId,
    lexical: &BTreeMap<String, BodyBinding>,
) -> Result<CheckedParameterValue> {
    let DraftParameter::Expression(expression) = argument;
    let checked = super::super::parameter::check_expression(
        program,
        &parameter.name,
        &parameter.parameter_type,
        &expression,
        &mut |reference| aliases.resolve_scalar(scope, lexical, reference),
        &mut |reference| aliases.resolve_timeline(lexical, reference),
    )?;
    Ok(CheckedParameterValue::Expression(checked))
}

fn resolved_value_type(
    locals: &BTreeMap<String, LocalType>,
    lexical: &BTreeMap<String, BodyBinding>,
    name: &str,
    span: &crate::source::SourceSpan,
) -> Result<ValueType> {
    lexical
        .get(name)
        .map(|binding| binding.value_type)
        .map_or_else(|| value_local(locals, name, span), Ok)
}
