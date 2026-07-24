use crate::diagnostic::Diagnostic;
use crate::model::{ValueRef, ValueType};
use crate::program::StackAccess;
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

pub(super) trait StackValue {
    fn value_type(self) -> ValueType;
}

impl StackValue for ValueRef {
    fn value_type(self) -> ValueType {
        self.value_type()
    }
}

impl StackValue for ValueType {
    fn value_type(self) -> ValueType {
        self
    }
}

impl<T: Copy + StackValue> EvaluationStack<T> {
    pub(super) fn nearest_accessible_type(
        &self,
        frame: &StackFrame,
        access: StackAccess,
        accepts: impl Fn(ValueType) -> bool,
    ) -> Option<ValueType> {
        self.values
            .iter()
            .copied()
            .zip(self.owners.iter().copied())
            .rev()
            .find_map(|(value, owner)| {
                let value_type = value.value_type();
                (Self::accessible(owner, frame, access) && accepts(value_type))
                    .then_some(value_type)
            })
    }

    pub(super) fn accessible_types(
        &self,
        frame: &StackFrame,
        access: StackAccess,
        accepts: impl Fn(ValueType) -> bool,
    ) -> Vec<ValueType> {
        let mut types = Vec::new();
        for (value, owner) in self.values.iter().copied().zip(self.owners.iter().copied()) {
            let value_type = value.value_type();
            if Self::accessible(owner, frame, access)
                && accepts(value_type)
                && !types.contains(&value_type)
            {
                types.push(value_type);
            }
        }
        types
    }

    pub(super) fn accessible_count(
        &self,
        frame: &StackFrame,
        access: StackAccess,
        required: ValueType,
    ) -> usize {
        self.values
            .iter()
            .copied()
            .zip(self.owners.iter().copied())
            .filter(|(value, owner)| {
                Self::accessible(*owner, frame, access) && value.value_type() == required
            })
            .count()
    }

    pub(super) fn take_one_matching(
        &mut self,
        frame: &StackFrame,
        access: StackAccess,
        required: ValueType,
        program: &str,
        port: &str,
        span: &SourceSpan,
    ) -> Result<T, Diagnostic> {
        let Some(index) = (0..self.values.len()).rev().find(|index| {
            Self::accessible(self.owners[*index], frame, access)
                && self.values[*index].value_type() == required
        }) else {
            return Err(self.underflow(
                frame,
                access,
                "E_STACK_UNDERFLOW",
                &format!("`{program}.{port}` needs one preceding {required} value"),
                required,
                0,
                span,
            ));
        };
        self.owners.remove(index);
        Ok(self.values.remove(index))
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn take_all_matching(
        &mut self,
        frame: &StackFrame,
        access: StackAccess,
        required: ValueType,
        min: usize,
        program: &str,
        port: &str,
        span: &SourceSpan,
    ) -> Result<Vec<T>, Diagnostic> {
        let indices = (0..self.values.len())
            .filter(|index| {
                Self::accessible(self.owners[*index], frame, access)
                    && self.values[*index].value_type() == required
            })
            .collect::<Vec<_>>();
        if indices.len() < min {
            return Err(self.underflow(
                frame,
                access,
                "E_MISSING_REQUIRED_INPUT",
                &format!("`{program}.{port}` needs at least {min} {required} value(s)"),
                required,
                indices.len(),
                span,
            ));
        }
        let values = indices
            .iter()
            .map(|index| self.values[*index])
            .collect::<Vec<_>>();
        for index in indices.into_iter().rev() {
            self.values.remove(index);
            self.owners.remove(index);
        }
        Ok(values)
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    fn underflow(
        &self,
        frame: &StackFrame,
        access: StackAccess,
        code: &'static str,
        requirement: &str,
        required: ValueType,
        available: usize,
        span: &SourceSpan,
    ) -> Diagnostic {
        let mut diagnostic = Diagnostic::new(
            code,
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
                .filter(|(value, owner)| {
                    Self::accessible(*owner, frame, StackAccess::Visible)
                        && value.value_type() == required
                })
                .count();
            if visible > available {
                diagnostic = diagnostic.note(format!(
                    "{} additional {required} value(s) are visible outside this invocation's owned values; set `stack_access: visible` to permit consuming them",
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

    fn accessible(owner: usize, frame: &StackFrame, access: StackAccess) -> bool {
        match access {
            StackAccess::Owned => owner == frame.depth,
            StackAccess::Visible => owner >= frame.visible_depth && owner <= frame.depth,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ValueId, ValueType};

    fn value(id: u32) -> ValueRef {
        ValueRef::new(ValueId::new(id), ValueType::Video)
    }

    fn root() -> (EvaluationStack, StackFrame) {
        EvaluationStack::isolated("source program", SourceSpan::file_start("workflow.yaml"))
    }

    #[test]
    fn owned_binding_consumes_only_values_owned_by_the_current_body() {
        let (mut stack, root) = root();
        stack.extend(&root, [value(0), value(1)]);
        let child = EvaluationStack::<ValueRef>::enter_body(
            &root,
            StackAccess::Visible,
            "body",
            SourceSpan::file_start("workflow.yaml"),
        );
        stack.push(&child, value(2));

        let consumed = stack
            .take_one_matching(
                &child,
                StackAccess::Owned,
                ValueType::Video,
                "repeat",
                "video",
                &SourceSpan::file_start("workflow.yaml"),
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
            SourceSpan::file_start("workflow.yaml"),
        );
        stack.push(&child, value(2));

        let after = stack
            .take_one_matching(
                &child,
                StackAccess::Visible,
                ValueType::Video,
                "flash",
                "after",
                &SourceSpan::file_start("workflow.yaml"),
            )
            .expect("after");
        let before = stack
            .take_one_matching(
                &child,
                StackAccess::Visible,
                ValueType::Video,
                "flash",
                "before",
                &SourceSpan::file_start("workflow.yaml"),
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
            SourceSpan::file_start("workflow.yaml"),
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
            SourceSpan::file_start("workflow.yaml"),
        );
        stack.push(&child, value(1));
        let _ = stack
            .take_one_matching(
                &child,
                StackAccess::Visible,
                ValueType::Video,
                "flash",
                "after",
                &SourceSpan::file_start("workflow.yaml"),
            )
            .expect("child value");

        let error = stack
            .take_one_matching(
                &child,
                StackAccess::Visible,
                ValueType::Video,
                "flash",
                "before",
                &SourceSpan::file_start("workflow.yaml"),
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
            "glue",
            SourceSpan::file_start("workflow.yaml"),
        );

        let error = stack
            .take_all_matching(
                &child,
                StackAccess::Owned,
                ValueType::Video,
                1,
                "concat",
                "videos",
                &SourceSpan::file_start("workflow.yaml"),
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
            "glue",
            SourceSpan::file_start("workflow.yaml"),
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
                &SourceSpan::file_start("workflow.yaml"),
            )
            .expect("visible suffix");

        assert_eq!(consumed, vec![value(0), value(1), value(2)]);
        assert!(stack.values().is_empty());
    }

    #[test]
    fn nonmatching_values_remain_ordered_when_matching_values_are_removed() {
        let (mut stack, root) = root();
        let test = |id| ValueRef::new(ValueId::new(id), ValueType::Test);
        stack.extend(&root, [value(0), test(1), value(2), test(3)]);
        let consumed = stack
            .take_all_matching(
                &root,
                StackAccess::Owned,
                ValueType::Video,
                1,
                "concat",
                "videos",
                &SourceSpan::file_start("workflow.yaml"),
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
            SourceSpan::file_start("workflow.yaml"),
        );
        stack.push(&parent, value(1));
        let child = EvaluationStack::<ValueRef>::enter_body(
            &parent,
            StackAccess::Visible,
            "child",
            SourceSpan::file_start("workflow.yaml"),
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
                &SourceSpan::file_start("workflow.yaml"),
            )
            .expect("visible values");

        assert_eq!(values, vec![value(0), value(1), value(2)]);
    }
}
