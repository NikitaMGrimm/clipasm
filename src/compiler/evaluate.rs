use std::collections::BTreeMap;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{
    AudioSpec, ExactNumber, FrameCount, FrameRate, TimelineExpression, TimelineViewId, ValueRef,
    ValueType, VideoSpec,
};
use crate::program::{
    Cardinality, InputPort, ParameterSlot, ProgramDefinition, ProgramImplementation,
    RequestedVideoExtent, ResolvedCall, ResolvedInput, ValueTypeSpec,
};
use crate::semantic::{DraftNode, GraphBuilder, SourceOrigin, SymbolId};
use crate::source::{SourceSpan, SourceUnitId, Spanned, SurfaceVisibility};

use super::EntrypointBindings;
use super::checked::{
    CheckedBody, CheckedInputValue, CheckedInvocation, CheckedItem, CheckedItemKind,
    CheckedPackage, CheckedParameterValue, CheckedScalarExpression, CheckedSourceProgram,
    ReferenceTarget,
};

use super::stack::{EvaluationStack, StackFrame};

#[derive(Clone, Debug)]
pub(super) struct Symbol {
    pub(super) name: String,
    pub(super) declared_at: SourceSpan,
    pub(super) value: Option<ValueRef>,
    pub(super) timeline_view: Option<TimelineViewId>,
    pub(super) value_type: ValueType,
}

#[derive(Clone, Copy, Debug)]
struct EvaluatedValue {
    value: ValueRef,
    timeline_view: TimelineViewId,
    placement_symbol: Option<SymbolId>,
}

impl EvaluatedValue {
    fn value_type(self) -> ValueType {
        self.value.value_type()
    }
}

#[derive(Clone, Debug)]
struct TimelinePlacement {
    view: TimelineViewId,
    start: TimelineExpression,
}

#[derive(Clone, Debug)]
struct TimelineChild {
    label: Option<String>,
    view: TimelineViewId,
    start: TimelineExpression,
}

#[derive(Clone, Debug)]
struct TimelineView {
    value_type: ValueType,
    extent: TimelineExpression,
    placements: BTreeMap<String, Vec<TimelinePlacement>>,
    children: Vec<TimelineChild>,
}

#[derive(Clone, Debug)]
pub(super) struct SurfaceRecord {
    pub(super) construct: String,
    pub(super) outputs: Vec<SurfaceOutput>,
    pub(super) span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(super) struct SurfaceOutput {
    pub(super) value: ValueRef,
    pub(super) id: Option<String>,
}

pub(super) struct Evaluation {
    pub(super) nodes: Vec<DraftNode>,
    pub(super) symbols: Vec<Symbol>,
    pub(super) public_symbols: BTreeMap<String, SymbolId>,
    pub(super) surface: Vec<SurfaceRecord>,
    pub(super) outputs: Vec<ValueRef>,
}

pub(super) fn evaluate(
    video: &VideoSpec,
    audio: AudioSpec,
    root_source: &crate::source::SourceProgram,
    checked: &CheckedPackage,
    bindings: &EntrypointBindings,
) -> Result<Evaluation> {
    let context = EvaluationContext {
        video,
        audio,
        registry: &checked.registry,
        programs: &checked.programs,
        root: checked.root,
    };
    let mut evaluator = Evaluator {
        nodes: Vec::new(),
        symbols: Vec::new(),
        public_symbols: BTreeMap::new(),
        surface: Vec::new(),
        timeline_views: Vec::new(),
        fps: video.fps(),
    };
    let root_program = context.programs[context.root.index()].definition();
    let root_definition = context.registry.definition(root_program);
    let root_call = super::entrypoint::bind_root_call(
        root_definition,
        root_source,
        context.registry,
        bindings,
        &mut evaluator.nodes,
        context.video,
        context.audio,
    )?;
    let evaluated_outputs = match &root_definition.implementation {
        ProgramImplementation::ClipAsm(_) => {
            evaluator.evaluate_program(&context, context.root, Some(&root_call), true)?
        }
        ProgramImplementation::External(external) => {
            let origin = SourceOrigin::new("root program", root_source.span().clone());
            let invocation = external.invocation(&root_call)?;
            let mut builder = GraphBuilder::for_program(
                &mut evaluator.nodes,
                context.video,
                context.audio,
                root_definition.descriptor.semantic_version,
                origin,
            );
            let value = builder.external_video(invocation)?;
            vec![evaluator.fresh_evaluated(value)]
        }
        ProgramImplementation::Direct(_) | ProgramImplementation::Body { .. } => {
            unreachable!("source unit definitions are ClipAsm or external")
        }
    };
    let outputs = evaluated_outputs
        .iter()
        .map(|output| output.value)
        .collect();
    Ok(Evaluation {
        nodes: evaluator.nodes,
        symbols: evaluator.symbols,
        public_symbols: evaluator.public_symbols,
        surface: evaluator.surface,
        outputs,
    })
}

struct EvaluationContext<'a> {
    video: &'a VideoSpec,
    audio: AudioSpec,
    registry: &'a crate::program::ProgramRegistry,
    programs: &'a [CheckedSourceProgram],
    root: SourceUnitId,
}

struct Evaluator {
    nodes: Vec<DraftNode>,
    symbols: Vec<Symbol>,
    public_symbols: BTreeMap<String, SymbolId>,
    surface: Vec<SurfaceRecord>,
    timeline_views: Vec<TimelineView>,
    fps: FrameRate,
}

struct EvalScope {
    local_symbols: Vec<SymbolId>,
    body_inputs: Vec<Option<EvaluatedValue>>,
    parameters: Vec<Spanned<crate::program::ParameterValue>>,
    scalar_locals: Vec<Option<CheckedScalarExpression>>,
}

#[derive(Clone)]
struct InvocationSite<'a> {
    construct: &'a str,
    span: &'a SourceSpan,
    requested_extent: Option<RequestedVideoExtent>,
}

struct TimelineSelectorContext<'a> {
    root_name: &'a str,
    path: &'a [String],
    contextual: bool,
    span: &'a SourceSpan,
    scope: &'a EvalScope,
    slots: &'a [Option<Vec<EvaluatedValue>>],
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

impl Evaluator {
    fn add_timeline_view(
        &mut self,
        value_type: ValueType,
        extent: TimelineExpression,
        children: Vec<TimelineChild>,
    ) -> TimelineViewId {
        let mut placements = BTreeMap::<String, Vec<TimelinePlacement>>::new();
        for child in &children {
            let Some(label) = &child.label else {
                continue;
            };
            placements
                .entry(label.clone())
                .or_default()
                .push(TimelinePlacement {
                    view: child.view,
                    start: child.start.clone(),
                });
        }
        let id = TimelineViewId::new(
            u32::try_from(self.timeline_views.len()).expect("timeline view count fits in u32"),
        );
        self.timeline_views.push(TimelineView {
            value_type,
            extent,
            placements,
            children,
        });
        id
    }

    fn fresh_evaluated(&mut self, value: ValueRef) -> EvaluatedValue {
        let extent = match self.nodes[value.id().get() as usize].kind() {
            crate::semantic::SemanticNodeKind::ImageVideo { frames, .. } => {
                TimelineExpression::constant(frame_seconds(frames.0, self.fps))
            }
            crate::semantic::SemanticNodeKind::DeferredImageVideo { extent, .. } => extent.clone(),
            crate::semantic::SemanticNodeKind::Slice { range, .. } => {
                TimelineExpression::constant(frame_seconds(range.frames().0, self.fps))
            }
            crate::semantic::SemanticNodeKind::DeferredSlice { range, .. } => {
                range.end.subtract(&range.start)
            }
            crate::semantic::SemanticNodeKind::DeferredReplaceRange {
                base,
                replacement,
                range,
            } => TimelineExpression::extent(*base, frame_seconds(1, self.fps))
                .subtract(&range.end.subtract(&range.start))
                .add(&TimelineExpression::extent(
                    *replacement,
                    frame_seconds(1, self.fps),
                )),
            _ => TimelineExpression::extent(value, frame_seconds(1, self.fps)),
        };
        let timeline_view = self.add_timeline_view(value.value_type(), extent, Vec::new());
        EvaluatedValue {
            value,
            timeline_view,
            placement_symbol: None,
        }
    }

    fn resolve_timeline_selector(
        &self,
        target: ReferenceTarget,
        context: &TimelineSelectorContext<'_>,
    ) -> Result<super::parameter::TimelineSelectorValue> {
        let target_view = self.selector_target_view(target, context)?;
        let Some(last) = context.path.last() else {
            return Err(Diagnostic::new(
                "E_INVALID_TIMELINE_SELECTOR",
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
        let view = &self.timeline_views[current.index()];
        let layout = self.timeline_layout_note(context.root_name, root);
        if view.value_type != ValueType::Video {
            return Err(Diagnostic::new(
                "E_UNSUPPORTED_TIMELINE_SELECTOR",
                "frame marker selectors currently require a Video timeline",
                context.span.clone(),
            ));
        }
        let end = Self::selector_end(view, &offset);
        Ok(match boundary {
            Some("start") => super::parameter::TimelineSelectorValue::Coordinate {
                owner: root,
                expression: offset,
                layout: layout.clone(),
            },
            Some("middle") => super::parameter::TimelineSelectorValue::Coordinate {
                owner: root,
                expression: offset
                    .add(&end)
                    .divide(&ExactNumber::from_integer(2))
                    .expect("two is nonzero"),
                layout: layout.clone(),
            },
            Some("end") => super::parameter::TimelineSelectorValue::Coordinate {
                owner: root,
                expression: end,
                layout,
            },
            Some(_) => unreachable!("known timeline boundary"),
            None => super::parameter::TimelineSelectorValue::Range {
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
                    Diagnostic::new(
                        "E_UNRESOLVED_TIMELINE",
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
                    Diagnostic::new(
                        "E_UNRESOLVED_TIMELINE",
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
        let mut candidates = Vec::new();
        for bound in &bound_views {
            self.collect_contextual_selector_candidates(*bound, &selector_path, &mut candidates);
        }
        match candidates.as_slice() {
            [(owner, view, start)] => Ok((*owner, *view, start.clone(), true)),
            [] => Ok((
                target_view,
                target_view,
                TimelineExpression::constant(ExactNumber::from_integer(0)),
                false,
            )),
            _ => {
                let mut diagnostic = Diagnostic::new(
                    "E_AMBIGUOUS_TIMELINE_PLACEMENT",
                    format!(
                        "selector `${}` matches {} placements in the bound timeline context",
                        selector_path.join("::"),
                        candidates.len()
                    ),
                    context.span.clone(),
                )
                .note(
                    "qualify the selector with more leading placement names or its owning timeline",
                );
                for (index, bound) in bound_views.into_iter().enumerate() {
                    diagnostic =
                        diagnostic.note(self.timeline_layout_note_for(
                            &format!("bound timeline {}", index + 1),
                            bound,
                        ));
                }
                Err(diagnostic)
            }
        }
    }

    fn collect_contextual_selector_candidates(
        &self,
        owner: TimelineViewId,
        selector_path: &[&str],
        candidates: &mut Vec<(TimelineViewId, TimelineViewId, TimelineExpression)>,
    ) {
        let zero = TimelineExpression::constant(ExactNumber::from_integer(0));
        let mut pending = vec![(owner, zero)];
        while let Some((current, base)) = pending.pop() {
            let view = &self.timeline_views[current.index()];
            if let Some(first) = selector_path.first()
                && let Some(placements) = view.placements.get(*first)
            {
                let mut matches = placements
                    .iter()
                    .map(|placement| (placement.view, base.add(&placement.start)))
                    .collect::<Vec<_>>();
                for name in &selector_path[1..] {
                    let mut next = Vec::new();
                    for (matched_view, matched_start) in matches {
                        if let Some(placements) = self.timeline_views[matched_view.index()]
                            .placements
                            .get(*name)
                        {
                            next.extend(placements.iter().map(|placement| {
                                (placement.view, matched_start.add(&placement.start))
                            }));
                        }
                    }
                    matches = next;
                    if matches.is_empty() {
                        break;
                    }
                }
                candidates.extend(
                    matches
                        .into_iter()
                        .map(|(view, start)| (owner, view, start)),
                );
            }
            for child in &view.children {
                let start = base.add(&child.start);
                pending.push((child.view, start));
            }
        }
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
            let placement = match self.timeline_views[current.index()]
                .placements
                .get(name)
                .map(Vec::as_slice)
            {
                Some([placement]) => placement,
                Some(placements) => {
                    return Err(Diagnostic::new(
                        "E_AMBIGUOUS_TIMELINE_PLACEMENT",
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
                    return Err(Diagnostic::new(
                        "E_UNKNOWN_TIMELINE_PLACEMENT",
                        format!(
                            "timeline `${}` has no placement named `{name}`",
                            context.root_name
                        ),
                        context.span.clone(),
                    )
                    .note(self.timeline_layout_note(context.root_name, root)));
                }
            };
            offset = offset.add(&placement.start);
            current = placement.view;
        }
        Ok((current, offset))
    }

    fn selector_end(view: &TimelineView, offset: &TimelineExpression) -> TimelineExpression {
        offset.add(&view.extent)
    }

    fn timeline_layout_note(&self, root_name: &str, root: TimelineViewId) -> String {
        self.timeline_layout_note_for(&format!("`${root_name}`"), root)
    }

    fn timeline_layout_note_for(&self, label: &str, root: TimelineViewId) -> String {
        const MAX_DEPTH: usize = 12;
        const MAX_NODES: usize = 64;

        let zero = TimelineExpression::constant(ExactNumber::from_integer(0));
        let root_view = &self.timeline_views[root.index()];
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

    #[allow(clippy::too_many_arguments)]
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
        let view = &self.timeline_views[view.index()];
        let children = &view.children;
        for (index, child) in children.iter().enumerate() {
            if *remaining == 0 {
                return;
            }
            *remaining -= 1;
            let last = index + 1 == children.len();
            let branch = if last { "└── " } else { "├── " };
            let start = base.add(&child.start);
            let child_view = &self.timeline_views[child.view.index()];
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
        parts.extend(expression.terms().iter().map(|term| {
            format!(
                "{}s×frames(v{})",
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
                Diagnostic::new(
                    "E_INTERNAL_BINDING",
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
        Err(Diagnostic::new(
            "E_TIMELINE_PLACEMENT_CONFLICT",
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
        let before_view = self.timeline_views[before.timeline_view.index()].clone();
        let after_view = self.timeline_views[after.timeline_view.index()].clone();
        let overlap_extent =
            overlap.map(|frames| TimelineExpression::constant(frame_seconds(frames.0, self.fps)));
        let after_start = overlap_extent.as_ref().map_or_else(
            || before_view.extent.clone(),
            |overlap| before_view.extent.subtract(overlap),
        );
        let extent = after_start.add(&after_view.extent);
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
                    ValueType::Video,
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
            let child_view = &self.timeline_views[value.timeline_view.index()];
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

    fn crop_outputs(
        &mut self,
        outputs: Vec<ValueRef>,
        source: EvaluatedValue,
        span: &SourceSpan,
    ) -> Result<Vec<EvaluatedValue>> {
        let Some(output) = outputs.first().copied() else {
            return Ok(Vec::new());
        };
        if output.value_type() != ValueType::Video {
            return Ok(outputs
                .into_iter()
                .map(|value| self.fresh_evaluated(value))
                .collect());
        }
        let range = match self.nodes[output.id().get() as usize].kind() {
            crate::semantic::SemanticNodeKind::Slice { range, .. } => {
                crate::model::TimelineRangeExpression {
                    start: TimelineExpression::constant(frame_seconds(range.start(), self.fps)),
                    end: TimelineExpression::constant(frame_seconds(range.end(), self.fps)),
                }
            }
            crate::semantic::SemanticNodeKind::DeferredSlice { range, .. } => range.clone(),
            _ => {
                return Err(Diagnostic::new(
                    "E_INTERNAL_BINDING",
                    "crop timeline behavior requires a slice output",
                    span.clone(),
                ));
            }
        };
        let source_view = self.timeline_views[source.timeline_view.index()].clone();
        let mut children = Vec::new();
        for child in source_view.children {
            let child_view = &self.timeline_views[child.view.index()];
            let child_end = child.start.add(&child_view.extent);
            if child.start.subtract(&range.start).is_nonnegative_constant()
                && range.end.subtract(&child_end).is_nonnegative_constant()
            {
                let start = child.start.subtract(&range.start);
                children.push(TimelineChild { start, ..child });
            }
        }
        let extent = range.end.subtract(&range.start);
        Ok(outputs
            .into_iter()
            .map(|value| EvaluatedValue {
                value,
                timeline_view: self.add_timeline_view(
                    ValueType::Video,
                    extent.clone(),
                    children.clone(),
                ),
                placement_symbol: source.placement_symbol,
            })
            .collect())
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
        let range = match self.nodes[output.id().get() as usize].kind() {
            crate::semantic::SemanticNodeKind::ReplaceRange { range, .. } => {
                crate::model::TimelineRangeExpression {
                    start: TimelineExpression::constant(frame_seconds(range.start(), self.fps)),
                    end: TimelineExpression::constant(frame_seconds(range.end(), self.fps)),
                }
            }
            crate::semantic::SemanticNodeKind::DeferredReplaceRange { range, .. } => range.clone(),
            _ => {
                return Err(Diagnostic::new(
                    "E_INTERNAL_BINDING",
                    "replace timeline behavior requires a replacement output",
                    span.clone(),
                ));
            }
        };
        let base_view = self.timeline_views[base.timeline_view.index()].clone();
        let replacement_view = self.timeline_views[replacement.timeline_view.index()].clone();
        let selected_extent = range.end.subtract(&range.start);
        let shift = replacement_view.extent.subtract(&selected_extent);
        let mut children = Vec::new();
        for mut child in base_view.children {
            let child_view = &self.timeline_views[child.view.index()];
            let child_end = child.start.add(&child_view.extent);
            let survives = if range.start.subtract(&child_end).is_nonnegative_constant() {
                true
            } else if child.start.subtract(&range.end).is_nonnegative_constant() {
                child.start = child.start.add(&shift);
                true
            } else {
                false
            };
            if !survives {
                continue;
            }
            children.push(child);
        }
        self.reject_reserved_timeline_label_collision(
            &children,
            "replacement",
            operation,
            base.timeline_view,
            span,
        )?;
        children.push(TimelineChild {
            label: Some("replacement".to_owned()),
            view: replacement.timeline_view,
            start: range.start.clone(),
        });
        let extent = base_view.extent.add(&shift);
        Ok(outputs
            .into_iter()
            .map(|value| EvaluatedValue {
                value,
                timeline_view: self.add_timeline_view(
                    ValueType::Video,
                    extent.clone(),
                    children.clone(),
                ),
                placement_symbol: base.placement_symbol,
            })
            .collect())
    }

    fn apply_timeline_behavior(
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
                let source = slots
                    .get(input.index())
                    .and_then(Option::as_ref)
                    .and_then(|values| values.first())
                    .copied()
                    .ok_or_else(|| {
                        Diagnostic::new(
                            "E_INTERNAL_BINDING",
                            "identity timeline behavior requires one input",
                            span.clone(),
                        )
                    })?;
                outputs
                    .into_iter()
                    .map(|value| {
                        let source_view = self.timeline_views[source.timeline_view.index()].clone();
                        let timeline_view = self.add_timeline_view(
                            source_view.value_type,
                            source_view.extent,
                            source_view.children,
                        );
                        Ok(EvaluatedValue {
                            value,
                            timeline_view,
                            placement_symbol: source.placement_symbol,
                        })
                    })
                    .collect()
            }
            crate::program::TimelineBehavior::Concat { input } => {
                let values = slots
                    .get(input.index())
                    .and_then(Option::as_ref)
                    .ok_or_else(|| {
                        Diagnostic::new(
                            "E_INTERNAL_BINDING",
                            "concat timeline behavior requires its input sequence",
                            span.clone(),
                        )
                    })?;
                Ok(self.concat_outputs(outputs, values))
            }
            crate::program::TimelineBehavior::BodyConcat { .. } => {
                let values = body_outputs.ok_or_else(|| {
                    Diagnostic::new(
                        "E_INTERNAL_BINDING",
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
                    return Err(Diagnostic::new(
                        "E_INTERNAL_BINDING",
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
                        Diagnostic::new(
                            "E_INTERNAL_BINDING",
                            "crossfade timeline behavior requires a crossfade output",
                            span.clone(),
                        )
                    })?;
                Ok(self.transition_outputs(outputs, before, after, Some(overlap)))
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn evaluate_program(
        &mut self,
        context: &EvaluationContext<'_>,
        unit: SourceUnitId,
        call: Option<&ResolvedCall>,
        public: bool,
    ) -> Result<Vec<EvaluatedValue>> {
        let CheckedSourceProgram::ClipAsm {
            program: checked_program,
            ..
        } = &context.programs[unit.index()]
        else {
            unreachable!("ClipAsm program implementation refers to a ClipAsm source unit");
        };
        let mut scope = EvalScope {
            local_symbols: Vec::with_capacity(checked_program.locals.len()),
            body_inputs: vec![None; checked_program.body_input_count],
            parameters: checked_program
                .parameters
                .iter()
                .enumerate()
                .map(|(index, parameter)| {
                    call.and_then(|call| call.parameter_at(ParameterSlot::new(index)).cloned())
                        .or_else(|| parameter.default.clone())
                        .ok_or_else(|| {
                            Diagnostic::new(
                                if public {
                                    "E_MISSING_ARGUMENT"
                                } else {
                                    "E_INTERNAL_BINDING"
                                },
                                if public {
                                    format!(
                                        "root program is missing parameter `{}`",
                                        parameter.name
                                    )
                                } else {
                                    format!(
                                        "authored program parameter `{}` was not bound",
                                        parameter.name
                                    )
                                },
                                parameter.declared_at.clone(),
                            )
                        })
                })
                .collect::<Result<Vec<_>>>()?,
            scalar_locals: checked_program
                .scalar_locals
                .iter()
                .map(|local| local.as_ref().map(|local| local.expression.clone()))
                .collect(),
        };
        for local in &checked_program.locals {
            let symbol = self.add_symbol(&local.name, &local.declared_at, local.value_type)?;
            scope.local_symbols.push(symbol);
            if public {
                self.public_symbols.insert(local.name.clone(), symbol);
            }
        }

        if let Some(call) = call {
            debug_assert_eq!(checked_program.inputs.len(), call.inputs().len());
            for (input, (_, binding)) in checked_program.inputs.iter().zip(call.inputs()) {
                let ResolvedInput::One(value) = binding else {
                    return Err(Diagnostic::new(
                        "E_INTERNAL_BINDING",
                        format!(
                            "authored program input `{}` requires exactly one value",
                            input.name
                        ),
                        checked_program.span.clone(),
                    ));
                };
                let symbol = scope.local_symbols[input.local.index()];
                let evaluated = self.fresh_evaluated(*value);
                self.bind_symbol(symbol, evaluated)?;
            }
        } else if let Some(input) = checked_program.inputs.first() {
            return Err(Diagnostic::new(
                "E_MISSING_REQUIRED_INPUT",
                format!("root program is missing input `{}`", input.name),
                input.declared_at.clone(),
            ));
        }
        let (mut stack, parent) =
            EvaluationStack::isolated("authored program", checked_program.span.clone());
        let mut body_frame = EvaluationStack::<EvaluatedValue>::enter_body(
            &parent,
            checked_program.stack_access,
            "source program",
            checked_program.span.clone(),
        );
        self.evaluate_body(
            context,
            &checked_program.body,
            &mut scope,
            &mut stack,
            &mut body_frame,
            None,
        )?;
        Ok(stack.finish_body(&body_frame))
    }

    fn add_symbol(
        &mut self,
        name: &str,
        span: &SourceSpan,
        value_type: ValueType,
    ) -> Result<SymbolId> {
        let symbol = SymbolId::new(u32::try_from(self.symbols.len()).map_err(|_| {
            Diagnostic::new(
                "E_GRAPH_TOO_LARGE",
                "too many named values were declared",
                span.clone(),
            )
        })?);
        self.symbols.push(Symbol {
            name: name.to_owned(),
            declared_at: span.clone(),
            value: None,
            timeline_view: None,
            value_type,
        });
        Ok(symbol)
    }

    fn evaluate_body(
        &mut self,
        context: &EvaluationContext<'_>,
        checked: &CheckedBody,
        scope: &mut EvalScope,
        stack: &mut EvaluationStack<EvaluatedValue>,
        frame: &mut StackFrame,
        requested_extent: Option<&RequestedVideoExtent>,
    ) -> Result<()> {
        for item in &checked.items {
            self.evaluate_item(context, item, scope, stack, frame, requested_extent)?;
        }
        Ok(())
    }

    fn evaluate_item(
        &mut self,
        context: &EvaluationContext<'_>,
        checked: &CheckedItem,
        scope: &mut EvalScope,
        stack: &mut EvaluationStack<EvaluatedValue>,
        frame: &mut StackFrame,
        requested_extent: Option<&RequestedVideoExtent>,
    ) -> Result<()> {
        let mut outputs = match &checked.kind {
            CheckedItemKind::Reference { target } => {
                vec![self.evaluate_checked_reference(
                    context,
                    *target,
                    &checked.origin.span,
                    scope,
                )?]
            }
            CheckedItemKind::Invocation(invocation) => self.evaluate_invocation(
                context,
                invocation,
                InvocationSite {
                    construct: &checked.origin.construct,
                    span: &checked.origin.span,
                    requested_extent: requested_extent.cloned(),
                },
                scope,
                stack,
                frame,
            )?,
            CheckedItemKind::StackBlock(block) => {
                let mut child = EvaluationStack::<EvaluatedValue>::enter_body(
                    frame,
                    block.access,
                    checked.origin.construct.clone(),
                    checked.origin.span.clone(),
                );
                self.evaluate_body(
                    context,
                    &block.body,
                    scope,
                    stack,
                    &mut child,
                    requested_extent,
                )?;
                stack.finish_body(&child)
            }
        };
        debug_assert_eq!(outputs.len(), checked.outputs.len());
        for (output, metadata) in outputs.iter_mut().zip(&checked.outputs) {
            debug_assert_eq!(output.value_type(), metadata.value_type);
            if let Some(local) = metadata.binding {
                let symbol = scope.local_symbols[local.index()];
                output.placement_symbol = Some(symbol);
                self.bind_symbol(symbol, *output)?;
            }
        }
        stack.extend(frame, outputs.iter().copied());
        if checked.origin.visibility == SurfaceVisibility::Visible {
            self.surface.push(SurfaceRecord {
                construct: checked.origin.construct.clone(),
                outputs: outputs
                    .into_iter()
                    .zip(&checked.outputs)
                    .map(|(value, metadata)| SurfaceOutput {
                        value: value.value,
                        id: metadata.name.clone(),
                    })
                    .collect(),
                span: checked.origin.span.clone(),
            });
        }
        Ok(())
    }

    fn evaluate_checked_reference(
        &mut self,
        context: &EvaluationContext<'_>,
        target: ReferenceTarget,
        span: &SourceSpan,
        scope: &EvalScope,
    ) -> Result<EvaluatedValue> {
        match target {
            ReferenceTarget::Local(local) => {
                let symbol = scope.local_symbols[local.index()];
                let value_type = self.symbols[symbol.index()].value_type;
                let existing_view = self.symbols[symbol.index()].timeline_view;
                let origin = SourceOrigin::new("reference", span.clone());
                let value = GraphBuilder::for_program(
                    &mut self.nodes,
                    context.video,
                    context.audio,
                    1,
                    origin,
                )
                .reference(symbol, value_type)?;
                let timeline_view =
                    existing_view.unwrap_or_else(|| self.fresh_evaluated(value).timeline_view);
                Ok(EvaluatedValue {
                    value,
                    timeline_view,
                    placement_symbol: Some(symbol),
                })
            }
            ReferenceTarget::BodyInput(input) => {
                scope.body_inputs[input.index()].ok_or_else(|| {
                    Diagnostic::new(
                        "E_INTERNAL_BINDING",
                        "lexical body input was not bound during evaluation",
                        span.clone(),
                    )
                })
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn evaluate_invocation(
        &mut self,
        context: &EvaluationContext<'_>,
        invocation: &CheckedInvocation,
        site: InvocationSite<'_>,
        scope: &mut EvalScope,
        stack: &mut EvaluationStack<EvaluatedValue>,
        frame: &mut StackFrame,
    ) -> Result<Vec<EvaluatedValue>> {
        let construct = site.construct;
        let span = site.span;
        let requested_extent = site.requested_extent;
        let definition = context.registry.definition(invocation.program);
        let signature = &invocation.signature;
        let checked_inputs = &invocation.inputs;
        let checked_parameters = &invocation.parameters;
        let origin = SourceOrigin::new(construct, span.clone());
        debug_assert_eq!(signature.inputs.len(), checked_inputs.len());
        debug_assert_eq!(definition.descriptor.inputs.len(), checked_inputs.len());
        let mut slots = vec![None; signature.inputs.len()];
        for (index, ((port, expected_type), input)) in definition
            .descriptor
            .inputs
            .iter()
            .zip(&signature.inputs)
            .zip(checked_inputs)
            .enumerate()
        {
            if let Some(input) = input {
                slots[index] = Some(self.evaluate_checked_input(
                    context,
                    input,
                    (port, *expected_type),
                    construct,
                    requested_extent.as_ref(),
                    scope,
                )?);
            }
        }
        for bound in stack.apply_binding_plan(&invocation.stack_plan) {
            debug_assert!(slots[bound.port.index()].is_none());
            slots[bound.port.index()] = Some(bound.values);
        }
        let inputs = definition
            .descriptor
            .inputs
            .iter()
            .zip(&slots)
            .map(|(port, values)| {
                let values = values.as_ref().ok_or_else(|| {
                    Diagnostic::new(
                        "E_INTERNAL_BINDING",
                        format!(
                            "checked call to `{construct}` has no binding for input `{}`",
                            port.name
                        ),
                        span.clone(),
                    )
                })?;
                match port.cardinality {
                    Cardinality::One => {
                        let [value] = values.as_slice() else {
                            return Err(Diagnostic::new(
                                "E_INTERNAL_BINDING",
                                format!(
                                    "checked call to `{construct}` has invalid cardinality for input `{}`",
                                    port.name
                                ),
                                span.clone(),
                            ));
                        };
                        Ok(ResolvedInput::One(value.value))
                    }
                    Cardinality::Variadic { .. } => Ok(ResolvedInput::Variadic(
                        values.iter().map(|value| value.value).collect(),
                    )),
                }
            })
            .collect::<Result<Vec<_>>>()?;

        debug_assert_eq!(
            definition.descriptor.parameters.len(),
            checked_parameters.len()
        );
        let mut parameters = Vec::with_capacity(checked_parameters.len());
        for (descriptor, binding) in definition
            .descriptor
            .parameters
            .iter()
            .zip(checked_parameters)
        {
            let value = binding
                .as_ref()
                .map(|binding| match binding {
                    CheckedParameterValue::Expression(expression) => {
                        super::parameter::evaluate_expression(
                            construct,
                            &descriptor.name,
                            &descriptor.parameter_type,
                            expression,
                            &scope.parameters,
                            &scope.scalar_locals,
                            &mut |target, root_name, path, contextual, selector_span| {
                                self.resolve_timeline_selector(
                                    target,
                                    &TimelineSelectorContext {
                                        root_name,
                                        path,
                                        contextual,
                                        span: selector_span,
                                        scope,
                                        slots: &slots,
                                    },
                                )
                            },
                        )
                    }
                })
                .transpose()?;
            if let Some(parameter) = &value
                && let crate::program::ParameterValue::TimeRange(range) = &parameter.value
                && let Some(owner) = range.marker_owner()
                && !slots
                    .iter()
                    .flatten()
                    .flatten()
                    .any(|input| input.timeline_view == owner)
            {
                let mut diagnostic = Diagnostic::new(
                    "E_TIMELINE_ROOT_MISMATCH",
                    format!(
                        "timeline range for `{}.{}` does not belong to any bound input timeline",
                        construct, descriptor.name
                    ),
                    parameter.span.clone(),
                )
                .note(self.timeline_layout_note_for("marker range root", owner));
                let mut bound_views = slots
                    .iter()
                    .flatten()
                    .flatten()
                    .map(|input| input.timeline_view)
                    .collect::<Vec<_>>();
                bound_views.sort_unstable();
                bound_views.dedup();
                for (index, bound) in bound_views.into_iter().enumerate() {
                    diagnostic = diagnostic.note(
                        self.timeline_layout_note_for(&format!("bound input {}", index + 1), bound),
                    );
                }
                return Err(diagnostic);
            }
            parameters.push(value);
        }
        let call = ResolvedCall::new(
            &definition.descriptor,
            signature,
            inputs,
            parameters,
            requested_extent.clone(),
            origin.clone(),
        )?;

        let outputs = match &definition.implementation {
            ProgramImplementation::Direct(lower) => {
                let mut builder = GraphBuilder::for_program(
                    &mut self.nodes,
                    context.video,
                    context.audio,
                    definition.descriptor.semantic_version,
                    origin,
                );
                let values = lower(&call, &mut builder)?;
                self.apply_timeline_behavior(
                    definition.timeline_behavior,
                    values,
                    &slots,
                    None,
                    construct,
                    span,
                )?
            }
            ProgramImplementation::Body { prepare, .. } => {
                let checked_body = invocation
                    .body
                    .as_deref()
                    .expect("checked body program has checked body metadata");
                let plan = {
                    let mut builder = GraphBuilder::for_program(
                        &mut self.nodes,
                        context.video,
                        context.audio,
                        definition.descriptor.semantic_version,
                        origin.clone(),
                    );
                    prepare(&call, &mut builder)?
                };
                let mut child = EvaluationStack::<EvaluatedValue>::enter_body(
                    frame,
                    invocation.access,
                    definition.descriptor.name.clone(),
                    span.clone(),
                );
                let initial_values = match definition.timeline_behavior {
                    crate::program::TimelineBehavior::BodyConcat { inputs } => inputs
                        .iter()
                        .zip(plan.initial_values.iter().copied())
                        .map(|(input, value)| {
                            let evaluated = Self::timeline_input(
                                &slots,
                                *input,
                                "body-concat initial value",
                                span,
                            )?;
                            debug_assert_eq!(evaluated.value, value);
                            Ok(evaluated)
                        })
                        .collect::<Result<Vec<_>>>()?,
                    _ => plan
                        .initial_values
                        .iter()
                        .copied()
                        .map(|value| self.fresh_evaluated(value))
                        .collect::<Vec<_>>(),
                };
                stack.extend(&child, initial_values);
                debug_assert_eq!(
                    invocation.body_input_ids.len(),
                    definition.descriptor.inputs.len()
                );
                let mut bound_body_inputs = Vec::with_capacity(invocation.body_input_ids.len());
                for (index, ((port, _binding), id)) in
                    call.inputs().zip(&invocation.body_input_ids).enumerate()
                {
                    let Some(id) = id else {
                        debug_assert!(matches!(port.cardinality, Cardinality::Variadic { .. }));
                        continue;
                    };
                    let Some(values) = slots[index].as_ref() else {
                        return Err(Diagnostic::new(
                            "E_INTERNAL_BINDING",
                            format!(
                                "body input `{}.{}` has no evaluated value",
                                definition.descriptor.name, port.name
                            ),
                            span.clone(),
                        ));
                    };
                    let [value] = values.as_slice() else {
                        return Err(Diagnostic::new(
                            "E_INTERNAL_BINDING",
                            format!(
                                "body input `{}.{}` requires exactly one value",
                                definition.descriptor.name, port.name
                            ),
                            span.clone(),
                        ));
                    };
                    let previous = scope.body_inputs[id.index()].replace(*value);
                    debug_assert!(previous.is_none());
                    bound_body_inputs.push(*id);
                }
                let body_requested_extent =
                    plan.requested_extent.as_ref().or(requested_extent.as_ref());
                self.evaluate_body(
                    context,
                    checked_body,
                    scope,
                    stack,
                    &mut child,
                    body_requested_extent,
                )?;
                for id in bound_body_inputs {
                    scope.body_inputs[id.index()] = None;
                }
                let owned = stack.finish_body(&child);
                let owned_values = owned.iter().map(|value| value.value).collect();
                let mut builder = GraphBuilder::for_program(
                    &mut self.nodes,
                    context.video,
                    context.audio,
                    definition.descriptor.semantic_version,
                    origin,
                );
                let values = plan.finalizer.finish(owned_values, &mut builder)?;
                self.apply_timeline_behavior(
                    definition.timeline_behavior,
                    values,
                    &slots,
                    Some(&owned),
                    construct,
                    span,
                )?
            }
            ProgramImplementation::ClipAsm(unit) => {
                self.evaluate_program(context, *unit, Some(&call), false)?
            }
            ProgramImplementation::External(external) => {
                let invocation = external.invocation(&call)?;
                let mut builder = GraphBuilder::for_program(
                    &mut self.nodes,
                    context.video,
                    context.audio,
                    definition.descriptor.semantic_version,
                    origin,
                );
                let value = builder.external_video(invocation)?;
                vec![self.fresh_evaluated(value)]
            }
        };

        validate_program_outputs(
            definition,
            &signature.outputs,
            outputs.iter().map(|output| output.value).collect(),
            span,
        )?;
        Ok(outputs)
    }

    fn evaluate_checked_input(
        &mut self,
        context: &EvaluationContext<'_>,
        input: &CheckedInputValue,
        input_contract: (&InputPort, ValueType),
        program: &str,
        requested_extent: Option<&RequestedVideoExtent>,
        scope: &mut EvalScope,
    ) -> Result<Vec<EvaluatedValue>> {
        let (port, expected_type) = input_contract;
        let (values, span) = match input {
            CheckedInputValue::References(targets, span) => (
                targets
                    .iter()
                    .map(|target| self.evaluate_checked_reference(context, *target, span, scope))
                    .collect::<Result<Vec<_>>>()?,
                span,
            ),
            CheckedInputValue::Body(body, span) => {
                let (mut local, mut frame) = EvaluationStack::isolated(
                    format!("inline input body for `{program}.{}`", port.name),
                    span.clone(),
                );
                self.evaluate_body(
                    context,
                    body,
                    scope,
                    &mut local,
                    &mut frame,
                    requested_extent,
                )?;
                let [result] = local.values() else {
                    return Err(output_count_error(
                        "E_INPUT_BODY_OUTPUT_COUNT",
                        &format!("inline input body for `{program}.{}`", port.name),
                        local.len(),
                        span,
                    )
                    .note("combine multiple Videos explicitly with `concat`"));
                };
                (vec![*result], span)
            }
        };
        values
            .into_iter()
            .map(|value_ref| {
                if value_ref.value_type() == expected_type {
                    return Ok(value_ref);
                }
                if !matches!(port.value_type, ValueTypeSpec::Exact(_)) {
                    return Err(Diagnostic::new(
                        "E_INTERNAL_BINDING",
                        format!(
                            "checked `{program}.{}` input expected {}, but evaluated to {}",
                            port.name,
                            expected_type,
                            value_ref.value_type()
                        ),
                        span.clone(),
                    ));
                }
                let origin = SourceOrigin::new("input adaptation", span.clone());
                let mut builder = GraphBuilder::for_program(
                    &mut self.nodes,
                    context.video,
                    context.audio,
                    1,
                    origin,
                );
                let adapted = match (value_ref.value_type(), expected_type) {
                    (ValueType::Video, ValueType::Audio) => builder.extract_audio(value_ref.value),
                    (ValueType::Audio, ValueType::Video) => builder.audio_on_black(value_ref.value),
                    _ => Err(Diagnostic::new(
                        "E_INTERNAL_BINDING",
                        format!(
                            "checked `{program}.{}` adaptation cannot convert {} to {}",
                            port.name,
                            value_ref.value_type(),
                            expected_type
                        ),
                        span.clone(),
                    )),
                }?;
                Ok(self.fresh_evaluated(adapted))
            })
            .collect()
    }

    fn bind_symbol(&mut self, id: SymbolId, value: EvaluatedValue) -> Result<()> {
        let symbol = self
            .symbols
            .get_mut(id.index())
            .expect("all symbols are collected before evaluation");
        let declared_type = symbol.value_type;
        if declared_type != value.value_type() {
            return Err(Diagnostic::new(
                "E_TYPE_MISMATCH",
                format!(
                    "name `{}` was declared as {}, but its value is {}",
                    symbol.name,
                    declared_type,
                    value.value_type()
                ),
                symbol.declared_at.clone(),
            ));
        }
        if symbol.value.replace(value.value).is_some() {
            return Err(Diagnostic::new(
                "E_DUPLICATE_NAME",
                format!("name `{}` was bound more than once", symbol.name),
                symbol.declared_at.clone(),
            ));
        }
        symbol.timeline_view = Some(value.timeline_view);
        Ok(())
    }
}

fn validate_program_outputs(
    definition: &ProgramDefinition,
    expected_outputs: &[ValueType],
    outputs: Vec<ValueRef>,
    span: &SourceSpan,
) -> Result<Vec<ValueRef>> {
    if outputs.len() != expected_outputs.len() {
        return Err(Diagnostic::new(
            "E_PROGRAM_OUTPUT_COUNT",
            format!(
                "program `{}` declares {} output(s), but its implementation returned {}",
                definition.descriptor.name,
                expected_outputs.len(),
                outputs.len()
            ),
            span.clone(),
        ));
    }
    for (index, (output, expected)) in outputs.iter().zip(expected_outputs).enumerate() {
        if output.value_type() != *expected {
            return Err(Diagnostic::new(
                "E_PROGRAM_OUTPUT_TYPE",
                format!(
                    "program `{}` declares output {} as {}, but its implementation returned {}",
                    definition.descriptor.name,
                    index + 1,
                    expected,
                    output.value_type()
                ),
                span.clone(),
            ));
        }
    }
    Ok(outputs)
}

fn output_count_error(
    code: &'static str,
    owner: &str,
    count: usize,
    span: &SourceSpan,
) -> Diagnostic {
    Diagnostic::new(
        code,
        format!("{owner} must leave exactly one value, but {count} values remain"),
        span.clone(),
    )
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::model::{FrameCount, ImageFit};
    use crate::program::{
        BodyFinalizer, BodyPlan, Cardinality, InputPort, ProgramDefinition, ProgramDescriptor,
        ProgramRegistry, ResolvedCall, StackAccess,
    };

    #[allow(clippy::unnecessary_wraps)]
    fn prepare_root(call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
        Ok(BodyPlan {
            initial_values: Vec::new(),
            requested_extent: call.requested_extent().cloned(),
            finalizer: Box::new(RootFinalizer),
        })
    }

    struct RootFinalizer;

    impl BodyFinalizer for RootFinalizer {
        fn finish(
            self: Box<Self>,
            stack: Vec<ValueRef>,
            builder: &mut GraphBuilder<'_>,
        ) -> Result<Vec<ValueRef>> {
            Ok(vec![builder.concat(stack)?])
        }
    }

    fn lower_source(_call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<Vec<ValueRef>> {
        Ok(vec![builder.image_video(
            PathBuf::from("source.png"),
            FrameCount(1),
            ImageFit::Cover,
        )?])
    }

    fn lower_alias(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<Vec<ValueRef>> {
        Ok(vec![builder.concat(vec![call.one_input("video")?])?])
    }

    fn lower_wrong_type(
        _call: &ResolvedCall,
        builder: &mut GraphBuilder<'_>,
    ) -> Result<Vec<ValueRef>> {
        Ok(vec![builder.audio_source(PathBuf::from("wrong.wav"))?])
    }

    fn lower_two(_call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<Vec<ValueRef>> {
        Ok(vec![
            builder.image_video(PathBuf::from("first.png"), FrameCount(1), ImageFit::Cover)?,
            builder.image_video(PathBuf::from("second.png"), FrameCount(1), ImageFit::Cover)?,
        ])
    }

    fn lower_same_two(
        _call: &ResolvedCall,
        builder: &mut GraphBuilder<'_>,
    ) -> Result<Vec<ValueRef>> {
        let value =
            builder.image_video(PathBuf::from("shared.png"), FrameCount(1), ImageFit::Cover)?;
        Ok(vec![value, value])
    }

    #[allow(clippy::unnecessary_wraps)]
    fn lower_zero(_call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<Vec<ValueRef>> {
        Ok(Vec::new())
    }

    #[allow(clippy::unnecessary_wraps)]
    fn prepare_wrong_body(
        call: &ResolvedCall,
        _builder: &mut GraphBuilder<'_>,
    ) -> Result<BodyPlan> {
        Ok(BodyPlan {
            initial_values: Vec::new(),
            requested_extent: call.requested_extent().cloned(),
            finalizer: Box::new(WrongTypeFinalizer),
        })
    }

    struct WrongTypeFinalizer;

    impl BodyFinalizer for WrongTypeFinalizer {
        fn finish(
            self: Box<Self>,
            _stack: Vec<ValueRef>,
            builder: &mut GraphBuilder<'_>,
        ) -> Result<Vec<ValueRef>> {
            Ok(vec![builder.audio_source(PathBuf::from("wrong.wav"))?])
        }
    }

    fn prepare_versioned_body(
        call: &ResolvedCall,
        builder: &mut GraphBuilder<'_>,
    ) -> Result<BodyPlan> {
        let prepared = builder.image_video(
            PathBuf::from("prepared.png"),
            FrameCount(1),
            ImageFit::Cover,
        )?;
        Ok(BodyPlan {
            initial_values: vec![prepared],
            requested_extent: call.requested_extent().cloned(),
            finalizer: Box::new(VersionedFinalizer),
        })
    }

    struct VersionedFinalizer;

    impl BodyFinalizer for VersionedFinalizer {
        fn finish(
            self: Box<Self>,
            stack: Vec<ValueRef>,
            builder: &mut GraphBuilder<'_>,
        ) -> Result<Vec<ValueRef>> {
            let [value] = stack.as_slice() else {
                panic!("versioned body starts with one value");
            };
            Ok(vec![builder.concat(vec![*value, *value])?])
        }
    }

    fn definition(
        name: &str,
        semantic_version: u32,
        default_stack_access: StackAccess,
        inputs: Vec<InputPort>,
        outputs: Vec<ValueType>,
        implementation: ProgramImplementation,
    ) -> ProgramDefinition {
        ProgramDefinition {
            descriptor: ProgramDescriptor {
                name: name.to_owned(),
                semantic_version,
                default_stack_access,
                inputs,
                parameters: vec![],
                outputs: outputs.into_iter().map(Into::into).collect(),
            },
            implementation,
            timeline_behavior: crate::program::TimelineBehavior::Fresh,
        }
    }

    fn output_programs() -> Vec<ProgramDefinition> {
        vec![
            definition(
                "source",
                3,
                StackAccess::Owned,
                vec![],
                vec![ValueType::Video],
                ProgramImplementation::Direct(lower_source),
            ),
            definition(
                "wrong_direct",
                5,
                StackAccess::Owned,
                vec![],
                vec![ValueType::Video],
                ProgramImplementation::Direct(lower_wrong_type),
            ),
            definition(
                "wrong_body",
                7,
                StackAccess::Owned,
                vec![],
                vec![ValueType::Video],
                ProgramImplementation::Body {
                    prepare: prepare_wrong_body,
                    contract: crate::program::BodyContract {
                        initial_values: Vec::new(),
                        outputs: crate::program::BodyOutputConstraint::Exactly(vec![
                            ValueType::Video.into(),
                        ]),
                        count_error_code: "E_BODY_OUTPUT_COUNT",
                    },
                },
            ),
            definition(
                "wrong_count",
                1,
                StackAccess::Owned,
                vec![],
                vec![ValueType::Video, ValueType::Video],
                ProgramImplementation::Direct(lower_source),
            ),
        ]
    }

    fn version_programs() -> Vec<ProgramDefinition> {
        let mut versioned_body = definition(
            "versioned_body",
            17,
            StackAccess::Owned,
            vec![],
            vec![ValueType::Video],
            ProgramImplementation::Body {
                prepare: prepare_versioned_body,
                contract: crate::program::BodyContract {
                    initial_values: Vec::new(),
                    outputs: crate::program::BodyOutputConstraint::Exactly(vec![
                        ValueType::Video.into(),
                    ]),
                    count_error_code: "E_BODY_OUTPUT_COUNT",
                },
            },
        );
        let ProgramImplementation::Body { contract, .. } = &mut versioned_body.implementation
        else {
            unreachable!("versioned body implementation")
        };
        contract.initial_values = vec![ValueType::Video.into()];
        vec![
            definition(
                "versioned_direct",
                11,
                StackAccess::Owned,
                vec![],
                vec![ValueType::Video],
                ProgramImplementation::Direct(lower_source),
            ),
            definition(
                "drop",
                1,
                StackAccess::Owned,
                vec![InputPort {
                    name: "value".to_owned(),
                    value_type: ValueType::Video.into(),
                    cardinality: Cardinality::One,
                }],
                vec![],
                ProgramImplementation::Direct(lower_zero),
            ),
            versioned_body,
        ]
    }

    fn visible_default_programs() -> Vec<ProgramDefinition> {
        vec![
            definition(
                "source",
                3,
                StackAccess::Owned,
                vec![],
                vec![ValueType::Video],
                ProgramImplementation::Direct(lower_source),
            ),
            definition(
                "visible_unary",
                1,
                StackAccess::Visible,
                vec![InputPort {
                    name: "video".to_owned(),
                    value_type: ValueType::Video.into(),
                    cardinality: Cardinality::One,
                }],
                vec![ValueType::Video],
                ProgramImplementation::Direct(lower_alias),
            ),
            definition(
                "visible_body",
                1,
                StackAccess::Visible,
                vec![],
                vec![ValueType::Video],
                ProgramImplementation::Body {
                    prepare: prepare_root,
                    contract: crate::program::BodyContract {
                        initial_values: Vec::new(),
                        outputs: crate::program::BodyOutputConstraint::Exactly(vec![
                            ValueType::Video.into(),
                        ]),
                        count_error_code: "E_BODY_OUTPUT_COUNT",
                    },
                },
            ),
        ]
    }

    fn parse_with_registry(
        source: &str,
        definitions: Vec<ProgramDefinition>,
    ) -> (crate::source::SourcePackage, ProgramRegistry) {
        let registry = ProgramRegistry::from_definitions(definitions).expect("registry");
        let workflow =
            crate::language::parse_str_with_registry(Path::new("test.clipasm"), source, &registry)
                .expect("workflow");
        (workflow, registry)
    }

    fn parse_with_synthetic_outputs(
        source: &str,
    ) -> (crate::source::SourcePackage, ProgramRegistry) {
        let mut definitions = crate::program::builtin_programs();
        definitions.push(definition(
            "two_output",
            1,
            StackAccess::Owned,
            vec![],
            vec![ValueType::Video, ValueType::Video],
            ProgramImplementation::Direct(lower_two),
        ));
        definitions.push(definition(
            "same_two_output",
            1,
            StackAccess::Owned,
            vec![],
            vec![ValueType::Video, ValueType::Video],
            ProgramImplementation::Direct(lower_same_two),
        ));
        definitions.push(definition(
            "zero_output",
            1,
            StackAccess::Owned,
            vec![],
            vec![],
            ProgramImplementation::Direct(lower_zero),
        ));
        parse_with_registry(source, definitions)
    }

    #[test]
    fn ids_bind_multiple_outputs_in_stack_order_and_support_forward_references() {
        let (workflow, registry) = parse_with_synthetic_outputs(
            "clipasm 1\nclip {\n  $before\n  $after\n  concat\n} as combined\ntwo_output as (before, after)\nconcat\n",
        );
        let compiled =
            crate::compiler::compile_with_registry(&workflow, &registry).expect("compile");

        let before = compiled.named_values()["before"];
        let after = compiled.named_values()["after"];
        assert!(before.id().get() < after.id().get());
        let entry = compiled
            .explain()
            .iter()
            .find(|entry| entry.construct() == "two_output")
            .expect("two-output explain entry");
        assert_eq!(entry.outputs().len(), 2);
        assert_eq!(entry.outputs()[0].id(), Some("before"));
        assert_eq!(entry.outputs()[1].id(), Some("after"));
    }

    #[test]
    fn multiple_output_bindings_name_distinct_occurrences_even_when_media_is_shared() {
        let (workflow, registry) = parse_with_synthetic_outputs(
            "clipasm 1\nsame_two_output as (left, right)\nconcat as joined\ntrim(value=$joined, range=$joined::right)\n",
        );
        let compiled =
            crate::compiler::compile_with_registry(&workflow, &registry).expect("compile");

        let range = compiled
            .nodes()
            .iter()
            .find_map(|node| match node.kind() {
                crate::semantic::SemanticNodeKind::Slice { range, .. } => Some(*range),
                _ => None,
            })
            .expect("slice created from the right tuple output");
        assert_eq!(range.start(), 1);
        assert_eq!(range.end(), 2);
        assert_eq!(
            compiled.named_values()["left"],
            compiled.named_values()["right"]
        );
    }

    #[test]
    fn multiple_output_bindings_reject_duplicate_names_within_one_tuple() {
        let (workflow, registry) =
            parse_with_synthetic_outputs("clipasm 1\ntwo_output as (same, same)\n");
        let error = crate::compiler::compile_with_registry(&workflow, &registry)
            .expect_err("duplicate tuple output names");
        assert_eq!(error.code, "E_DUPLICATE_NAME");
    }

    #[test]
    fn zero_output_items_leave_the_stack_unchanged() {
        let (workflow, registry) =
            parse_with_synthetic_outputs("clipasm 1\nimage(\"card.png\", 1s)\nzero_output\n");
        let compiled =
            crate::compiler::compile_with_registry(&workflow, &registry).expect("compile");
        let entry = compiled
            .explain()
            .iter()
            .find(|entry| entry.construct() == "zero_output")
            .expect("zero-output explain entry");
        assert!(entry.outputs().is_empty());
    }

    #[test]
    fn unnamed_multiple_outputs_are_appended_and_may_be_consumed() {
        let (workflow, registry) = parse_with_synthetic_outputs("clipasm 1\ntwo_output\nconcat\n");
        let compiled =
            crate::compiler::compile_with_registry(&workflow, &registry).expect("compile");
        assert_eq!(compiled.outputs().len(), 1);
    }

    #[test]
    fn output_bindings_require_the_exact_supported_cardinality() {
        for (source, expected) in [
            (
                "clipasm 1\ntwo_output as pair\n",
                "`as name` requires exactly one output",
            ),
            (
                "clipasm 1\ntwo_output as (first, second, third)\n",
                "3 name(s)",
            ),
            (
                "clipasm 1\nimage(\"card.png\", 1s) as (card, extra)\n",
                "2 name(s)",
            ),
            ("clipasm 1\nzero_output as none\n", "produces 0 value(s)"),
        ] {
            let (workflow, registry) = parse_with_synthetic_outputs(source);
            let error = crate::compiler::compile_with_registry(&workflow, &registry)
                .expect_err("invalid output binding");
            assert_eq!(error.code, "E_OUTPUT_BINDING_COUNT");
            assert!(error.message.contains(expected), "{}", error.message);
        }
    }

    #[test]
    fn direct_and_body_outputs_must_match_their_declarations() {
        for source in [
            "clipasm 1\nwrong_direct\n",
            "clipasm 1\nwrong_body { source }\n",
        ] {
            let (workflow, registry) = parse_with_registry(source, output_programs());
            let error =
                crate::compiler::compile_with_registry(&workflow, &registry).expect_err("type");
            assert_eq!(error.code, "E_PROGRAM_OUTPUT_TYPE");
        }
    }

    #[test]
    fn program_output_count_must_match_its_declaration() {
        let (workflow, registry) =
            parse_with_registry("clipasm 1\nwrong_count\n", output_programs());
        let error =
            crate::compiler::compile_with_registry(&workflow, &registry).expect_err("output count");
        assert_eq!(error.code, "E_PROGRAM_OUTPUT_COUNT");
    }

    #[test]
    fn scoped_builders_propagate_program_semantic_versions() {
        let (workflow, registry) = parse_with_registry(
            "clipasm 1\n@owned { versioned_direct } as unused\n@owned drop\nversioned_body {}\n",
            version_programs(),
        );
        let compiled =
            crate::compiler::compile_with_registry(&workflow, &registry).expect("compile");

        let direct = compiled
            .nodes()
            .iter()
            .find(|node| node.origin().construct == "versioned_direct")
            .expect("direct node");
        assert_eq!(direct.semantic_version(), 11);

        let body_nodes = compiled
            .nodes()
            .iter()
            .filter(|node| node.origin().construct == "versioned_body")
            .collect::<Vec<_>>();
        assert_eq!(body_nodes.len(), 2);
        assert!(body_nodes.iter().all(|node| node.semantic_version() == 17));
    }

    #[test]
    fn descriptor_stack_access_defaults_apply_per_invocation_and_can_be_overridden() {
        let (workflow, registry) = parse_with_registry(
            "clipasm 1\nsource\nvisible_body { visible_unary }\n",
            visible_default_programs(),
        );
        crate::compiler::compile_with_registry(&workflow, &registry)
            .expect("visible descriptor defaults capture the source");

        let (workflow, registry) = parse_with_registry(
            "clipasm 1\nsource\nvisible_body { @owned visible_unary }\n",
            visible_default_programs(),
        );
        let error = crate::compiler::compile_with_registry(&workflow, &registry)
            .expect_err("owned override blocks capture");
        assert_eq!(error.code, "E_STACK_UNDERFLOW");
        assert!(error.message.contains("only 0 owned"));
    }
}
