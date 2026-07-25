use std::collections::BTreeMap;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, ValueRef, ValueType, VideoSpec};
use crate::program::{
    BoundParameters, ProgramDefinition, ProgramImplementation, ResolvedCall, ResolvedInputPort,
    ResolvedSignature,
};
use crate::semantic::{DraftNode, GraphBuilder, SourceOrigin, SymbolId, require_value_type};
use crate::source::{SourceSpan, SourceUnitId, Spanned};

use super::EntrypointBindings;
use super::checked::{
    CheckedBody, CheckedInputValue, CheckedItem, CheckedItemKind, CheckedPackage,
    CheckedParameterValue, CheckedProgram, CheckedReferenceTarget,
};

use super::stack::{EvaluationStack, StackFrame};

#[derive(Clone, Debug)]
pub(super) struct Symbol {
    pub(super) name: String,
    pub(super) declared_at: SourceSpan,
    pub(super) value: Option<ValueRef>,
    pub(super) value_type: ValueType,
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
    checked: &CheckedPackage,
    bindings: &EntrypointBindings,
) -> Result<Evaluation> {
    let context = EvaluationContext {
        video,
        registry: &checked.registry,
        programs: &checked.programs,
        root: checked.root,
    };
    let mut evaluator = Evaluator {
        nodes: Vec::new(),
        symbols: Vec::new(),
        public_symbols: BTreeMap::new(),
        surface: Vec::new(),
    };
    let root_call = evaluator.bind_entrypoint_call(&context, bindings)?;
    let outputs = evaluator.evaluate_program(&context, context.root, Some(&root_call), true)?;
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
    registry: &'a crate::program::ProgramRegistry,
    programs: &'a [CheckedProgram],
    root: SourceUnitId,
}

struct Evaluator {
    nodes: Vec<DraftNode>,
    symbols: Vec<Symbol>,
    public_symbols: BTreeMap<String, SymbolId>,
    surface: Vec<SurfaceRecord>,
}

struct EvalScope {
    values: BTreeMap<String, SymbolId>,
    local_symbols: Vec<SymbolId>,
    body_inputs: Vec<Option<ValueRef>>,
    parameters: Vec<Spanned<crate::program::ParameterValue>>,
}

impl Evaluator {
    fn bind_entrypoint_call(
        &mut self,
        context: &EvaluationContext<'_>,
        bindings: &EntrypointBindings,
    ) -> Result<ResolvedCall> {
        super::entrypoint::bind_root_call(
            &context.programs[context.root.0],
            context.registry,
            bindings,
            &mut self.nodes,
            context.video,
        )
    }

    #[allow(clippy::too_many_lines)]
    fn evaluate_program(
        &mut self,
        context: &EvaluationContext<'_>,
        unit: SourceUnitId,
        call: Option<&ResolvedCall>,
        public: bool,
    ) -> Result<Vec<ValueRef>> {
        let checked_program = &context.programs[unit.0];
        let mut scope = EvalScope {
            values: BTreeMap::new(),
            local_symbols: Vec::with_capacity(checked_program.locals.len()),
            body_inputs: vec![None; checked_program.body_input_count],
            parameters: checked_program
                .parameters
                .iter()
                .map(|parameter| {
                    call.and_then(|call| call.parameters().get(&parameter.name).cloned())
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
        };
        for local in &checked_program.locals {
            let symbol = self.add_symbol(&local.name, &local.declared_at, local.value_type)?;
            scope.values.insert(local.name.clone(), symbol);
            scope.local_symbols.push(symbol);
            if public {
                self.public_symbols.insert(local.name.clone(), symbol);
            }
        }

        if let Some(call) = call {
            for input in &checked_program.inputs {
                let values = call.inputs().get(&input.name).ok_or_else(|| {
                    Diagnostic::new(
                        "E_INTERNAL_BINDING",
                        format!("authored program input `{}` was not bound", input.name),
                        checked_program.span.clone(),
                    )
                })?;
                let [value] = values.as_slice() else {
                    return Err(Diagnostic::new(
                        "E_INTERNAL_BINDING",
                        format!(
                            "authored program input `{}` requires exactly one value",
                            input.name
                        ),
                        checked_program.span.clone(),
                    ));
                };
                let key = scope.values[&input.name];
                self.bind_symbol(key, *value)?;
            }
        } else if let Some(input) = checked_program.inputs.first() {
            return Err(Diagnostic::new(
                "E_MISSING_REQUIRED_INPUT",
                format!("root program is missing input `{}`", input.name),
                checked_program.span.clone(),
            ));
        }
        for clip in &checked_program.clips {
            let (mut stack, mut frame) =
                EvaluationStack::isolated(format!("named clip `{}`", clip.name), clip.span.clone());
            self.evaluate_body(
                context, &clip.body, &mut scope, &mut stack, &mut frame, None,
            )?;
            let [output] = stack.values() else {
                return Err(output_count_error(
                    "E_CLIP_OUTPUT_COUNT",
                    &format!("named clip `{}`", clip.name),
                    stack.len(),
                    &clip.span,
                ));
            };
            require_value_type(
                *output,
                ValueType::Video,
                "named clip",
                "output",
                &clip.span,
            )?;
            let key = scope.values[&clip.name];
            self.bind_symbol(key, *output)?;
            self.surface.push(SurfaceRecord {
                construct: "named clip".to_owned(),
                outputs: vec![SurfaceOutput {
                    value: *output,
                    id: Some(clip.name.clone()),
                }],
                span: clip.span.clone(),
            });
        }

        let (mut stack, parent) =
            EvaluationStack::isolated("authored program", checked_program.span.clone());
        let mut body_frame = EvaluationStack::<ValueRef>::enter_body(
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
            value_type,
        });
        Ok(symbol)
    }

    fn evaluate_body(
        &mut self,
        context: &EvaluationContext<'_>,
        checked: &CheckedBody,
        scope: &mut EvalScope,
        stack: &mut EvaluationStack,
        frame: &mut StackFrame,
        requested_frames: Option<FrameCount>,
    ) -> Result<()> {
        for item in &checked.items {
            self.evaluate_item(context, item, scope, stack, frame, requested_frames)?;
        }
        Ok(())
    }

    fn evaluate_item(
        &mut self,
        context: &EvaluationContext<'_>,
        checked: &CheckedItem,
        scope: &mut EvalScope,
        stack: &mut EvaluationStack,
        frame: &mut StackFrame,
        requested_frames: Option<FrameCount>,
    ) -> Result<()> {
        let outputs = match &checked.kind {
            CheckedItemKind::Reference {
                target: Some(target),
            } => vec![self.evaluate_checked_reference(context, *target, &checked.span, scope)?],
            CheckedItemKind::Invocation {
                program,
                signature,
                access,
                stack_plan,
                inputs,
                parameters,
                body,
                body_input_ids,
                ..
            } => self.evaluate_invocation(
                context,
                &checked.construct,
                *program,
                signature,
                *access,
                stack_plan,
                inputs,
                parameters,
                body.as_deref(),
                body_input_ids,
                scope,
                stack,
                frame,
                requested_frames,
                &checked.span,
            )?,
            CheckedItemKind::Reference { target: None } => {
                unreachable!("checked reference target is resolved")
            }
        };
        let output_names = checked.output_names.clone();
        debug_assert_eq!(outputs.len(), output_names.len());
        debug_assert_eq!(outputs.len(), checked.output_bindings.len());
        for (output, binding) in outputs.iter().copied().zip(&checked.output_bindings) {
            if let Some(local) = binding {
                self.bind_symbol(scope.local_symbols[local.index()], output)?;
            }
        }
        stack.extend(frame, outputs.iter().copied());
        self.surface.push(SurfaceRecord {
            construct: checked.construct.clone(),
            outputs: outputs
                .into_iter()
                .zip(output_names)
                .map(|(value, id)| SurfaceOutput { value, id })
                .collect(),
            span: checked.span.clone(),
        });
        Ok(())
    }

    fn evaluate_checked_reference(
        &mut self,
        context: &EvaluationContext<'_>,
        target: CheckedReferenceTarget,
        span: &SourceSpan,
        scope: &EvalScope,
    ) -> Result<ValueRef> {
        match target {
            CheckedReferenceTarget::Local(local) => {
                let symbol = scope.local_symbols[local.index()];
                let value_type = self.symbols[symbol.index()].value_type;
                let origin = SourceOrigin::new("reference", span.clone());
                GraphBuilder::for_program(&mut self.nodes, context.video, 1, origin)
                    .reference(symbol, value_type)
            }
            CheckedReferenceTarget::BodyInput(input) => scope.body_inputs[input.index()]
                .ok_or_else(|| {
                    Diagnostic::new(
                        "E_INTERNAL_BINDING",
                        "lexical body input was not bound during evaluation",
                        span.clone(),
                    )
                }),
        }
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn evaluate_invocation(
        &mut self,
        context: &EvaluationContext<'_>,
        construct: &str,
        program: crate::program::ProgramId,
        signature: &ResolvedSignature,
        access: crate::program::StackAccess,
        stack_plan: &super::stack::StackBindingPlan,
        checked_inputs: &[Option<CheckedInputValue>],
        checked_parameters: &[Option<CheckedParameterValue>],
        checked_body: Option<&CheckedBody>,
        body_input_ids: &BTreeMap<String, super::check::BodyInputId>,
        scope: &mut EvalScope,
        stack: &mut EvaluationStack,
        frame: &mut StackFrame,
        requested_frames: Option<FrameCount>,
        span: &SourceSpan,
    ) -> Result<Vec<ValueRef>> {
        let definition = context.registry.definition(program);
        let origin = SourceOrigin::new(construct, span.clone());
        debug_assert_eq!(signature.inputs.len(), checked_inputs.len());
        let mut slots = vec![None; signature.inputs.len()];
        for (index, (port, input)) in signature.inputs.iter().zip(checked_inputs).enumerate() {
            if let Some(input) = input {
                slots[index] = Some(self.evaluate_checked_input(
                    context,
                    input,
                    port,
                    construct,
                    requested_frames,
                    scope,
                )?);
            }
        }
        for bound in stack.apply_binding_plan(stack_plan) {
            debug_assert!(slots[bound.port].is_none());
            slots[bound.port] = Some(bound.values);
        }
        let inputs = signature
            .inputs
            .iter()
            .zip(slots)
            .map(|(port, values)| {
                values
                    .map(|values| (port.name.clone(), values))
                    .ok_or_else(|| {
                        Diagnostic::new(
                            "E_INTERNAL_BINDING",
                            format!(
                                "checked call to `{construct}` has no binding for input `{}`",
                                port.name
                            ),
                            span.clone(),
                        )
                    })
            })
            .collect::<Result<BTreeMap<_, _>>>()?;

        debug_assert_eq!(
            definition.descriptor.parameters.len(),
            checked_parameters.len()
        );
        let parameters = definition
            .descriptor
            .parameters
            .iter()
            .zip(checked_parameters)
            .filter_map(|(descriptor, binding)| {
                binding.as_ref().map(|binding| {
                    let value = match binding {
                        CheckedParameterValue::Literal(value) => value.clone(),
                        CheckedParameterValue::Reference(parameter) => {
                            scope.parameters[parameter.index()].clone()
                        }
                    };
                    (descriptor.name.clone(), value)
                })
            })
            .collect::<BoundParameters>();
        let call = ResolvedCall::new(
            definition.descriptor.name.clone(),
            inputs,
            parameters,
            requested_frames,
            origin.clone(),
        );

        let outputs = match &definition.implementation {
            ProgramImplementation::Direct(lower) => {
                let mut builder = GraphBuilder::for_program(
                    &mut self.nodes,
                    context.video,
                    definition.descriptor.semantic_version,
                    origin,
                );
                lower(&call, &mut builder)?
            }
            ProgramImplementation::Body { prepare, .. } => {
                let checked_body =
                    checked_body.expect("checked body program has checked body metadata");
                let plan = {
                    let mut builder = GraphBuilder::for_program(
                        &mut self.nodes,
                        context.video,
                        definition.descriptor.semantic_version,
                        origin.clone(),
                    );
                    prepare(&call, &mut builder)?
                };
                let mut child = EvaluationStack::<ValueRef>::enter_body(
                    frame,
                    access,
                    definition.descriptor.name.clone(),
                    span.clone(),
                );
                stack.extend(&child, plan.initial_values);
                let mut bound_body_inputs = Vec::with_capacity(body_input_ids.len());
                for port in signature
                    .inputs
                    .iter()
                    .filter(|port| matches!(port.cardinality, crate::program::Cardinality::One))
                {
                    let id = body_input_ids[&port.name];
                    let previous =
                        scope.body_inputs[id.index()].replace(call.one_input(&port.name)?);
                    debug_assert!(previous.is_none());
                    bound_body_inputs.push(id);
                }
                self.evaluate_body(
                    context,
                    checked_body,
                    scope,
                    stack,
                    &mut child,
                    plan.requested_frames.or(requested_frames),
                )?;
                for id in bound_body_inputs {
                    scope.body_inputs[id.index()] = None;
                }
                let owned = stack.finish_body(&child);
                let mut builder = GraphBuilder::for_program(
                    &mut self.nodes,
                    context.video,
                    definition.descriptor.semantic_version,
                    origin,
                );
                plan.finalizer.finish(owned, &mut builder)?
            }
            ProgramImplementation::Authored(unit) => {
                self.evaluate_program(context, *unit, Some(&call), false)?
            }
            ProgramImplementation::External(external) => {
                let invocation = external.invocation(&call)?;
                let mut builder = GraphBuilder::for_program(
                    &mut self.nodes,
                    context.video,
                    definition.descriptor.semantic_version,
                    origin,
                );
                vec![builder.external_video(invocation)?]
            }
        };

        validate_program_outputs(definition, &signature.outputs, outputs, span)
    }

    fn evaluate_checked_input(
        &mut self,
        context: &EvaluationContext<'_>,
        input: &CheckedInputValue,
        port: &ResolvedInputPort,
        program: &str,
        requested_frames: Option<FrameCount>,
        scope: &mut EvalScope,
    ) -> Result<Vec<ValueRef>> {
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
                    requested_frames,
                )?;
                let [result] = local.values() else {
                    return Err(output_count_error(
                        "E_INPUT_BODY_OUTPUT_COUNT",
                        &format!("inline input body for `{program}.{}`", port.name),
                        local.len(),
                        span,
                    )
                    .note("combine multiple Videos explicitly with `concat` or a nested `glue`"));
                };
                (vec![*result], span)
            }
        };
        values
            .into_iter()
            .map(|value_ref| {
                if value_ref.value_type() == port.value_type {
                    return Ok(value_ref);
                }
                if !port.allow_adaptation {
                    return Err(Diagnostic::new(
                        "E_INTERNAL_BINDING",
                        format!(
                            "checked `{program}.{}` input expected {}, but evaluated to {}",
                            port.name,
                            port.value_type,
                            value_ref.value_type()
                        ),
                        span.clone(),
                    ));
                }
                let origin = SourceOrigin::new("input adaptation", span.clone());
                let mut builder =
                    GraphBuilder::for_program(&mut self.nodes, context.video, 1, origin);
                match (value_ref.value_type(), port.value_type) {
                    (ValueType::Video, ValueType::Audio) => builder.extract_audio(value_ref),
                    (ValueType::Audio, ValueType::Video) => builder.audio_on_black(value_ref),
                    _ => Err(Diagnostic::new(
                        "E_INTERNAL_BINDING",
                        format!(
                            "checked `{program}.{}` adaptation cannot convert {} to {}",
                            port.name,
                            value_ref.value_type(),
                            port.value_type
                        ),
                        span.clone(),
                    )),
                }
            })
            .collect()
    }

    fn bind_symbol(&mut self, id: SymbolId, value: ValueRef) -> Result<()> {
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
        if symbol.value.replace(value).is_some() {
            return Err(Diagnostic::new(
                "E_DUPLICATE_NAME",
                format!("name `{}` was bound more than once", symbol.name),
                symbol.declared_at.clone(),
            ));
        }
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
    use crate::frontend::yaml::Language;
    use crate::model::{FrameCount, ImageFit};
    use crate::program::{
        BodyFinalizer, BodyPlan, Cardinality, InputPort, ProgramDefinition, ProgramDescriptor,
        ProgramRegistry, ResolvedCall, StackAccess,
    };

    #[allow(clippy::unnecessary_wraps)]
    fn prepare_root(call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
        Ok(BodyPlan {
            initial_values: Vec::new(),
            requested_frames: call.requested_frames(),
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
            requested_frames: call.requested_frames(),
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
            requested_frames: call.requested_frames(),
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
                type_selector: None,
                outputs: outputs.into_iter().map(Into::into).collect(),
            },
            implementation,
        }
    }

    fn output_programs() -> Vec<ProgramDefinition> {
        vec![
            definition(
                "glue",
                1,
                StackAccess::Owned,
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
                "glue",
                1,
                StackAccess::Owned,
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
            definition(
                "versioned_direct",
                11,
                StackAccess::Owned,
                vec![],
                vec![ValueType::Video],
                ProgramImplementation::Direct(lower_source),
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
        let language = Language::new(registry.clone()).expect("language");
        let workflow = crate::frontend::yaml::parse_str_with_language(
            Path::new("test.yaml"),
            source,
            &language,
        )
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
            "- program:\n    version: 1\n    clips:\n      combined:\n        - $before\n        - $after\n        - concat\n\n- two_output:\n  ids: [before, after]\n- concat\n",
        );
        let compiled =
            crate::compiler::compile_with_registry(&workflow, registry).expect("compile");

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
    fn zero_output_items_leave_the_stack_unchanged() {
        let (workflow, registry) = parse_with_synthetic_outputs(
            "- program:\n    version: 1\n\n- image: {path: card.png, duration: 1s}\n- zero_output\n",
        );
        let compiled =
            crate::compiler::compile_with_registry(&workflow, registry).expect("compile");
        let entry = compiled
            .explain()
            .iter()
            .find(|entry| entry.construct() == "zero_output")
            .expect("zero-output explain entry");
        assert!(entry.outputs().is_empty());
    }

    #[test]
    fn unnamed_multiple_outputs_are_appended_and_may_be_consumed() {
        let (workflow, registry) =
            parse_with_synthetic_outputs("- program:\n    version: 1\n\n- two_output\n- concat\n");
        let compiled =
            crate::compiler::compile_with_registry(&workflow, registry).expect("compile");
        assert_eq!(compiled.outputs().len(), 1);
    }

    #[test]
    fn output_bindings_require_the_exact_supported_cardinality() {
        for (source, expected) in [
            (
                "- program:\n    version: 1\n\n- two_output:\n  id: pair\n",
                "`id` requires exactly one output",
            ),
            (
                "- program:\n    version: 1\n\n- two_output:\n  ids: [only]\n",
                "`ids` contains 1 name(s)",
            ),
            (
                "- program:\n    version: 1\n\n- image: {path: card.png, duration: 1s}\n  ids: [card]\n",
                "produces 1 value(s)",
            ),
            (
                "- program:\n    version: 1\n\n- zero_output:\n  id: none\n",
                "produces 0 value(s)",
            ),
        ] {
            let (workflow, registry) = parse_with_synthetic_outputs(source);
            let error = crate::compiler::compile_with_registry(&workflow, registry)
                .expect_err("invalid output binding");
            assert_eq!(error.code, "E_OUTPUT_BINDING_COUNT");
            assert!(error.message.contains(expected), "{}", error.message);
        }
    }

    #[test]
    fn direct_and_body_outputs_must_match_their_declarations() {
        for source in [
            "- program:\n    version: 1\n\n- wrong_direct\n",
            "- program:\n    version: 1\n\n- wrong_body: [source]\n",
        ] {
            let (workflow, registry) = parse_with_registry(source, output_programs());
            let error =
                crate::compiler::compile_with_registry(&workflow, registry).expect_err("type");
            assert_eq!(error.code, "E_PROGRAM_OUTPUT_TYPE");
        }
    }

    #[test]
    fn program_output_count_must_match_its_declaration() {
        let (workflow, registry) = parse_with_registry(
            "- program:\n    version: 1\n\n- wrong_count\n",
            output_programs(),
        );
        let error =
            crate::compiler::compile_with_registry(&workflow, registry).expect_err("output count");
        assert_eq!(error.code, "E_PROGRAM_OUTPUT_COUNT");
    }

    #[test]
    fn scoped_builders_propagate_program_semantic_versions() {
        let (workflow, registry) = parse_with_registry(
            "- program:\n    version: 1\n    clips:\n      unused: versioned_direct\n\n- versioned_body: []\n",
            version_programs(),
        );
        let compiled =
            crate::compiler::compile_with_registry(&workflow, registry).expect("compile");

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
            "- program:\n    version: 1\n\n- source\n- visible_body:\n    - visible_unary\n",
            visible_default_programs(),
        );
        crate::compiler::compile_with_registry(&workflow, registry)
            .expect("visible descriptor defaults capture the source");

        let (workflow, registry) = parse_with_registry(
            "- program:\n    version: 1\n\n- source\n- visible_body:\n    - visible_unary:\n        stack_access: owned\n",
            visible_default_programs(),
        );
        let error = crate::compiler::compile_with_registry(&workflow, registry)
            .expect_err("owned override blocks capture");
        assert_eq!(error.code, "E_STACK_UNDERFLOW");
        assert!(error.message.contains("only 0 owned"));
    }
}
