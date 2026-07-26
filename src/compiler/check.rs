use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostic::{Diagnostic, Result};
use crate::model::ValueType;
use crate::program::{
    Cardinality, InputSlot, ParameterDescriptor, ParameterType, ProgramDefinition,
    ProgramDescriptor, ProgramId, ProgramImplementation, ProgramRegistry, ValueTypeSpec,
    builtin_programs,
};
use crate::source::{
    OutputBindings, ProgramBody, SourceInput, SourcePackage, SourceProgram,
    SourceProgramImplementation, SourceUnitId, Spanned,
};

use super::draft::{
    DraftBody, DraftInput, DraftInvocation, DraftItemKind, DraftParameter, IdTable, InvocationId,
    StackBlockId,
};
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
    BodyInputId, CheckedBody, CheckedInputValue, CheckedInvocation, CheckedItem, CheckedItemKind,
    CheckedLocal, CheckedOutput, CheckedPackage, CheckedParameter, CheckedParameterValue,
    CheckedProgram, CheckedProgramInput, CheckedSourceProgram, CheckedStackBlock, ParameterId,
    ReferenceTarget, ValueLocalId,
};

pub(super) fn check(package: &SourcePackage) -> Result<CheckedPackage> {
    let unit_order = super::link::source_unit_order(package)?;
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
    let mut unit_programs = vec![None; package.units().len()];
    let mut programs = vec![None; package.units().len()];

    for unit_id in unit_order {
        let unit = &package.units()[unit_id.index()];
        let namespace = unit
            .imports
            .iter()
            .map(|import| {
                let program = unit_programs[import.target.index()].ok_or_else(|| {
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
        let id = ProgramId::new(
            u32::try_from(definitions.len()).expect("linked program catalog fits in u32"),
        );
        let (definition, checked_program) = match unit.program().implementation() {
            SourceProgramImplementation::Body(body) => {
                let (outputs, checked) = check_program(
                    unit.program(),
                    body,
                    &definitions,
                    &builtin_names,
                    &namespace,
                )?;
                (
                    clipasm_definition(unit_id, unit.program(), outputs)?,
                    CheckedSourceProgram::ClipAsm {
                        definition: id,
                        program: checked,
                    },
                )
            }
            SourceProgramImplementation::External(external) => (
                external_definition(unit_id, unit.program(), external)?,
                CheckedSourceProgram::External { definition: id },
            ),
        };
        definitions.push(definition);
        unit_programs[unit_id.index()] = Some(id);
        programs[unit_id.index()] = Some(checked_program);
    }

    let programs = programs
        .into_iter()
        .map(|program| program.expect("source-unit ordering visits every linked program"))
        .collect();
    let registry = ProgramRegistry::from_linked(definitions, builtin_count)?;
    Ok(CheckedPackage {
        root: package.root,
        registry,
        programs,
    })
}

#[cfg(test)]
pub(super) fn check_with_registry(
    package: &SourcePackage,
    registry: &ProgramRegistry,
) -> Result<CheckedPackage> {
    debug_assert_eq!(package.units().len(), 1);
    debug_assert!(package.root().imports.is_empty());
    let mut definitions = registry.definitions().to_vec();
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
    let definition =
        ProgramId::new(u32::try_from(definitions.len()).expect("test catalog fits in u32"));
    let SourceProgramImplementation::Body(body) = package.root().program().implementation() else {
        unreachable!("registry-backed compiler tests use ClipAsm source bodies");
    };
    let (outputs, program) = check_program(
        package.root().program(),
        body,
        &definitions,
        &names,
        &BTreeMap::new(),
    )?;
    definitions.push(clipasm_definition(
        package.root,
        package.root().program(),
        outputs,
    )?);
    Ok(CheckedPackage {
        root: package.root,
        registry: ProgramRegistry::from_definitions(definitions)?,
        programs: vec![CheckedSourceProgram::ClipAsm {
            definition,
            program,
        }],
    })
}

fn clipasm_definition(
    unit: SourceUnitId,
    program: &SourceProgram,
    outputs: Vec<ValueType>,
) -> Result<ProgramDefinition> {
    let parameters = program
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
    Ok(ProgramDefinition {
        descriptor: ProgramDescriptor {
            name: format!("source_program_{}", unit.index()),
            semantic_version: 1,
            default_stack_access: program.stack_access(),
            inputs: program
                .inputs()
                .iter()
                .map(SourceInput::descriptor)
                .collect(),
            parameters,
            outputs: outputs.into_iter().map(Into::into).collect(),
        },
        implementation: ProgramImplementation::ClipAsm(unit),
    })
}

fn external_definition(
    unit: SourceUnitId,
    program: &SourceProgram,
    external: &crate::source::SourceExternalImplementation,
) -> Result<ProgramDefinition> {
    let parameters = parameter_descriptors(program)?;
    let parameter_defaults = parameter_defaults(program, "external program")?;
    let preserve_index = program
        .inputs()
        .iter()
        .position(|input| input.name == external.preserve.value)
        .ok_or_else(|| {
            Diagnostic::new(
                "E_INVALID_EXTERNAL_PROGRAM",
                format!(
                    "external output preserves unknown input `{}`",
                    external.preserve.value
                ),
                external.preserve.span.clone(),
            )
        })?;
    let preserved = &program.inputs()[preserve_index];
    if preserved.value_type.exact() != Some(ValueType::Video) {
        return Err(Diagnostic::new(
            "E_INVALID_EXTERNAL_PROGRAM",
            format!(
                "external preserve input `{}` must be Video",
                external.preserve.value
            ),
            external.preserve.span.clone(),
        ));
    }
    for parameter in program.parameters() {
        if !matches!(
            parameter.parameter_type,
            ParameterType::Integer | ParameterType::File | ParameterType::Keyword(_)
        ) {
            return Err(Diagnostic::new(
                "E_INVALID_EXTERNAL_PROGRAM",
                format!(
                    "external parameter `{}` uses unsupported type {:?}",
                    parameter.name.value, parameter.parameter_type
                ),
                parameter.name.span.clone(),
            ));
        }
    }
    Ok(ProgramDefinition {
        descriptor: ProgramDescriptor {
            name: format!("source_program_{}", unit.index()),
            semantic_version: external.semantic_version.value,
            default_stack_access: program.stack_access(),
            inputs: program
                .inputs()
                .iter()
                .map(SourceInput::descriptor)
                .collect(),
            parameters,
            outputs: vec![ValueType::Video.into()],
        },
        implementation: ProgramImplementation::External(crate::external::ExternalRuntime::new(
            external.executable.clone(),
            external
                .arguments
                .iter()
                .map(|argument| match argument {
                    crate::source::SourceExternalArgument::Text(value) => {
                        crate::external::ExternalArgumentValue::Text {
                            value: value.value.clone(),
                        }
                    }
                    crate::source::SourceExternalArgument::File(path) => {
                        crate::external::ExternalArgumentValue::File { path: path.clone() }
                    }
                })
                .collect(),
            InputSlot::new(preserve_index),
            parameter_defaults,
        )),
    })
}

fn parameter_defaults(
    program: &SourceProgram,
    owner: &str,
) -> Result<Vec<Option<Spanned<crate::program::ParameterValue>>>> {
    program
        .parameters()
        .iter()
        .map(|parameter| {
            parameter
                .default
                .as_ref()
                .map(|default| {
                    super::parameter::from_expression(
                        owner,
                        &parameter.name.value,
                        &parameter.parameter_type,
                        default,
                    )
                    .map(|value| Spanned::new(value, default.span().clone()))
                })
                .transpose()
        })
        .collect()
}

fn parameter_descriptors(program: &SourceProgram) -> Result<Vec<ParameterDescriptor>> {
    program
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
        .collect()
}

#[allow(clippy::too_many_lines)]
fn check_program(
    program: &SourceProgram,
    body: &ProgramBody,
    definitions: &[ProgramDefinition],
    builtins: &BTreeMap<String, ProgramId>,
    namespace: &BTreeMap<String, ProgramId>,
) -> Result<(Vec<ValueType>, CheckedProgram)> {
    let draft = super::draft::DraftProgram::build(program, body, definitions, builtins, namespace)?;
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
            &input.declared_at,
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
    collect_body_names(&draft.body, &mut local_types, definitions)?;
    validate_local_dependencies(&local_types)?;
    let resolved = super::typecheck::resolve_program_types(draft, &mut local_types, definitions)?;
    ensure_local_types_resolved(&local_types)?;
    resolved.into_checked(program, &local_types, definitions)
}

impl super::typecheck::ResolvedDraftProgram {
    fn into_checked(
        self,
        program: &SourceProgram,
        local_types: &BTreeMap<String, LocalType>,
        definitions: &[ProgramDefinition],
    ) -> Result<(Vec<ValueType>, CheckedProgram)> {
        let bindings = prepare_program_bindings(program, &self.draft, local_types)?;
        let Self {
            draft,
            invocations,
            stack_blocks,
            outputs,
        } = self;
        let mut materializer = CheckedMaterializer {
            local_types,
            local_ids: &bindings.local_ids,
            parameter_ids: &bindings.parameter_ids,
            definitions,
            invocations,
            stack_blocks,
            body_input_count: 0,
        };
        let checked_body = materializer.body(draft.body, &BTreeMap::new())?;
        materializer.ensure_consumed(&draft.span)?;
        let body_input_count = materializer.body_input_count;
        Ok((
            outputs,
            CheckedProgram {
                span: program.span().clone(),
                stack_access: program.stack_access(),
                inputs: program
                    .inputs()
                    .iter()
                    .map(|input| CheckedProgramInput {
                        name: input.name.clone(),
                        declared_at: input.declared_at.clone(),
                        local: bindings.local_ids[&input.name],
                    })
                    .collect(),
                locals: bindings.locals,
                parameters: bindings.parameters,
                body_input_count,
                body: checked_body,
            },
        ))
    }
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
            .map(|expression| {
                super::parameter::from_expression(
                    "authored program",
                    &parameter.name.value,
                    &parameter.parameter_type,
                    expression,
                )
                .map(|value| crate::source::Spanned::new(value, expression.span().clone()))
            })
            .transpose()?;
        parameters.push(CheckedParameter {
            name: parameter.name.value.clone(),
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
        declare(&input.name, &input.declared_at)?;
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
        match &item.kind {
            DraftItemKind::Reference(_) => {}
            DraftItemKind::Invocation(invocation) => {
                if let Some(body) = invocation.body.as_deref() {
                    declare_body_outputs(body, declare)?;
                }
                for input in invocation.inputs.iter().flatten() {
                    if let DraftInput::Body(body) = input {
                        declare_body_outputs(body, declare)?;
                    }
                }
            }
            DraftItemKind::StackBlock(block) => declare_body_outputs(&block.body, declare)?,
        }
    }
    Ok(())
}

fn resolve_value_target(
    name: &str,
    span: &crate::source::SourceSpan,
    locals: &BTreeMap<String, ValueLocalId>,
    lexical: &BTreeMap<String, BodyBinding>,
) -> Result<ReferenceTarget> {
    lexical
        .get(name)
        .map(|binding| ReferenceTarget::BodyInput(binding.id))
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
                        "`as name` requires exactly one output",
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
                        &format!("`as (...)` contains {} name(s)", names.len()),
                        span,
                    ));
                }
                for (name, output) in names.iter().zip(output_types) {
                    insert_local(locals, &name.value, output, &name.span)?;
                }
            }
        }
        match &item.kind {
            DraftItemKind::Reference(_) => {}
            DraftItemKind::Invocation(invocation) => {
                if let Some(body) = invocation.body.as_deref() {
                    collect_body_names(body, locals, definitions)?;
                }
                for input in invocation.inputs.iter().flatten() {
                    if let DraftInput::Body(body) = input {
                        collect_body_names(body, locals, definitions)?;
                    }
                }
            }
            DraftItemKind::StackBlock(block) => {
                collect_body_names(&block.body, locals, definitions)?;
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
            span: item.origin.span.clone(),
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
                        span: item.origin.span.clone(),
                    },
                })
                .collect()
        }
        DraftItemKind::StackBlock(block) => {
            let mut dependencies = BTreeSet::new();
            collect_body_dependencies(
                &block.body,
                definitions,
                &BTreeSet::new(),
                &mut dependencies,
            );
            let count = match &item.output_bindings {
                OutputBindings::None => 0,
                OutputBindings::One(_) => 1,
                OutputBindings::Many(names, _) => names.len(),
            };
            (0..count)
                .map(|_| LocalType::Inferred {
                    dependencies: dependencies.clone(),
                    span: item.origin.span.clone(),
                })
                .collect()
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

struct CheckedMaterializer<'a> {
    local_types: &'a BTreeMap<String, LocalType>,
    local_ids: &'a BTreeMap<String, ValueLocalId>,
    parameter_ids: &'a BTreeMap<String, ParameterId>,
    definitions: &'a [ProgramDefinition],
    invocations: IdTable<InvocationId, super::typecheck::ResolvedInvocation>,
    stack_blocks: IdTable<StackBlockId, Vec<ValueType>>,
    body_input_count: usize,
}

#[derive(Clone, Copy)]
struct BodyBinding {
    value_type: ValueType,
    id: BodyInputId,
}

struct MaterializedArguments {
    inputs: Vec<Option<CheckedInputValue>>,
    parameters: Vec<Option<CheckedParameterValue>>,
}

impl CheckedMaterializer<'_> {
    #[allow(clippy::too_many_lines)]
    fn body(
        &mut self,
        body: DraftBody,
        lexical: &BTreeMap<String, BodyBinding>,
    ) -> Result<CheckedBody> {
        let mut checked_items = Vec::with_capacity(body.items.len());
        for item in body.items {
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
                        Diagnostic::new(
                            "E_INTERNAL_TYPE_RESOLUTION",
                            format!("invocation {} was consumed more than once", id.0),
                            item.origin.span.clone(),
                        )
                    })?;
                    let validated = self.explicit_arguments(
                        &name.value,
                        inputs,
                        parameters,
                        definition,
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
                        Diagnostic::new(
                            "E_INTERNAL_TYPE_RESOLUTION",
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

    fn ensure_consumed(&self, span: &crate::source::SourceSpan) -> Result<()> {
        if let Some(index) = self.invocations.first_present() {
            return Err(Diagnostic::new(
                "E_INTERNAL_TYPE_RESOLUTION",
                format!("invocation {index} was resolved but not materialized"),
                span.clone(),
            ));
        }
        if let Some(index) = self.stack_blocks.first_present() {
            return Err(Diagnostic::new(
                "E_INTERNAL_TYPE_RESOLUTION",
                format!("stack block {index} was resolved but not materialized"),
                span.clone(),
            ));
        }
        Ok(())
    }

    fn allocate_body_input(&mut self, span: &crate::source::SourceSpan) -> Result<BodyInputId> {
        let id = BodyInputId(u32::try_from(self.body_input_count).map_err(|_| {
            Diagnostic::new(
                "E_GRAPH_TOO_LARGE",
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
                            self.local_types,
                            self.parameter_ids,
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

fn check_parameter_argument(
    program: &str,
    parameter: &ParameterDescriptor,
    argument: DraftParameter,
    local_types: &BTreeMap<String, LocalType>,
    parameter_ids: &BTreeMap<String, ParameterId>,
) -> Result<CheckedParameterValue> {
    let DraftParameter::Expression(expression) = argument;
    let checked = super::parameter::check_expression(
        program,
        &parameter.name,
        &parameter.parameter_type,
        &expression,
        &mut |reference| match local_types.get(&reference.value) {
            Some(LocalType::Parameter(actual)) => Ok((
                *parameter_ids
                    .get(&reference.value)
                    .ok_or_else(|| missing_reference(&reference.value, &reference.span))?,
                actual.clone(),
            )),
            Some(LocalType::Value(_) | LocalType::Inferred { .. }) => Err(Diagnostic::new(
                "E_INVALID_ARGUMENT_TYPE",
                format!(
                    "graph value `${}` cannot be used in scalar parameter `{program}.{}`",
                    reference.value, parameter.name
                ),
                reference.span.clone(),
            )),
            None => Err(missing_reference(&reference.value, &reference.span)),
        },
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
            "cannot infer the type of named value `${name}` from available constraints; add `<Video>` or `<Audio>`"
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
            DraftItemKind::StackBlock(block) => {
                collect_body_dependencies(&block.body, definitions, shadows, dependencies);
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
    super::parameter::from_expression(
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
        format!(
            "reference `${name}` does not name an input, parameter, body alias, or output binding"
        ),
        span.clone(),
    )
}

fn binding_count_error(
    item: &super::draft::DraftItem,
    output_count: usize,
    binding: &str,
    span: &crate::source::SourceSpan,
) -> Diagnostic {
    Diagnostic::new(
        "E_OUTPUT_BINDING_COUNT",
        format!(
            "`{}` produces {output_count} value(s), but {binding}",
            item.origin.construct
        ),
        span.clone(),
    )
}
