use std::collections::BTreeMap;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::ValueType;
use crate::program::{
    BodyOutputConstraint, Cardinality, InputPort, ParameterDescriptor, ParameterType,
    ProgramDefinition, ProgramDescriptor, ProgramId, ProgramImplementation, ProgramRegistry,
    builtin_programs,
};
use crate::source::{
    ArgumentValue, Item, ItemKind, Literal, OutputBindings, ProgramBody, SourcePackage,
    SourceProgram, SourceUnitId,
};

use super::stack::EvaluationStack;

#[derive(Clone, Debug)]
enum LocalType {
    Value(ValueType),
    Parameter(ParameterType),
    Alias(String),
}

pub(super) fn build_catalog(package: &SourcePackage) -> Result<ProgramRegistry> {
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
    let mut unit_programs = BTreeMap::new();
    let mut namespaces = BTreeMap::new();

    for (index, unit) in package.units().iter().enumerate() {
        let unit_id = SourceUnitId(index);
        let namespace = unit
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
        let outputs = infer_outputs(
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
                validate_literal_default(
                    &parameter.parameter_type,
                    parameter.default.as_ref(),
                    &parameter.name.value,
                )?;
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
                outputs,
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
        namespaces.insert(unit_id, namespace);
    }

    ProgramRegistry::from_linked(definitions, builtin_count, namespaces)
}

fn infer_outputs(
    unit: SourceUnitId,
    program: &SourceProgram,
    definitions: &[ProgramDefinition],
    builtins: &BTreeMap<String, ProgramId>,
    namespace: &BTreeMap<String, ProgramId>,
) -> Result<Vec<ValueType>> {
    let mut locals = BTreeMap::new();
    for input in program.inputs() {
        insert_local(
            &mut locals,
            &input.name,
            LocalType::Value(input.value_type),
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
    for clip in program.clips() {
        collect_body_names(
            &clip.body,
            &mut locals,
            unit,
            definitions,
            builtins,
            namespace,
        )?;
    }
    collect_body_names(
        &program.body,
        &mut locals,
        unit,
        definitions,
        builtins,
        namespace,
    )?;
    resolve_local_types(&mut locals)?;

    for clip in program.clips() {
        let (mut stack, mut frame) = EvaluationStack::<ValueType>::isolated(
            format!("named clip `{}` inference", clip.name),
            clip.span.clone(),
        );
        infer_body(
            &clip.body,
            &locals,
            unit,
            definitions,
            builtins,
            namespace,
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
    }

    let (mut stack, mut frame) = EvaluationStack::<ValueType>::isolated(
        "authored program inference",
        program.span().clone(),
    );
    infer_body(
        &program.body,
        &locals,
        unit,
        definitions,
        builtins,
        namespace,
        &mut stack,
        &mut frame,
    )?;
    Ok(stack.values().to_vec())
}

fn collect_body_names(
    body: &ProgramBody,
    locals: &mut BTreeMap<String, LocalType>,
    unit: SourceUnitId,
    definitions: &[ProgramDefinition],
    builtins: &BTreeMap<String, ProgramId>,
    namespace: &BTreeMap<String, ProgramId>,
) -> Result<()> {
    for item in &body.items {
        match &item.output_bindings {
            OutputBindings::None => {}
            OutputBindings::One(name) => {
                let output_types = item_output_types(item, unit, definitions, builtins, namespace)?;
                let [output] = output_types.as_slice() else {
                    return Err(binding_count_error(
                        item,
                        output_types.len(),
                        "`id`",
                        &name.span,
                    ));
                };
                insert_local(locals, &name.value, output.clone(), &name.span)?;
            }
            OutputBindings::Many(names, span) => {
                let output_types = item_output_types(item, unit, definitions, builtins, namespace)?;
                if output_types.len() <= 1 || output_types.len() != names.len() {
                    return Err(binding_count_error(item, output_types.len(), "`ids`", span));
                }
                for (name, output) in names.iter().zip(output_types) {
                    insert_local(locals, &name.value, output, &name.span)?;
                }
            }
        }
        if let ItemKind::Invocation(invocation) = &item.kind {
            if let Some(body) = &invocation.body {
                collect_body_names(body, locals, unit, definitions, builtins, namespace)?;
            }
            for argument in invocation.arguments.values() {
                if let ArgumentValue::Body(body) = argument {
                    collect_body_names(body, locals, unit, definitions, builtins, namespace)?;
                }
            }
        }
    }
    Ok(())
}

fn item_output_types(
    item: &Item,
    unit: SourceUnitId,
    definitions: &[ProgramDefinition],
    builtins: &BTreeMap<String, ProgramId>,
    namespace: &BTreeMap<String, ProgramId>,
) -> Result<Vec<LocalType>> {
    match &item.kind {
        ItemKind::Reference(reference) => Ok(vec![LocalType::Alias(reference.name.value.clone())]),
        ItemKind::Invocation(invocation) => definition_for(
            unit,
            &invocation.program.value,
            definitions,
            builtins,
            namespace,
            &invocation.program.span,
        )
        .map(|definition| {
            definition
                .descriptor
                .outputs
                .iter()
                .copied()
                .map(LocalType::Value)
                .collect()
        }),
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn infer_body(
    body: &ProgramBody,
    locals: &BTreeMap<String, LocalType>,
    unit: SourceUnitId,
    definitions: &[ProgramDefinition],
    builtins: &BTreeMap<String, ProgramId>,
    namespace: &BTreeMap<String, ProgramId>,
    stack: &mut EvaluationStack<ValueType>,
    frame: &mut super::stack::StackFrame,
) -> Result<()> {
    for item in &body.items {
        match &item.kind {
            ItemKind::Reference(reference) => {
                stack.extend([value_local(
                    locals,
                    &reference.name.value,
                    &reference.name.span,
                )?]);
            }
            ItemKind::Invocation(invocation) => {
                let definition = definition_for(
                    unit,
                    &invocation.program.value,
                    definitions,
                    builtins,
                    namespace,
                    &invocation.program.span,
                )?;
                validate_explicit_arguments(
                    invocation,
                    definition,
                    locals,
                    unit,
                    definitions,
                    builtins,
                    namespace,
                )?;
                let access = invocation
                    .stack_access
                    .as_ref()
                    .map_or(definition.descriptor.default_stack_access, |access| {
                        access.value
                    });
                let explicit_inputs = definition
                    .descriptor
                    .inputs
                    .iter()
                    .filter(|port| invocation.arguments.contains_key(&port.name))
                    .count();
                let missing = definition.descriptor.inputs.len() - explicit_inputs;
                if missing > 0 {
                    if definition
                        .descriptor
                        .inputs
                        .iter()
                        .any(|port| matches!(port.cardinality, Cardinality::Variadic { .. }))
                    {
                        let port = &definition.descriptor.inputs[0];
                        let Cardinality::Variadic { min } = port.cardinality else {
                            unreachable!("validated variadic descriptor")
                        };
                        let values = stack.take_variadic(
                            frame,
                            access,
                            min,
                            &invocation.program.value,
                            &port.name,
                            &item.span,
                        )?;
                        ensure_types(&values, port, &item.span)?;
                    } else {
                        let values = stack.take_fixed(
                            frame,
                            access,
                            missing,
                            &invocation.program.value,
                            &item.span,
                        )?;
                        for (port, value) in definition
                            .descriptor
                            .inputs
                            .iter()
                            .filter(|port| !invocation.arguments.contains_key(&port.name))
                            .zip(values)
                        {
                            if value != port.value_type {
                                return Err(type_error(
                                    &invocation.program.value,
                                    port,
                                    value,
                                    &item.span,
                                ));
                            }
                        }
                    }
                }

                match definition.implementation {
                    ProgramImplementation::Direct(_) | ProgramImplementation::Authored(_) => {
                        if invocation.body.is_some() {
                            return Err(Diagnostic::new(
                                "E_UNEXPECTED_PROGRAM_BODY",
                                format!(
                                    "program `{}` does not accept a caller-supplied body",
                                    invocation.program.value
                                ),
                                invocation.program.span.clone(),
                            ));
                        }
                    }
                    ProgramImplementation::Body(_) => {
                        let body = invocation.body.as_ref().ok_or_else(|| {
                            Diagnostic::new(
                                "E_MISSING_PROGRAM_BODY",
                                format!(
                                    "body program `{}` requires a `body`",
                                    invocation.program.value
                                ),
                                invocation.program.span.clone(),
                            )
                        })?;
                        let contract = definition
                            .body_contract
                            .as_ref()
                            .expect("validated body program contract");
                        let mut child = stack.enter_body(
                            frame,
                            access,
                            invocation.program.value.clone(),
                            invocation.program.span.clone(),
                        );
                        stack.extend(contract.initial_values.iter().copied());
                        infer_body(
                            body,
                            locals,
                            unit,
                            definitions,
                            builtins,
                            namespace,
                            stack,
                            &mut child,
                        )?;
                        let body_outputs = stack.finish_body(frame, child);
                        validate_body_outputs(
                            &invocation.program.value,
                            &body_outputs,
                            &contract.outputs,
                            contract.count_error_code,
                            &body.span,
                        )?;
                    }
                }
                stack.extend(definition.descriptor.outputs.iter().copied());
            }
        }
    }
    Ok(())
}

fn validate_explicit_arguments(
    invocation: &crate::source::Invocation,
    definition: &ProgramDefinition,
    locals: &BTreeMap<String, LocalType>,
    unit: SourceUnitId,
    definitions: &[ProgramDefinition],
    builtins: &BTreeMap<String, ProgramId>,
    namespace: &BTreeMap<String, ProgramId>,
) -> Result<()> {
    for (name, argument) in &invocation.arguments {
        if let Some(port) = definition
            .descriptor
            .inputs
            .iter()
            .find(|port| port.name == *name)
        {
            validate_input_argument(
                invocation,
                port,
                argument,
                locals,
                unit,
                definitions,
                builtins,
                namespace,
            )?;
        } else if let Some(parameter) = definition
            .descriptor
            .parameters
            .iter()
            .find(|parameter| parameter.name == *name)
        {
            validate_parameter_argument(&invocation.program.value, parameter, argument, locals)?;
        } else {
            return Err(Diagnostic::new(
                "E_UNKNOWN_PROGRAM_ARGUMENT",
                format!(
                    "unknown argument `{name}` for program `{}`",
                    invocation.program.value
                ),
                argument.span().clone(),
            ));
        }
    }
    for parameter in &definition.descriptor.parameters {
        if parameter.required && !invocation.arguments.contains_key(&parameter.name) {
            return Err(Diagnostic::new(
                "E_MISSING_ARGUMENT",
                format!(
                    "missing required parameter `{}.{}`",
                    invocation.program.value, parameter.name
                ),
                invocation.program.span.clone(),
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_input_argument(
    invocation: &crate::source::Invocation,
    port: &InputPort,
    argument: &ArgumentValue,
    locals: &BTreeMap<String, LocalType>,
    unit: SourceUnitId,
    definitions: &[ProgramDefinition],
    builtins: &BTreeMap<String, ProgramId>,
    namespace: &BTreeMap<String, ProgramId>,
) -> Result<()> {
    if matches!(port.cardinality, Cardinality::Variadic { .. })
        && !matches!(
            argument,
            ArgumentValue::Reference(_) | ArgumentValue::References(_, _)
        )
    {
        return Err(Diagnostic::new(
            "E_INVALID_ARGUMENT_TYPE",
            format!(
                "explicit variadic input `{}.{}` must use `$name` references",
                invocation.program.value, port.name
            ),
            argument.span().clone(),
        ));
    }

    let values = match argument {
        ArgumentValue::Reference(reference) => {
            vec![value_local(locals, &reference.value, &reference.span)?]
        }
        ArgumentValue::References(references, _) => references
            .iter()
            .map(|reference| value_local(locals, &reference.value, &reference.span))
            .collect::<Result<Vec<_>>>()?,
        ArgumentValue::Body(body) => {
            let (mut stack, mut frame) = EvaluationStack::<ValueType>::isolated(
                format!(
                    "inline input body for `{}.{}` inference",
                    invocation.program.value, port.name
                ),
                body.span.clone(),
            );
            infer_body(
                body,
                locals,
                unit,
                definitions,
                builtins,
                namespace,
                &mut stack,
                &mut frame,
            )?;
            let [value] = stack.values() else {
                return Err(Diagnostic::new(
                    "E_INPUT_BODY_OUTPUT_COUNT",
                    format!(
                        "inline input body for `{}.{}` must leave exactly one value, but {} values remain",
                        invocation.program.value,
                        port.name,
                        stack.len()
                    ),
                    body.span.clone(),
                ));
            };
            vec![*value]
        }
        ArgumentValue::Literal(_) => {
            return Err(Diagnostic::new(
                "E_INVALID_ARGUMENT_TYPE",
                format!(
                    "input `{}.{}` requires a graph value",
                    invocation.program.value, port.name
                ),
                argument.span().clone(),
            ));
        }
    };

    match port.cardinality {
        Cardinality::One if values.len() != 1 => {
            return Err(Diagnostic::new(
                "E_INVALID_ARGUMENT_TYPE",
                format!(
                    "input `{}.{}` requires exactly one value",
                    invocation.program.value, port.name
                ),
                argument.span().clone(),
            ));
        }
        Cardinality::Variadic { min } if values.len() < min => {
            return Err(Diagnostic::new(
                "E_MISSING_REQUIRED_INPUT",
                format!(
                    "input `{}.{}` requires at least {min} value(s)",
                    invocation.program.value, port.name
                ),
                argument.span().clone(),
            ));
        }
        _ => {}
    }
    ensure_types(&values, port, argument.span())
}

fn validate_body_outputs(
    program: &str,
    values: &[ValueType],
    constraint: &BodyOutputConstraint,
    count_error_code: &'static str,
    span: &crate::source::SourceSpan,
) -> Result<()> {
    match constraint {
        BodyOutputConstraint::Exactly(expected) => {
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
        BodyOutputConstraint::Variadic { value_type, min } => {
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

fn validate_parameter_argument(
    program: &str,
    parameter: &ParameterDescriptor,
    argument: &ArgumentValue,
    locals: &BTreeMap<String, LocalType>,
) -> Result<()> {
    match argument {
        ArgumentValue::Literal(literal) => {
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
        ArgumentValue::Reference(reference) => match locals.get(&reference.value) {
            Some(LocalType::Parameter(actual)) if actual == &parameter.parameter_type => Ok(()),
            Some(LocalType::Parameter(_)) => Err(Diagnostic::new(
                "E_INVALID_ARGUMENT_TYPE",
                format!(
                    "parameter `${}` is not compatible with `{program}.{}`",
                    reference.value, parameter.name
                ),
                reference.span.clone(),
            )),
            Some(LocalType::Value(_)) => Err(Diagnostic::new(
                "E_INVALID_ARGUMENT_TYPE",
                format!(
                    "graph value `${}` cannot be used as scalar parameter `{program}.{}`",
                    reference.value, parameter.name
                ),
                reference.span.clone(),
            )),
            Some(LocalType::Alias(_)) => {
                unreachable!("local aliases are resolved before validation")
            }
            None => Err(missing_reference(&reference.value, &reference.span)),
        },
        ArgumentValue::References(_, _) | ArgumentValue::Body(_) => Err(Diagnostic::new(
            "E_INVALID_ARGUMENT_TYPE",
            format!(
                "parameter `{program}.{}` requires a scalar value",
                parameter.name
            ),
            argument.span().clone(),
        )),
    }
}

fn definition_for<'a>(
    _unit: SourceUnitId,
    name: &str,
    definitions: &'a [ProgramDefinition],
    builtins: &BTreeMap<String, ProgramId>,
    namespace: &BTreeMap<String, ProgramId>,
    span: &crate::source::SourceSpan,
) -> Result<&'a ProgramDefinition> {
    builtins
        .get(name)
        .or_else(|| namespace.get(name))
        .map(|id| &definitions[id.index()])
        .ok_or_else(|| {
            Diagnostic::new(
                "E_UNKNOWN_PROGRAM",
                format!("unknown program `{name}`"),
                span.clone(),
            )
        })
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

fn resolve_local_types(locals: &mut BTreeMap<String, LocalType>) -> Result<()> {
    let names = locals.keys().cloned().collect::<Vec<_>>();
    for name in names {
        if matches!(
            locals.get(&name),
            Some(LocalType::Value(_) | LocalType::Parameter(_))
        ) {
            continue;
        }
        let mut path = Vec::new();
        let mut positions = BTreeMap::new();
        let mut current = name.clone();
        let resolved = loop {
            match locals.get(&current).cloned() {
                Some(LocalType::Value(value_type)) => break LocalType::Value(value_type),
                Some(LocalType::Parameter(parameter_type)) => {
                    break LocalType::Parameter(parameter_type);
                }
                Some(LocalType::Alias(target)) => {
                    if let Some(start) = positions.get(&current).copied() {
                        let mut cycle = path[start..].to_vec();
                        cycle.push(current.clone());
                        return Err(Diagnostic::new(
                            "E_DEPENDENCY_CYCLE",
                            format!("named-value dependency cycle: {}", cycle.join(" -> ")),
                            crate::source::SourceSpan::file_start("<source-inference>"),
                        ));
                    }
                    positions.insert(current.clone(), path.len());
                    path.push(current);
                    current = target;
                }
                None => {
                    return Err(missing_reference(
                        &current,
                        &crate::source::SourceSpan::file_start("<source-inference>"),
                    ));
                }
            }
        };
        for entry in path {
            locals.insert(entry, resolved.clone());
        }
    }
    Ok(())
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
        Some(LocalType::Alias(_)) => unreachable!("local aliases are resolved before validation"),
        None => Err(missing_reference(name, span)),
    }
}

fn validate_literal_default(
    parameter_type: &ParameterType,
    default: Option<&Literal>,
    name: &str,
) -> Result<()> {
    if let Some(default) = default
        && !literal_matches(parameter_type, default)
    {
        return Err(Diagnostic::new(
            "E_INVALID_PARAMETER_DEFAULT",
            format!("default for parameter `{name}` has the wrong value type"),
            default.span().clone(),
        ));
    }
    Ok(())
}

fn literal_matches(parameter_type: &ParameterType, literal: &Literal) -> bool {
    matches!(
        (parameter_type, literal),
        (ParameterType::Integer, Literal::Integer(_, _))
            | (
                ParameterType::File
                    | ParameterType::Duration
                    | ParameterType::TimeRange
                    | ParameterType::Keyword(_),
                Literal::String(_, _)
            )
    )
}

fn ensure_types(
    values: &[ValueType],
    port: &InputPort,
    span: &crate::source::SourceSpan,
) -> Result<()> {
    for value in values {
        if *value != port.value_type {
            return Err(type_error("program", port, *value, span));
        }
    }
    Ok(())
}

fn type_error(
    program: &str,
    port: &InputPort,
    actual: ValueType,
    span: &crate::source::SourceSpan,
) -> Diagnostic {
    Diagnostic::new(
        "E_TYPE_MISMATCH",
        format!(
            "program `{program}` input `{}` expected {}, but found {actual}",
            port.name, port.value_type
        ),
        span.clone(),
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
    item: &Item,
    output_count: usize,
    binding: &str,
    span: &crate::source::SourceSpan,
) -> Diagnostic {
    let program = match &item.kind {
        ItemKind::Reference(_) => "reference",
        ItemKind::Invocation(invocation) => &invocation.program.value,
    };
    Diagnostic::new(
        "E_OUTPUT_BINDING_COUNT",
        format!(
            "{binding} cannot name program `{program}` because it produces {output_count} output(s)"
        ),
        span.clone(),
    )
}
