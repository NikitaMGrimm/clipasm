use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::ValueType;
use crate::program::{
    Cardinality, InputPort, ParameterDescriptor, ParameterType, ProgramDefinition,
    ProgramDescriptor, ProgramId, ProgramImplementation, ProgramRegistry, ResolvedSignature,
    ValueConstraint, ValueTypeSpec, builtin_programs,
};
use crate::source::{
    ArgumentValue, ItemKind, Literal, OutputBindings, ProgramBody, SourcePackage, SourceProgram,
    SourceUnitId,
};

use super::draft::{DraftBody, DraftInput, DraftInvocation, DraftItemKind, DraftParameter};
use super::stack::{
    EvaluationStack, StackBindingInput, StackBindingOutcome, StackBindingPlan, StackCompatibility,
};

#[derive(Clone, Debug)]
pub(super) enum LocalType {
    Value(ValueType),
    Parameter(ParameterType),
    Inferred {
        constraint: ValueConstraint,
        dependencies: BTreeSet<String>,
        span: crate::source::SourceSpan,
    },
}

pub(super) use super::checked::{
    BodyInputId, CheckedBody, CheckedClip, CheckedInputValue, CheckedItem, CheckedItemKind,
    CheckedLocal, CheckedPackage, CheckedParameter, CheckedParameterValue, CheckedProgram,
    CheckedReferenceTarget, ParameterId, ValueLocalId,
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
    let mut external_programs = BTreeMap::new();
    for (index, external) in package.external_programs().iter().enumerate() {
        let external_id = crate::external::ExternalProgramId::new(
            u32::try_from(index).expect("external program catalog fits in u32"),
        );
        let program_id =
            ProgramId::new(u32::try_from(definitions.len()).expect("program catalog fits in u32"));
        definitions.push(external.definition(external_id, format!("external_program_{index}")));
        external_programs.insert(external_id, program_id);
    }
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
                primary_parameter: None,
                type_parameter: None,
                outputs: outputs.into_iter().map(Into::into).collect(),
            },
            implementation: ProgramImplementation::Authored(unit_id),
            body_contract: None,
            postfix: None,
        };
        let id = ProgramId::new(
            u32::try_from(definitions.len()).expect("linked program catalog fits in u32"),
        );
        definitions.push(definition);
        unit_programs.insert(unit_id, id);
        programs.push(Arc::new(checked_program));
    }

    let registry = ProgramRegistry::from_linked(definitions, builtin_count, unit_programs)?;
    Ok(CheckedPackage { registry, programs })
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
        registry,
        programs: vec![Arc::new(program)],
    })
}

#[allow(clippy::too_many_lines)]
fn check_program(
    unit: SourceUnitId,
    program: &SourceProgram,
    definitions: &[ProgramDefinition],
    builtins: &BTreeMap<String, ProgramId>,
    namespace: &BTreeMap<String, ProgramId>,
) -> Result<(Vec<ValueType>, CheckedProgram)> {
    let draft = super::draft::DraftProgram::build(program, definitions, builtins, namespace)?;
    let mut locals = BTreeMap::new();
    for input in program.inputs() {
        insert_local(
            &mut locals,
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
            &mut locals,
            &parameter.name.value,
            LocalType::Parameter(parameter.parameter_type.clone()),
            &parameter.name.span,
        )?;
    }
    for clip in program.clips() {
        insert_local(
            &mut locals,
            &clip.name,
            LocalType::Value(ValueType::Video),
            &clip.span,
        )?;
    }
    for clip in &draft.clips {
        collect_body_names(&clip.body, &mut locals, definitions)?;
    }
    collect_body_names(&draft.body, &mut locals, definitions)?;
    super::infer::infer_local_types(&draft, &mut locals, definitions)?;
    ensure_local_types_resolved(&mut locals, unit, definitions, builtins, namespace)?;

    let mut checked_clips = Vec::with_capacity(draft.clips.len());
    for clip in &draft.clips {
        let (mut stack, mut frame) = EvaluationStack::<ValueType>::isolated(
            format!("named clip `{}` inference", clip.name),
            clip.span.clone(),
        );
        let checked = infer_body(
            &clip.body,
            &locals,
            unit,
            definitions,
            &mut stack,
            &mut frame,
        )?;
        let [output] = stack.values() else {
            return Err(Diagnostic::new(
                "E_CLIP_OUTPUT_COUNT",
                format!(
                    "named clip `{}` must leave exactly one value, but {} values remain",
                    clip.name,
                    stack.len()
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
        checked_clips.push(checked);
    }

    let (mut stack, mut frame) = EvaluationStack::<ValueType>::isolated(
        "authored program inference",
        program.span().clone(),
    );
    let mut checked_body = infer_body(
        &draft.body,
        &locals,
        unit,
        definitions,
        &mut stack,
        &mut frame,
    )?;
    let outputs = stack.values().to_vec();
    let (locals, parameters, body_input_count) = assign_local_ids(
        program,
        &locals,
        definitions,
        &mut checked_clips,
        &mut checked_body,
    )?;
    Ok((
        outputs,
        CheckedProgram {
            span: program.span().clone(),
            stack_access: program.stack_access(),
            inputs: program.inputs().to_vec(),
            locals,
            parameters,
            body_input_count,
            clips: program
                .clips()
                .iter()
                .zip(checked_clips)
                .map(|(clip, body)| CheckedClip {
                    name: clip.name.clone(),
                    span: clip.span.clone(),
                    body,
                })
                .collect(),
            body: checked_body,
        },
    ))
}

fn assign_local_ids(
    program: &SourceProgram,
    local_types: &BTreeMap<String, LocalType>,
    definitions: &[ProgramDefinition],
    clips: &mut [CheckedBody],
    body: &mut CheckedBody,
) -> Result<(Vec<CheckedLocal>, Vec<CheckedParameter>, usize)> {
    let mut locals = Vec::new();
    let mut ids = BTreeMap::new();
    let mut parameter_ids = BTreeMap::new();
    let mut parameters = Vec::with_capacity(program.parameters().len());
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
                super::bind::bind_literal_value(
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
            declared_at: parameter.name.span.clone(),
            default,
        });
    }

    let mut declare = |name: &str, span: &crate::source::SourceSpan| -> Result<ValueLocalId> {
        let value_type = value_local(local_types, name, span)?;
        let id = ValueLocalId(u32::try_from(locals.len()).map_err(|_| {
            Diagnostic::new(
                "E_GRAPH_TOO_LARGE",
                "too many named values were declared",
                span.clone(),
            )
        })?);
        ids.insert(name.to_owned(), id);
        locals.push(CheckedLocal {
            name: name.to_owned(),
            declared_at: span.clone(),
            value_type,
        });
        Ok(id)
    };

    for input in program.inputs() {
        declare(&input.name, program.span())?;
    }
    for clip in program.clips() {
        declare(&clip.name, &clip.span)?;
    }
    for (clip, checked) in program.clips().iter().zip(&mut *clips) {
        assign_body_output_ids(&clip.body, checked, &mut declare)?;
    }
    assign_body_output_ids(program.body(), body, &mut declare)?;

    let mut body_input_count = 0_usize;
    let lexical = BTreeMap::new();
    for (clip, checked) in program.clips().iter().zip(&mut *clips) {
        resolve_body_references(
            &clip.body,
            checked,
            &ids,
            &parameter_ids,
            definitions,
            &lexical,
            &mut body_input_count,
        )?;
    }
    resolve_body_references(
        program.body(),
        body,
        &ids,
        &parameter_ids,
        definitions,
        &lexical,
        &mut body_input_count,
    )?;
    Ok((locals, parameters, body_input_count))
}

fn assign_body_output_ids(
    source: &ProgramBody,
    checked: &mut CheckedBody,
    declare: &mut impl FnMut(&str, &crate::source::SourceSpan) -> Result<ValueLocalId>,
) -> Result<()> {
    debug_assert_eq!(source.items.len(), checked.items.len());
    for (item, checked_item) in source.items.iter().zip(&mut checked.items) {
        checked_item.output_bindings = match &item.output_bindings {
            OutputBindings::None => vec![None; checked_item.output_types.len()],
            OutputBindings::One(name) => vec![Some(declare(&name.value, &name.span)?)],
            OutputBindings::Many(names, _) => names
                .iter()
                .map(|name| declare(&name.value, &name.span).map(Some))
                .collect::<Result<Vec<_>>>()?,
        };
        if let (
            ItemKind::Invocation(invocation),
            CheckedItemKind::Invocation {
                signature,
                inputs,
                body,
                ..
            },
        ) = (&item.kind, &mut checked_item.kind)
        {
            if let (Some(source_body), Some(checked_body)) = (&invocation.body, body.as_deref_mut())
            {
                assign_body_output_ids(source_body, checked_body, declare)?;
            }
            for (port, input) in signature.inputs.iter().zip(inputs) {
                let Some(CheckedInputValue::Body(checked_body, _)) = input else {
                    continue;
                };
                let ArgumentValue::Body(source_body) = invocation
                    .arguments
                    .get(&port.name)
                    .expect("checked input body has a source argument")
                else {
                    unreachable!("checked input body matches canonical source")
                };
                assign_body_output_ids(source_body, checked_body, declare)?;
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
) -> Result<CheckedReferenceTarget> {
    lexical
        .get(name)
        .copied()
        .map(CheckedReferenceTarget::BodyInput)
        .or_else(|| locals.get(name).copied().map(CheckedReferenceTarget::Local))
        .ok_or_else(|| missing_reference(name, span))
}

#[allow(clippy::too_many_lines)]
fn resolve_body_references(
    source: &ProgramBody,
    checked: &mut CheckedBody,
    locals: &BTreeMap<String, ValueLocalId>,
    parameter_ids: &BTreeMap<String, ParameterId>,
    definitions: &[ProgramDefinition],
    lexical: &BTreeMap<String, BodyInputId>,
    body_input_count: &mut usize,
) -> Result<()> {
    debug_assert_eq!(source.items.len(), checked.items.len());
    for (item, checked_item) in source.items.iter().zip(&mut checked.items) {
        match (&item.kind, &mut checked_item.kind) {
            (ItemKind::Reference(reference), CheckedItemKind::Reference { target }) => {
                let resolved = resolve_value_target(
                    &reference.name.value,
                    &reference.name.span,
                    locals,
                    lexical,
                )?;
                *target = Some(resolved);
            }
            (
                ItemKind::Invocation(invocation),
                CheckedItemKind::Invocation {
                    program,
                    signature,
                    inputs,
                    parameters,
                    body,
                    body_input_ids,
                    ..
                },
            ) => {
                let definition = &definitions[program.index()];
                for (index, port) in signature.inputs.iter().enumerate() {
                    let Some(argument) = invocation.arguments.get(&port.name) else {
                        debug_assert!(inputs[index].is_none());
                        continue;
                    };
                    inputs[index] = Some(match argument {
                        ArgumentValue::Reference(reference) => CheckedInputValue::References(
                            vec![resolve_value_target(
                                &reference.value,
                                &reference.span,
                                locals,
                                lexical,
                            )?],
                            reference.span.clone(),
                        ),
                        ArgumentValue::References(references, span) => {
                            CheckedInputValue::References(
                                references
                                    .iter()
                                    .map(|reference| {
                                        resolve_value_target(
                                            &reference.value,
                                            &reference.span,
                                            locals,
                                            lexical,
                                        )
                                    })
                                    .collect::<Result<Vec<_>>>()?,
                                span.clone(),
                            )
                        }
                        ArgumentValue::Body(source_body) => {
                            let Some(CheckedInputValue::Body(mut checked_body, span)) =
                                inputs[index].take()
                            else {
                                unreachable!("checked input body was constructed")
                            };
                            resolve_body_references(
                                source_body,
                                &mut checked_body,
                                locals,
                                parameter_ids,
                                definitions,
                                lexical,
                                body_input_count,
                            )?;
                            CheckedInputValue::Body(checked_body, span)
                        }
                        ArgumentValue::Literal(_) => {
                            unreachable!("validated graph input is not a literal")
                        }
                    });
                }
                *parameters = definition
                    .descriptor
                    .parameters
                    .iter()
                    .map(|descriptor| {
                        let Some(argument) = invocation.arguments.get(&descriptor.name) else {
                            return Ok(None);
                        };
                        let value = match argument {
                            ArgumentValue::Literal(literal) => {
                                CheckedParameterValue::Literal(crate::source::Spanned::new(
                                    super::bind::bind_literal_value(
                                        &definition.descriptor.name,
                                        &descriptor.name,
                                        &descriptor.parameter_type,
                                        literal,
                                    )?,
                                    literal.span().clone(),
                                ))
                            }
                            ArgumentValue::Reference(reference) => {
                                CheckedParameterValue::Reference(
                                    *parameter_ids.get(&reference.value).ok_or_else(|| {
                                        missing_reference(&reference.value, &reference.span)
                                    })?,
                                )
                            }
                            ArgumentValue::References(_, _) | ArgumentValue::Body(_) => {
                                unreachable!("validated scalar parameter representation")
                            }
                        };
                        Ok(Some(value))
                    })
                    .collect::<Result<Vec<_>>>()?;
                if let (Some(source_body), Some(checked_body)) =
                    (&invocation.body, body.as_deref_mut())
                {
                    let mut child_lexical = lexical.clone();
                    for port in &signature.inputs {
                        if !matches!(port.cardinality, Cardinality::One) {
                            continue;
                        }
                        let id = BodyInputId(u32::try_from(*body_input_count).map_err(|_| {
                            Diagnostic::new(
                                "E_GRAPH_TOO_LARGE",
                                "too many lexical body inputs were declared",
                                item.span.clone(),
                            )
                        })?);
                        *body_input_count = body_input_count
                            .checked_add(1)
                            .expect("body input count fits in usize");
                        body_input_ids.insert(port.name.clone(), id);
                        child_lexical.insert(port.name.clone(), id);
                    }
                    resolve_body_references(
                        source_body,
                        checked_body,
                        locals,
                        parameter_ids,
                        definitions,
                        &child_lexical,
                        body_input_count,
                    )?;
                }
            }
            _ => unreachable!("checked item kind matches canonical source"),
        }
    }
    Ok(())
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
                let output_types = item_output_types(item, definitions)?;
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
                let output_types = item_output_types(item, definitions)?;
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
) -> Result<Vec<LocalType>> {
    match &item.kind {
        DraftItemKind::Reference(reference) => Ok(vec![LocalType::Inferred {
            constraint: ValueConstraint::Any,
            dependencies: BTreeSet::from([reference.value.clone()]),
            span: item.span.clone(),
        }]),
        DraftItemKind::Invocation(invocation) => {
            let definition = &definitions[invocation.program.index()];
            let selected = selected_generic_type(definition, invocation)?;
            let mut dependencies = BTreeSet::new();
            collect_invocation_dependencies(
                invocation,
                definitions,
                &BTreeSet::new(),
                &mut dependencies,
            );
            Ok(definition
                .descriptor
                .outputs
                .iter()
                .copied()
                .map(|output| match output {
                    ValueTypeSpec::Exact(value_type) => LocalType::Value(value_type),
                    ValueTypeSpec::Generic => selected.map_or_else(
                        || LocalType::Inferred {
                            constraint: definition
                                .descriptor
                                .type_parameter
                                .as_ref()
                                .expect("generic output has a type parameter")
                                .constraint,
                            dependencies: dependencies.clone(),
                            span: item.span.clone(),
                        },
                        LocalType::Value,
                    ),
                })
                .collect())
        }
    }
}

fn selected_generic_type(
    definition: &ProgramDefinition,
    invocation: &DraftInvocation,
) -> Result<Option<ValueType>> {
    let Some(type_parameter) = &definition.descriptor.type_parameter else {
        return Ok(None);
    };
    let index = definition
        .descriptor
        .parameters
        .iter()
        .position(|parameter| parameter.name == type_parameter.selector)
        .expect("validated type selector exists");
    invocation.parameters[index]
        .as_ref()
        .map(|argument| match argument {
            DraftParameter::Literal(Literal::String(value, _)) if value == "Video" => {
                Ok(ValueType::Video)
            }
            DraftParameter::Literal(Literal::String(value, _)) if value == "Audio" => {
                Ok(ValueType::Audio)
            }
            DraftParameter::Literal(literal) => Err(Diagnostic::new(
                "E_INVALID_ARGUMENT_VALUE",
                format!(
                    "parameter `{}.{}` must be `Video` or `Audio`",
                    definition.descriptor.name, type_parameter.selector
                ),
                literal.span().clone(),
            )),
            DraftParameter::Reference(reference) => Err(Diagnostic::new(
                "E_INVALID_ARGUMENT_VALUE",
                format!(
                    "parameter `{}.{}` must be `Video` or `Audio`",
                    definition.descriptor.name, type_parameter.selector
                ),
                reference.span.clone(),
            )),
        })
        .transpose()
}

#[allow(clippy::too_many_lines)]
fn resolve_invocation_signature(
    definition: &ProgramDefinition,
    invocation: &DraftInvocation,
    validated: &ValidatedArguments,
    stack: &EvaluationStack<ValueType>,
    frame: &super::stack::StackFrame,
    access: crate::program::StackAccess,
    span: &crate::source::SourceSpan,
) -> Result<Option<ResolvedSignature>> {
    let descriptor = &definition.descriptor;
    let Some(type_parameter) = &descriptor.type_parameter else {
        let signature = descriptor.resolve_signature(None);
        validate_explicit_input_types(invocation, validated, &signature)?;
        return Ok(Some(signature));
    };

    let selected = selected_generic_type(definition, invocation)?;

    let mut explicit = Vec::new();
    for (index, port) in descriptor.inputs.iter().enumerate() {
        if !matches!(port.value_type, ValueTypeSpec::Generic) {
            continue;
        }
        if let Some(values) = &validated.input_types[index] {
            for value_type in values {
                if !explicit.contains(value_type) {
                    explicit.push(*value_type);
                }
            }
        }
    }
    if explicit.len() > 1 {
        return Err(Diagnostic::new(
            "E_GENERIC_TYPE_MISMATCH",
            format!(
                "program `{}` requires all generic inputs to have one type, but found {}",
                descriptor.name,
                explicit
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" and ")
            ),
            span.clone(),
        ));
    }

    let inferred = explicit.first().copied().or_else(|| {
        let generic_ports = descriptor
            .inputs
            .iter()
            .enumerate()
            .filter(|(_, port)| matches!(port.value_type, ValueTypeSpec::Generic))
            .collect::<Vec<_>>();
        let (_, generic_port) = generic_ports.first()?;
        match generic_port.cardinality {
            Cardinality::One if generic_ports.len() == 1 => stack
                .nearest_accessible_type(frame, access, |value_type| {
                    type_parameter.constraint.accepts(value_type)
                })
                .or_else(|| {
                    (access == crate::program::StackAccess::Owned).then(|| {
                        stack.nearest_accessible_type(
                            frame,
                            crate::program::StackAccess::Visible,
                            |value_type| type_parameter.constraint.accepts(value_type),
                        )
                    })?
                }),
            Cardinality::One => {
                let missing = generic_ports
                    .iter()
                    .filter(|(index, _)| invocation.inputs[*index].is_none())
                    .count();
                let types = stack.accessible_types(frame, access, |value_type| {
                    type_parameter.constraint.accepts(value_type)
                });
                let viable = types
                    .iter()
                    .copied()
                    .filter(|value_type| {
                        stack.accessible_count(frame, access, *value_type) >= missing
                    })
                    .collect::<Vec<_>>();
                if viable.len() == 1 {
                    Some(viable[0])
                } else if viable.is_empty() {
                    stack.nearest_accessible_type(frame, access, |value_type| {
                        type_parameter.constraint.accepts(value_type)
                    })
                } else {
                    None
                }
            }
            Cardinality::Variadic { .. } => {
                let types = stack.accessible_types(frame, access, |value_type| {
                    type_parameter.constraint.accepts(value_type)
                });
                (types.len() == 1).then_some(types[0])
            }
        }
    });

    let value_type = match (selected, inferred) {
        (Some(selected), Some(inferred)) if selected != inferred => {
            return Err(Diagnostic::new(
                "E_GENERIC_TYPE_MISMATCH",
                format!(
                    "program `{}` selects {selected}, but its explicit or nearest input is {inferred}",
                    descriptor.name
                ),
                span.clone(),
            ));
        }
        (Some(selected), _) => selected,
        (None, Some(inferred)) => inferred,
        (None, None) => {
            let types = stack.accessible_types(frame, access, |candidate| {
                type_parameter.constraint.accepts(candidate)
            });
            if types.len() > 1 {
                return Err(Diagnostic::new(
                    "E_AMBIGUOUS_GENERIC_TYPE",
                    format!(
                        "program `{}` can bind both Video and Audio; set `type: Video` or `type: Audio`",
                        descriptor.name
                    ),
                    span.clone(),
                ));
            }
            if matches!(definition.implementation, ProgramImplementation::Body(_))
                && !descriptor
                    .inputs
                    .iter()
                    .any(|port| matches!(port.value_type, ValueTypeSpec::Generic))
            {
                return Ok(None);
            }
            return Err(Diagnostic::new(
                "E_STACK_UNDERFLOW",
                format!(
                    "program `{}` needs a preceding Video or Audio value",
                    descriptor.name
                ),
                span.clone(),
            ));
        }
    };
    if !type_parameter.constraint.accepts(value_type) {
        return Err(Diagnostic::new(
            "E_TYPE_MISMATCH",
            format!("program `{}` does not accept {value_type}", descriptor.name),
            span.clone(),
        ));
    }
    let signature = descriptor.resolve_signature(Some(value_type));
    validate_explicit_input_types(invocation, validated, &signature)?;
    Ok(Some(signature))
}

fn validate_explicit_input_types(
    invocation: &DraftInvocation,
    validated: &ValidatedArguments,
    signature: &ResolvedSignature,
) -> Result<()> {
    for (index, port) in signature.inputs.iter().enumerate() {
        let Some(values) = &validated.input_types[index] else {
            continue;
        };
        for value in values {
            if *value == port.value_type
                || port.allow_adaptation
                    && matches!(
                        (*value, port.value_type),
                        (ValueType::Video, ValueType::Audio) | (ValueType::Audio, ValueType::Video)
                    )
            {
                continue;
            }
            return Err(Diagnostic::new(
                "E_TYPE_MISMATCH",
                format!(
                    "program `{}` input `{}` expected {}, but found {value}",
                    invocation.name.value, port.name, port.value_type
                ),
                invocation.inputs[index]
                    .as_ref()
                    .expect("validated explicit input")
                    .span()
                    .clone(),
            ));
        }
    }
    Ok(())
}

fn output_names(bindings: &OutputBindings, count: usize) -> Vec<Option<String>> {
    match bindings {
        OutputBindings::None => vec![None; count],
        OutputBindings::One(name) => vec![Some(name.value.clone())],
        OutputBindings::Many(names, _) => {
            names.iter().map(|name| Some(name.value.clone())).collect()
        }
    }
}

fn checked_stack_plan(
    program: &str,
    signature: &ResolvedSignature,
    invocation: &DraftInvocation,
    stack: &EvaluationStack<ValueType>,
    frame: &super::stack::StackFrame,
    access: crate::program::StackAccess,
    span: &crate::source::SourceSpan,
) -> Result<StackBindingPlan> {
    let missing = signature
        .inputs
        .iter()
        .enumerate()
        .filter(|(index, _)| invocation.inputs[*index].is_none())
        .map(|(index, port)| StackBindingInput {
            port: index,
            requirement: port.value_type,
            cardinality: port.cardinality,
        })
        .collect::<Vec<_>>();
    match stack.plan_bindings(frame, access, &missing, |value, required| {
        if value == required {
            StackCompatibility::Definite
        } else {
            StackCompatibility::Incompatible
        }
    }) {
        StackBindingOutcome::Resolved(plan) => Ok(plan),
        StackBindingOutcome::Deferred => {
            unreachable!("concrete stack compatibility is never deferred")
        }
        StackBindingOutcome::Impossible(failure) => {
            let port = &signature.inputs[failure.port];
            let (code, requirement) = match port.cardinality {
                Cardinality::One => (
                    "E_STACK_UNDERFLOW",
                    format!(
                        "`{program}.{}` needs one preceding {} value",
                        port.name, port.value_type
                    ),
                ),
                Cardinality::Variadic { min } => (
                    "E_MISSING_REQUIRED_INPUT",
                    format!(
                        "`{program}.{}` needs at least {min} {} value(s)",
                        port.name, port.value_type
                    ),
                ),
            };
            Err(stack.underflow(
                frame,
                access,
                code,
                &requirement,
                port.value_type,
                failure.available,
                &failure.selected,
                span,
            ))
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn infer_body(
    body: &DraftBody,
    locals: &BTreeMap<String, LocalType>,
    unit: SourceUnitId,
    definitions: &[ProgramDefinition],
    stack: &mut EvaluationStack<ValueType>,
    frame: &mut super::stack::StackFrame,
) -> Result<CheckedBody> {
    let mut checked_items = Vec::with_capacity(body.items.len());
    for item in &body.items {
        let checked = match &item.kind {
            DraftItemKind::Reference(reference) => {
                let output = value_local(locals, &reference.value, &reference.span)?;
                stack.extend(frame, [output]);
                CheckedItem {
                    span: item.span.clone(),
                    construct: "reference".to_owned(),
                    output_names: output_names(&item.output_bindings, 1),
                    output_types: vec![output],
                    output_bindings: Vec::new(),
                    kind: CheckedItemKind::Reference { target: None },
                }
            }
            DraftItemKind::Invocation(invocation) => {
                let program = invocation.program;
                let definition = &definitions[program.index()];
                let validated =
                    validate_explicit_arguments(invocation, definition, locals, unit, definitions)?;
                let access = invocation.access;
                let mut signature = resolve_invocation_signature(
                    definition, invocation, &validated, stack, frame, access, &item.span,
                )?;
                let stack_plan = if let Some(resolved) = &signature {
                    let plan = checked_stack_plan(
                        &invocation.name.value,
                        resolved,
                        invocation,
                        stack,
                        frame,
                        access,
                        &item.span,
                    )?;
                    stack.apply_binding_plan(&plan);
                    plan
                } else {
                    StackBindingPlan { inputs: Vec::new() }
                };

                let checked_body = match definition.implementation {
                    ProgramImplementation::Direct(_)
                    | ProgramImplementation::Authored(_)
                    | ProgramImplementation::External(_) => None,
                    ProgramImplementation::Body(_) => {
                        let body = invocation.body.as_deref().expect("draft body program");
                        let contract = definition
                            .body_contract
                            .as_ref()
                            .expect("validated body program contract");
                        let mut child = EvaluationStack::<ValueType>::enter_body(
                            frame,
                            access,
                            invocation.name.value.clone(),
                            invocation.name.span.clone(),
                        );
                        let initial_values = signature.as_ref().map_or_else(
                            || {
                                contract.exact_initial_values().expect(
                                    "body-inferred generic contracts cannot require generic initial values",
                                )
                            },
                            |resolved| contract.resolve(resolved.generic).initial_values,
                        );
                        stack.extend(&child, initial_values);
                        let mut body_locals = locals.clone();
                        if let Some(resolved) = &signature {
                            for port in &resolved.inputs {
                                if matches!(port.cardinality, Cardinality::One) {
                                    body_locals.insert(
                                        port.name.clone(),
                                        LocalType::Value(port.value_type),
                                    );
                                }
                            }
                        }
                        let checked_body =
                            infer_body(body, &body_locals, unit, definitions, stack, &mut child)?;
                        let body_outputs = stack.finish_body(&child);
                        if signature.is_none() {
                            let type_parameter = definition
                                .descriptor
                                .type_parameter
                                .as_ref()
                                .expect("deferred body signature is generic");
                            let value_type = infer_body_generic_type(
                                &invocation.name.value,
                                &body_outputs,
                                type_parameter.constraint,
                                contract.count_error_code,
                                &body.span,
                            )?;
                            signature =
                                Some(definition.descriptor.resolve_signature(Some(value_type)));
                        }
                        let resolved_contract = contract
                            .resolve(signature.as_ref().expect("body signature resolved").generic);
                        validate_body_outputs(
                            &invocation.name.value,
                            &body_outputs,
                            &resolved_contract.outputs,
                            contract.count_error_code,
                            &body.span,
                        )?;
                        Some(Box::new(checked_body))
                    }
                };
                let signature = signature.expect("invocation signature resolved");
                let output_types = signature.outputs.clone();
                stack.extend(frame, output_types.iter().copied());
                let inputs = validated
                    .input_bodies
                    .into_iter()
                    .zip(&invocation.inputs)
                    .map(|(body, input)| {
                        body.map(|body| {
                            CheckedInputValue::Body(
                                Box::new(body),
                                input.as_ref().expect("checked input body").span().clone(),
                            )
                        })
                    })
                    .collect();
                CheckedItem {
                    span: item.span.clone(),
                    construct: invocation.name.value.clone(),
                    output_names: output_names(&item.output_bindings, output_types.len()),
                    output_types,
                    output_bindings: Vec::new(),
                    kind: CheckedItemKind::Invocation {
                        program,
                        signature,
                        access,
                        stack_plan,
                        inputs,
                        parameters: Vec::new(),
                        body: checked_body,
                        body_input_ids: BTreeMap::new(),
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

struct ValidatedArguments {
    input_bodies: Vec<Option<CheckedBody>>,
    input_types: Vec<Option<Vec<ValueType>>>,
}

fn validate_explicit_arguments(
    invocation: &DraftInvocation,
    definition: &ProgramDefinition,
    locals: &BTreeMap<String, LocalType>,
    unit: SourceUnitId,
    definitions: &[ProgramDefinition],
) -> Result<ValidatedArguments> {
    let mut input_bodies = Vec::with_capacity(invocation.inputs.len());
    let mut input_types = Vec::with_capacity(invocation.inputs.len());
    for (port, argument) in definition.descriptor.inputs.iter().zip(&invocation.inputs) {
        let Some(argument) = argument else {
            input_bodies.push(None);
            input_types.push(None);
            continue;
        };
        let (values, body) =
            validate_input_argument(invocation, port, argument, locals, unit, definitions)?;
        input_bodies.push(body);
        input_types.push(Some(values));
    }
    for (parameter, argument) in definition
        .descriptor
        .parameters
        .iter()
        .zip(&invocation.parameters)
    {
        if let Some(argument) = argument {
            validate_parameter_argument(&invocation.name.value, parameter, argument, locals)?;
        }
    }
    Ok(ValidatedArguments {
        input_bodies,
        input_types,
    })
}

fn validate_input_argument(
    invocation: &DraftInvocation,
    port: &InputPort,
    argument: &DraftInput,
    locals: &BTreeMap<String, LocalType>,
    unit: SourceUnitId,
    definitions: &[ProgramDefinition],
) -> Result<(Vec<ValueType>, Option<CheckedBody>)> {
    let mut checked_body = None;
    let values = match argument {
        DraftInput::Reference(reference) => {
            vec![value_local(locals, &reference.value, &reference.span)?]
        }
        DraftInput::References(references, _) => references
            .iter()
            .map(|reference| value_local(locals, &reference.value, &reference.span))
            .collect::<Result<Vec<_>>>()?,
        DraftInput::Body(body) => {
            let (mut stack, mut frame) = EvaluationStack::<ValueType>::isolated(
                format!(
                    "inline input body for `{}.{}` inference",
                    invocation.name.value, port.name
                ),
                body.span.clone(),
            );
            let checked = infer_body(body, locals, unit, definitions, &mut stack, &mut frame)?;
            checked_body = Some(checked);
            let [value] = stack.values() else {
                return Err(Diagnostic::new(
                    "E_INPUT_BODY_OUTPUT_COUNT",
                    format!(
                        "inline input body for `{}.{}` must leave exactly one value, but {} values remain",
                        invocation.name.value,
                        port.name,
                        stack.len()
                    ),
                    body.span.clone(),
                ));
            };
            vec![*value]
        }
    };
    Ok((values, checked_body))
}

fn validate_parameter_argument(
    program: &str,
    parameter: &ParameterDescriptor,
    argument: &DraftParameter,
    locals: &BTreeMap<String, LocalType>,
) -> Result<()> {
    match argument {
        DraftParameter::Literal(literal) => {
            if literal_matches(&parameter.parameter_type, literal) {
                Ok(())
            } else {
                Err(Diagnostic::new(
                    "E_INVALID_ARGUMENT_TYPE",
                    format!(
                        "parameter `{program}.{}` has the wrong value type",
                        parameter.name
                    ),
                    literal.span().clone(),
                ))
            }
        }
        DraftParameter::Reference(reference) => match locals.get(&reference.value) {
            Some(LocalType::Parameter(actual)) if actual == &parameter.parameter_type => Ok(()),
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

fn infer_body_generic_type(
    program: &str,
    values: &[ValueType],
    constraint: crate::program::ValueConstraint,
    count_error_code: &'static str,
    span: &crate::source::SourceSpan,
) -> Result<ValueType> {
    let Some(first) = values.first().copied() else {
        return Err(Diagnostic::new(
            count_error_code,
            format!("`{program}` body must produce at least one Video or Audio value"),
            span.clone(),
        ));
    };
    if !constraint.accepts(first) {
        return Err(Diagnostic::new(
            "E_TYPE_MISMATCH",
            format!("`{program}` body does not accept {first}"),
            span.clone(),
        ));
    }
    if let Some(other) = values.iter().copied().find(|value| *value != first) {
        return Err(Diagnostic::new(
            "E_GENERIC_TYPE_MISMATCH",
            format!("`{program}` body cannot mix {first} and {other}"),
            span.clone(),
        ));
    }
    Ok(first)
}

fn validate_body_outputs(
    program: &str,
    values: &[ValueType],
    constraint: &crate::program::ResolvedBodyOutputConstraint,
    count_error_code: &'static str,
    span: &crate::source::SourceSpan,
) -> Result<()> {
    match constraint {
        crate::program::ResolvedBodyOutputConstraint::Exactly(expected) => {
            if values.len() != expected.len() {
                return Err(Diagnostic::new(
                    count_error_code,
                    format!(
                        "`{program}` body must leave exactly {} value(s), but {} values remain",
                        expected.len(),
                        values.len()
                    ),
                    span.clone(),
                ));
            }
            for (index, (actual, expected)) in values.iter().zip(expected).enumerate() {
                if actual != expected {
                    return Err(Diagnostic::new(
                        "E_TYPE_MISMATCH",
                        format!(
                            "`{program}` body output {} expected {expected}, but found {actual}",
                            index + 1
                        ),
                        span.clone(),
                    ));
                }
            }
        }
        crate::program::ResolvedBodyOutputConstraint::Variadic { value_type, min } => {
            if values.len() < *min {
                return Err(Diagnostic::new(
                    count_error_code,
                    format!("`{program}` body must produce at least {min} {value_type}"),
                    span.clone(),
                ));
            }
            for actual in values {
                if actual != value_type {
                    return Err(Diagnostic::new(
                        "E_TYPE_MISMATCH",
                        format!(
                            "`{program}` body expected only {value_type} values, but found {actual}"
                        ),
                        span.clone(),
                    ));
                }
            }
        }
    }
    Ok(())
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

fn ensure_local_types_resolved(
    locals: &mut BTreeMap<String, LocalType>,
    _unit: SourceUnitId,
    _definitions: &[ProgramDefinition],
    _builtins: &BTreeMap<String, ProgramId>,
    _namespace: &BTreeMap<String, ProgramId>,
) -> Result<()> {
    struct Frame {
        name: String,
        dependencies: Vec<String>,
        next: usize,
    }

    let unresolved = locals
        .iter()
        .filter_map(|(name, local)| match local {
            LocalType::Inferred {
                dependencies, span, ..
            } => Some((name.clone(), (dependencies.clone(), span.clone()))),
            LocalType::Value(_) | LocalType::Parameter(_) => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut states = BTreeMap::<String, u8>::new();

    for root in unresolved.keys() {
        if states.get(root).copied().unwrap_or(0) != 0 {
            continue;
        }
        let mut path = vec![root.clone()];
        let mut positions = BTreeMap::from([(root.clone(), 0_usize)]);
        let mut stack = vec![Frame {
            name: root.clone(),
            dependencies: unresolved[root].0.iter().cloned().collect(),
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
                return Err(missing_reference(&dependency, &unresolved[&frame.name].1));
            }
            if !unresolved.contains_key(&dependency) {
                continue;
            }
            match states.get(&dependency).copied().unwrap_or(0) {
                0 => {
                    states.insert(dependency.clone(), 1);
                    positions.insert(dependency.clone(), path.len());
                    path.push(dependency.clone());
                    stack.push(Frame {
                        name: dependency.clone(),
                        dependencies: unresolved[&dependency].0.iter().cloned().collect(),
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
                        unresolved[&dependency].1.clone(),
                    ));
                }
                2 => {}
                _ => unreachable!("inference dependency state is closed"),
            }
        }
    }

    if let Some((name, (_, span))) = unresolved.first_key_value() {
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
    super::bind::bind_literal_value(
        "authored program",
        &parameter.name.value,
        &parameter.parameter_type,
        default,
    )?;
    Ok(())
}

fn literal_matches(parameter_type: &ParameterType, literal: &Literal) -> bool {
    matches!(
        (parameter_type, literal),
        (ParameterType::Integer, Literal::Integer(_, _))
            | (ParameterType::File, Literal::File(_, _))
            | (
                ParameterType::File
                    | ParameterType::Duration
                    | ParameterType::TimeRange
                    | ParameterType::Keyword(_),
                Literal::String(_, _)
            )
    )
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
