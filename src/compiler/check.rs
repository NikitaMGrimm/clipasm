use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostic::{Diagnostic, Result};
use crate::model::ValueType;
use crate::program::{
    Cardinality, ParameterDescriptor, ParameterType, ProgramDefinition, ProgramDescriptor,
    ProgramId, ProgramImplementation, ProgramRegistry, ValueTypeSpec, builtin_programs,
};
use crate::source::{OutputBindings, SourcePackage, SourceProgram, SourceUnitId};

use super::draft::{DraftBody, DraftInput, DraftInvocation, DraftItemKind, DraftParameter};
#[derive(Clone, Debug)]
pub(super) enum LocalType {
    Value(ValueType),
    Parameter(ParameterType),
    Inferred {
        dependencies: BTreeSet<String>,
        span: crate::source::SourceSpan,
    },
}

pub(super) use super::checked::{
    BodyInputId, CheckedBody, CheckedClip, CheckedInputValue, CheckedItem, CheckedItemKind,
    CheckedLocal, CheckedOutput, CheckedPackage, CheckedParameter, CheckedParameterValue,
    CheckedProgram, CheckedProgramInput, ParameterId, ReferenceTarget, ValueLocalId,
};

pub(super) fn check(package: &SourcePackage) -> Result<CheckedPackage> {
    let mut definitions = builtin_programs();
    let builtin_count = definitions.len();
    let builtin_names = definitions[..builtin_count]
        .iter()
        .enumerate()
        .map(|(index, definition)| {
            (
                definition.descriptor.name.clone(),
                ProgramId::new(u32::try_from(index).expect("built-in catalog fits in u32")),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let external_programs = register_external_programs(package, &mut definitions);
    let mut unit_programs = BTreeMap::new();
    let mut programs = Vec::with_capacity(package.units().len());

    for (index, unit) in package.units().iter().enumerate() {
        let unit_id = SourceUnitId(index);
        let mut namespace = unit
            .imports
            .iter()
            .map(|import| {
                let program = unit_programs.get(&import.target).copied().ok_or_else(|| {
                    Diagnostic::new(
                        "E_INTERNAL_PROGRAM_LINK",
                        format!(
                            "import `{}` refers to a source program that was not linked first",
                            import.alias.value
                        ),
                        import.alias.span.clone(),
                    )
                })?;
                Ok((import.alias.value.clone(), program))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        for external in &unit.externals {
            let program = external_programs[&external.target];
            if namespace
                .insert(external.alias.value.clone(), program)
                .is_some()
            {
                return Err(Diagnostic::new(
                    "E_DUPLICATE_PROGRAM_IMPORT",
                    format!("duplicate program import alias `{}`", external.alias.value),
                    external.alias.span.clone(),
                ));
            }
        }
        let (outputs, checked_program) = check_program(
            unit_id,
            unit.program(),
            &definitions,
            &builtin_names,
            &namespace,
        )?;
        let parameters = unit
            .program()
            .parameters()
            .iter()
            .map(|parameter| {
                validate_parameter_default(parameter)?;
                Ok(ParameterDescriptor {
                    name: parameter.name.value.clone(),
                    parameter_type: parameter.parameter_type.clone(),
                    required: parameter.default.is_none(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let definition = ProgramDefinition {
            descriptor: ProgramDescriptor {
                name: format!("source_program_{index}"),
                semantic_version: 1,
                default_stack_access: unit.program().stack_access(),
                inputs: unit.program().inputs().to_vec(),
                parameters,
                type_selector: None,
                outputs: outputs.into_iter().map(Into::into).collect(),
            },
            implementation: ProgramImplementation::Authored(unit_id),
        };
        let id = ProgramId::new(
            u32::try_from(definitions.len()).expect("linked program catalog fits in u32"),
        );
        definitions.push(definition);
        unit_programs.insert(unit_id, id);
        programs.push(checked_program);
    }

    let registry = ProgramRegistry::from_linked(definitions, builtin_count)?;
    Ok(CheckedPackage {
        root: package.root,
        registry,
        programs,
    })
}

fn register_external_programs(
    package: &SourcePackage,
    definitions: &mut Vec<ProgramDefinition>,
) -> BTreeMap<crate::external::ExternalProgramId, ProgramId> {
    package
        .external_programs()
        .iter()
        .enumerate()
        .map(|(index, external)| {
            let external_id = crate::external::ExternalProgramId::new(
                u32::try_from(index).expect("external program catalog fits in u32"),
            );
            let program_id = ProgramId::new(
                u32::try_from(definitions.len()).expect("program catalog fits in u32"),
            );
            definitions.push(external.definition(format!("external_program_{index}")));
            (external_id, program_id)
        })
        .collect()
}

#[cfg(test)]
pub(super) fn check_with_registry(
    package: &SourcePackage,
    registry: ProgramRegistry,
) -> Result<CheckedPackage> {
    debug_assert_eq!(package.units().len(), 1);
    debug_assert!(package.root().imports.is_empty());
    let definitions = registry.definitions();
    let names = definitions
        .iter()
        .enumerate()
        .map(|(index, definition)| {
            (
                definition.descriptor.name.clone(),
                ProgramId::new(u32::try_from(index).expect("test catalog fits in u32")),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let (_, program) = check_program(
        package.root,
        package.root().program(),
        definitions,
        &names,
        &BTreeMap::new(),
    )?;
    Ok(CheckedPackage {
        root: package.root,
        registry,
        programs: vec![program],
    })
}

#[allow(clippy::too_many_lines)]
fn check_program(
    _unit: SourceUnitId,
    program: &SourceProgram,
    definitions: &[ProgramDefinition],
    builtins: &BTreeMap<String, ProgramId>,
    namespace: &BTreeMap<String, ProgramId>,
) -> Result<(Vec<ValueType>, CheckedProgram)> {
    let draft = super::draft::DraftProgram::build(program, definitions, builtins, namespace)?;
    let mut local_types = BTreeMap::new();
    for input in program.inputs() {
        insert_local(
            &mut local_types,
            &input.name,
            LocalType::Value(
                input
                    .value_type
                    .exact()
                    .expect("authored inputs are concrete"),
            ),
            program.span(),
        )?;
    }
    for parameter in program.parameters() {
        insert_local(
            &mut local_types,
            &parameter.name.value,
            LocalType::Parameter(parameter.parameter_type.clone()),
            &parameter.name.span,
        )?;
    }
    for clip in program.clips() {
        insert_local(
            &mut local_types,
            &clip.name,
            LocalType::Value(ValueType::Video),
            &clip.span,
        )?;
    }
    for clip in &draft.clips {
        collect_body_names(&clip.body, &mut local_types, definitions)?;
    }
    collect_body_names(&draft.body, &mut local_types, definitions)?;
    validate_local_dependencies(&local_types)?;
    let inference = super::typecheck::resolve_program_types(&draft, &mut local_types, definitions)?;
    ensure_local_types_resolved(&local_types)?;

    let bindings = prepare_program_bindings(program, &draft, &local_types)?;
    let mut body_input_count = 0_usize;
    let lexical_types = BTreeMap::new();
    let lexical_ids = BTreeMap::new();
    let mut checked_clips = Vec::with_capacity(draft.clips.len());
    for (clip, outputs) in draft.clips.iter().zip(&inference.clip_outputs) {
        let [output] = outputs.as_slice() else {
            return Err(Diagnostic::new(
                "E_CLIP_OUTPUT_COUNT",
                format!(
                    "named clip `{}` must leave exactly one value, but {} values remain",
                    clip.name,
                    outputs.len()
                ),
                clip.span.clone(),
            ));
        };
        if *output != ValueType::Video {
            return Err(Diagnostic::new(
                "E_TYPE_MISMATCH",
                format!(
                    "named clip `{}` must produce Video, but found {output}",
                    clip.name
                ),
                clip.span.clone(),
            ));
        }
        checked_clips.push(materialize_body(
            &clip.body,
            &local_types,
            &bindings.local_ids,
            &bindings.parameter_ids,
            &lexical_types,
            &lexical_ids,
            &mut body_input_count,
            definitions,
            &inference.invocations,
        )?);
    }

    let checked_body = materialize_body(
        &draft.body,
        &local_types,
        &bindings.local_ids,
        &bindings.parameter_ids,
        &lexical_types,
        &lexical_ids,
        &mut body_input_count,
        definitions,
        &inference.invocations,
    )?;
    let mut clips = program
        .clips()
        .iter()
        .zip(checked_clips)
        .map(|(clip, body)| CheckedClip {
            name: clip.name.clone(),
            span: clip.span.clone(),
            local: bindings.local_ids[&clip.name],
            body,
        })
        .collect::<Vec<_>>();
    clips.sort_by(|left, right| left.name.cmp(&right.name));
    Ok((
        inference.outputs,
        CheckedProgram {
            span: program.span().clone(),
            stack_access: program.stack_access(),
            inputs: program
                .inputs()
                .iter()
                .map(|input| CheckedProgramInput {
                    name: input.name.clone(),
                    value_type: input
                        .value_type
                        .exact()
                        .expect("authored program inputs are concrete"),
                    local: bindings.local_ids[&input.name],
                })
                .collect(),
            locals: bindings.locals,
            parameters: bindings.parameters,
            body_input_count,
            clips,
            body: checked_body,
        },
    ))
}

struct ProgramBindings {
    locals: Vec<CheckedLocal>,
    local_ids: BTreeMap<String, ValueLocalId>,
    parameters: Vec<CheckedParameter>,
    parameter_ids: BTreeMap<String, ParameterId>,
}

fn prepare_program_bindings(
    program: &SourceProgram,
    draft: &super::draft::DraftProgram,
    local_types: &BTreeMap<String, LocalType>,
) -> Result<ProgramBindings> {
    let mut parameters = Vec::with_capacity(program.parameters().len());
    let mut parameter_ids = BTreeMap::new();
    for parameter in program.parameters() {
        let id = ParameterId(u32::try_from(parameters.len()).map_err(|_| {
            Diagnostic::new(
                "E_GRAPH_TOO_LARGE",
                "too many scalar parameters were declared",
                parameter.name.span.clone(),
            )
        })?);
        parameter_ids.insert(parameter.name.value.clone(), id);
        let default = parameter
            .default
            .as_ref()
            .map(|literal| {
                super::parameter::from_literal(
                    "authored program",
                    &parameter.name.value,
                    &parameter.parameter_type,
                    literal,
                )
                .map(|value| crate::source::Spanned::new(value, literal.span().clone()))
            })
            .transpose()?;
        parameters.push(CheckedParameter {
            name: parameter.name.value.clone(),
            parameter_type: parameter.parameter_type.clone(),
            declared_at: parameter.name.span.clone(),
            default,
        });
    }

    let mut locals = Vec::new();
    let mut local_ids = BTreeMap::new();
    let mut declare = |name: &str, span: &crate::source::SourceSpan| -> Result<()> {
        let value_type = value_local(local_types, name, span)?;
        let id = ValueLocalId(u32::try_from(locals.len()).map_err(|_| {
            Diagnostic::new(
                "E_GRAPH_TOO_LARGE",
                "too many named values were declared",
                span.clone(),
            )
        })?);
        local_ids.insert(name.to_owned(), id);
        locals.push(CheckedLocal {
            name: name.to_owned(),
            declared_at: span.clone(),
            value_type,
        });
        Ok(())
    };
    for input in program.inputs() {
        declare(&input.name, program.span())?;
    }
    for clip in program.clips() {
        declare(&clip.name, &clip.span)?;
    }
    for clip in &draft.clips {
        declare_body_outputs(&clip.body, &mut declare)?;
    }
    declare_body_outputs(&draft.body, &mut declare)?;

    Ok(ProgramBindings {
        locals,
        local_ids,
        parameters,
        parameter_ids,
    })
}

fn declare_body_outputs(
    body: &DraftBody,
    declare: &mut impl FnMut(&str, &crate::source::SourceSpan) -> Result<()>,
) -> Result<()> {
    for item in &body.items {
        match &item.output_bindings {
            OutputBindings::None => {}
            OutputBindings::One(name) => declare(&name.value, &name.span)?,
            OutputBindings::Many(names, _) => {
                for name in names {
                    declare(&name.value, &name.span)?;
                }
            }
        }
        if let DraftItemKind::Invocation(invocation) = &item.kind {
            if let Some(body) = invocation.body.as_deref() {
                declare_body_outputs(body, declare)?;
            }
            for input in invocation.inputs.iter().flatten() {
                if let DraftInput::Body(body) = input {
                    declare_body_outputs(body, declare)?;
                }
            }
        }
    }
    Ok(())
}

fn resolve_value_target(
    name: &str,
    span: &crate::source::SourceSpan,
    locals: &BTreeMap<String, ValueLocalId>,
    lexical: &BTreeMap<String, BodyInputId>,
) -> Result<ReferenceTarget> {
    lexical
        .get(name)
        .copied()
        .map(ReferenceTarget::BodyInput)
        .or_else(|| locals.get(name).copied().map(ReferenceTarget::Local))
        .ok_or_else(|| missing_reference(name, span))
}

fn collect_body_names(
    body: &DraftBody,
    locals: &mut BTreeMap<String, LocalType>,
    definitions: &[ProgramDefinition],
) -> Result<()> {
    for item in &body.items {
        match &item.output_bindings {
            OutputBindings::None => {}
            OutputBindings::One(name) => {
                let output_types = item_output_types(item, definitions);
                let [output] = output_types.as_slice() else {
                    return Err(binding_count_error(
                        item,
                        output_types.len(),
                        "`id` requires exactly one output",
                        &name.span,
                    ));
                };
                insert_local(locals, &name.value, output.clone(), &name.span)?;
            }
            OutputBindings::Many(names, span) => {
                let output_types = item_output_types(item, definitions);
                if output_types.len() <= 1 || output_types.len() != names.len() {
                    return Err(binding_count_error(
                        item,
                        output_types.len(),
                        &format!("`ids` contains {} name(s)", names.len()),
                        span,
                    ));
                }
                for (name, output) in names.iter().zip(output_types) {
                    insert_local(locals, &name.value, output, &name.span)?;
                }
            }
        }
        if let DraftItemKind::Invocation(invocation) = &item.kind {
            if let Some(body) = invocation.body.as_deref() {
                collect_body_names(body, locals, definitions)?;
            }
            for input in invocation.inputs.iter().flatten() {
                if let DraftInput::Body(body) = input {
                    collect_body_names(body, locals, definitions)?;
                }
            }
        }
    }
    Ok(())
}

fn item_output_types(
    item: &super::draft::DraftItem,
    definitions: &[ProgramDefinition],
) -> Vec<LocalType> {
    match &item.kind {
        DraftItemKind::Reference(reference) => vec![LocalType::Inferred {
            dependencies: BTreeSet::from([reference.value.clone()]),
            span: item.span.clone(),
        }],
        DraftItemKind::Invocation(invocation) => {
            let definition = &definitions[invocation.program.index()];
            let mut dependencies = BTreeSet::new();
            collect_invocation_dependencies(
                invocation,
                definitions,
                &BTreeSet::new(),
                &mut dependencies,
            );
            definition
                .descriptor
                .outputs
                .iter()
                .copied()
                .map(|output| match output {
                    ValueTypeSpec::Exact(value_type) => LocalType::Value(value_type),
                    ValueTypeSpec::Generic => LocalType::Inferred {
                        dependencies: dependencies.clone(),
                        span: item.span.clone(),
                    },
                })
                .collect()
        }
    }
}

fn checked_outputs(
    bindings: &OutputBindings,
    types: &[ValueType],
    local_ids: &BTreeMap<String, ValueLocalId>,
) -> Result<Vec<CheckedOutput>> {
    let names = match bindings {
        OutputBindings::None => vec![None; types.len()],
        OutputBindings::One(name) => vec![Some(name.value.clone())],
        OutputBindings::Many(names, _) => {
            names.iter().map(|name| Some(name.value.clone())).collect()
        }
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
                        Diagnostic::new(
                            "E_INTERNAL_BINDING",
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

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn materialize_body(
    body: &DraftBody,
    local_types: &BTreeMap<String, LocalType>,
    local_ids: &BTreeMap<String, ValueLocalId>,
    parameter_ids: &BTreeMap<String, ParameterId>,
    lexical_types: &BTreeMap<String, ValueType>,
    lexical_ids: &BTreeMap<String, BodyInputId>,
    body_input_count: &mut usize,
    definitions: &[ProgramDefinition],
    invocations: &[super::typecheck::ResolvedInvocation],
) -> Result<CheckedBody> {
    let mut checked_items = Vec::with_capacity(body.items.len());
    for item in &body.items {
        let checked = match &item.kind {
            DraftItemKind::Reference(reference) => {
                let output = resolved_value_type(
                    local_types,
                    lexical_types,
                    &reference.value,
                    &reference.span,
                )?;
                let target = resolve_value_target(
                    &reference.value,
                    &reference.span,
                    local_ids,
                    lexical_ids,
                )?;
                CheckedItem {
                    span: item.span.clone(),
                    construct: "reference".to_owned(),
                    outputs: checked_outputs(&item.output_bindings, &[output], local_ids)?,
                    kind: CheckedItemKind::Reference { target },
                }
            }
            DraftItemKind::Invocation(invocation) => {
                let definition = &definitions[invocation.program.index()];
                let resolved = &invocations[invocation.id.0];
                let validated = materialize_explicit_arguments(
                    invocation,
                    definition,
                    local_types,
                    local_ids,
                    parameter_ids,
                    lexical_types,
                    lexical_ids,
                    body_input_count,
                    definitions,
                    invocations,
                )?;
                let mut body_input_ids = BTreeMap::new();
                let checked_body = match definition.implementation {
                    ProgramImplementation::Direct(_)
                    | ProgramImplementation::Authored(_)
                    | ProgramImplementation::External(_) => None,
                    ProgramImplementation::Body { .. } => {
                        let body = invocation.body.as_deref().expect("draft body program");
                        let mut body_local_types = lexical_types.clone();
                        let mut body_lexical_ids = lexical_ids.clone();
                        for port in &resolved.signature.inputs {
                            if !matches!(port.cardinality, Cardinality::One) {
                                continue;
                            }
                            let id = allocate_body_input(body_input_count, &item.span)?;
                            body_input_ids.insert(port.name.clone(), id);
                            body_local_types.insert(port.name.clone(), port.value_type);
                            body_lexical_ids.insert(port.name.clone(), id);
                        }
                        Some(Box::new(materialize_body(
                            body,
                            local_types,
                            local_ids,
                            parameter_ids,
                            &body_local_types,
                            &body_lexical_ids,
                            body_input_count,
                            definitions,
                            invocations,
                        )?))
                    }
                };
                CheckedItem {
                    span: item.span.clone(),
                    construct: invocation.name.value.clone(),
                    outputs: checked_outputs(
                        &item.output_bindings,
                        &resolved.signature.outputs,
                        local_ids,
                    )?,
                    kind: CheckedItemKind::Invocation {
                        program: invocation.program,
                        signature: resolved.signature.clone(),
                        access: invocation.access,
                        stack_plan: resolved.stack_plan.clone(),
                        inputs: validated.inputs,
                        parameters: validated.parameters,
                        body: checked_body,
                        body_input_ids,
                    },
                }
            }
        };
        checked_items.push(checked);
    }
    Ok(CheckedBody {
        items: checked_items,
    })
}

fn allocate_body_input(
    body_input_count: &mut usize,
    span: &crate::source::SourceSpan,
) -> Result<BodyInputId> {
    let id = BodyInputId(u32::try_from(*body_input_count).map_err(|_| {
        Diagnostic::new(
            "E_GRAPH_TOO_LARGE",
            "too many lexical body inputs were declared",
            span.clone(),
        )
    })?);
    *body_input_count = body_input_count
        .checked_add(1)
        .expect("body input count fits in usize");
    Ok(id)
}

struct MaterializedArguments {
    inputs: Vec<Option<CheckedInputValue>>,
    parameters: Vec<Option<CheckedParameterValue>>,
}

#[allow(clippy::too_many_arguments)]
fn materialize_explicit_arguments(
    invocation: &DraftInvocation,
    definition: &ProgramDefinition,
    local_types: &BTreeMap<String, LocalType>,
    local_ids: &BTreeMap<String, ValueLocalId>,
    parameter_ids: &BTreeMap<String, ParameterId>,
    lexical_types: &BTreeMap<String, ValueType>,
    lexical_ids: &BTreeMap<String, BodyInputId>,
    body_input_count: &mut usize,
    definitions: &[ProgramDefinition],
    invocations: &[super::typecheck::ResolvedInvocation],
) -> Result<MaterializedArguments> {
    let inputs = invocation
        .inputs
        .iter()
        .map(|argument| {
            argument
                .as_ref()
                .map(|argument| {
                    materialize_input_argument(
                        argument,
                        local_types,
                        local_ids,
                        parameter_ids,
                        lexical_types,
                        lexical_ids,
                        body_input_count,
                        definitions,
                        invocations,
                    )
                })
                .transpose()
        })
        .collect::<Result<Vec<_>>>()?;
    let parameters = definition
        .descriptor
        .parameters
        .iter()
        .zip(&invocation.parameters)
        .map(|(parameter, argument)| {
            argument
                .as_ref()
                .map(|argument| {
                    check_parameter_argument(
                        &invocation.name.value,
                        parameter,
                        argument,
                        local_types,
                        parameter_ids,
                    )
                })
                .transpose()
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(MaterializedArguments { inputs, parameters })
}

#[allow(clippy::too_many_arguments)]
fn materialize_input_argument(
    argument: &DraftInput,
    local_types: &BTreeMap<String, LocalType>,
    local_ids: &BTreeMap<String, ValueLocalId>,
    parameter_ids: &BTreeMap<String, ParameterId>,
    lexical_types: &BTreeMap<String, ValueType>,
    lexical_ids: &BTreeMap<String, BodyInputId>,
    body_input_count: &mut usize,
    definitions: &[ProgramDefinition],
    invocations: &[super::typecheck::ResolvedInvocation],
) -> Result<CheckedInputValue> {
    match argument {
        DraftInput::Reference(reference) => Ok(CheckedInputValue::References(
            vec![resolve_value_target(
                &reference.value,
                &reference.span,
                local_ids,
                lexical_ids,
            )?],
            reference.span.clone(),
        )),
        DraftInput::References(references, span) => Ok(CheckedInputValue::References(
            references
                .iter()
                .map(|reference| {
                    resolve_value_target(&reference.value, &reference.span, local_ids, lexical_ids)
                })
                .collect::<Result<Vec<_>>>()?,
            span.clone(),
        )),
        DraftInput::Body(body) => Ok(CheckedInputValue::Body(
            Box::new(materialize_body(
                body,
                local_types,
                local_ids,
                parameter_ids,
                lexical_types,
                lexical_ids,
                body_input_count,
                definitions,
                invocations,
            )?),
            body.span.clone(),
        )),
    }
}

fn check_parameter_argument(
    program: &str,
    parameter: &ParameterDescriptor,
    argument: &DraftParameter,
    local_types: &BTreeMap<String, LocalType>,
    parameter_ids: &BTreeMap<String, ParameterId>,
) -> Result<CheckedParameterValue> {
    match argument {
        DraftParameter::Literal(literal) => {
            Ok(CheckedParameterValue::Literal(crate::source::Spanned::new(
                super::parameter::from_literal(
                    program,
                    &parameter.name,
                    &parameter.parameter_type,
                    literal,
                )?,
                literal.span().clone(),
            )))
        }
        DraftParameter::Reference(reference) => match local_types.get(&reference.value) {
            Some(LocalType::Parameter(actual)) if actual == &parameter.parameter_type => {
                Ok(CheckedParameterValue::Reference(
                    *parameter_ids
                        .get(&reference.value)
                        .ok_or_else(|| missing_reference(&reference.value, &reference.span))?,
                ))
            }
            Some(LocalType::Parameter(_)) => Err(Diagnostic::new(
                "E_INVALID_ARGUMENT_TYPE",
                format!(
                    "parameter `${}` is not compatible with `{program}.{}`",
                    reference.value, parameter.name
                ),
                reference.span.clone(),
            )),
            Some(LocalType::Value(_) | LocalType::Inferred { .. }) => Err(Diagnostic::new(
                "E_INVALID_ARGUMENT_TYPE",
                format!(
                    "graph value `${}` cannot be used as scalar parameter `{program}.{}`",
                    reference.value, parameter.name
                ),
                reference.span.clone(),
            )),
            None => Err(missing_reference(&reference.value, &reference.span)),
        },
    }
}

fn resolved_value_type(
    locals: &BTreeMap<String, LocalType>,
    lexical: &BTreeMap<String, ValueType>,
    name: &str,
    span: &crate::source::SourceSpan,
) -> Result<ValueType> {
    lexical
        .get(name)
        .copied()
        .map_or_else(|| value_local(locals, name, span), Ok)
}

fn insert_local(
    locals: &mut BTreeMap<String, LocalType>,
    name: &str,
    local: LocalType,
    span: &crate::source::SourceSpan,
) -> Result<()> {
    if locals.insert(name.to_owned(), local).is_some() {
        return Err(Diagnostic::new(
            "E_DUPLICATE_NAME",
            format!("duplicate local name `{name}`"),
            span.clone(),
        ));
    }
    Ok(())
}

fn validate_local_dependencies(locals: &BTreeMap<String, LocalType>) -> Result<()> {
    struct Frame {
        name: String,
        dependencies: Vec<String>,
        next: usize,
    }

    let inferred = locals
        .iter()
        .filter_map(|(name, local)| match local {
            LocalType::Inferred {
                dependencies, span, ..
            } => Some((name.clone(), (dependencies.clone(), span.clone()))),
            LocalType::Value(_) | LocalType::Parameter(_) => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut states = BTreeMap::<String, u8>::new();

    for root in inferred.keys() {
        if states.get(root).copied().unwrap_or(0) != 0 {
            continue;
        }
        let mut path = vec![root.clone()];
        let mut positions = BTreeMap::from([(root.clone(), 0_usize)]);
        let mut stack = vec![Frame {
            name: root.clone(),
            dependencies: inferred[root].0.iter().cloned().collect(),
            next: 0,
        }];
        states.insert(root.clone(), 1);

        while let Some(frame) = stack.last_mut() {
            let Some(dependency) = frame.dependencies.get(frame.next).cloned() else {
                let frame = stack.pop().expect("active inference frame");
                path.pop();
                positions.remove(&frame.name);
                states.insert(frame.name, 2);
                continue;
            };
            frame.next += 1;
            if !locals.contains_key(&dependency) {
                return Err(missing_reference(&dependency, &inferred[&frame.name].1));
            }
            if !inferred.contains_key(&dependency) {
                continue;
            }
            match states.get(&dependency).copied().unwrap_or(0) {
                0 => {
                    states.insert(dependency.clone(), 1);
                    positions.insert(dependency.clone(), path.len());
                    path.push(dependency.clone());
                    stack.push(Frame {
                        name: dependency.clone(),
                        dependencies: inferred[&dependency].0.iter().cloned().collect(),
                        next: 0,
                    });
                }
                1 => {
                    let start = positions[&dependency];
                    let mut cycle = path[start..].to_vec();
                    cycle.push(dependency.clone());
                    return Err(Diagnostic::new(
                        "E_DEPENDENCY_CYCLE",
                        format!("named-value dependency cycle: {}", cycle.join(" -> ")),
                        inferred[&dependency].1.clone(),
                    ));
                }
                2 => {}
                _ => unreachable!("inference dependency state is closed"),
            }
        }
    }
    Ok(())
}

fn ensure_local_types_resolved(locals: &BTreeMap<String, LocalType>) -> Result<()> {
    if let Some((name, LocalType::Inferred { span, .. })) = locals
        .iter()
        .find(|(_, local)| matches!(local, LocalType::Inferred { .. }))
    {
        return Err(unresolved_local_type(name, span));
    }
    Ok(())
}

fn unresolved_local_type(name: &str, span: &crate::source::SourceSpan) -> Diagnostic {
    Diagnostic::new(
        "E_TYPE_INFERENCE_DEPENDENCY",
        format!(
            "cannot infer the type of named value `${name}` from available constraints; add `type: Video` or `type: Audio`"
        ),
        span.clone(),
    )
}

fn collect_body_dependencies(
    body: &DraftBody,
    definitions: &[ProgramDefinition],
    shadows: &BTreeSet<String>,
    dependencies: &mut BTreeSet<String>,
) {
    for item in &body.items {
        match &item.kind {
            DraftItemKind::Reference(reference) => {
                if !shadows.contains(&reference.value) {
                    dependencies.insert(reference.value.clone());
                }
            }
            DraftItemKind::Invocation(invocation) => {
                collect_invocation_dependencies(invocation, definitions, shadows, dependencies);
            }
        }
    }
}

fn collect_invocation_dependencies(
    invocation: &DraftInvocation,
    definitions: &[ProgramDefinition],
    shadows: &BTreeSet<String>,
    dependencies: &mut BTreeSet<String>,
) {
    for input in invocation.inputs.iter().flatten() {
        match input {
            DraftInput::Reference(reference) => {
                if !shadows.contains(&reference.value) {
                    dependencies.insert(reference.value.clone());
                }
            }
            DraftInput::References(references, _) => {
                for reference in references {
                    if !shadows.contains(&reference.value) {
                        dependencies.insert(reference.value.clone());
                    }
                }
            }
            DraftInput::Body(body) => {
                collect_body_dependencies(body, definitions, shadows, dependencies);
            }
        }
    }

    if let Some(body) = invocation.body.as_deref() {
        let definition = &definitions[invocation.program.index()];
        let mut body_shadows = shadows.clone();
        for input in &definition.descriptor.inputs {
            if matches!(input.cardinality, Cardinality::One) {
                body_shadows.insert(input.name.clone());
            }
        }
        collect_body_dependencies(body, definitions, &body_shadows, dependencies);
    }
}

fn value_local(
    locals: &BTreeMap<String, LocalType>,
    name: &str,
    span: &crate::source::SourceSpan,
) -> Result<ValueType> {
    match locals.get(name) {
        Some(LocalType::Value(value_type)) => Ok(*value_type),
        Some(LocalType::Parameter(_)) => Err(Diagnostic::new(
            "E_PARAMETER_NOT_VALUE",
            format!("parameter `${name}` is not a graph value"),
            span.clone(),
        )),
        Some(LocalType::Inferred { .. }) => Err(Diagnostic::new(
            "E_UNRESOLVED_LOCAL_TYPE",
            format!("named value `${name}` has not finished type inference"),
            span.clone(),
        )),
        None => Err(missing_reference(name, span)),
    }
}

fn validate_parameter_default(parameter: &crate::source::SourceParameter) -> Result<()> {
    let Some(default) = parameter.default.as_ref() else {
        return Ok(());
    };
    super::parameter::from_literal(
        "authored program",
        &parameter.name.value,
        &parameter.parameter_type,
        default,
    )?;
    Ok(())
}

fn missing_reference(name: &str, span: &crate::source::SourceSpan) -> Diagnostic {
    Diagnostic::new(
        "E_MISSING_REFERENCE",
        format!("reference `${name}` does not name a local input, parameter, clip, or id"),
        span.clone(),
    )
}

fn binding_count_error(
    item: &super::draft::DraftItem,
    output_count: usize,
    binding: &str,
    span: &crate::source::SourceSpan,
) -> Diagnostic {
    let construct = match &item.kind {
        DraftItemKind::Reference(_) => "reference",
        DraftItemKind::Invocation(invocation) => &invocation.name.value,
    };
    Diagnostic::new(
        "E_OUTPUT_BINDING_COUNT",
        format!("`{construct}` produces {output_count} value(s), but {binding}"),
        span.clone(),
    )
}
