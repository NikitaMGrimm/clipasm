use std::collections::BTreeMap;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::ValueType;
use crate::program::{
    Cardinality, ProgramDefinition, ProgramId, ProgramImplementation, StackAccess, ValueConstraint,
    ValueTypeSpec,
};
use crate::source::{
    ArgumentValue, ItemKind, Literal, OutputBindings, ProgramBody, SourceProgram, SourceSpan,
};

use super::check::LocalType;
use super::stack::{
    EvaluationStack, StackBindingInput, StackBindingOutcome, StackCompatibility, StackFrame,
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
    #[cfg(test)]
    const TEST: u8 = 0b100;

    #[must_use]
    pub(super) const fn from_value_type(value_type: ValueType) -> Self {
        match value_type {
            ValueType::Video => Self(Self::VIDEO),
            ValueType::Audio => Self(Self::AUDIO),
            #[cfg(test)]
            ValueType::Test => Self(Self::TEST),
        }
    }

    #[must_use]
    pub(super) const fn from_constraint(constraint: ValueConstraint) -> Self {
        match constraint {
            ValueConstraint::Timeline | ValueConstraint::Any => Self(Self::VIDEO | Self::AUDIO),
        }
    }

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
            #[cfg(test)]
            Self::TEST => Some(ValueType::Test),
            _ => None,
        }
    }
}

impl From<ValueType> for TypeDomain {
    fn from(value_type: ValueType) -> Self {
        Self::from_value_type(value_type)
    }
}

impl From<ValueConstraint> for TypeDomain {
    fn from(constraint: ValueConstraint) -> Self {
        Self::from_constraint(constraint)
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
    pub(super) fn allocate(&mut self, constraint: ValueConstraint) -> TypeVarId {
        self.allocate_domain(constraint.into())
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

#[derive(Default)]
struct PassState {
    deferred: usize,
}

impl PassState {
    fn mark_deferred(&mut self) {
        self.deferred = self
            .deferred
            .checked_add(1)
            .expect("too many deferred type-inference decisions");
    }
}

pub(super) fn resolve_local_types(
    program: &SourceProgram,
    locals: &mut BTreeMap<String, LocalType>,
    definitions: &[ProgramDefinition],
    builtins: &BTreeMap<String, ProgramId>,
    namespace: &BTreeMap<String, ProgramId>,
) -> Result<()> {
    let mut arena = TypeArena::default();
    let mut slots = BTreeMap::new();

    for (name, local) in locals.iter() {
        let slot = match local {
            LocalType::Value(value_type) => LocalSlot::Value(arena.allocate_exact(*value_type)),
            LocalType::Parameter(_) => LocalSlot::Parameter,
            LocalType::Alias(_) => LocalSlot::Value(arena.allocate(ValueConstraint::Any)),
            LocalType::Deferred(deferred) => {
                let definition = &definitions[deferred.program.index()];
                let constraint = definition
                    .descriptor
                    .type_parameter
                    .as_ref()
                    .expect("deferred output belongs to generic program")
                    .constraint;
                LocalSlot::Value(arena.allocate(constraint))
            }
        };
        slots.insert(name.clone(), slot);
    }

    for (name, local) in locals.iter() {
        if let LocalType::Alias(target) = local {
            let span = SourceSpan::file_start("<type-inference>");
            let left = value_slot(&slots, name, &span)?;
            let right = value_slot(&slots, target, &span)?;
            equate(&mut arena, left, right, &span)?;
        }
    }

    loop {
        let before = arena.revision();
        let mut attempt = arena.clone();
        let mut state = PassState::default();

        for clip in program.clips() {
            let (mut stack, mut frame) = EvaluationStack::isolated(
                format!("named clip `{}` type inference", clip.name),
                clip.span.clone(),
            );
            infer_body(
                &clip.body,
                &slots,
                &BTreeMap::new(),
                definitions,
                builtins,
                namespace,
                &mut attempt,
                &mut stack,
                &mut frame,
                &mut state,
            )?;
        }

        let (mut stack, mut frame) =
            EvaluationStack::isolated("source program type inference", program.span().clone());
        infer_body(
            program.body(),
            &slots,
            &BTreeMap::new(),
            definitions,
            builtins,
            namespace,
            &mut attempt,
            &mut stack,
            &mut frame,
            &mut state,
        )?;

        for slot in slots.values().copied() {
            if let LocalSlot::Value(variable) = slot {
                arena
                    .constrain_domain(variable, attempt.domain(variable))
                    .map_err(|_| type_mismatch(program.span()))?;
            }
        }

        if arena.revision() == before {
            if state.deferred > 0 {
                return Err(Diagnostic::new(
                    "E_TYPE_INFERENCE_DEPENDENCY",
                    "generic type inference depends on an unresolved stack selection; add `type: Video` or `type: Audio`",
                    program.span().clone(),
                ));
            }
            break;
        }
    }

    for (name, slot) in slots {
        let LocalSlot::Value(variable) = slot else {
            continue;
        };
        let Some(value_type) = arena.domain(variable).concrete() else {
            continue;
        };
        locals.insert(name, LocalType::Value(value_type));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn infer_body(
    body: &ProgramBody,
    globals: &BTreeMap<String, LocalSlot>,
    lexical: &BTreeMap<String, TypeVarId>,
    definitions: &[ProgramDefinition],
    builtins: &BTreeMap<String, ProgramId>,
    namespace: &BTreeMap<String, ProgramId>,
    arena: &mut TypeArena,
    stack: &mut EvaluationStack<TypeVarId>,
    frame: &mut StackFrame,
    state: &mut PassState,
) -> Result<()> {
    for item in &body.items {
        let outputs = match &item.kind {
            ItemKind::Reference(reference) => {
                vec![lookup_value(
                    globals,
                    lexical,
                    &reference.name.value,
                    &reference.name.span,
                )?]
            }
            ItemKind::Invocation(invocation) => infer_invocation(
                invocation,
                globals,
                lexical,
                definitions,
                builtins,
                namespace,
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
    invocation: &crate::source::Invocation,
    globals: &BTreeMap<String, LocalSlot>,
    lexical: &BTreeMap<String, TypeVarId>,
    definitions: &[ProgramDefinition],
    builtins: &BTreeMap<String, ProgramId>,
    namespace: &BTreeMap<String, ProgramId>,
    arena: &mut TypeArena,
    stack: &mut EvaluationStack<TypeVarId>,
    frame: &mut StackFrame,
    state: &mut PassState,
) -> Result<Vec<TypeVarId>> {
    let deferred_before = state.deferred;
    let program = program_id_for(
        &invocation.program.value,
        builtins,
        namespace,
        &invocation.program.span,
    )?;
    let definition = &definitions[program.index()];
    let access = invocation
        .stack_access
        .as_ref()
        .map_or(definition.descriptor.default_stack_access, |value| {
            value.value
        });

    let generic = definition
        .descriptor
        .type_parameter
        .as_ref()
        .map(|parameter| arena.allocate(parameter.constraint));
    if let Some(variable) = generic {
        let selector = &definition
            .descriptor
            .type_parameter
            .as_ref()
            .expect("generic definition")
            .selector;
        if let Some(argument) = invocation.arguments.get(selector) {
            let selected = match argument {
                ArgumentValue::Literal(Literal::String(value, _)) if value == "Video" => {
                    ValueType::Video
                }
                ArgumentValue::Literal(Literal::String(value, _)) if value == "Audio" => {
                    ValueType::Audio
                }
                _ => return Ok(Vec::new()),
            };
            constrain(arena, variable, selected, argument.span())?;
        }
    }

    let mut slots = vec![None; definition.descriptor.inputs.len()];
    for (index, port) in definition.descriptor.inputs.iter().enumerate() {
        let Some(argument) = invocation.arguments.get(&port.name) else {
            continue;
        };
        let values = explicit_values(
            argument,
            globals,
            lexical,
            definitions,
            builtins,
            namespace,
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

    bind_missing(
        definition, generic, arena, stack, frame, access, &mut slots, state,
    );
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

    if let ProgramImplementation::Body(_) = definition.implementation {
        let body = invocation.body.as_ref().expect("checked later");
        let contract = definition.body_contract.as_ref().expect("body contract");
        let mut child = EvaluationStack::<TypeVarId>::enter_body(
            frame,
            access,
            invocation.program.value.clone(),
            invocation.program.span.clone(),
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
            builtins,
            namespace,
            arena,
            stack,
            &mut child,
            state,
        )?;
        if state.deferred > deferred_before {
            return Ok(invocation_outputs(definition, generic, arena));
        }
        let body_outputs = stack.finish_body(&child);
        match &contract.outputs {
            crate::program::BodyOutputConstraint::Exactly(expected) => {
                if body_outputs.len() == expected.len() {
                    for (actual, expected) in body_outputs.iter().zip(expected) {
                        match expected {
                            ValueTypeSpec::Exact(value_type) => {
                                constrain(arena, *actual, *value_type, &body.span)?;
                            }
                            ValueTypeSpec::Generic => {
                                equate(
                                    arena,
                                    *actual,
                                    generic.expect("generic body output"),
                                    &body.span,
                                )?;
                            }
                        }
                    }
                }
            }
            crate::program::BodyOutputConstraint::Variadic { value_type, .. } => {
                for actual in body_outputs {
                    match value_type {
                        ValueTypeSpec::Exact(expected) => {
                            constrain(arena, actual, *expected, &body.span)?;
                        }
                        ValueTypeSpec::Generic => {
                            equate(
                                arena,
                                actual,
                                generic.expect("generic body output"),
                                &body.span,
                            )?;
                        }
                    }
                }
            }
        }
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
    argument: &ArgumentValue,
    globals: &BTreeMap<String, LocalSlot>,
    lexical: &BTreeMap<String, TypeVarId>,
    definitions: &[ProgramDefinition],
    builtins: &BTreeMap<String, ProgramId>,
    namespace: &BTreeMap<String, ProgramId>,
    arena: &mut TypeArena,
    state: &mut PassState,
) -> Result<Vec<TypeVarId>> {
    match argument {
        ArgumentValue::Reference(reference) => Ok(vec![lookup_value(
            globals,
            lexical,
            &reference.value,
            &reference.span,
        )?]),
        ArgumentValue::References(references, _) => references
            .iter()
            .map(|reference| lookup_value(globals, lexical, &reference.value, &reference.span))
            .collect(),
        ArgumentValue::Body(body) => {
            let (mut stack, mut frame) =
                EvaluationStack::isolated("inline input type inference", body.span.clone());
            infer_body(
                body,
                globals,
                lexical,
                definitions,
                builtins,
                namespace,
                arena,
                &mut stack,
                &mut frame,
                state,
            )?;
            Ok(stack.values().to_vec())
        }
        ArgumentValue::Literal(_) => Ok(Vec::new()),
    }
}

#[allow(clippy::too_many_arguments)]
fn infer_generic_from_stack(
    definition: &ProgramDefinition,
    invocation: &crate::source::Invocation,
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
            port: missing[0].0,
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
                    &invocation.program.span,
                )?;
            }
            StackBindingOutcome::Deferred => state.mark_deferred(),
            StackBindingOutcome::Impossible(_) => {}
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
                port: *index,
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
                invocation.program.value
            ),
            invocation.program.span.clone(),
        ));
    }
    if possible.len() == 1 && !saw_deferred {
        constrain(arena, generic, possible[0], &invocation.program.span)?;
    } else if possible.len() != 1 || saw_deferred {
        state.mark_deferred();
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn bind_missing(
    definition: &ProgramDefinition,
    generic: Option<TypeVarId>,
    arena: &TypeArena,
    stack: &mut EvaluationStack<TypeVarId>,
    frame: &StackFrame,
    access: StackAccess,
    slots: &mut [Option<Vec<TypeVarId>>],
    state: &mut PassState,
) {
    let inputs = definition
        .descriptor
        .inputs
        .iter()
        .enumerate()
        .filter(|(index, _)| slots[*index].is_none())
        .map(|(index, port)| StackBindingInput {
            port: index,
            requirement: match port.value_type {
                ValueTypeSpec::Exact(value_type) => Requirement::Exact(value_type),
                ValueTypeSpec::Generic => Requirement::Generic(generic.expect("generic port")),
            },
            cardinality: port.cardinality,
        })
        .collect::<Vec<_>>();
    if inputs.is_empty() {
        return;
    }
    match stack.plan_bindings(frame, access, &inputs, |value, requirement| {
        compatibility(arena, value, requirement)
    }) {
        StackBindingOutcome::Resolved(plan) => {
            for bound in stack.apply_binding_plan(&plan) {
                slots[bound.port] = Some(bound.values);
            }
        }
        StackBindingOutcome::Deferred => state.mark_deferred(),
        StackBindingOutcome::Impossible(_) => {}
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains_represent_the_closed_value_type_set() {
        let all = TypeDomain::from(ValueConstraint::Timeline);
        let video = TypeDomain::from(ValueType::Video);
        let audio = TypeDomain::from(ValueType::Audio);

        assert!(all.contains(ValueType::Video));
        assert!(all.contains(ValueType::Audio));
        assert!(!all.contains(ValueType::Test));
        assert_eq!(
            TypeDomain::from(ValueType::Test).concrete(),
            Some(ValueType::Test)
        );
        assert_eq!(video.concrete(), Some(ValueType::Video));
        assert_eq!(audio.concrete(), Some(ValueType::Audio));
        assert!(video.intersection(audio).is_empty());
        assert_eq!(all.intersection(video), video);
        assert_eq!(TypeDomain::from(ValueConstraint::Any), all);
    }

    #[test]
    fn constraints_narrow_a_variable_once() {
        let mut arena = TypeArena::default();
        let variable = arena.allocate(ValueConstraint::Timeline);
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
        let first = arena.allocate(ValueConstraint::Any);
        let second = arena.allocate_exact(ValueType::Video);
        let third = arena.allocate(ValueConstraint::Timeline);

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
        let variable = original.allocate(ValueConstraint::Timeline);
        let mut attempt = original.clone();

        attempt
            .constrain(variable, ValueType::Video)
            .expect("Video satisfies Timeline");

        assert_eq!(original.domain(variable).concrete(), None);
        assert_eq!(attempt.domain(variable).concrete(), Some(ValueType::Video));
        assert!(attempt.revision() > original.revision());
    }
}
