//! Monotonic type and stack resolution for authored programs.
//!
//! One recursive resolver owns selectors, explicit inputs, stack plans, body
//! contracts, and ordered output types. Exploratory passes narrow stable type
//! variables; the final pass records the concrete invocation decisions consumed
//! by checked-source materialization.

use std::collections::BTreeMap;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::ValueType;
use crate::program::{
    Cardinality, InputSlot, ProgramDefinition, ProgramImplementation, ResolvedSignature,
    StackAccess, ValueTypeSpec,
};
use crate::source::{Literal, OutputBindings, SourceSpan};

use super::check::LocalType;
use super::draft::{
    DraftBody, DraftInput, DraftInvocation, DraftItemKind, DraftParameter, DraftProgram,
};
use super::stack::{
    EvaluationStack, StackBindingInput, StackBindingOutcome, StackBindingPlan, StackCompatibility,
    StackFrame,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct TypeVarId(u32);

impl TypeVarId {
    const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct TypeDomain(u8);

impl TypeDomain {
    const VIDEO: u8 = 0b01;
    const AUDIO: u8 = 0b10;

    #[must_use]
    pub(super) const fn from_value_type(value_type: ValueType) -> Self {
        match value_type {
            ValueType::Video => Self(Self::VIDEO),
            ValueType::Audio => Self(Self::AUDIO),
        }
    }

    const TIMELINE: Self = Self(Self::VIDEO | Self::AUDIO);

    #[cfg(test)]
    #[must_use]
    pub(super) const fn contains(self, value_type: ValueType) -> bool {
        let candidate = Self::from_value_type(value_type);
        candidate.0 != 0 && self.0 & candidate.0 == candidate.0
    }

    #[must_use]
    pub(super) const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    #[must_use]
    pub(super) const fn is_subset_of(self, other: Self) -> bool {
        self.0 & !other.0 == 0
    }

    #[must_use]
    pub(super) const fn overlaps(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    #[must_use]
    pub(super) const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub(super) const fn concrete(self) -> Option<ValueType> {
        match self.0 {
            Self::VIDEO => Some(ValueType::Video),
            Self::AUDIO => Some(ValueType::Audio),
            _ => None,
        }
    }
}

impl From<ValueType> for TypeDomain {
    fn from(value_type: ValueType) -> Self {
        Self::from_value_type(value_type)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TypeConflict {
    pub(super) left: TypeDomain,
    pub(super) right: TypeDomain,
}

#[derive(Clone, Debug)]
struct TypeNode {
    parent: TypeVarId,
    domain: TypeDomain,
    rank: u8,
}

#[derive(Clone, Debug, Default)]
pub(super) struct TypeArena {
    nodes: Vec<TypeNode>,
    revision: u64,
}

impl TypeArena {
    #[must_use]
    pub(super) fn allocate(&mut self) -> TypeVarId {
        self.allocate_domain(TypeDomain::TIMELINE)
    }

    #[must_use]
    pub(super) fn allocate_exact(&mut self, value_type: ValueType) -> TypeVarId {
        self.allocate_domain(value_type.into())
    }

    pub(super) fn equate(
        &mut self,
        left: TypeVarId,
        right: TypeVarId,
    ) -> std::result::Result<(), TypeConflict> {
        let left_root = self.root(left);
        let right_root = self.root(right);
        let left_domain = self.nodes[left_root.index()].domain;
        let right_domain = self.nodes[right_root.index()].domain;
        let intersection = left_domain.intersection(right_domain);

        if intersection.is_empty() {
            return Err(TypeConflict {
                left: left_domain,
                right: right_domain,
            });
        }
        if left_root == right_root {
            return Ok(());
        }

        let left_rank = self.nodes[left_root.index()].rank;
        let right_rank = self.nodes[right_root.index()].rank;
        let (parent, child) = if left_rank < right_rank {
            (right_root, left_root)
        } else {
            (left_root, right_root)
        };

        self.nodes[child.index()].parent = parent;
        self.nodes[parent.index()].domain = intersection;
        if left_rank == right_rank {
            self.nodes[parent.index()].rank = self.nodes[parent.index()]
                .rank
                .checked_add(1)
                .expect("type arena rank overflow");
        }
        self.bump_revision();
        Ok(())
    }

    pub(super) fn constrain(
        &mut self,
        variable: TypeVarId,
        value_type: ValueType,
    ) -> std::result::Result<(), TypeConflict> {
        let root = self.root(variable);
        let current = self.nodes[root.index()].domain;
        let required = TypeDomain::from(value_type);
        let intersection = current.intersection(required);

        if intersection.is_empty() {
            return Err(TypeConflict {
                left: current,
                right: required,
            });
        }
        if intersection != current {
            self.nodes[root.index()].domain = intersection;
            self.bump_revision();
        }
        Ok(())
    }

    pub(super) fn constrain_domain(
        &mut self,
        variable: TypeVarId,
        required: TypeDomain,
    ) -> std::result::Result<(), TypeConflict> {
        let root = self.root(variable);
        let current = self.nodes[root.index()].domain;
        let intersection = current.intersection(required);

        if intersection.is_empty() {
            return Err(TypeConflict {
                left: current,
                right: required,
            });
        }
        if intersection != current {
            self.nodes[root.index()].domain = intersection;
            self.bump_revision();
        }
        Ok(())
    }

    #[must_use]
    pub(super) fn domain(&self, variable: TypeVarId) -> TypeDomain {
        self.nodes[self.root(variable).index()].domain
    }

    #[must_use]
    pub(super) const fn revision(&self) -> u64 {
        self.revision
    }

    fn allocate_domain(&mut self, domain: TypeDomain) -> TypeVarId {
        let raw = u32::try_from(self.nodes.len()).expect("too many type variables");
        let variable = TypeVarId(raw);
        self.nodes.push(TypeNode {
            parent: variable,
            domain,
            rank: 0,
        });
        self.bump_revision();
        variable
    }

    fn root(&self, variable: TypeVarId) -> TypeVarId {
        let mut current = variable;
        loop {
            let parent = self.nodes[current.index()].parent;
            if parent == current {
                return current;
            }
            current = parent;
        }
    }

    fn bump_revision(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("type arena revision overflow");
    }
}

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

#[derive(Clone, Debug)]
pub(super) struct ResolvedInvocation {
    pub(super) signature: ResolvedSignature,
    pub(super) stack_plan: StackBindingPlan,
}

#[derive(Clone, Debug)]
pub(super) struct InferenceResult {
    pub(super) invocations: Vec<ResolvedInvocation>,
    pub(super) outputs: Vec<ValueType>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PassPurpose {
    Infer,
    Resolve,
}

struct PassState {
    purpose: PassPurpose,
    deferred: usize,
    invocations: Vec<Option<ResolvedInvocation>>,
}

impl PassState {
    fn infer(invocation_count: usize) -> Self {
        Self {
            purpose: PassPurpose::Infer,
            deferred: 0,
            invocations: vec![None; invocation_count],
        }
    }

    fn resolve(invocation_count: usize) -> Self {
        Self {
            purpose: PassPurpose::Resolve,
            deferred: 0,
            invocations: vec![None; invocation_count],
        }
    }

    const fn is_resolving(&self) -> bool {
        matches!(self.purpose, PassPurpose::Resolve)
    }

    fn mark_deferred(&mut self) {
        self.deferred = self
            .deferred
            .checked_add(1)
            .expect("too many deferred type-inference decisions");
    }

    fn record(&mut self, invocation: &DraftInvocation, resolved: ResolvedInvocation) {
        if self.is_resolving() {
            self.invocations[invocation.id.0] = Some(resolved);
        }
    }
}

fn allocate_body_generics(
    body: &DraftBody,
    definitions: &[ProgramDefinition],
    arena: &mut TypeArena,
    generics: &mut [Option<TypeVarId>],
) {
    for item in &body.items {
        let DraftItemKind::Invocation(invocation) = &item.kind else {
            continue;
        };
        let definition = &definitions[invocation.program.index()];
        if definition.descriptor.type_selector.is_some() {
            generics[invocation.id.0] = Some(arena.allocate());
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
}

struct TypeState {
    arena: TypeArena,
    slots: BTreeMap<String, LocalSlot>,
    invocation_generics: Vec<Option<TypeVarId>>,
}

pub(super) fn resolve_program_types(
    program: &DraftProgram,
    locals: &mut BTreeMap<String, LocalType>,
    definitions: &[ProgramDefinition],
) -> Result<InferenceResult> {
    let mut types = prepare_type_state(program, locals, definitions);
    infer_fixpoint(program, definitions, &mut types)?;
    apply_resolved_local_types(locals, &types);
    resolve_final_program(program, definitions, &types)
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
    let mut invocation_generics = vec![None; program.invocation_count];
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
    types: &mut TypeState,
) -> Result<()> {
    loop {
        let before = types.arena.revision();
        let mut attempt = types.arena.clone();
        let mut state = PassState::infer(program.invocation_count);
        infer_program_body(
            program,
            definitions,
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
            .chain(types.invocation_generics.iter().flatten().copied())
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
    types: &TypeState,
) -> Result<InferenceResult> {
    let mut arena = types.arena.clone();
    let mut state = PassState::resolve(program.invocation_count);
    let outputs = infer_program_body(
        program,
        definitions,
        &types.slots,
        &types.invocation_generics,
        &mut arena,
        &mut state,
        "type resolution",
    )?;
    if state.deferred > 0 {
        return Err(inference_dependency(&program.span));
    }
    let invocations = state
        .invocations
        .into_iter()
        .enumerate()
        .map(|(index, resolved)| {
            resolved.ok_or_else(|| {
                Diagnostic::new(
                    "E_INTERNAL_TYPE_RESOLUTION",
                    format!("invocation {index} was not resolved"),
                    program.span.clone(),
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(InferenceResult {
        invocations,
        outputs,
    })
}

#[allow(clippy::too_many_arguments)]
fn infer_program_body(
    program: &DraftProgram,
    definitions: &[ProgramDefinition],
    slots: &BTreeMap<String, LocalSlot>,
    invocation_generics: &[Option<TypeVarId>],
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
    Diagnostic::new(
        "E_TYPE_INFERENCE_DEPENDENCY",
        "generic type inference depends on an unresolved stack selection; add `type: Video` or `type: Audio`",
        span.clone(),
    )
}

#[allow(clippy::too_many_arguments)]
fn infer_body(
    body: &DraftBody,
    globals: &BTreeMap<String, LocalSlot>,
    lexical: &BTreeMap<String, TypeVarId>,
    definitions: &[ProgramDefinition],
    invocation_generics: &[Option<TypeVarId>],
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
                    &reference.value,
                    &reference.span,
                )?]
            }
            DraftItemKind::Invocation(invocation) => infer_invocation(
                invocation,
                globals,
                lexical,
                definitions,
                invocation_generics,
                arena,
                stack,
                frame,
                state,
            )?,
        };
        constrain_bindings(globals, &item.output_bindings, &outputs, arena, &item.span)?;
        stack.extend(frame, outputs);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn infer_invocation(
    invocation: &DraftInvocation,
    globals: &BTreeMap<String, LocalSlot>,
    lexical: &BTreeMap<String, TypeVarId>,
    definitions: &[ProgramDefinition],
    invocation_generics: &[Option<TypeVarId>],
    arena: &mut TypeArena,
    stack: &mut EvaluationStack<TypeVarId>,
    frame: &mut StackFrame,
    state: &mut PassState,
) -> Result<Vec<TypeVarId>> {
    let deferred_before = state.deferred;
    let definition = &definitions[invocation.program.index()];
    let access = invocation.access;

    let generic = invocation_generics[invocation.id.0];
    if let Some(variable) = generic {
        let selector = definition
            .descriptor
            .type_selector
            .expect("generic definition");
        let selector_name = &definition.descriptor.parameter(selector).name;
        if let Some(argument) = &invocation.parameters[selector.index()] {
            let (selected, span) = match argument {
                DraftParameter::Literal(Literal::String(value, span)) if value == "Video" => {
                    (ValueType::Video, span)
                }
                DraftParameter::Literal(Literal::String(value, span)) if value == "Audio" => {
                    (ValueType::Audio, span)
                }
                DraftParameter::Literal(literal) => {
                    return Err(Diagnostic::new(
                        "E_INVALID_ARGUMENT_VALUE",
                        format!(
                            "parameter `{}.{selector_name}` must be `Video` or `Audio`",
                            definition.descriptor.name
                        ),
                        literal.span().clone(),
                    ));
                }
                DraftParameter::Reference(reference) => {
                    return Err(Diagnostic::new(
                        "E_INVALID_ARGUMENT_VALUE",
                        format!(
                            "parameter `{}.{selector_name}` must be `Video` or `Audio`",
                            definition.descriptor.name
                        ),
                        reference.span.clone(),
                    ));
                }
            };
            constrain(arena, variable, selected, span)?;
        }
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
            &invocation.name.value,
            &port.name,
            globals,
            lexical,
            definitions,
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
            definition, invocation, variable, arena, stack, frame, access, &slots, state,
        )?;
    }

    let stack_plan = bind_missing(
        definition, invocation, generic, arena, stack, frame, access, &mut slots, state,
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
            invocation.name.value.clone(),
            invocation.name.span.clone(),
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
            &invocation.name.value,
            &body_outputs,
            &contract.outputs,
            contract.count_error_code,
            generic,
            arena,
            state,
            &body.span,
        )?;
    }

    if state.is_resolving() {
        let generic = generic
            .map(|variable| concrete_type(arena, variable, &invocation.name.span))
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

#[allow(clippy::too_many_arguments)]
fn explicit_values(
    argument: &DraftInput,
    program: &str,
    port: &str,
    globals: &BTreeMap<String, LocalSlot>,
    lexical: &BTreeMap<String, TypeVarId>,
    definitions: &[ProgramDefinition],
    invocation_generics: &[Option<TypeVarId>],
    arena: &mut TypeArena,
    state: &mut PassState,
) -> Result<Vec<TypeVarId>> {
    match argument {
        DraftInput::Reference(reference) => Ok(vec![lookup_value(
            globals,
            lexical,
            &reference.value,
            &reference.span,
        )?]),
        DraftInput::References(references, _) => references
            .iter()
            .map(|reference| lookup_value(globals, lexical, &reference.value, &reference.span))
            .collect(),
        DraftInput::Body(body) => {
            let (mut stack, mut frame) =
                EvaluationStack::isolated("inline input type inference", body.span.clone());
            infer_body(
                body,
                globals,
                lexical,
                definitions,
                invocation_generics,
                arena,
                &mut stack,
                &mut frame,
                state,
            )?;
            if stack.len() != 1 {
                return Err(Diagnostic::new(
                    "E_INPUT_BODY_OUTPUT_COUNT",
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

#[allow(clippy::too_many_arguments)]
fn infer_generic_from_stack(
    definition: &ProgramDefinition,
    invocation: &DraftInvocation,
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
                equate(
                    arena,
                    generic,
                    stack.values()[selected],
                    &invocation.name.span,
                )?;
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
                    constrain(arena, generic, value_type, &invocation.name.span)?;
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
        return Err(Diagnostic::new(
            "E_AMBIGUOUS_GENERIC_TYPE",
            format!(
                "program `{}` can bind both Video and Audio; set `type: Video` or `type: Audio`",
                invocation.name.value
            ),
            invocation.name.span.clone(),
        ));
    }
    if possible.len() == 1 && !saw_deferred {
        constrain(arena, generic, possible[0], &invocation.name.span)?;
    } else if possible.len() != 1 || saw_deferred {
        state.mark_deferred();
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn bind_missing(
    definition: &ProgramDefinition,
    invocation: &DraftInvocation,
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
                        Diagnostic::new(
                            "E_STACK_UNDERFLOW",
                            format!(
                                "program `{}` needs a preceding Video or Audio value",
                                invocation.name.value
                            ),
                            invocation.name.span.clone(),
                        )
                    })?,
            };
            let (code, requirement) = match port.cardinality {
                Cardinality::One => (
                    "E_STACK_UNDERFLOW",
                    format!(
                        "`{}.{}` needs one preceding {required} value",
                        invocation.name.value, port.name
                    ),
                ),
                Cardinality::Variadic { min } => (
                    "E_MISSING_REQUIRED_INPUT",
                    format!(
                        "`{}.{}` needs at least {min} {required} value(s)",
                        invocation.name.value, port.name
                    ),
                ),
            };
            Err(stack.underflow_with(
                frame,
                access,
                code,
                &requirement,
                required,
                failure.available,
                &failure.selected,
                &invocation.name.span,
                |value| arena.domain(value).concrete(),
            ))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn constrain_body_outputs(
    program: &str,
    values: &[TypeVarId],
    constraint: &crate::program::BodyOutputConstraint,
    count_error_code: &'static str,
    generic: Option<TypeVarId>,
    arena: &mut TypeArena,
    state: &PassState,
    span: &SourceSpan,
) -> Result<()> {
    match constraint {
        crate::program::BodyOutputConstraint::Exactly(expected) => {
            if values.len() != expected.len() {
                if state.is_resolving() {
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
                return Ok(());
            }
            for (actual, expected) in values.iter().zip(expected) {
                constrain_body_output(*actual, *expected, generic, arena, span)?;
            }
        }
        crate::program::BodyOutputConstraint::Variadic { value_type, min } => {
            if values.len() < *min {
                if state.is_resolving() {
                    return Err(Diagnostic::new(
                        count_error_code,
                        format!("`{program}` body must produce at least {min} value(s)"),
                        span.clone(),
                    ));
                }
                return Ok(());
            }
            for actual in values {
                constrain_body_output(*actual, *value_type, generic, arena, span)?;
            }
        }
    }
    Ok(())
}

fn constrain_body_output(
    actual: TypeVarId,
    expected: ValueTypeSpec,
    generic: Option<TypeVarId>,
    arena: &mut TypeArena,
    span: &SourceSpan,
) -> Result<()> {
    match expected {
        ValueTypeSpec::Exact(value_type) => constrain(arena, actual, value_type, span),
        ValueTypeSpec::Generic => {
            equate(arena, actual, generic.expect("generic body output"), span)
        }
    }
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
    bindings: &OutputBindings,
    outputs: &[TypeVarId],
    arena: &mut TypeArena,
    span: &SourceSpan,
) -> Result<()> {
    match bindings {
        OutputBindings::None => {}
        OutputBindings::One(name) => {
            if let [output] = outputs {
                equate(
                    arena,
                    value_slot(globals, &name.value, &name.span)?,
                    *output,
                    span,
                )?;
            }
        }
        OutputBindings::Many(names, _) => {
            for (name, output) in names.iter().zip(outputs) {
                equate(
                    arena,
                    value_slot(globals, &name.value, &name.span)?,
                    *output,
                    span,
                )?;
            }
        }
    }
    Ok(())
}

fn lookup_value(
    globals: &BTreeMap<String, LocalSlot>,
    lexical: &BTreeMap<String, TypeVarId>,
    name: &str,
    span: &SourceSpan,
) -> Result<TypeVarId> {
    lexical
        .get(name)
        .copied()
        .map_or_else(|| value_slot(globals, name, span), Ok)
}

fn value_slot(
    slots: &BTreeMap<String, LocalSlot>,
    name: &str,
    span: &SourceSpan,
) -> Result<TypeVarId> {
    match slots.get(name) {
        Some(LocalSlot::Value(variable)) => Ok(*variable),
        Some(LocalSlot::Parameter) => Err(Diagnostic::new(
            "E_PARAMETER_NOT_VALUE",
            format!("parameter `${name}` is not a graph value"),
            span.clone(),
        )),
        None => Err(Diagnostic::new(
            "E_MISSING_REFERENCE",
            format!("reference `${name}` does not name a local input, clip, or id"),
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
    Diagnostic::new(
        "E_GENERIC_TYPE_MISMATCH",
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
