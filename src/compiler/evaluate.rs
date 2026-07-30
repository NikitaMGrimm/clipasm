use std::collections::BTreeMap;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
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
                            Diagnostic::builtin(
                                if public {
                                    BuiltinDiagnostic::MissingArgument
                                } else {
                                    BuiltinDiagnostic::InternalBinding
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
                    return Err(Diagnostic::builtin(
                        BuiltinDiagnostic::InternalBinding,
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
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::MissingRequiredInput,
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
            Diagnostic::builtin(
                BuiltinDiagnostic::GraphTooLarge,
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
                    Diagnostic::builtin(
                        BuiltinDiagnostic::InternalBinding,
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
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::InternalProgramContract,
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
                Diagnostic::builtin(
                    BuiltinDiagnostic::InternalProgramContract,
                    format!(
                        "body program `{}` has an unresolved generic initial value",
                        definition.descriptor.name
                    ),
                    span.clone(),
                )
            })?;
            if value.value_type() != expected {
                return Err(Diagnostic::builtin(
                    BuiltinDiagnostic::InternalProgramContract,
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
                    Diagnostic::builtin(
                        BuiltinDiagnostic::InternalBinding,
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
                            return Err(Diagnostic::builtin(
                                BuiltinDiagnostic::InternalBinding,
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
                let mut diagnostic = Diagnostic::builtin(
                    BuiltinDiagnostic::TimelineRootMismatch,
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
                        return Err(Diagnostic::builtin(
                            BuiltinDiagnostic::InternalBinding,
                            format!(
                                "body input `{}.{}` has no evaluated value",
                                definition.descriptor.name, port.name
                            ),
                            span.clone(),
                        ));
                    };
                    let [value] = values.as_slice() else {
                        return Err(Diagnostic::builtin(
                            BuiltinDiagnostic::InternalBinding,
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
                        BuiltinDiagnostic::InputBodyOutputCount,
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
                    return Err(Diagnostic::builtin(
                        BuiltinDiagnostic::InternalBinding,
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
                    _ => Err(Diagnostic::builtin(
                        BuiltinDiagnostic::InternalBinding,
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
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::TypeMismatch,
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
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::DuplicateName,
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
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::ProgramOutputCount,
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
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::ProgramOutputType,
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
    diagnostic: BuiltinDiagnostic,
    owner: &str,
    count: usize,
    span: &SourceSpan,
) -> Diagnostic {
    Diagnostic::builtin(
        diagnostic,
        format!("{owner} must leave exactly one value, but {count} values remain"),
        span.clone(),
    )
}

#[cfg(test)]
mod tests;
