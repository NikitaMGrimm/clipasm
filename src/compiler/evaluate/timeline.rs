use std::collections::BTreeMap;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{
    ExactNumber, FrameCount, FrameRate, NativeRange, TimelineExpression, TimelineViewId, ValueRef,
    ValueType,
};
use crate::source::SourceSpan;

use super::{EvaluatedValue, Evaluator, ReferenceTarget, SymbolId, TimelineSelectorContext};
use crate::compiler::parameter::TimelineSelectorValue;

#[derive(Clone, Debug)]
struct TimelineChild {
    label: Option<String>,
    view: TimelineViewId,
    start: TimelineExpression,
}

#[derive(Debug)]
struct TimelineView {
    value_type: ValueType,
    extent: TimelineExpression,
    placements: BTreeMap<String, Vec<usize>>,
    children: Vec<TimelineChild>,
}

pub(super) struct TimelineState {
    views: Vec<TimelineView>,
    fps: FrameRate,
    sample_rate: u32,
}

impl TimelineState {
    pub(super) fn new(fps: FrameRate, sample_rate: u32) -> Self {
        Self {
            views: Vec::new(),
            fps,
            sample_rate,
        }
    }
}

#[derive(Clone)]
enum ContextualMatches {
    None,
    One {
        view: TimelineViewId,
        start: TimelineExpression,
    },
    Multiple,
}

impl ContextualMatches {
    fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::None, other) | (other, Self::None) => other,
            (Self::Multiple, _) | (_, Self::Multiple) | (Self::One { .. }, Self::One { .. }) => {
                Self::Multiple
            }
        }
    }

    fn shifted(self, offset: &TimelineExpression) -> Self {
        match self {
            Self::One { view, start } => Self::One {
                view,
                start: offset.add(&start),
            },
            other => other,
        }
    }

    fn is_multiple(&self) -> bool {
        matches!(self, Self::Multiple)
    }
}

fn frame_seconds(frame: u64, fps: FrameRate) -> ExactNumber {
    ExactNumber::from_unsigned_integer(frame)
        .multiply(&ExactNumber::from_unsigned_integer(u64::from(
            fps.denominator(),
        )))
        .divide(&ExactNumber::from_unsigned_integer(u64::from(
            fps.numerator(),
        )))
        .expect("frame-rate numerator is nonzero")
}

fn sample_seconds(sample: u64, sample_rate: u32) -> ExactNumber {
    ExactNumber::from_unsigned_integer(sample)
        .divide(&ExactNumber::from_unsigned_integer(u64::from(sample_rate)))
        .expect("sample rate is nonzero")
}

fn native_unit_seconds(value_type: ValueType, fps: FrameRate, sample_rate: u32) -> ExactNumber {
    match value_type {
        ValueType::Video => frame_seconds(1, fps),
        ValueType::Audio => sample_seconds(1, sample_rate),
    }
}

impl Evaluator {
    fn add_timeline_view(
        &mut self,
        value_type: ValueType,
        extent: TimelineExpression,
        children: Vec<TimelineChild>,
    ) -> TimelineViewId {
        debug_assert!(children.iter().all(|child| {
            child.view.index() < self.timeline.views.len()
                && self.timeline.views[child.view.index()].value_type == value_type
        }));
        let mut placements = BTreeMap::<String, Vec<usize>>::new();
        for (index, child) in children.iter().enumerate() {
            let Some(label) = &child.label else {
                continue;
            };
            placements.entry(label.clone()).or_default().push(index);
        }
        let id = TimelineViewId::new(
            u32::try_from(self.timeline.views.len()).expect("timeline view count fits in u32"),
        );
        self.timeline.views.push(TimelineView {
            value_type,
            extent,
            placements,
            children,
        });
        id
    }

    pub(super) fn fresh_evaluated(&mut self, value: ValueRef) -> EvaluatedValue {
        let extent = match self.nodes[value.id().get() as usize].kind() {
            crate::semantic::SemanticNodeKind::ImageVideo { frames, .. } => {
                TimelineExpression::constant(frame_seconds(frames.0, self.timeline.fps))
            }
            crate::semantic::SemanticNodeKind::DeferredImageVideo { extent, .. } => extent.clone(),
            crate::semantic::SemanticNodeKind::Slice { range, .. } => match range {
                NativeRange::Frames(range) => {
                    TimelineExpression::constant(frame_seconds(range.frames().0, self.timeline.fps))
                }
                NativeRange::Samples(range) => TimelineExpression::constant(sample_seconds(
                    range.samples(),
                    self.timeline.sample_rate,
                )),
            },
            crate::semantic::SemanticNodeKind::DeferredSlice { range, .. } => {
                range.end.subtract(&range.start)
            }
            crate::semantic::SemanticNodeKind::DeferredReplaceRange {
                base,
                replacement,
                range,
            } => {
                let seconds_per_unit = native_unit_seconds(
                    base.value_type(),
                    self.timeline.fps,
                    self.timeline.sample_rate,
                );
                TimelineExpression::extent(*base, seconds_per_unit.clone())
                    .subtract(&range.end.subtract(&range.start))
                    .add(&TimelineExpression::extent(*replacement, seconds_per_unit))
            }
            _ => TimelineExpression::extent(
                value,
                native_unit_seconds(
                    value.value_type(),
                    self.timeline.fps,
                    self.timeline.sample_rate,
                ),
            ),
        };
        let timeline_view = self.add_timeline_view(value.value_type(), extent, Vec::new());
        EvaluatedValue {
            value,
            timeline_view,
            placement_symbol: None,
        }
    }

    pub(super) fn resolve_timeline_selector(
        &self,
        target: ReferenceTarget,
        context: &TimelineSelectorContext<'_>,
    ) -> Result<TimelineSelectorValue> {
        let target_view = self.selector_target_view(target, context)?;
        let Some(last) = context.path.last() else {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InvalidTimelineSelector,
                "a timeline selector requires a placement or boundary",
                context.span.clone(),
            ));
        };
        let boundary = matches!(last.as_str(), "start" | "middle" | "end").then_some(last.as_str());
        let placement_path = if boundary.is_some() {
            &context.path[..context.path.len() - 1]
        } else {
            context.path
        };
        let (root, current, offset, path_consumed) =
            self.selector_root(target_view, placement_path, context)?;
        let remaining_path = if path_consumed {
            &[][..]
        } else {
            placement_path
        };
        let (current, offset) =
            self.walk_selector_path(root, current, offset, remaining_path, context)?;
        let view = &self.timeline.views[current.index()];
        let layout = self.timeline_layout_note(context.root_name, root);
        let end = Self::selector_end(view, &offset);
        Ok(match boundary {
            Some("start") => TimelineSelectorValue::Coordinate {
                owner: root,
                expression: offset,
                layout: layout.clone(),
            },
            Some("middle") => TimelineSelectorValue::Coordinate {
                owner: root,
                expression: offset
                    .add(&end)
                    .divide(&ExactNumber::from_integer(2))
                    .expect("two is nonzero"),
                layout: layout.clone(),
            },
            Some("end") => TimelineSelectorValue::Coordinate {
                owner: root,
                expression: end,
                layout,
            },
            Some(_) => unreachable!("known timeline boundary"),
            None => TimelineSelectorValue::Range {
                owner: root,
                start: offset,
                end,
            },
        })
    }

    fn selector_target_view(
        &self,
        target: ReferenceTarget,
        context: &TimelineSelectorContext<'_>,
    ) -> Result<TimelineViewId> {
        match target {
            ReferenceTarget::Local(local) => {
                let symbol = context.scope.local_symbols[local.index()];
                self.symbols[symbol.index()].timeline_view.ok_or_else(|| {
                    Diagnostic::builtin(
                        BuiltinDiagnostic::UnresolvedTimeline,
                        format!(
                            "timeline `${}` is not resolved at this use",
                            context.root_name
                        ),
                        context.span.clone(),
                    )
                })
            }
            ReferenceTarget::BodyInput(input) => context.scope.body_inputs[input.index()]
                .map(|value| value.timeline_view)
                .ok_or_else(|| {
                    Diagnostic::builtin(
                        BuiltinDiagnostic::UnresolvedTimeline,
                        format!("timeline `${}` is not bound at this use", context.root_name),
                        context.span.clone(),
                    )
                }),
        }
    }

    fn selector_root(
        &self,
        target_view: TimelineViewId,
        placement_path: &[String],
        context: &TimelineSelectorContext<'_>,
    ) -> Result<(TimelineViewId, TimelineViewId, TimelineExpression, bool)> {
        if !context.contextual {
            return Ok((
                target_view,
                target_view,
                TimelineExpression::constant(ExactNumber::from_integer(0)),
                false,
            ));
        }
        let mut bound_views = context
            .slots
            .iter()
            .flatten()
            .flatten()
            .map(|value| value.timeline_view)
            .collect::<Vec<_>>();
        bound_views.sort_unstable();
        bound_views.dedup();
        if bound_views.contains(&target_view) {
            return Ok((
                target_view,
                target_view,
                TimelineExpression::constant(ExactNumber::from_integer(0)),
                false,
            ));
        }
        let mut selector_path = Vec::with_capacity(placement_path.len() + 1);
        selector_path.push(context.root_name);
        selector_path.extend(placement_path.iter().map(String::as_str));
        let mut candidate = None;
        let mut ambiguous = false;
        for bound in &bound_views {
            match self.contextual_selector_matches(*bound, &selector_path) {
                ContextualMatches::None => {}
                ContextualMatches::One { view, start } if candidate.is_none() => {
                    candidate = Some((*bound, view, start));
                }
                ContextualMatches::One { .. } | ContextualMatches::Multiple => {
                    ambiguous = true;
                    break;
                }
            }
        }
        if ambiguous {
            let mut diagnostic = Diagnostic::builtin(
                BuiltinDiagnostic::AmbiguousTimelinePlacement,
                format!(
                    "selector `${}` matches multiple placements in the bound timeline context",
                    selector_path.join("::")
                ),
                context.span.clone(),
            )
            .note("qualify the selector with more leading placement names or its owning timeline");
            for (index, bound) in bound_views.into_iter().enumerate() {
                diagnostic = diagnostic.note(
                    self.timeline_layout_note_for(&format!("bound timeline {}", index + 1), bound),
                );
            }
            return Err(diagnostic);
        }
        if let Some((owner, view, start)) = candidate {
            return Ok((owner, view, start, true));
        }
        Ok((
            target_view,
            target_view,
            TimelineExpression::constant(ExactNumber::from_integer(0)),
            false,
        ))
    }

    fn contextual_selector_matches(
        &self,
        root: TimelineViewId,
        selector_path: &[&str],
    ) -> ContextualMatches {
        debug_assert!(!selector_path.is_empty());
        let view_count = root.index() + 1;
        let views = &self.timeline.views[..view_count];
        let mut direct = vec![ContextualMatches::None; view_count];
        for (path_index, name) in selector_path.iter().enumerate().rev() {
            let suffix = direct;
            direct = vec![ContextualMatches::None; view_count];
            for (view_index, view) in views.iter().enumerate() {
                let Some(placements) = view.placements.get(*name) else {
                    continue;
                };
                let mut matches = ContextualMatches::None;
                for &child_index in placements {
                    let placement = &view.children[child_index];
                    let candidate = if path_index + 1 == selector_path.len() {
                        ContextualMatches::One {
                            view: placement.view,
                            start: placement.start.clone(),
                        }
                    } else {
                        suffix[placement.view.index()]
                            .clone()
                            .shifted(&placement.start)
                    };
                    matches = matches.merge(candidate);
                    if matches.is_multiple() {
                        break;
                    }
                }
                direct[view_index] = matches;
            }
        }

        let mut subtree = vec![ContextualMatches::None; view_count];
        for (view_index, view) in views.iter().enumerate() {
            let mut matches = direct[view_index].clone();
            for child in &view.children {
                matches = matches.merge(subtree[child.view.index()].clone().shifted(&child.start));
                if matches.is_multiple() {
                    break;
                }
            }
            subtree[view_index] = matches;
        }
        subtree[root.index()].clone()
    }

    fn walk_selector_path(
        &self,
        root: TimelineViewId,
        mut current: TimelineViewId,
        mut offset: TimelineExpression,
        path: &[String],
        context: &TimelineSelectorContext<'_>,
    ) -> Result<(TimelineViewId, TimelineExpression)> {
        for name in path {
            let (next, start) = {
                let view = &self.timeline.views[current.index()];
                let child_index = match view.placements.get(name).map(Vec::as_slice) {
                    Some([child_index]) => *child_index,
                    Some(placements) => {
                        return Err(Diagnostic::builtin(
                            BuiltinDiagnostic::AmbiguousTimelinePlacement,
                            format!(
                                "timeline `${}` has {} placements named `{name}` at this selector level",
                                context.root_name,
                                placements.len()
                            ),
                            context.span.clone(),
                        )
                        .note("qualify the selector with a distinct placement name")
                        .note(self.timeline_layout_note(context.root_name, root)));
                    }
                    None => {
                        return Err(Diagnostic::builtin(
                            BuiltinDiagnostic::UnknownTimelinePlacement,
                            format!(
                                "timeline `${}` has no placement named `{name}`",
                                context.root_name
                            ),
                            context.span.clone(),
                        )
                        .note(self.timeline_layout_note(context.root_name, root)));
                    }
                };
                let placement = &view.children[child_index];
                (placement.view, placement.start.clone())
            };
            offset = offset.add(&start);
            current = next;
        }
        Ok((current, offset))
    }

    fn selector_end(view: &TimelineView, offset: &TimelineExpression) -> TimelineExpression {
        offset.add(&view.extent)
    }

    fn timeline_layout_note(&self, root_name: &str, root: TimelineViewId) -> String {
        self.timeline_layout_note_for(&format!("`${root_name}`"), root)
    }

    pub(super) fn timeline_layout_note_for(&self, label: &str, root: TimelineViewId) -> String {
        const MAX_DEPTH: usize = 12;
        const MAX_NODES: usize = 64;

        let zero = TimelineExpression::constant(ExactNumber::from_integer(0));
        let root_view = &self.timeline.views[root.index()];
        let mut lines = vec![
            format!("timeline layout for {label}:"),
            format!(
                "{label} {}",
                Self::timeline_range_text(&zero, &root_view.extent)
            ),
        ];
        let mut remaining = MAX_NODES;
        self.push_timeline_children(root, &zero, "", &mut lines, &mut remaining, 0, MAX_DEPTH);
        if remaining == 0 {
            lines.push("… timeline layout truncated".to_owned());
        }
        lines.join("\n")
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the recursive formatter keeps traversal limits and output state explicit instead of allocating a context object"
    )]
    fn push_timeline_children(
        &self,
        view: TimelineViewId,
        base: &TimelineExpression,
        prefix: &str,
        lines: &mut Vec<String>,
        remaining: &mut usize,
        depth: usize,
        max_depth: usize,
    ) {
        let view = &self.timeline.views[view.index()];
        let children = &view.children;
        for (index, child) in children.iter().enumerate() {
            if *remaining == 0 {
                return;
            }
            *remaining -= 1;
            let last = index + 1 == children.len();
            let branch = if last { "└── " } else { "├── " };
            let start = base.add(&child.start);
            let child_view = &self.timeline.views[child.view.index()];
            let end = start.add(&child_view.extent);
            let label = match &child.label {
                Some(label)
                    if view
                        .placements
                        .get(label)
                        .is_some_and(|placements| placements.len() == 1) =>
                {
                    label.clone()
                }
                Some(label) => format!("{label} (not directly addressable)"),
                None => "<unnamed> (not directly addressable)".to_owned(),
            };
            lines.push(format!(
                "{prefix}{branch}{label} {}",
                Self::timeline_range_text(&start, &end)
            ));
            if child_view.children.is_empty() {
                continue;
            }
            let child_prefix = format!("{prefix}{}", if last { "    " } else { "│   " });
            if depth + 1 >= max_depth {
                lines.push(format!("{child_prefix}└── … nested layout omitted"));
                continue;
            }
            self.push_timeline_children(
                child.view,
                &start,
                &child_prefix,
                lines,
                remaining,
                depth + 1,
                max_depth,
            );
        }
    }

    fn timeline_range_text(start: &TimelineExpression, end: &TimelineExpression) -> String {
        format!(
            "[{}..{})",
            Self::timeline_expression_text(start),
            Self::timeline_expression_text(end)
        )
    }

    fn timeline_expression_text(expression: &TimelineExpression) -> String {
        if let Some(value) = expression.constant_value() {
            return format!("{}s", value.authored_display());
        }
        let mut parts = Vec::new();
        if !expression.constant_part().is_zero() {
            parts.push(format!(
                "{}s",
                expression.constant_part().authored_display()
            ));
        }
        if !expression.project_frame_part().is_zero() {
            parts.push(format!(
                "{}f",
                expression.project_frame_part().authored_display()
            ));
        }
        parts.extend(expression.terms().iter().map(|term| {
            let (units, prefix) = match term.value.value_type() {
                ValueType::Video => ("frames", 'v'),
                ValueType::Audio => ("samples", 'a'),
            };
            format!(
                "{}s×{units}({prefix}{})",
                term.coefficient.authored_display(),
                term.value.id().get()
            )
        }));
        if parts.is_empty() {
            "0s".to_owned()
        } else {
            parts.join(" + ")
        }
    }

    fn timeline_input(
        slots: &[Option<Vec<EvaluatedValue>>],
        input: crate::program::InputSlot,
        behavior: &str,
        span: &SourceSpan,
    ) -> Result<EvaluatedValue> {
        slots
            .get(input.index())
            .and_then(Option::as_ref)
            .and_then(|values| values.first())
            .copied()
            .ok_or_else(|| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::InternalBinding,
                    format!("{behavior} timeline behavior requires one input"),
                    span.clone(),
                )
            })
    }

    fn reject_reserved_timeline_label_collision(
        &self,
        children: &[TimelineChild],
        label: &str,
        operation: &str,
        root: TimelineViewId,
        span: &SourceSpan,
    ) -> Result<()> {
        if !children
            .iter()
            .any(|child| child.label.as_deref() == Some(label))
        {
            return Ok(());
        }
        Err(Diagnostic::builtin(
            BuiltinDiagnostic::TimelinePlacementConflict,
            format!(
                "`{operation}` cannot expose reserved placement `{label}` because the resulting timeline already contains that name"
            ),
            span.clone(),
        )
        .note(format!(
            "rename the surviving `{label}` placement before applying `{operation}`"
        ))
        .note(self.timeline_layout_note_for("base timeline", root)))
    }

    fn transition_outputs(
        &mut self,
        outputs: Vec<ValueRef>,
        before: EvaluatedValue,
        after: EvaluatedValue,
        overlap: Option<FrameCount>,
    ) -> Vec<EvaluatedValue> {
        let zero = TimelineExpression::constant(ExactNumber::from_integer(0));
        let before_extent = self.timeline.views[before.timeline_view.index()]
            .extent
            .clone();
        let after_extent = self.timeline.views[after.timeline_view.index()]
            .extent
            .clone();
        let overlap_extent = overlap
            .map(|frames| TimelineExpression::constant(frame_seconds(frames.0, self.timeline.fps)));
        let after_start = overlap_extent.as_ref().map_or_else(
            || before_extent.clone(),
            |overlap| before_extent.subtract(overlap),
        );
        let extent = after_start.add(&after_extent);
        let mut children = vec![
            TimelineChild {
                label: Some("before".to_owned()),
                view: before.timeline_view,
                start: zero,
            },
            TimelineChild {
                label: Some("after".to_owned()),
                view: after.timeline_view,
                start: after_start.clone(),
            },
        ];
        if let Some(overlap_extent) = overlap_extent {
            let overlap_view = self.add_timeline_view(ValueType::Video, overlap_extent, Vec::new());
            children.push(TimelineChild {
                label: Some("overlap".to_owned()),
                view: overlap_view,
                start: after_start,
            });
        }
        outputs
            .into_iter()
            .map(|value| EvaluatedValue {
                value,
                timeline_view: self.add_timeline_view(
                    value.value_type(),
                    extent.clone(),
                    children.clone(),
                ),
                placement_symbol: None,
            })
            .collect()
    }

    fn concat_outputs(
        &mut self,
        outputs: Vec<ValueRef>,
        values: &[EvaluatedValue],
    ) -> Vec<EvaluatedValue> {
        let mut offset = TimelineExpression::constant(ExactNumber::from_integer(0));
        let mut children = Vec::with_capacity(values.len());
        for value in values {
            let child_view = &self.timeline.views[value.timeline_view.index()];
            if let Some(symbol) = value.placement_symbol {
                children.push(TimelineChild {
                    label: Some(self.symbols[symbol.index()].name.clone()),
                    view: value.timeline_view,
                    start: offset.clone(),
                });
            } else if child_view.children.is_empty() {
                children.push(TimelineChild {
                    label: None,
                    view: value.timeline_view,
                    start: offset.clone(),
                });
            } else {
                children.extend(child_view.children.iter().cloned().map(|mut child| {
                    child.start = offset.add(&child.start);
                    child
                }));
            }
            offset = offset.add(&child_view.extent);
        }
        outputs
            .into_iter()
            .map(|value| EvaluatedValue {
                value,
                timeline_view: self.add_timeline_view(
                    value.value_type(),
                    offset.clone(),
                    children.clone(),
                ),
                placement_symbol: None,
            })
            .collect()
    }

    fn native_range_expression(&self, range: NativeRange) -> crate::model::TimelineRangeExpression {
        match range {
            NativeRange::Frames(range) => crate::model::TimelineRangeExpression {
                start: TimelineExpression::constant(frame_seconds(
                    range.start(),
                    self.timeline.fps,
                )),
                end: TimelineExpression::constant(frame_seconds(range.end(), self.timeline.fps)),
            },
            NativeRange::Samples(range) => crate::model::TimelineRangeExpression {
                start: TimelineExpression::constant(sample_seconds(
                    range.start(),
                    self.timeline.sample_rate,
                )),
                end: TimelineExpression::constant(sample_seconds(
                    range.end(),
                    self.timeline.sample_rate,
                )),
            },
        }
    }

    fn slice_range_expression(
        &self,
        output: ValueRef,
        span: &SourceSpan,
    ) -> Result<crate::model::TimelineRangeExpression> {
        match self.nodes[output.id().get() as usize].kind() {
            crate::semantic::SemanticNodeKind::Slice { range, .. } => {
                Ok(self.native_range_expression(*range))
            }
            crate::semantic::SemanticNodeKind::DeferredSlice { range, .. } => Ok(range.clone()),
            _ => Err(Diagnostic::builtin(
                BuiltinDiagnostic::InternalBinding,
                "crop timeline behavior requires a slice output",
                span.clone(),
            )),
        }
    }

    fn replacement_range_expression(
        &self,
        output: ValueRef,
        span: &SourceSpan,
    ) -> Result<crate::model::TimelineRangeExpression> {
        match self.nodes[output.id().get() as usize].kind() {
            crate::semantic::SemanticNodeKind::ReplaceRange { range, .. } => {
                Ok(self.native_range_expression(*range))
            }
            crate::semantic::SemanticNodeKind::DeferredReplaceRange { range, .. } => {
                Ok(range.clone())
            }
            _ => Err(Diagnostic::builtin(
                BuiltinDiagnostic::InternalBinding,
                "replace timeline behavior requires a replacement output",
                span.clone(),
            )),
        }
    }

    fn crop_value(
        &mut self,
        output: ValueRef,
        source: EvaluatedValue,
        placement_symbol: Option<SymbolId>,
        span: &SourceSpan,
    ) -> Result<EvaluatedValue> {
        let range = self.slice_range_expression(output, span)?;
        let source_children = self.timeline.views[source.timeline_view.index()]
            .children
            .clone();
        let mut children = Vec::new();
        for child in source_children {
            let child_view = &self.timeline.views[child.view.index()];
            let child_end = child.start.add(&child_view.extent);
            if child.start.subtract(&range.start).is_nonnegative_constant()
                && range.end.subtract(&child_end).is_nonnegative_constant()
            {
                let start = child.start.subtract(&range.start);
                children.push(TimelineChild { start, ..child });
            }
        }
        let extent = range.end.subtract(&range.start);
        Ok(EvaluatedValue {
            value: output,
            timeline_view: self.add_timeline_view(output.value_type(), extent, children),
            placement_symbol,
        })
    }

    fn crop_outputs(
        &mut self,
        outputs: Vec<ValueRef>,
        source: EvaluatedValue,
        span: &SourceSpan,
    ) -> Result<Vec<EvaluatedValue>> {
        outputs
            .into_iter()
            .map(|output| self.crop_value(output, source, source.placement_symbol, span))
            .collect()
    }

    fn replace_outputs(
        &mut self,
        outputs: Vec<ValueRef>,
        base: EvaluatedValue,
        replacement: EvaluatedValue,
        operation: &str,
        span: &SourceSpan,
    ) -> Result<Vec<EvaluatedValue>> {
        let Some(output) = outputs.first().copied() else {
            return Ok(Vec::new());
        };
        let range = self.replacement_range_expression(output, span)?;
        let (base_extent, base_children) = {
            let base_view = &self.timeline.views[base.timeline_view.index()];
            (base_view.extent.clone(), base_view.children.clone())
        };
        let replacement_extent = self.timeline.views[replacement.timeline_view.index()]
            .extent
            .clone();
        let selected_extent = range.end.subtract(&range.start);
        let shift = replacement_extent.subtract(&selected_extent);
        let mut before = Vec::new();
        let mut after = Vec::new();
        for mut child in base_children {
            let child_view = &self.timeline.views[child.view.index()];
            let child_end = child.start.add(&child_view.extent);
            if range.start.subtract(&child_end).is_nonnegative_constant() {
                before.push(child);
            } else if child.start.subtract(&range.end).is_nonnegative_constant() {
                child.start = child.start.add(&shift);
                after.push(child);
            }
        }
        let replacement_index = before.len();
        let mut children = before;
        children.extend(after);
        self.reject_reserved_timeline_label_collision(
            &children,
            "replacement",
            operation,
            base.timeline_view,
            span,
        )?;
        children.insert(
            replacement_index,
            TimelineChild {
                label: Some("replacement".to_owned()),
                view: replacement.timeline_view,
                start: range.start.clone(),
            },
        );
        let extent = base_extent.add(&shift);
        Ok(outputs
            .into_iter()
            .map(|value| EvaluatedValue {
                value,
                timeline_view: self.add_timeline_view(
                    value.value_type(),
                    extent.clone(),
                    children.clone(),
                ),
                placement_symbol: base.placement_symbol,
            })
            .collect())
    }

    fn identity_outputs(
        &mut self,
        outputs: Vec<ValueRef>,
        source: EvaluatedValue,
    ) -> Vec<EvaluatedValue> {
        let (value_type, extent, children) = {
            let source_view = &self.timeline.views[source.timeline_view.index()];
            (
                source_view.value_type,
                source_view.extent.clone(),
                source_view.children.clone(),
            )
        };
        outputs
            .into_iter()
            .map(|value| EvaluatedValue {
                value,
                timeline_view: self.add_timeline_view(value_type, extent.clone(), children.clone()),
                placement_symbol: source.placement_symbol,
            })
            .collect()
    }

    fn repeat_outputs(
        &mut self,
        outputs: Vec<ValueRef>,
        source: EvaluatedValue,
        span: &SourceSpan,
    ) -> Result<Vec<EvaluatedValue>> {
        let [output] = outputs.as_slice() else {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InternalProgramContract,
                "repeat timeline behavior requires exactly one output",
                span.clone(),
            ));
        };
        if *output == source.value {
            return Ok(self.identity_outputs(outputs, source));
        }
        let count = match self.nodes[output.id().get() as usize].kind() {
            crate::semantic::SemanticNodeKind::Repeat { count, .. } => count.get(),
            _ => {
                return Err(Diagnostic::builtin(
                    BuiltinDiagnostic::InternalProgramContract,
                    "repeat timeline behavior requires a repeat output",
                    span.clone(),
                ));
            }
        };
        let source_view = &self.timeline.views[source.timeline_view.index()];
        let extent = source_view
            .extent
            .multiply(&ExactNumber::from_unsigned_integer(count));
        Ok(outputs
            .into_iter()
            .map(|value| EvaluatedValue {
                value,
                timeline_view: self.add_timeline_view(
                    value.value_type(),
                    extent.clone(),
                    Vec::new(),
                ),
                placement_symbol: None,
            })
            .collect())
    }

    pub(super) fn evaluate_body_initial_values(
        &mut self,
        behavior: crate::program::TimelineBehavior,
        values: &[ValueRef],
        slots: &[Option<Vec<EvaluatedValue>>],
        span: &SourceSpan,
    ) -> Result<Vec<EvaluatedValue>> {
        match behavior {
            crate::program::TimelineBehavior::BodyConcat { inputs } => {
                if inputs.len() != values.len() {
                    return Err(Diagnostic::builtin(
                        BuiltinDiagnostic::InternalProgramContract,
                        format!(
                            "body-concat timeline behavior maps {} input(s), but the body plan prepared {} initial value(s)",
                            inputs.len(),
                            values.len()
                        ),
                        span.clone(),
                    ));
                }
                inputs
                    .iter()
                    .zip(values)
                    .map(|(input, value)| {
                        let evaluated =
                            Self::timeline_input(slots, *input, "body-concat initial value", span)?;
                        if evaluated.value != *value {
                            return Err(Diagnostic::builtin(
                                BuiltinDiagnostic::InternalProgramContract,
                                "body-concat initial value does not match its mapped input",
                                span.clone(),
                            ));
                        }
                        Ok(evaluated)
                    })
                    .collect()
            }
            crate::program::TimelineBehavior::Replace { base } => {
                let [selected] = values else {
                    return Err(Diagnostic::builtin(
                        BuiltinDiagnostic::InternalProgramContract,
                        "replace timeline behavior requires exactly one selected body value",
                        span.clone(),
                    ));
                };
                let base = Self::timeline_input(slots, base, "replace body input", span)?;
                Ok(vec![self.crop_value(*selected, base, None, span)?])
            }
            _ => Ok(values
                .iter()
                .copied()
                .map(|value| self.fresh_evaluated(value))
                .collect()),
        }
    }

    pub(super) fn apply_timeline_behavior(
        &mut self,
        behavior: crate::program::TimelineBehavior,
        outputs: Vec<ValueRef>,
        slots: &[Option<Vec<EvaluatedValue>>],
        body_outputs: Option<&[EvaluatedValue]>,
        operation: &str,
        span: &SourceSpan,
    ) -> Result<Vec<EvaluatedValue>> {
        match behavior {
            crate::program::TimelineBehavior::Fresh => Ok(outputs
                .into_iter()
                .map(|value| self.fresh_evaluated(value))
                .collect()),
            crate::program::TimelineBehavior::Identity { input } => {
                let source = Self::timeline_input(slots, input, "identity", span)?;
                Ok(self.identity_outputs(outputs, source))
            }
            crate::program::TimelineBehavior::Repeat { input } => {
                let source = Self::timeline_input(slots, input, operation, span)?;
                self.repeat_outputs(outputs, source, span)
            }
            crate::program::TimelineBehavior::Concat { input } => {
                let values = slots
                    .get(input.index())
                    .and_then(Option::as_ref)
                    .ok_or_else(|| {
                        Diagnostic::builtin(
                            BuiltinDiagnostic::InternalBinding,
                            "concat timeline behavior requires its input sequence",
                            span.clone(),
                        )
                    })?;
                Ok(self.concat_outputs(outputs, values))
            }
            crate::program::TimelineBehavior::BodyConcat { .. } => {
                let values = body_outputs.ok_or_else(|| {
                    Diagnostic::builtin(
                        BuiltinDiagnostic::InternalBinding,
                        "body-concat timeline behavior requires body outputs",
                        span.clone(),
                    )
                })?;
                Ok(self.concat_outputs(outputs, values))
            }
            crate::program::TimelineBehavior::Crop { input } => {
                let source = Self::timeline_input(slots, input, "crop", span)?;
                self.crop_outputs(outputs, source, span)
            }
            crate::program::TimelineBehavior::Replace { base } => {
                let base = Self::timeline_input(slots, base, "replace", span)?;
                let [replacement] = body_outputs.unwrap_or_default() else {
                    return Err(Diagnostic::builtin(
                        BuiltinDiagnostic::InternalBinding,
                        "replace timeline behavior requires one body output",
                        span.clone(),
                    ));
                };
                self.replace_outputs(outputs, base, *replacement, operation, span)
            }
            crate::program::TimelineBehavior::FlashCut { before, after } => {
                let before = Self::timeline_input(slots, before, "flash-cut", span)?;
                let after = Self::timeline_input(slots, after, "flash-cut", span)?;
                Ok(self.transition_outputs(outputs, before, after, None))
            }
            crate::program::TimelineBehavior::Crossfade { before, after } => {
                let before = Self::timeline_input(slots, before, "crossfade", span)?;
                let after = Self::timeline_input(slots, after, "crossfade", span)?;
                let overlap = outputs
                    .first()
                    .and_then(|value| match self.nodes[value.id().get() as usize].kind() {
                        crate::semantic::SemanticNodeKind::Crossfade { frames, .. } => {
                            Some(*frames)
                        }
                        _ => None,
                    })
                    .ok_or_else(|| {
                        Diagnostic::builtin(
                            BuiltinDiagnostic::InternalBinding,
                            "crossfade timeline behavior requires a crossfade output",
                            span.clone(),
                        )
                    })?;
                Ok(self.transition_outputs(outputs, before, after, Some(overlap)))
            }
        }
    }
}
