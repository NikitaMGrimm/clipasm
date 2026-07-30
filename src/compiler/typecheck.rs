//! Monotonic type and stack resolution for authored programs.
//!
//! One recursive resolver owns selectors, explicit inputs, stack plans, body
//! contracts, and ordered output types. Exploratory passes narrow stable type
//! variables; the final pass records the concrete invocation decisions consumed
//! by checked-source materialization.

use std::collections::BTreeMap;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::ValueType;
use crate::program::{
    Cardinality, InputSlot, ProgramDefinition, ProgramImplementation, ResolvedSignature,
    StackAccess, ValueTypeSpec,
};
use crate::source::{OutputBindings, SourceSpan};

use super::check::LocalType;
use super::draft::{
    BodyId, DraftBody, DraftInput, DraftInvocation, DraftItemKind, DraftProgram, IdTable,
    InvocationId, StackBlockId,
};
use super::scalar_scope::ScalarScopes;
use super::stack::{
    EvaluationStack, StackBindingInput, StackBindingOutcome, StackBindingPlan, StackCompatibility,
    StackFrame,
};

mod arena;

#[cfg(test)]
use arena::TypeConflict;
use arena::{TypeArena, TypeDomain, TypeVarId};

#[derive(Clone, Copy)]
enum LocalSlot {
    Value(TypeVarId),
    Parameter,
}

#[derive(Clone, Copy)]
enum Requirement {
    Exact(ValueType),
    Generic(TypeVarId),
}

#[derive(Debug)]
pub(super) struct ResolvedInvocation {
    pub(super) signature: ResolvedSignature,
    pub(super) stack_plan: StackBindingPlan,
}

#[derive(Debug)]
pub(super) struct ResolvedDraftProgram {
    pub(super) draft: DraftProgram,
    pub(super) invocations: IdTable<InvocationId, ResolvedInvocation>,
    pub(super) stack_blocks: IdTable<StackBlockId, Vec<ValueType>>,
    pub(super) outputs: Vec<ValueType>,
}

struct PassState {
    deferred: usize,
    resolutions: Option<PassResolutions>,
}

struct PassResolutions {
    invocations: IdTable<InvocationId, ResolvedInvocation>,
    stack_blocks: IdTable<StackBlockId, Vec<ValueType>>,
}

struct FinalResolution {
    decisions: PassResolutions,
    outputs: Vec<ValueType>,
}

impl PassState {
    const fn infer() -> Self {
        Self {
            deferred: 0,
            resolutions: None,
        }
    }

    fn resolve(invocation_count: usize, stack_block_count: usize) -> Self {
        Self {
            deferred: 0,
            resolutions: Some(PassResolutions {
                invocations: IdTable::with_slot_count(invocation_count),
                stack_blocks: IdTable::with_slot_count(stack_block_count),
            }),
        }
    }

    const fn is_resolving(&self) -> bool {
        self.resolutions.is_some()
    }

    fn mark_deferred(&mut self) {
        self.deferred = self
            .deferred
            .checked_add(1)
            .expect("too many deferred type-inference decisions");
    }

    fn record(&mut self, invocation: &DraftInvocation, resolved: ResolvedInvocation) {
        if let Some(resolutions) = &mut self.resolutions {
            resolutions.invocations.insert(invocation.id, resolved);
        }
    }

    fn record_stack_block(
        &mut self,
        block: &super::draft::DraftStackBlock,
        outputs: Vec<ValueType>,
    ) {
        if let Some(resolutions) = &mut self.resolutions {
            resolutions.stack_blocks.insert(block.id, outputs);
        }
    }
}

fn allocate_body_generics(
    body: &DraftBody,
    definitions: &[ProgramDefinition],
    arena: &mut TypeArena,
    generics: &mut IdTable<InvocationId, TypeVarId>,
) {
    for item in &body.items {
        match &item.kind {
            DraftItemKind::Reference(_) | DraftItemKind::ScalarBinding { .. } => {}
            DraftItemKind::Invocation(invocation) => {
                let definition = &definitions[invocation.program.index()];
                if definition.descriptor.is_generic() {
                    generics.insert(invocation.id, arena.allocate());
                }
                for input in invocation.inputs.iter().flatten() {
                    if let DraftInput::Body(body) = input {
                        allocate_body_generics(body, definitions, arena, generics);
                    }
                }
                if let Some(body) = invocation.body.as_deref() {
                    allocate_body_generics(body, definitions, arena, generics);
                }
            }
            DraftItemKind::StackBlock(block) => {
                allocate_body_generics(&block.body, definitions, arena, generics);
            }
        }
    }
}

struct TypeState {
    arena: TypeArena,
    slots: BTreeMap<String, LocalSlot>,
    invocation_generics: IdTable<InvocationId, TypeVarId>,
}

pub(super) fn resolve_program_types(
    program: DraftProgram,
    locals: &mut BTreeMap<String, LocalType>,
    definitions: &[ProgramDefinition],
    scalar_scopes: &ScalarScopes,
) -> Result<ResolvedDraftProgram> {
    let mut types = prepare_type_state(&program, locals, definitions);
    infer_fixpoint(&program, definitions, scalar_scopes, &mut types)?;
    apply_resolved_local_types(locals, &types);
    let FinalResolution { decisions, outputs } =
        resolve_final_program(&program, definitions, scalar_scopes, &types)?;
    Ok(ResolvedDraftProgram {
        draft: program,
        invocations: decisions.invocations,
        stack_blocks: decisions.stack_blocks,
        outputs,
    })
}

fn prepare_type_state(
    program: &DraftProgram,
    locals: &BTreeMap<String, LocalType>,
    definitions: &[ProgramDefinition],
) -> TypeState {
    let mut arena = TypeArena::default();
    let slots = locals
        .iter()
        .map(|(name, local)| {
            let slot = match local {
                LocalType::Value(value_type) => LocalSlot::Value(arena.allocate_exact(*value_type)),
                LocalType::Parameter(_) => LocalSlot::Parameter,
                LocalType::Inferred { .. } => LocalSlot::Value(arena.allocate()),
            };
            (name.clone(), slot)
        })
        .collect();
    let mut invocation_generics = IdTable::with_slot_count(program.invocation_count);
    allocate_body_generics(
        &program.body,
        definitions,
        &mut arena,
        &mut invocation_generics,
    );
    TypeState {
        arena,
        slots,
        invocation_generics,
    }
}

fn infer_fixpoint(
    program: &DraftProgram,
    definitions: &[ProgramDefinition],
    scalar_scopes: &ScalarScopes,
    types: &mut TypeState,
) -> Result<()> {
    loop {
        let before = types.arena.revision();
        let mut attempt = types.arena.clone();
        let mut state = PassState::infer();
        infer_program_body(
            program,
            definitions,
            scalar_scopes,
            &types.slots,
            &types.invocation_generics,
            &mut attempt,
            &mut state,
            "type inference",
        )?;
        for variable in types
            .slots
            .values()
            .filter_map(|slot| match slot {
                LocalSlot::Value(variable) => Some(*variable),
                LocalSlot::Parameter => None,
            })
            .chain(types.invocation_generics.values().copied())
        {
            types
                .arena
                .constrain_domain(variable, attempt.domain(variable))
                .map_err(|_| type_mismatch(&program.span))?;
        }
        if types.arena.revision() == before {
            if state.deferred > 0 {
                return Err(inference_dependency(&program.span));
            }
            return Ok(());
        }
    }
}

fn apply_resolved_local_types(locals: &mut BTreeMap<String, LocalType>, types: &TypeState) {
    for (name, slot) in &types.slots {
        let LocalSlot::Value(variable) = slot else {
            continue;
        };
        if let Some(value_type) = types.arena.domain(*variable).concrete() {
            locals.insert(name.clone(), LocalType::Value(value_type));
        }
    }
}

fn resolve_final_program(
    program: &DraftProgram,
    definitions: &[ProgramDefinition],
    scalar_scopes: &ScalarScopes,
    types: &TypeState,
) -> Result<FinalResolution> {
    let mut arena = types.arena.clone();
    let mut state = PassState::resolve(program.invocation_count, program.stack_block_count);
    let outputs = infer_program_body(
        program,
        definitions,
        scalar_scopes,
        &types.slots,
        &types.invocation_generics,
        &mut arena,
        &mut state,
        "type resolution",
    )?;
    if state.deferred > 0 {
        return Err(inference_dependency(&program.span));
    }
    let resolutions = state
        .resolutions
        .expect("final type-resolution pass owns resolution tables");
    if let Some(index) = resolutions.invocations.first_missing() {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InternalTypeResolution,
            format!("invocation {index} was not resolved"),
            program.span.clone(),
        ));
    }
    if let Some(index) = resolutions.stack_blocks.first_missing() {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InternalTypeResolution,
            format!("stack block {index} was not resolved"),
            program.span.clone(),
        ));
    }
    Ok(FinalResolution {
        decisions: resolutions,
        outputs,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "the phase entry keeps the draft, scope, solver, and pass state explicit for retryable inference"
)]
fn infer_program_body(
    program: &DraftProgram,
    definitions: &[ProgramDefinition],
    scalar_scopes: &ScalarScopes,
    slots: &BTreeMap<String, LocalSlot>,
    invocation_generics: &IdTable<InvocationId, TypeVarId>,
    arena: &mut TypeArena,
    state: &mut PassState,
    phase: &str,
) -> Result<Vec<ValueType>> {
    let (mut stack, mut frame) =
        EvaluationStack::isolated(format!("source program {phase}"), program.span.clone());
    infer_body(
        &program.body,
        slots,
        &BTreeMap::new(),
        definitions,
        scalar_scopes,
        invocation_generics,
        arena,
        &mut stack,
        &mut frame,
        state,
    )?;
    Ok(if state.is_resolving() {
        concrete_values(arena, stack.values(), &program.span)?
    } else {
        Vec::new()
    })
}

fn concrete_values(
    arena: &TypeArena,
    values: &[TypeVarId],
    span: &SourceSpan,
) -> Result<Vec<ValueType>> {
    values
        .iter()
        .map(|value| {
            arena
                .domain(*value)
                .concrete()
                .ok_or_else(|| inference_dependency(span))
        })
        .collect()
}

fn inference_dependency(span: &SourceSpan) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::TypeInferenceDependency,
        "generic type inference depends on an unresolved stack selection; add `<Video>` or `<Audio>`",
        span.clone(),
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "recursive body inference must share lexical bindings, solver state, stack state, and pass state"
)]
fn infer_body(
    body: &DraftBody,
    globals: &BTreeMap<String, LocalSlot>,
    lexical: &BTreeMap<String, TypeVarId>,
    definitions: &[ProgramDefinition],
    scalar_scopes: &ScalarScopes,
    invocation_generics: &IdTable<InvocationId, TypeVarId>,
    arena: &mut TypeArena,
    stack: &mut EvaluationStack<TypeVarId>,
    frame: &mut StackFrame,
    state: &mut PassState,
) -> Result<()> {
    for item in &body.items {
        let outputs = match &item.kind {
            DraftItemKind::Reference(reference) => {
                vec![lookup_value(
                    globals,
                    lexical,
                    scalar_scopes,
                    body.id,
                    &reference.value,
                    &reference.span,
                )?]
            }
            DraftItemKind::ScalarBinding { .. } => Vec::new(),
            DraftItemKind::Invocation(invocation) => infer_invocation(
                &item.origin,
                invocation,
                globals,
                lexical,
                definitions,
                scalar_scopes,
                body.id,
                invocation_generics,
                arena,
                stack,
                frame,
                state,
            )?,
            DraftItemKind::StackBlock(block) => {
                let mut child = EvaluationStack::<TypeVarId>::enter_body(
                    frame,
                    block.access,
                    "stack block",
                    item.origin.span.clone(),
                );
                infer_body(
                    &block.body,
                    globals,
                    lexical,
                    definitions,
                    scalar_scopes,
                    invocation_generics,
                    arena,
                    stack,
                    &mut child,
                    state,
                )?;
                let outputs = stack.finish_body(&child);
                if state.is_resolving() {
                    state.record_stack_block(
                        block,
                        concrete_values(arena, &outputs, &item.origin.span)?,
                    );
                }
                outputs
            }
        };
        constrain_bindings(globals, item, &outputs, arena)?;
        stack.extend(frame, outputs);
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "invocation inference orchestrates one ordered solver transaction; a context object would hide mutable retry state"
)]
fn infer_invocation(
    origin: &crate::source::ItemOrigin,
    invocation: &DraftInvocation,
    globals: &BTreeMap<String, LocalSlot>,
    lexical: &BTreeMap<String, TypeVarId>,
    definitions: &[ProgramDefinition],
    scalar_scopes: &ScalarScopes,
    scope: BodyId,
    invocation_generics: &IdTable<InvocationId, TypeVarId>,
    arena: &mut TypeArena,
    stack: &mut EvaluationStack<TypeVarId>,
    frame: &mut StackFrame,
    state: &mut PassState,
) -> Result<Vec<TypeVarId>> {
    let deferred_before = state.deferred;
    let definition = &definitions[invocation.program.index()];
    let access = invocation.access;

    let generic = invocation_generics.get(invocation.id).copied();
    if let Some(variable) = generic
        && let Some(argument) = &invocation.type_argument
    {
        constrain(arena, variable, argument.value, &argument.span)?;
    }

    let mut slots = vec![None; definition.descriptor.inputs.len()];
    for (index, (port, argument)) in definition
        .descriptor
        .inputs
        .iter()
        .zip(&invocation.inputs)
        .enumerate()
    {
        let Some(argument) = argument else {
            continue;
        };
        let values = explicit_values(
            argument,
            &origin.construct,
            &port.name,
            globals,
            lexical,
            definitions,
            scalar_scopes,
            scope,
            invocation_generics,
            arena,
            state,
        )?;
        if let (Some(variable), ValueTypeSpec::Generic) = (generic, port.value_type) {
            for value in &values {
                equate(arena, variable, *value, argument.span())?;
            }
        }
        slots[index] = Some(values);
    }

    if let Some(variable) = generic {
        infer_generic_from_stack(
            definition, origin, variable, arena, stack, frame, access, &slots, state,
        )?;
    }

    let stack_plan = bind_missing(
        definition, origin, generic, arena, stack, frame, access, &mut slots, state,
    )?;
    if state.deferred > deferred_before {
        return Ok(invocation_outputs(definition, generic, arena));
    }

    let mut lexical_body = lexical.clone();
    for (port, values) in definition.descriptor.inputs.iter().zip(&slots) {
        if matches!(port.cardinality, Cardinality::One)
            && let Some(values) = values
            && let [value] = values.as_slice()
        {
            lexical_body.insert(port.name.clone(), *value);
        }
    }

    if let ProgramImplementation::Body { contract, .. } = &definition.implementation {
        let body = invocation.body.as_deref().expect("draft body program");
        let mut child = EvaluationStack::<TypeVarId>::enter_body(
            frame,
            access,
            origin.construct.clone(),
            origin.span.clone(),
        );
        for initial in &contract.initial_values {
            let variable = match initial {
                ValueTypeSpec::Exact(value_type) => arena.allocate_exact(*value_type),
                ValueTypeSpec::Generic => generic.expect("generic body initial value"),
            };
            stack.push(&child, variable);
        }
        infer_body(
            body,
            globals,
            &lexical_body,
            definitions,
            scalar_scopes,
            invocation_generics,
            arena,
            stack,
            &mut child,
            state,
        )?;
        if state.deferred > deferred_before {
            return Ok(invocation_outputs(definition, generic, arena));
        }
        let body_outputs = stack.finish_body(&child);
        constrain_body_outputs(
            &origin.construct,
            &body_outputs,
            &contract.outputs,
            contract.count_diagnostic,
            generic,
            arena,
            state,
            &body.span,
        )?;
    }

    if state.is_resolving() {
        let generic = generic
            .map(|variable| concrete_type(arena, variable, &origin.span))
            .transpose()?;
        state.record(
            invocation,
            ResolvedInvocation {
                signature: definition.descriptor.resolve_signature(generic),
                stack_plan: stack_plan.unwrap_or(StackBindingPlan { inputs: Vec::new() }),
            },
        );
    }

    Ok(invocation_outputs(definition, generic, arena))
}

fn invocation_outputs(
    definition: &ProgramDefinition,
    generic: Option<TypeVarId>,
    arena: &mut TypeArena,
) -> Vec<TypeVarId> {
    definition
        .descriptor
        .outputs
        .iter()
        .map(|output| match output {
            ValueTypeSpec::Exact(value_type) => arena.allocate_exact(*value_type),
            ValueTypeSpec::Generic => generic.expect("generic output"),
        })
        .collect()
}

#[expect(
    clippy::too_many_arguments,
    reason = "explicit input inference recurses with the same lexical and solver state as its owning invocation"
)]
fn explicit_values(
    argument: &DraftInput,
    program: &str,
    port: &str,
    globals: &BTreeMap<String, LocalSlot>,
    lexical: &BTreeMap<String, TypeVarId>,
    definitions: &[ProgramDefinition],
    scalar_scopes: &ScalarScopes,
    scope: BodyId,
    invocation_generics: &IdTable<InvocationId, TypeVarId>,
    arena: &mut TypeArena,
    state: &mut PassState,
) -> Result<Vec<TypeVarId>> {
    match argument {
        DraftInput::Reference(reference) => Ok(vec![lookup_value(
            globals,
            lexical,
            scalar_scopes,
            scope,
            &reference.value,
            &reference.span,
        )?]),
        DraftInput::Body(body) => {
            let (mut stack, mut frame) =
                EvaluationStack::isolated("inline input type inference", body.span.clone());
            infer_body(
                body,
                globals,
                lexical,
                definitions,
                scalar_scopes,
                invocation_generics,
                arena,
                &mut stack,
                &mut frame,
                state,
            )?;
            if stack.len() != 1 {
                return Err(Diagnostic::builtin(
                    BuiltinDiagnostic::InputBodyOutputCount,
                    format!(
                        "inline input body for `{program}.{port}` must leave exactly one value, but {} values remain",
                        stack.len()
                    ),
                    body.span.clone(),
                ));
            }
            Ok(stack.values().to_vec())
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "generic stack selection needs the descriptor, stack frame, solver, slots, and deferred-pass state together"
)]
fn infer_generic_from_stack(
    definition: &ProgramDefinition,
    origin: &crate::source::ItemOrigin,
    generic: TypeVarId,
    arena: &mut TypeArena,
    stack: &EvaluationStack<TypeVarId>,
    frame: &StackFrame,
    access: StackAccess,
    slots: &[Option<Vec<TypeVarId>>],
    state: &mut PassState,
) -> Result<()> {
    if arena.domain(generic).concrete().is_some() {
        return Ok(());
    }
    let missing = definition
        .descriptor
        .inputs
        .iter()
        .enumerate()
        .filter(|(index, port)| {
            slots[*index].is_none() && matches!(port.value_type, ValueTypeSpec::Generic)
        })
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }
    if missing.len() == 1 && matches!(missing[0].1.cardinality, Cardinality::One) {
        let input = [StackBindingInput {
            port: InputSlot::new(missing[0].0),
            requirement: Requirement::Generic(generic),
            cardinality: Cardinality::One,
        }];
        match stack.plan_bindings(frame, access, &input, |value, requirement| {
            compatibility(arena, value, requirement)
        }) {
            StackBindingOutcome::Resolved(plan) => {
                let selected = plan.inputs[0].indices[0];
                equate(arena, generic, stack.values()[selected], &origin.span)?;
            }
            StackBindingOutcome::Deferred => state.mark_deferred(),
            StackBindingOutcome::Impossible(_) => {
                if state.is_resolving()
                    && access == StackAccess::Owned
                    && let Some(value_type) =
                        stack.nearest_accessible_with(frame, StackAccess::Visible, |value| {
                            arena.domain(value).concrete()
                        })
                {
                    constrain(arena, generic, value_type, &origin.span)?;
                }
            }
        }
        return Ok(());
    }

    let mut possible = Vec::new();
    let mut saw_deferred = false;
    for candidate in [ValueType::Video, ValueType::Audio] {
        let mut attempt = arena.clone();
        if attempt.constrain(generic, candidate).is_err() {
            continue;
        }
        let inputs = missing
            .iter()
            .map(|(index, port)| StackBindingInput {
                port: InputSlot::new(*index),
                requirement: Requirement::Exact(candidate),
                cardinality: port.cardinality,
            })
            .collect::<Vec<_>>();
        match stack.plan_bindings(frame, access, &inputs, |value, requirement| {
            compatibility(&attempt, value, requirement)
        }) {
            StackBindingOutcome::Resolved(_) => possible.push(candidate),
            StackBindingOutcome::Deferred => saw_deferred = true,
            StackBindingOutcome::Impossible(_) => {}
        }
    }
    if possible.len() > 1 && !saw_deferred {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::AmbiguousGenericType,
            format!(
                "program `{}` can bind both Video and Audio; use `<Video>` or `<Audio>`",
                origin.construct
            ),
            origin.span.clone(),
        ));
    }
    if possible.len() == 1 && !saw_deferred {
        constrain(arena, generic, possible[0], &origin.span)?;
    } else if possible.len() != 1 || saw_deferred {
        state.mark_deferred();
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "stack binding keeps all diagnostic and solver inputs explicit rather than introducing a second binder context"
)]
fn bind_missing(
    definition: &ProgramDefinition,
    origin: &crate::source::ItemOrigin,
    generic: Option<TypeVarId>,
    arena: &TypeArena,
    stack: &mut EvaluationStack<TypeVarId>,
    frame: &StackFrame,
    access: StackAccess,
    slots: &mut [Option<Vec<TypeVarId>>],
    state: &mut PassState,
) -> Result<Option<StackBindingPlan>> {
    let inputs = definition
        .descriptor
        .inputs
        .iter()
        .enumerate()
        .filter(|(index, _)| slots[*index].is_none())
        .map(|(index, port)| StackBindingInput {
            port: InputSlot::new(index),
            requirement: match port.value_type {
                ValueTypeSpec::Exact(value_type) => Requirement::Exact(value_type),
                ValueTypeSpec::Generic => Requirement::Generic(generic.expect("generic port")),
            },
            cardinality: port.cardinality,
        })
        .collect::<Vec<_>>();
    if inputs.is_empty() {
        return Ok(Some(StackBindingPlan { inputs: Vec::new() }));
    }
    match stack.plan_bindings(frame, access, &inputs, |value, requirement| {
        compatibility(arena, value, requirement)
    }) {
        StackBindingOutcome::Resolved(plan) => {
            for bound in stack.apply_binding_plan(&plan) {
                slots[bound.port.index()] = Some(bound.values);
            }
            Ok(Some(plan))
        }
        StackBindingOutcome::Deferred => {
            state.mark_deferred();
            Ok(None)
        }
        StackBindingOutcome::Impossible(_failure) if !state.is_resolving() => Ok(None),
        StackBindingOutcome::Impossible(failure) => {
            let port = definition.descriptor.input(failure.port);
            let required = match port.value_type {
                ValueTypeSpec::Exact(value_type) => value_type,
                ValueTypeSpec::Generic => generic
                    .and_then(|variable| arena.domain(variable).concrete())
                    .ok_or_else(|| {
                        Diagnostic::builtin(
                            BuiltinDiagnostic::StackUnderflow,
                            format!(
                                "program `{}` needs a preceding Video or Audio value",
                                origin.construct
                            ),
                            origin.span.clone(),
                        )
                    })?,
            };
            let (diagnostic, requirement) = match port.cardinality {
                Cardinality::One => (
                    BuiltinDiagnostic::StackUnderflow,
                    format!(
                        "`{}.{}` needs one preceding {required} value",
                        origin.construct, port.name
                    ),
                ),
                Cardinality::Variadic { min } => (
                    BuiltinDiagnostic::MissingRequiredInput,
                    format!(
                        "`{}.{}` needs at least {min} {required} value(s)",
                        origin.construct, port.name
                    ),
                ),
            };
            Err(stack.underflow_with(
                frame,
                access,
                diagnostic,
                &requirement,
                required,
                failure.available,
                &failure.selected,
                &origin.span,
                |value| arena.domain(value).concrete(),
            ))
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "body-output validation reports one complete contract using explicit solver and diagnostic inputs"
)]
fn constrain_body_outputs(
    program: &str,
    values: &[TypeVarId],
    constraint: &crate::program::BodyOutputConstraint,
    count_diagnostic: crate::program::BodyCountDiagnostic,
    generic: Option<TypeVarId>,
    arena: &mut TypeArena,
    state: &PassState,
    span: &SourceSpan,
) -> Result<()> {
    match constraint {
        crate::program::BodyOutputConstraint::Exactly(expected) => {
            if values.len() != expected.len() {
                if state.is_resolving() {
                    return Err(count_diagnostic.build(
                        format!(
                            "`{program}` body must leave exactly {} value(s), but {} values remain",
                            expected.len(),
                            values.len()
                        ),
                        span.clone(),
                    ));
                }
                return Ok(());
            }
            for (actual, expected) in values.iter().zip(expected) {
                constrain_body_output(program, *actual, *expected, generic, arena, span)?;
            }
        }
        crate::program::BodyOutputConstraint::Variadic { value_type, min } => {
            if values.len() < *min {
                if state.is_resolving() {
                    return Err(count_diagnostic.build(
                        format!("`{program}` body must produce at least {min} value(s)"),
                        span.clone(),
                    ));
                }
                return Ok(());
            }
            for actual in values {
                constrain_body_output(program, *actual, *value_type, generic, arena, span)?;
            }
        }
    }
    Ok(())
}

fn constrain_body_output(
    program: &str,
    actual: TypeVarId,
    expected: ValueTypeSpec,
    generic: Option<TypeVarId>,
    arena: &mut TypeArena,
    span: &SourceSpan,
) -> Result<()> {
    let result = match expected {
        ValueTypeSpec::Exact(value_type) => arena.constrain(actual, value_type),
        ValueTypeSpec::Generic => arena.equate(actual, generic.expect("generic body output")),
    };
    result.map_err(|_| {
        Diagnostic::builtin(
            BuiltinDiagnostic::GenericTypeMismatch,
            format!("`{program}` body must contain only one value type"),
            span.clone(),
        )
    })
}

fn concrete_type(arena: &TypeArena, value: TypeVarId, span: &SourceSpan) -> Result<ValueType> {
    arena
        .domain(value)
        .concrete()
        .ok_or_else(|| inference_dependency(span))
}

fn compatibility(
    arena: &TypeArena,
    value: TypeVarId,
    requirement: Requirement,
) -> StackCompatibility {
    let value_domain = arena.domain(value);
    let required_domain = match requirement {
        Requirement::Exact(value_type) => TypeDomain::from(value_type),
        Requirement::Generic(variable) => arena.domain(variable),
    };
    if !value_domain.overlaps(required_domain) {
        StackCompatibility::Incompatible
    } else if value_domain.is_subset_of(required_domain) {
        StackCompatibility::Definite
    } else {
        StackCompatibility::Possible
    }
}

fn constrain_bindings(
    globals: &BTreeMap<String, LocalSlot>,
    item: &super::draft::DraftItem,
    outputs: &[TypeVarId],
    arena: &mut TypeArena,
) -> Result<()> {
    item.validate_output_binding_count(outputs.len())?;
    match &item.output_bindings {
        OutputBindings::None => {}
        OutputBindings::One(name) => {
            let [output] = outputs else {
                unreachable!("validated single output binding")
            };
            equate(
                arena,
                value_slot(globals, &name.value, &name.span)?,
                *output,
                &item.origin.span,
            )?;
        }
        OutputBindings::Many(names, _) => {
            debug_assert_eq!(names.len(), outputs.len());
            for (name, output) in names.iter().zip(outputs) {
                equate(
                    arena,
                    value_slot(globals, &name.value, &name.span)?,
                    *output,
                    &item.origin.span,
                )?;
            }
        }
    }
    Ok(())
}

fn lookup_value(
    globals: &BTreeMap<String, LocalSlot>,
    lexical: &BTreeMap<String, TypeVarId>,
    scalar_scopes: &ScalarScopes,
    scope: BodyId,
    name: &str,
    span: &SourceSpan,
) -> Result<TypeVarId> {
    if let Some(value) = lexical.get(name) {
        return Ok(*value);
    }
    match globals.get(name) {
        Some(LocalSlot::Value(variable)) => Ok(*variable),
        Some(LocalSlot::Parameter) => Err(Diagnostic::builtin(
            BuiltinDiagnostic::ParameterNotValue,
            format!("parameter `${name}` is not a graph value"),
            span.clone(),
        )),
        None if scalar_scopes.resolve(scope, name).is_some() => Err(Diagnostic::builtin(
            BuiltinDiagnostic::ScalarNotValue,
            format!("scalar alias `${name}` is not a graph value"),
            span.clone(),
        )),
        None => Err(Diagnostic::builtin(
            BuiltinDiagnostic::MissingReference,
            format!("reference `${name}` does not name an input, body alias, or output binding"),
            span.clone(),
        )),
    }
}

fn value_slot(
    slots: &BTreeMap<String, LocalSlot>,
    name: &str,
    span: &SourceSpan,
) -> Result<TypeVarId> {
    match slots.get(name) {
        Some(LocalSlot::Value(variable)) => Ok(*variable),
        Some(LocalSlot::Parameter) => Err(Diagnostic::builtin(
            BuiltinDiagnostic::ParameterNotValue,
            format!("parameter `${name}` is not a graph value"),
            span.clone(),
        )),
        None => Err(Diagnostic::builtin(
            BuiltinDiagnostic::MissingReference,
            format!("reference `${name}` does not name an input, body alias, or output binding"),
            span.clone(),
        )),
    }
}

fn equate(
    arena: &mut TypeArena,
    left: TypeVarId,
    right: TypeVarId,
    span: &SourceSpan,
) -> Result<()> {
    arena.equate(left, right).map_err(|_| type_mismatch(span))
}

fn constrain(
    arena: &mut TypeArena,
    variable: TypeVarId,
    value_type: ValueType,
    span: &SourceSpan,
) -> Result<()> {
    arena
        .constrain(variable, value_type)
        .map_err(|_| type_mismatch(span))
}

fn type_mismatch(span: &SourceSpan) -> Diagnostic {
    Diagnostic::builtin(
        BuiltinDiagnostic::GenericTypeMismatch,
        "generic inputs and outputs must resolve to one value type",
        span.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains_represent_the_closed_value_type_set() {
        let all = TypeDomain::TIMELINE;
        let video = TypeDomain::from(ValueType::Video);
        let audio = TypeDomain::from(ValueType::Audio);

        assert!(all.contains(ValueType::Video));
        assert!(all.contains(ValueType::Audio));
        assert_eq!(video.concrete(), Some(ValueType::Video));
        assert_eq!(audio.concrete(), Some(ValueType::Audio));
        assert!(video.intersection(audio).is_empty());
        assert_eq!(all.intersection(video), video);
        assert_eq!(TypeDomain::TIMELINE, all);
    }

    #[test]
    fn constraints_narrow_a_variable_once() {
        let mut arena = TypeArena::default();
        let variable = arena.allocate();
        let after_allocation = arena.revision();

        arena
            .constrain(variable, ValueType::Audio)
            .expect("Audio satisfies Timeline");
        assert_eq!(arena.domain(variable).concrete(), Some(ValueType::Audio));
        assert!(arena.revision() > after_allocation);

        let after_narrowing = arena.revision();
        arena
            .constrain(variable, ValueType::Audio)
            .expect("repeating a constraint is harmless");
        assert_eq!(arena.revision(), after_narrowing);
    }

    #[test]
    fn equating_variables_intersects_their_domains_transitively() {
        let mut arena = TypeArena::default();
        let first = arena.allocate();
        let second = arena.allocate_exact(ValueType::Video);
        let third = arena.allocate();

        arena.equate(first, second).expect("domains overlap");
        arena.equate(third, first).expect("domains overlap");

        for variable in [first, second, third] {
            assert_eq!(arena.domain(variable).concrete(), Some(ValueType::Video));
        }
    }

    #[test]
    fn conflicts_leave_the_arena_unchanged() {
        let mut arena = TypeArena::default();
        let video = arena.allocate_exact(ValueType::Video);
        let audio = arena.allocate_exact(ValueType::Audio);
        let before_conflict = arena.revision();

        assert_eq!(
            arena.equate(video, audio),
            Err(TypeConflict {
                left: TypeDomain::from(ValueType::Video),
                right: TypeDomain::from(ValueType::Audio),
            })
        );
        assert_eq!(arena.revision(), before_conflict);
        assert_eq!(arena.domain(video).concrete(), Some(ValueType::Video));
        assert_eq!(arena.domain(audio).concrete(), Some(ValueType::Audio));

        assert!(arena.constrain(video, ValueType::Audio).is_err());
        assert_eq!(arena.revision(), before_conflict);
        assert_eq!(arena.domain(video).concrete(), Some(ValueType::Video));
    }

    #[test]
    fn a_clone_can_be_narrowed_for_a_retry_without_changing_the_original() {
        let mut original = TypeArena::default();
        let variable = original.allocate();
        let mut attempt = original.clone();

        attempt
            .constrain(variable, ValueType::Video)
            .expect("Video satisfies Timeline");

        assert_eq!(original.domain(variable).concrete(), None);
        assert_eq!(attempt.domain(variable).concrete(), Some(ValueType::Video));
        assert!(attempt.revision() > original.revision());
    }
}
