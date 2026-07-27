use std::collections::BTreeMap;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{AudioSpec, TimelineViewId, ValueRef, ValueType, VideoSpec};
use crate::program::{
    Cardinality, InputPort, ParameterSlot, ProgramDefinition, ProgramImplementation,
    RequestedVideoExtent, ResolvedCall, ResolvedInput, ValueTypeSpec,
};
use crate::semantic::{DraftNode, GraphBuilder, SourceOrigin, SymbolId};
use crate::source::{SourceSpan, SourceUnitId, Spanned, SurfaceVisibility};

mod timeline;

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
        timeline: timeline::TimelineState::new(video.fps(), audio.sample_rate()),
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
    timeline: timeline::TimelineState,
}

struct EvalScope {
    local_symbols: Vec<SymbolId>,
    body_inputs: Vec<Option<EvaluatedValue>>,
    parameters: Vec<Spanned<crate::program::ParameterValue>>,
    scalar_aliases: Vec<CheckedScalarExpression>,
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

impl Evaluator {
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
            scalar_aliases: checked_program
                .scalar_aliases
                .iter()
                .map(|alias| alias.expression.clone())
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

    fn resolved_generic_type(
        definition: &crate::program::ProgramDefinition,
        signature: &crate::program::ResolvedSignature,
    ) -> Option<ValueType> {
        definition
            .descriptor
            .inputs
            .iter()
            .zip(&signature.inputs)
            .find_map(|(port, value_type)| {
                matches!(port.value_type, crate::program::ValueTypeSpec::Generic)
                    .then_some(*value_type)
            })
            .or_else(|| {
                definition
                    .descriptor
                    .outputs
                    .iter()
                    .zip(&signature.outputs)
                    .find_map(|(spec, value_type)| {
                        matches!(spec, crate::program::ValueTypeSpec::Generic)
                            .then_some(*value_type)
                    })
            })
    }

    fn validate_body_initial_values(
        definition: &crate::program::ProgramDefinition,
        signature: &crate::program::ResolvedSignature,
        contract: &crate::program::BodyContract,
        values: &[ValueRef],
        span: &SourceSpan,
    ) -> Result<()> {
        if values.len() != contract.initial_values.len() {
            return Err(Diagnostic::new(
                "E_INTERNAL_PROGRAM_CONTRACT",
                format!(
                    "body program `{}` prepared {} initial value(s), but its contract declares {}",
                    definition.descriptor.name,
                    values.len(),
                    contract.initial_values.len()
                ),
                span.clone(),
            ));
        }
        let generic = Self::resolved_generic_type(definition, signature);
        for (index, (value, expected)) in values.iter().zip(&contract.initial_values).enumerate() {
            let expected = expected.exact().or(generic).ok_or_else(|| {
                Diagnostic::new(
                    "E_INTERNAL_PROGRAM_CONTRACT",
                    format!(
                        "body program `{}` has an unresolved generic initial value",
                        definition.descriptor.name
                    ),
                    span.clone(),
                )
            })?;
            if value.value_type() != expected {
                return Err(Diagnostic::new(
                    "E_INTERNAL_PROGRAM_CONTRACT",
                    format!(
                        "body program `{}` prepared initial value {} as {}, but its contract requires {}",
                        definition.descriptor.name,
                        index + 1,
                        value.value_type(),
                        expected
                    ),
                    span.clone(),
                ));
            }
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_lines,
        reason = "invocation evaluation preserves the ordered input, body, timeline, output, and contract lifecycle in one place"
    )]
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
                            &scope.scalar_aliases,
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
            ProgramImplementation::Body { prepare, contract } => {
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
                Self::validate_body_initial_values(
                    definition,
                    signature,
                    contract,
                    &plan.initial_values,
                    span,
                )?;
                let mut child = EvaluationStack::<EvaluatedValue>::enter_body(
                    frame,
                    invocation.access,
                    definition.descriptor.name.clone(),
                    span.clone(),
                );
                let initial_values = self.evaluate_body_initial_values(
                    definition.timeline_behavior,
                    &plan.initial_values,
                    &slots,
                    span,
                )?;
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
    use crate::model::{FrameCount, ImageFit, NativeRange};
    use crate::program::{
        BodyFinalizer, BodyPlan, Cardinality, InputPort, ProgramDefinition, ProgramDescriptor,
        ProgramRegistry, ResolvedCall, StackAccess,
    };

    #[expect(
        clippy::unnecessary_wraps,
        reason = "test body preparers must match the fallible BodyPrepareFn signature"
    )]
    fn prepare_root(call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
        Ok(BodyPlan {
            initial_values: Vec::new(),
            requested_extent: call.requested_extent().cloned(),
            finalizer: Box::new(RootFinalizer),
        })
    }

    fn prepare_unexpected_initial_value(
        _call: &ResolvedCall,
        builder: &mut GraphBuilder<'_>,
    ) -> Result<BodyPlan> {
        Ok(BodyPlan {
            initial_values: vec![builder.image_video(
                PathBuf::from("unexpected.png"),
                FrameCount(1),
                ImageFit::Cover,
            )?],
            requested_extent: None,
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

    #[expect(
        clippy::unnecessary_wraps,
        reason = "test direct lowerers must match the fallible DirectProgramFn signature"
    )]
    fn lower_zero(_call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<Vec<ValueRef>> {
        Ok(Vec::new())
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "test body preparers must match the fallible BodyPrepareFn signature"
    )]
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
                crate::semantic::SemanticNodeKind::Slice {
                    range: NativeRange::Frames(range),
                    ..
                } => Some(*range),
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
    #[test]
    fn body_prepare_values_must_match_the_declared_contract() {
        let mut programs = vec![
            definition(
                "source",
                1,
                StackAccess::Owned,
                vec![],
                vec![ValueType::Video],
                ProgramImplementation::Direct(lower_source),
            ),
            definition(
                "bad_body_plan",
                1,
                StackAccess::Owned,
                vec![],
                vec![ValueType::Video],
                ProgramImplementation::Body {
                    prepare: prepare_unexpected_initial_value,
                    contract: crate::program::BodyContract {
                        initial_values: vec![],
                        outputs: crate::program::BodyOutputConstraint::Exactly(vec![
                            ValueType::Video.into(),
                        ]),
                        count_error_code: "E_BODY_OUTPUT_COUNT",
                    },
                },
            ),
        ];
        let (workflow, registry) = parse_with_registry(
            "clipasm 1\nbad_body_plan { source }\n",
            std::mem::take(&mut programs),
        );

        let error = crate::compiler::compile_with_registry(&workflow, &registry)
            .expect_err("prepare function must obey the body contract");
        assert_eq!(error.code, "E_INTERNAL_PROGRAM_CONTRACT");
        assert!(error.message.contains("prepared 1 initial value"));
    }
}
