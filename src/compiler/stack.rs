use crate::diagnostic::{BuiltinDiagnostic, Diagnostic};
use crate::model::{ValueRef, ValueType};
use crate::program::{Cardinality, InputSlot, StackAccess};
use crate::source::SourceSpan;

#[derive(Clone, Debug)]
pub(super) struct VisibilityBoundary {
    owner: String,
    span: SourceSpan,
}

impl VisibilityBoundary {
    pub(super) fn new(owner: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            owner: owner.into(),
            span,
        }
    }

    pub(super) fn owner(&self) -> &str {
        &self.owner
    }

    pub(super) const fn span(&self) -> &SourceSpan {
        &self.span
    }
}

#[derive(Clone, Debug)]
pub(super) struct StackFrame {
    depth: usize,
    visible_depth: usize,
    boundary: VisibilityBoundary,
}

impl StackFrame {
    fn root(boundary: VisibilityBoundary) -> Self {
        Self {
            depth: 0,
            visible_depth: 0,
            boundary,
        }
    }
}

#[derive(Debug)]
pub(super) struct EvaluationStack<T = ValueRef> {
    values: Vec<T>,
    owners: Vec<usize>,
}

impl<T> EvaluationStack<T> {
    pub(super) fn isolated(owner: impl Into<String>, span: SourceSpan) -> (Self, StackFrame) {
        let boundary = VisibilityBoundary::new(owner, span);
        (
            Self {
                values: Vec::new(),
                owners: Vec::new(),
            },
            StackFrame::root(boundary),
        )
    }

    pub(super) fn len(&self) -> usize {
        self.values.len()
    }

    pub(super) fn push(&mut self, frame: &StackFrame, value: T) {
        self.values.push(value);
        self.owners.push(frame.depth);
    }

    pub(super) fn extend(&mut self, frame: &StackFrame, values: impl IntoIterator<Item = T>) {
        for value in values {
            self.push(frame, value);
        }
    }

    pub(super) fn values(&self) -> &[T] {
        &self.values
    }

    pub(super) fn enter_body(
        parent: &StackFrame,
        access: StackAccess,
        owner: impl Into<String>,
        span: SourceSpan,
    ) -> StackFrame {
        let depth = parent.depth + 1;
        match access {
            StackAccess::Owned => StackFrame {
                depth,
                visible_depth: depth,
                boundary: VisibilityBoundary::new(owner, span),
            },
            StackAccess::Visible => StackFrame {
                depth,
                visible_depth: parent.visible_depth,
                boundary: parent.boundary.clone(),
            },
        }
    }

    pub(super) fn finish_body(&mut self, child: &StackFrame) -> Vec<T> {
        let mut owned = Vec::new();
        let mut retained_values = Vec::with_capacity(self.values.len());
        let mut retained_owners = Vec::with_capacity(self.owners.len());
        for (value, owner) in self.values.drain(..).zip(self.owners.drain(..)) {
            if owner == child.depth {
                owned.push(value);
            } else {
                debug_assert!(owner < child.depth);
                retained_values.push(value);
                retained_owners.push(owner);
            }
        }
        self.values = retained_values;
        self.owners = retained_owners;
        owned
    }
}

#[cfg(test)]
pub(super) trait StackValue {
    fn value_type(self) -> ValueType;
}

#[cfg(test)]
impl StackValue for ValueRef {
    fn value_type(self) -> ValueType {
        self.value_type()
    }
}

#[cfg(test)]
impl StackValue for ValueType {
    fn value_type(self) -> ValueType {
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StackCompatibility {
    Incompatible,
    Possible,
    Definite,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct StackBindingInput<R> {
    pub(super) port: InputSlot,
    pub(super) requirement: R,
    pub(super) cardinality: Cardinality,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PlannedStackInput {
    pub(super) port: InputSlot,
    pub(super) indices: Vec<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct StackBindingPlan {
    pub(super) inputs: Vec<PlannedStackInput>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct StackBindingFailure {
    pub(super) port: InputSlot,
    pub(super) available: usize,
    pub(super) selected: Vec<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum StackBindingOutcome {
    Resolved(StackBindingPlan),
    Deferred,
    Impossible(StackBindingFailure),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct BoundStackInput<T> {
    pub(super) port: InputSlot,
    pub(super) values: Vec<T>,
}

impl<T: Copy> EvaluationStack<T> {
    pub(super) fn plan_bindings<R: Copy>(
        &self,
        frame: &StackFrame,
        access: StackAccess,
        inputs: &[StackBindingInput<R>],
        mut compatibility: impl FnMut(T, R) -> StackCompatibility,
    ) -> StackBindingOutcome {
        let mut available = vec![true; self.values.len()];
        let mut planned: Vec<PlannedStackInput> = Vec::with_capacity(inputs.len());

        for input in inputs
            .iter()
            .filter(|input| matches!(input.cardinality, Cardinality::One))
            .rev()
        {
            let mut selected = None;
            for index in (0..self.values.len()).rev() {
                if !available[index] || !Self::accessible(self.owners[index], frame, access) {
                    continue;
                }
                match compatibility(self.values[index], input.requirement) {
                    StackCompatibility::Incompatible => {}
                    StackCompatibility::Possible => return StackBindingOutcome::Deferred,
                    StackCompatibility::Definite => {
                        selected = Some(index);
                        break;
                    }
                }
            }
            let Some(index) = selected else {
                return StackBindingOutcome::Impossible(StackBindingFailure {
                    port: input.port,
                    available: 0,
                    selected: planned
                        .iter()
                        .flat_map(|input| input.indices.iter().copied())
                        .collect(),
                });
            };
            available[index] = false;
            planned.push(PlannedStackInput {
                port: input.port,
                indices: vec![index],
            });
        }

        for input in inputs
            .iter()
            .filter(|input| matches!(input.cardinality, Cardinality::Variadic { .. }))
        {
            let mut indices = Vec::new();
            let mut possible = 0;
            for (index, is_available) in available.iter().copied().enumerate() {
                if !is_available || !Self::accessible(self.owners[index], frame, access) {
                    continue;
                }
                match compatibility(self.values[index], input.requirement) {
                    StackCompatibility::Incompatible => {}
                    StackCompatibility::Possible => possible += 1,
                    StackCompatibility::Definite => indices.push(index),
                }
            }
            let Cardinality::Variadic { min } = input.cardinality else {
                unreachable!("filtered variadic stack input")
            };
            if indices.len() + possible < min {
                return StackBindingOutcome::Impossible(StackBindingFailure {
                    port: input.port,
                    available: indices.len(),
                    selected: planned
                        .iter()
                        .flat_map(|input| input.indices.iter().copied())
                        .collect(),
                });
            }
            if possible > 0 {
                return StackBindingOutcome::Deferred;
            }
            for index in &indices {
                available[*index] = false;
            }
            planned.push(PlannedStackInput {
                port: input.port,
                indices,
            });
        }

        planned.sort_by_key(|input| input.port);
        StackBindingOutcome::Resolved(StackBindingPlan { inputs: planned })
    }

    pub(super) fn nearest_accessible_with<R>(
        &self,
        frame: &StackFrame,
        access: StackAccess,
        mut value: impl FnMut(T) -> Option<R>,
    ) -> Option<R> {
        self.values
            .iter()
            .copied()
            .zip(self.owners.iter().copied())
            .rev()
            .find_map(|(candidate, owner)| {
                Self::accessible(owner, frame, access)
                    .then(|| value(candidate))
                    .flatten()
            })
    }

    fn accessible(owner: usize, frame: &StackFrame, access: StackAccess) -> bool {
        match access {
            StackAccess::Owned => owner == frame.depth,
            StackAccess::Visible => owner >= frame.visible_depth && owner <= frame.depth,
        }
    }
}

impl<T: Copy> EvaluationStack<T> {
    pub(super) fn apply_binding_plan(
        &mut self,
        plan: &StackBindingPlan,
    ) -> Vec<BoundStackInput<T>> {
        let bound = plan
            .inputs
            .iter()
            .map(|input| BoundStackInput {
                port: input.port,
                values: input
                    .indices
                    .iter()
                    .map(|index| self.values[*index])
                    .collect(),
            })
            .collect();
        debug_assert_eq!(self.values.len(), self.owners.len());
        let mut consumed = vec![false; self.values.len()];
        for index in plan
            .inputs
            .iter()
            .flat_map(|input| input.indices.iter().copied())
        {
            debug_assert!(!consumed[index]);
            consumed[index] = true;
        }
        let mut index = 0;
        self.values.retain(|_| {
            let retain = !consumed[index];
            index += 1;
            retain
        });
        index = 0;
        self.owners.retain(|_| {
            let retain = !consumed[index];
            index += 1;
            retain
        });
        bound
    }
}

#[cfg(test)]
impl<T: Copy + StackValue> EvaluationStack<T> {
    #[expect(
        clippy::too_many_arguments,
        reason = "the test adapter mirrors the complete production underflow diagnostic contract"
    )]
    pub(super) fn underflow(
        &self,
        frame: &StackFrame,
        access: StackAccess,
        diagnostic: BuiltinDiagnostic,
        requirement: &str,
        required: ValueType,
        available: usize,
        selected: &[usize],
        span: &SourceSpan,
    ) -> Diagnostic {
        self.underflow_with(
            frame,
            access,
            diagnostic,
            requirement,
            required,
            available,
            selected,
            span,
            |value| Some(value.value_type()),
        )
    }
}

impl<T: Copy> EvaluationStack<T> {
    #[expect(
        clippy::too_many_arguments,
        reason = "underflow diagnostics need the selected indices, availability, access boundary, and value-type projection"
    )]
    pub(super) fn underflow_with(
        &self,
        frame: &StackFrame,
        access: StackAccess,
        diagnostic: BuiltinDiagnostic,
        requirement: &str,
        required: ValueType,
        available: usize,
        selected: &[usize],
        span: &SourceSpan,
        value_type: impl Fn(T) -> Option<ValueType>,
    ) -> Diagnostic {
        let mut diagnostic = Diagnostic::builtin(
            diagnostic,
            format!(
                "{requirement}, but only {available} {} {required} value(s) are available",
                access.label(),
            ),
            span.clone(),
        );
        if access == StackAccess::Owned {
            let visible = self
                .values
                .iter()
                .copied()
                .zip(self.owners.iter().copied())
                .enumerate()
                .filter(|(index, (value, owner))| {
                    !selected.contains(index)
                        && Self::accessible(*owner, frame, StackAccess::Visible)
                        && value_type(*value) == Some(required)
                })
                .count();
            if visible > available {
                diagnostic = diagnostic.note(format!(
                    "{} additional {required} value(s) are visible outside this invocation's owned values; add `@visible` to permit consuming them",
                    visible - available
                ));
            }
        } else {
            diagnostic = diagnostic.note(format!(
                "the body of `{}` at {}:{}:{} establishes the nearest stack visibility boundary",
                frame.boundary.owner(),
                frame.boundary.span().file().display(),
                frame.boundary.span().line,
                frame.boundary.span().column
            ));
        }
        diagnostic
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ValueId, ValueType};

    const VIDEO_DOMAIN: u8 = 1;
    const AUDIO_DOMAIN: u8 = 2;
    const TIMELINE_DOMAIN: u8 = VIDEO_DOMAIN | AUDIO_DOMAIN;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct AbstractValue {
        id: u8,
        domain: u8,
    }

    fn abstract_compatibility(value: AbstractValue, required: u8) -> StackCompatibility {
        match value.domain & required {
            0 => StackCompatibility::Incompatible,
            _ if value.domain == required => StackCompatibility::Definite,
            _ => StackCompatibility::Possible,
        }
    }

    fn value(id: u32) -> ValueRef {
        ValueRef::new(ValueId::new(id), ValueType::Video)
    }

    trait TestStackExt {
        fn take_one_matching(
            &mut self,
            frame: &StackFrame,
            access: StackAccess,
            required: ValueType,
            program: &str,
            port: &str,
            span: &SourceSpan,
        ) -> Result<ValueRef, Diagnostic>;

        #[expect(
            clippy::too_many_arguments,
            reason = "the test helper mirrors variadic binding inputs so tests stay at the public stack-planning boundary"
        )]
        fn take_all_matching(
            &mut self,
            frame: &StackFrame,
            access: StackAccess,
            required: ValueType,
            min: usize,
            program: &str,
            port: &str,
            span: &SourceSpan,
        ) -> Result<Vec<ValueRef>, Diagnostic>;
    }

    impl TestStackExt for EvaluationStack<ValueRef> {
        fn take_one_matching(
            &mut self,
            frame: &StackFrame,
            access: StackAccess,
            required: ValueType,
            program: &str,
            port: &str,
            span: &SourceSpan,
        ) -> Result<ValueRef, Diagnostic> {
            let inputs = [StackBindingInput {
                port: InputSlot::new(0),
                requirement: required,
                cardinality: Cardinality::One,
            }];
            let plan = match self.plan_bindings(frame, access, &inputs, exact_compatibility) {
                StackBindingOutcome::Resolved(plan) => plan,
                StackBindingOutcome::Deferred => unreachable!("exact compatibility is concrete"),
                StackBindingOutcome::Impossible(failure) => {
                    return Err(self.underflow(
                        frame,
                        access,
                        BuiltinDiagnostic::StackUnderflow,
                        &format!("`{program}.{port}` needs one preceding {required} value"),
                        required,
                        failure.available,
                        &failure.selected,
                        span,
                    ));
                }
            };
            Ok(self.apply_binding_plan(&plan)[0].values[0])
        }

        fn take_all_matching(
            &mut self,
            frame: &StackFrame,
            access: StackAccess,
            required: ValueType,
            min: usize,
            program: &str,
            port: &str,
            span: &SourceSpan,
        ) -> Result<Vec<ValueRef>, Diagnostic> {
            let inputs = [StackBindingInput {
                port: InputSlot::new(0),
                requirement: required,
                cardinality: Cardinality::Variadic { min },
            }];
            let plan = match self.plan_bindings(frame, access, &inputs, exact_compatibility) {
                StackBindingOutcome::Resolved(plan) => plan,
                StackBindingOutcome::Deferred => unreachable!("exact compatibility is concrete"),
                StackBindingOutcome::Impossible(failure) => {
                    return Err(self.underflow(
                        frame,
                        access,
                        BuiltinDiagnostic::MissingRequiredInput,
                        &format!("`{program}.{port}` needs at least {min} {required} value(s)"),
                        required,
                        failure.available,
                        &failure.selected,
                        span,
                    ));
                }
            };
            Ok(self.apply_binding_plan(&plan)[0].values.clone())
        }
    }

    fn exact_compatibility(value: ValueRef, required: ValueType) -> StackCompatibility {
        if value.value_type() == required {
            StackCompatibility::Definite
        } else {
            StackCompatibility::Incompatible
        }
    }

    fn root() -> (EvaluationStack, StackFrame) {
        EvaluationStack::isolated("source program", SourceSpan::file_start("workflow.clipasm"))
    }

    #[test]
    fn planner_selects_missing_fixed_inputs_from_last_port_to_first() {
        let (mut stack, frame) =
            EvaluationStack::isolated("inference", SourceSpan::file_start("workflow.clipasm"));
        stack.extend(
            &frame,
            [
                AbstractValue {
                    id: 0,
                    domain: VIDEO_DOMAIN,
                },
                AbstractValue {
                    id: 1,
                    domain: AUDIO_DOMAIN,
                },
                AbstractValue {
                    id: 2,
                    domain: VIDEO_DOMAIN,
                },
            ],
        );
        let inputs = [
            StackBindingInput {
                port: InputSlot::new(0),
                requirement: VIDEO_DOMAIN,
                cardinality: Cardinality::One,
            },
            StackBindingInput {
                port: InputSlot::new(2),
                requirement: VIDEO_DOMAIN,
                cardinality: Cardinality::One,
            },
        ];

        let StackBindingOutcome::Resolved(plan) =
            stack.plan_bindings(&frame, StackAccess::Owned, &inputs, abstract_compatibility)
        else {
            panic!("fixed binding should resolve");
        };
        assert_eq!(
            plan.inputs,
            vec![
                PlannedStackInput {
                    port: InputSlot::new(0),
                    indices: vec![0],
                },
                PlannedStackInput {
                    port: InputSlot::new(2),
                    indices: vec![2],
                },
            ]
        );

        let bound = stack.apply_binding_plan(&plan);
        assert_eq!(bound[0].values[0].id, 0);
        assert_eq!(bound[1].values[0].id, 2);
        assert_eq!(
            stack
                .values()
                .iter()
                .map(|value| value.id)
                .collect::<Vec<_>>(),
            [1]
        );
    }

    #[test]
    fn planner_defers_when_a_nearer_value_may_match_a_fixed_input() {
        let (mut stack, frame) =
            EvaluationStack::isolated("inference", SourceSpan::file_start("workflow.clipasm"));
        stack.extend(
            &frame,
            [
                AbstractValue {
                    id: 0,
                    domain: VIDEO_DOMAIN,
                },
                AbstractValue {
                    id: 1,
                    domain: TIMELINE_DOMAIN,
                },
            ],
        );
        let inputs = [StackBindingInput {
            port: InputSlot::new(0),
            requirement: VIDEO_DOMAIN,
            cardinality: Cardinality::One,
        }];

        assert_eq!(
            stack.plan_bindings(&frame, StackAccess::Owned, &inputs, abstract_compatibility),
            StackBindingOutcome::Deferred
        );
        assert_eq!(stack.values().len(), 2, "planning is pure");
    }

    #[test]
    fn planner_defers_a_variadic_binding_when_its_consumed_set_is_uncertain() {
        let (mut stack, frame) =
            EvaluationStack::isolated("inference", SourceSpan::file_start("workflow.clipasm"));
        stack.extend(
            &frame,
            [
                AbstractValue {
                    id: 0,
                    domain: VIDEO_DOMAIN,
                },
                AbstractValue {
                    id: 1,
                    domain: TIMELINE_DOMAIN,
                },
            ],
        );
        let inputs = [StackBindingInput {
            port: InputSlot::new(0),
            requirement: VIDEO_DOMAIN,
            cardinality: Cardinality::Variadic { min: 1 },
        }];

        assert_eq!(
            stack.plan_bindings(&frame, StackAccess::Owned, &inputs, abstract_compatibility),
            StackBindingOutcome::Deferred
        );
    }

    #[test]
    fn planner_distinguishes_impossible_binding_from_deferred_binding() {
        let (mut stack, frame) =
            EvaluationStack::isolated("inference", SourceSpan::file_start("workflow.clipasm"));
        stack.push(
            &frame,
            AbstractValue {
                id: 0,
                domain: AUDIO_DOMAIN,
            },
        );
        let inputs = [StackBindingInput {
            port: InputSlot::new(3),
            requirement: VIDEO_DOMAIN,
            cardinality: Cardinality::Variadic { min: 1 },
        }];

        assert_eq!(
            stack.plan_bindings(&frame, StackAccess::Owned, &inputs, abstract_compatibility),
            StackBindingOutcome::Impossible(StackBindingFailure {
                port: InputSlot::new(3),
                available: 0,
                selected: vec![],
            })
        );
    }

    #[test]
    fn owned_binding_consumes_only_values_owned_by_the_current_body() {
        let (mut stack, root) = root();
        stack.extend(&root, [value(0), value(1)]);
        let child = EvaluationStack::<ValueRef>::enter_body(
            &root,
            StackAccess::Visible,
            "body",
            SourceSpan::file_start("workflow.clipasm"),
        );
        stack.push(&child, value(2));

        let consumed = stack
            .take_one_matching(
                &child,
                StackAccess::Owned,
                ValueType::Video,
                "repeat",
                "video",
                &SourceSpan::file_start("workflow.clipasm"),
            )
            .expect("owned value");

        assert_eq!(consumed, value(2));
        assert_eq!(stack.values(), &[value(0), value(1)]);
    }

    #[test]
    fn visible_binding_can_consume_ancestor_values_without_capturing_neighbors() {
        let (mut stack, root) = root();
        stack.extend(&root, [value(0), value(1)]);
        let child = EvaluationStack::<ValueRef>::enter_body(
            &root,
            StackAccess::Visible,
            "body",
            SourceSpan::file_start("workflow.clipasm"),
        );
        stack.push(&child, value(2));

        let after = stack
            .take_one_matching(
                &child,
                StackAccess::Visible,
                ValueType::Video,
                "flash_cut",
                "after",
                &SourceSpan::file_start("workflow.clipasm"),
            )
            .expect("after");
        let before = stack
            .take_one_matching(
                &child,
                StackAccess::Visible,
                ValueType::Video,
                "flash_cut",
                "before",
                &SourceSpan::file_start("workflow.clipasm"),
            )
            .expect("before");

        assert_eq!([before, after], [value(1), value(2)]);
        assert_eq!(stack.values(), &[value(0)]);
    }

    #[test]
    fn finishing_a_child_returns_only_child_owned_values() {
        let (mut stack, root) = root();
        stack.extend(&root, [value(0), value(1)]);
        let child = EvaluationStack::<ValueRef>::enter_body(
            &root,
            StackAccess::Visible,
            "body",
            SourceSpan::file_start("workflow.clipasm"),
        );
        stack.push(&child, value(2));

        let owned = stack.finish_body(&child);

        assert_eq!(owned, vec![value(2)]);
        assert_eq!(stack.values(), &[value(0), value(1)]);
    }

    #[test]
    fn owned_body_establishes_a_new_visibility_boundary() {
        let (mut stack, root) = root();
        stack.push(&root, value(0));
        let child = EvaluationStack::<ValueRef>::enter_body(
            &root,
            StackAccess::Owned,
            "during",
            SourceSpan::file_start("workflow.clipasm"),
        );
        stack.push(&child, value(1));
        let _ = stack
            .take_one_matching(
                &child,
                StackAccess::Visible,
                ValueType::Video,
                "flash_cut",
                "after",
                &SourceSpan::file_start("workflow.clipasm"),
            )
            .expect("child value");

        let error = stack
            .take_one_matching(
                &child,
                StackAccess::Visible,
                ValueType::Video,
                "flash_cut",
                "before",
                &SourceSpan::file_start("workflow.clipasm"),
            )
            .expect_err("boundary blocks outer value");

        assert!(error.message.contains("only 0 visible Video"));
        assert!(error.notes.iter().any(|note| note.contains("during")));
    }

    #[test]
    fn owned_variadic_underflow_reports_visible_values_outside_ownership() {
        let (mut stack, root) = root();
        stack.push(&root, value(0));
        let child = EvaluationStack::<ValueRef>::enter_body(
            &root,
            StackAccess::Visible,
            "stack block",
            SourceSpan::file_start("workflow.clipasm"),
        );

        let error = stack
            .take_all_matching(
                &child,
                StackAccess::Owned,
                ValueType::Video,
                1,
                "concat",
                "videos",
                &SourceSpan::file_start("workflow.clipasm"),
            )
            .expect_err("owned concat cannot capture");

        assert_eq!(error.code, "E_MISSING_REQUIRED_INPUT");
        assert!(error.message.contains("only 0 owned Video"));
        assert!(
            error
                .notes
                .iter()
                .any(|note| note.contains("1 additional Video value"))
        );
    }

    #[test]
    fn visible_variadic_consumes_all_matching_values_in_physical_order() {
        let (mut stack, root) = root();
        stack.extend(&root, [value(0), value(1)]);
        let child = EvaluationStack::<ValueRef>::enter_body(
            &root,
            StackAccess::Visible,
            "stack block",
            SourceSpan::file_start("workflow.clipasm"),
        );
        stack.push(&child, value(2));

        let consumed = stack
            .take_all_matching(
                &child,
                StackAccess::Visible,
                ValueType::Video,
                1,
                "concat",
                "videos",
                &SourceSpan::file_start("workflow.clipasm"),
            )
            .expect("visible suffix");

        assert_eq!(consumed, vec![value(0), value(1), value(2)]);
        assert!(stack.values().is_empty());
    }

    #[test]
    fn nonmatching_values_remain_ordered_when_matching_values_are_removed() {
        let (mut stack, root) = root();
        let test = |id| ValueRef::new(ValueId::new(id), ValueType::Audio);
        stack.extend(&root, [value(0), test(1), value(2), test(3)]);
        let consumed = stack
            .take_all_matching(
                &root,
                StackAccess::Owned,
                ValueType::Video,
                1,
                "concat",
                "videos",
                &SourceSpan::file_start("workflow.clipasm"),
            )
            .expect("Videos");

        assert_eq!(consumed, vec![value(0), value(2)]);
        assert_eq!(stack.values(), &[test(1), test(3)]);
    }

    #[test]
    fn nested_visible_bodies_share_the_nearest_visibility_boundary() {
        let (mut stack, root) = root();
        stack.push(&root, value(0));
        let parent = EvaluationStack::<ValueRef>::enter_body(
            &root,
            StackAccess::Visible,
            "parent",
            SourceSpan::file_start("workflow.clipasm"),
        );
        stack.push(&parent, value(1));
        let child = EvaluationStack::<ValueRef>::enter_body(
            &parent,
            StackAccess::Visible,
            "child",
            SourceSpan::file_start("workflow.clipasm"),
        );
        stack.push(&child, value(2));

        let values = stack
            .take_all_matching(
                &child,
                StackAccess::Visible,
                ValueType::Video,
                1,
                "concat",
                "videos",
                &SourceSpan::file_start("workflow.clipasm"),
            )
            .expect("visible values");

        assert_eq!(values, vec![value(0), value(1), value(2)]);
    }
}
