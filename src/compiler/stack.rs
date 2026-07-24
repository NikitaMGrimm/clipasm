use crate::diagnostic::{Diagnostic, SourceSpan};
use crate::model::ValueRef;
use crate::program::StackAccess;

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
    visible_start: usize,
    owned_start: usize,
    boundary: VisibilityBoundary,
}

impl StackFrame {
    fn root(boundary: VisibilityBoundary) -> Self {
        Self {
            visible_start: 0,
            owned_start: 0,
            boundary,
        }
    }
}

#[derive(Debug)]
pub(super) struct EvaluationStack {
    values: Vec<ValueRef>,
}

impl EvaluationStack {
    pub(super) fn isolated(owner: impl Into<String>, span: SourceSpan) -> (Self, StackFrame) {
        let boundary = VisibilityBoundary::new(owner, span);
        (Self { values: Vec::new() }, StackFrame::root(boundary))
    }

    pub(super) fn len(&self) -> usize {
        self.values.len()
    }

    pub(super) fn push(&mut self, value: ValueRef) {
        self.values.push(value);
    }

    pub(super) fn extend(&mut self, values: impl IntoIterator<Item = ValueRef>) {
        self.values.extend(values);
    }

    pub(super) fn values(&self) -> &[ValueRef] {
        &self.values
    }

    pub(super) fn enter_body(
        &self,
        parent: &StackFrame,
        access: StackAccess,
        owner: impl Into<String>,
        span: SourceSpan,
    ) -> StackFrame {
        let body_start = self.values.len();
        match access {
            StackAccess::Owned => StackFrame {
                visible_start: body_start,
                owned_start: body_start,
                boundary: VisibilityBoundary::new(owner, span),
            },
            StackAccess::Visible => StackFrame {
                visible_start: parent.visible_start,
                owned_start: body_start,
                boundary: parent.boundary.clone(),
            },
        }
    }

    pub(super) fn take_fixed(
        &mut self,
        frame: &mut StackFrame,
        access: StackAccess,
        count: usize,
        program: &str,
        span: &SourceSpan,
    ) -> Result<Vec<ValueRef>, Diagnostic> {
        let start = self.accessible_start(frame, access);
        let available = self.values.len().saturating_sub(start);
        if available < count {
            return Err(self.underflow(
                frame,
                access,
                "E_STACK_UNDERFLOW",
                &format!("`{program}` needs {count} preceding value(s)"),
                available,
                span,
            ));
        }
        let consumed_start = self.values.len() - count;
        let values = self.values.split_off(consumed_start);
        self.capture(frame, access, consumed_start);
        Ok(values)
    }

    pub(super) fn take_variadic(
        &mut self,
        frame: &mut StackFrame,
        access: StackAccess,
        min: usize,
        program: &str,
        port: &str,
        span: &SourceSpan,
    ) -> Result<Vec<ValueRef>, Diagnostic> {
        let start = self.accessible_start(frame, access);
        let available = self.values.len().saturating_sub(start);
        if available < min {
            return Err(self.underflow(
                frame,
                access,
                "E_MISSING_REQUIRED_INPUT",
                &format!("`{program}.{port}` needs at least {min} value(s)"),
                available,
                span,
            ));
        }
        let values = self.values.split_off(start);
        self.capture(frame, access, start);
        Ok(values)
    }

    pub(super) fn finish_body(
        &mut self,
        parent: &mut StackFrame,
        child: &StackFrame,
    ) -> Vec<ValueRef> {
        debug_assert!(child.visible_start <= child.owned_start);
        debug_assert!(child.owned_start <= self.values.len());
        let captured_start = child.owned_start;
        let values = self.values.split_off(captured_start);
        parent.owned_start = parent.owned_start.min(captured_start);
        values
    }

    fn accessible_start(&self, frame: &StackFrame, access: StackAccess) -> usize {
        let start = match access {
            StackAccess::Owned => frame.owned_start,
            StackAccess::Visible => frame.visible_start,
        };
        debug_assert!(start <= self.values.len());
        start
    }

    fn capture(&self, frame: &mut StackFrame, access: StackAccess, consumed_start: usize) {
        if access == StackAccess::Visible {
            frame.owned_start = frame.owned_start.min(consumed_start);
        }
        debug_assert!(frame.visible_start <= frame.owned_start);
        debug_assert!(frame.owned_start <= self.values.len());
    }

    fn underflow(
        &self,
        frame: &StackFrame,
        access: StackAccess,
        code: &'static str,
        requirement: &str,
        available: usize,
        span: &SourceSpan,
    ) -> Diagnostic {
        let mut diagnostic = Diagnostic::new(
            code,
            format!(
                "{requirement}, but only {available} {} value(s) are available",
                access.label()
            ),
            span.clone(),
        );
        if access == StackAccess::Owned {
            let visible = self.values.len().saturating_sub(frame.visible_start);
            if visible > available {
                diagnostic = diagnostic.note(format!(
                    "{} additional value(s) are visible outside this invocation's owned suffix; set `stack_access: visible` to permit capturing them",
                    visible - available
                ));
            }
        } else {
            diagnostic = diagnostic.note(format!(
                "the body of `{}` at {}:{}:{} establishes the nearest stack visibility boundary",
                frame.boundary.owner(),
                frame.boundary.span().file.display(),
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

    fn value(id: u32) -> ValueRef {
        ValueRef::new(ValueId::new(id), ValueType::Video)
    }

    fn root() -> (EvaluationStack, StackFrame) {
        EvaluationStack::isolated("source program", SourceSpan::file_start("workflow.yaml"))
    }

    #[test]
    fn owned_binding_consumes_only_the_owned_suffix() {
        let (mut stack, root) = root();
        stack.extend([value(0), value(1)]);
        let mut child = stack.enter_body(
            &root,
            StackAccess::Visible,
            "body",
            SourceSpan::file_start("workflow.yaml"),
        );
        stack.push(value(2));

        let consumed = stack
            .take_fixed(
                &mut child,
                StackAccess::Owned,
                1,
                "repeat",
                &SourceSpan::file_start("workflow.yaml"),
            )
            .expect("owned value");

        assert_eq!(consumed, vec![value(2)]);
        assert_eq!(stack.values(), &[value(0), value(1)]);
        assert_eq!(child.owned_start, 2);
        assert_eq!(root.owned_start, 0);
    }

    #[test]
    fn visible_binding_captures_below_the_owned_frontier() {
        let (mut stack, root) = root();
        stack.extend([value(0), value(1)]);
        let mut child = stack.enter_body(
            &root,
            StackAccess::Visible,
            "body",
            SourceSpan::file_start("workflow.yaml"),
        );
        stack.push(value(2));

        let consumed = stack
            .take_fixed(
                &mut child,
                StackAccess::Visible,
                2,
                "flash",
                &SourceSpan::file_start("workflow.yaml"),
            )
            .expect("visible values");

        assert_eq!(consumed, vec![value(1), value(2)]);
        assert_eq!(stack.values(), &[value(0)]);
        assert_eq!(child.owned_start, 1);
    }

    #[test]
    fn finishing_a_child_propagates_capture_one_level() {
        let (mut stack, mut root) = root();
        stack.extend([value(0), value(1)]);
        root.owned_start = 1;
        let mut child = stack.enter_body(
            &root,
            StackAccess::Visible,
            "body",
            SourceSpan::file_start("workflow.yaml"),
        );
        stack.push(value(2));
        let _ = stack
            .take_fixed(
                &mut child,
                StackAccess::Visible,
                3,
                "concat",
                &SourceSpan::file_start("workflow.yaml"),
            )
            .expect("capture");
        stack.push(value(3));

        let owned = stack.finish_body(&mut root, &child);

        assert_eq!(owned, vec![value(3)]);
        assert_eq!(root.owned_start, 0);
        assert!(stack.values().is_empty());
    }

    #[test]
    fn nested_capture_propagates_when_each_body_finishes() {
        let (mut stack, mut root) = root();
        stack.extend([value(0), value(1)]);
        root.owned_start = 1;

        let mut middle = stack.enter_body(
            &root,
            StackAccess::Visible,
            "middle",
            SourceSpan::file_start("workflow.yaml"),
        );
        let mut inner = stack.enter_body(
            &middle,
            StackAccess::Visible,
            "inner",
            SourceSpan::file_start("workflow.yaml"),
        );
        let _ = stack
            .take_fixed(
                &mut inner,
                StackAccess::Visible,
                2,
                "flash",
                &SourceSpan::file_start("workflow.yaml"),
            )
            .expect("capture root values");
        stack.push(value(2));

        let inner_owned = stack.finish_body(&mut middle, &inner);
        assert_eq!(inner_owned, vec![value(2)]);
        assert_eq!(middle.owned_start, 0);
        stack.extend(inner_owned);

        let middle_owned = stack.finish_body(&mut root, &middle);
        assert_eq!(middle_owned, vec![value(2)]);
        assert_eq!(root.owned_start, 0);
    }

    #[test]
    fn owned_body_establishes_a_new_visibility_boundary() {
        let (mut stack, root) = root();
        stack.push(value(0));
        let mut child = stack.enter_body(
            &root,
            StackAccess::Owned,
            "during",
            SourceSpan::file_start("workflow.yaml"),
        );
        stack.push(value(1));

        let error = stack
            .take_fixed(
                &mut child,
                StackAccess::Visible,
                2,
                "flash",
                &SourceSpan::file_start("workflow.yaml"),
            )
            .expect_err("boundary blocks outer value");

        assert!(error.message.contains("only 1 visible"));
        assert!(error.notes.iter().any(|note| note.contains("during")));
    }

    #[test]
    fn owned_variadic_underflow_reports_visible_values_outside_ownership() {
        let (mut stack, root) = root();
        stack.push(value(0));
        let mut child = stack.enter_body(
            &root,
            StackAccess::Visible,
            "glue",
            SourceSpan::file_start("workflow.yaml"),
        );

        let error = stack
            .take_variadic(
                &mut child,
                StackAccess::Owned,
                1,
                "concat",
                "videos",
                &SourceSpan::file_start("workflow.yaml"),
            )
            .expect_err("owned concat cannot capture");

        assert_eq!(error.code, "E_MISSING_REQUIRED_INPUT");
        assert!(error.message.contains("only 0 owned"));
        assert!(
            error
                .notes
                .iter()
                .any(|note| note.contains("1 additional value"))
        );
    }

    #[test]
    fn visible_variadic_consumes_the_complete_visible_suffix() {
        let (mut stack, root) = root();
        stack.extend([value(0), value(1)]);
        let mut child = stack.enter_body(
            &root,
            StackAccess::Visible,
            "glue",
            SourceSpan::file_start("workflow.yaml"),
        );
        stack.push(value(2));

        let consumed = stack
            .take_variadic(
                &mut child,
                StackAccess::Visible,
                1,
                "concat",
                "videos",
                &SourceSpan::file_start("workflow.yaml"),
            )
            .expect("visible suffix");

        assert_eq!(consumed, vec![value(0), value(1), value(2)]);
        assert_eq!(child.owned_start, 0);
        assert!(stack.values().is_empty());
    }

    #[test]
    fn capture_propagates_outward_one_body_exit_at_a_time() {
        let (mut stack, mut root) = root();
        stack.extend([value(0), value(1)]);
        root.owned_start = 1;
        let mut parent = stack.enter_body(
            &root,
            StackAccess::Visible,
            "parent",
            SourceSpan::file_start("workflow.yaml"),
        );
        stack.push(value(2));
        let mut child = stack.enter_body(
            &parent,
            StackAccess::Visible,
            "child",
            SourceSpan::file_start("workflow.yaml"),
        );
        stack.push(value(3));
        let _ = stack
            .take_fixed(
                &mut child,
                StackAccess::Visible,
                4,
                "capture",
                &SourceSpan::file_start("workflow.yaml"),
            )
            .expect("deep capture");
        stack.push(value(4));

        let child_values = stack.finish_body(&mut parent, &child);
        assert_eq!(child_values, vec![value(4)]);
        assert_eq!(parent.owned_start, 0);
        assert_eq!(root.owned_start, 1);

        stack.push(value(5));
        let parent_values = stack.finish_body(&mut root, &parent);
        assert_eq!(parent_values, vec![value(5)]);
        assert_eq!(root.owned_start, 0);
    }
}
