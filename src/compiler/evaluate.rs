use std::collections::BTreeMap;
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, ValueRef, ValueType, VideoSpec};
use crate::program::{
    BoundParameters, ParameterType, ProgramDefinition, ProgramImplementation, ResolvedCall,
    ResolvedInputPort, ResolvedSignature,
};
use crate::semantic::{DraftNode, GraphBuilder, SourceOrigin, SymbolId, require_value_type};
use crate::source::SourceSpan;
use crate::source::{
    ArgumentValue, Invocation, Item, ItemKind, Literal, OutputBindings, ProgramBody, Reference,
    SourcePackage, SourceProgram, SourceUnitId, Spanned,
};

use super::EntrypointBindings;
use super::check::{CheckedBody, CheckedItem, CheckedItemKind, CheckedPackage};

use super::stack::{EvaluationStack, StackFrame};

#[derive(Clone, Debug)]
pub(super) enum DeclaredValueType {
    Known(ValueType),
    Alias(SymbolId),
    AliasName(String),
}

#[derive(Clone, Debug)]
pub(super) struct Symbol {
    pub(super) name: String,
    pub(super) declared_at: SourceSpan,
    pub(super) value: Option<ValueRef>,
    pub(super) declared_type: DeclaredValueType,
    pub(super) value_type: Option<ValueType>,
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
    pub(super) symbols: BTreeMap<SymbolId, Symbol>,
    pub(super) symbol_order: Vec<SymbolId>,
    pub(super) public_symbols: BTreeMap<String, SymbolId>,
    pub(super) surface: Vec<SurfaceRecord>,
    pub(super) outputs: Vec<ValueRef>,
}

pub(super) fn evaluate(
    package: &SourcePackage,
    video: &VideoSpec,
    checked: CheckedPackage,
    bindings: &EntrypointBindings,
) -> Result<Evaluation> {
    let mut evaluator = Evaluator {
        package,
        video,
        checked,
        nodes: Vec::new(),
        symbols: BTreeMap::new(),
        symbol_order: Vec::new(),
        public_symbols: BTreeMap::new(),
        surface: Vec::new(),
    };
    let root_call = evaluator.bind_entrypoint_call(bindings)?;
    let outputs = evaluator.evaluate_program(package.root, Some(&root_call), true)?;
    Ok(Evaluation {
        nodes: evaluator.nodes,
        symbols: evaluator.symbols,
        symbol_order: evaluator.symbol_order,
        public_symbols: evaluator.public_symbols,
        surface: evaluator.surface,
        outputs,
    })
}

struct Evaluator<'a> {
    package: &'a SourcePackage,
    video: &'a VideoSpec,
    checked: CheckedPackage,
    nodes: Vec<DraftNode>,
    symbols: BTreeMap<SymbolId, Symbol>,
    symbol_order: Vec<SymbolId>,
    public_symbols: BTreeMap<String, SymbolId>,
    surface: Vec<SurfaceRecord>,
}

struct EvalScope {
    values: BTreeMap<String, SymbolId>,
    body_values: Vec<BTreeMap<String, ValueRef>>,
    parameters: BoundParameters,
    public: bool,
}

impl Evaluator<'_> {
    #[allow(clippy::too_many_lines)]
    fn bind_entrypoint_call(&mut self, bindings: &EntrypointBindings) -> Result<ResolvedCall> {
        let program = self.package.root().program();
        let Some(definition) = self
            .checked
            .registry
            .source_program(self.package.root)
            .cloned()
        else {
            debug_assert!(bindings.video_inputs.is_empty());
            debug_assert!(bindings.parameters.is_empty());
            return Ok(ResolvedCall::new(
                "root".to_owned(),
                BTreeMap::new(),
                BoundParameters::new(),
                None,
                SourceOrigin::new("root program", program.span().clone()),
            ));
        };
        let mut arguments = BTreeMap::new();
        for (name, binding) in &bindings.video_inputs {
            arguments.insert(
                name.clone(),
                ArgumentValue::Body(entrypoint_video_body(binding)),
            );
        }
        for (name, binding) in &bindings.parameters {
            let parameter_type = program
                .parameters()
                .iter()
                .find(|parameter| parameter.name.value == *name)
                .map(|parameter| &parameter.parameter_type);
            let literal = match parameter_type {
                Some(ParameterType::Integer) => binding.value.parse::<i64>().map_or_else(
                    |_| Literal::String(binding.value.clone(), binding.span.clone()),
                    |value| Literal::Integer(value, binding.span.clone()),
                ),
                _ => Literal::String(binding.value.clone(), binding.span.clone()),
            };
            arguments.insert(name.clone(), ArgumentValue::Literal(literal));
        }
        let invocation = Invocation {
            program: Spanned::new("root".to_owned(), program.span().clone()),
            stack_access: None,
            arguments,
            body: None,
        };
        let (mut stack, mut frame) =
            EvaluationStack::isolated("root program call", program.span().clone());
        let mut scope = EvalScope {
            values: BTreeMap::new(),
            body_values: Vec::new(),
            parameters: BoundParameters::new(),
            public: false,
        };
        let video_program = self
            .checked
            .registry
            .id("video")
            .expect("native video program is registered");
        let video_definition = self.checked.registry.definition(video_program);
        let video_signature = video_definition.descriptor.resolve_signature(None);
        let checked_input = CheckedBody {
            items: vec![CheckedItem {
                output_types: video_signature.outputs.clone(),
                kind: CheckedItemKind::Invocation {
                    program: video_program,
                    signature: video_signature.clone(),
                    access: video_definition.descriptor.default_stack_access,
                    body: None,
                    input_bodies: BTreeMap::new(),
                },
            }],
        };
        let signature = definition.descriptor.resolve_signature(None);
        super::bind::bind_call(
            &definition,
            &signature,
            &invocation,
            super::bind::BindContext {
                stack: &mut stack,
                frame: &mut frame,
                access: definition.descriptor.default_stack_access,
                requested_frames: None,
                origin: SourceOrigin::new("root program", program.span().clone()),
            },
            |value, _port| match value {
                ArgumentValue::Body(body) => {
                    let (mut input_stack, mut input_frame) =
                        EvaluationStack::isolated("entrypoint Video input", body.span.clone());
                    self.evaluate_body(
                        body,
                        &checked_input,
                        &mut scope,
                        &mut input_stack,
                        &mut input_frame,
                        None,
                    )?;
                    Ok(input_stack.values().to_vec())
                }
                _ => unreachable!("entrypoint graph inputs are synthetic bodies"),
            },
            |_reference, _descriptor| {
                unreachable!("entrypoint scalar bindings do not use references")
            },
        )
    }

    #[allow(clippy::too_many_lines)]
    fn evaluate_program(
        &mut self,
        unit: SourceUnitId,
        call: Option<&ResolvedCall>,
        public: bool,
    ) -> Result<Vec<ValueRef>> {
        let checked_program = Arc::clone(&self.checked.programs[unit.0]);
        let program = Arc::clone(&checked_program.source);
        let mut scope = EvalScope {
            values: BTreeMap::new(),
            body_values: Vec::new(),
            parameters: call
                .map(|call| call.parameters().clone())
                .unwrap_or_default(),
            public,
        };

        if let Some(call) = call {
            for input in program.inputs() {
                let values = call.inputs().get(&input.name).ok_or_else(|| {
                    Diagnostic::new(
                        "E_INTERNAL_BINDING",
                        format!("authored program input `{}` was not bound", input.name),
                        program.span().clone(),
                    )
                })?;
                let [value] = values.as_slice() else {
                    return Err(Diagnostic::new(
                        "E_INTERNAL_BINDING",
                        format!(
                            "authored program input `{}` requires exactly one value",
                            input.name
                        ),
                        program.span().clone(),
                    ));
                };
                let key = self.add_scope_symbol(
                    &mut scope,
                    &input.name,
                    program.span(),
                    DeclaredValueType::Known(
                        input
                            .value_type
                            .exact()
                            .expect("authored inputs are concrete"),
                    ),
                )?;
                self.symbols
                    .get_mut(&key)
                    .expect("new input symbol")
                    .value_type = Some(
                    input
                        .value_type
                        .exact()
                        .expect("authored inputs are concrete"),
                );
                self.bind_symbol(key, *value)?;
            }
        } else if let Some(input) = program.inputs().first() {
            return Err(Diagnostic::new(
                "E_MISSING_REQUIRED_INPUT",
                format!("root program is missing input `{}`", input.name),
                program.span().clone(),
            ));
        }
        Self::fill_parameter_defaults(&program, &mut scope, public)?;

        for clip in program.clips() {
            self.add_scope_symbol(
                &mut scope,
                &clip.name,
                &clip.span,
                DeclaredValueType::Known(ValueType::Video),
            )?;
        }
        for (clip, checked) in program.clips().iter().zip(&checked_program.clips) {
            self.collect_body_names(&clip.body, checked, &mut scope)?;
        }
        self.collect_body_names(program.body(), &checked_program.body, &mut scope)?;
        self.link_scope_aliases(&scope)?;
        resolve_symbol_types(&mut self.symbols, &self.symbol_order)?;

        let mut clips = program
            .clips()
            .iter()
            .zip(&checked_program.clips)
            .collect::<Vec<_>>();
        clips.sort_by(|(left, _), (right, _)| left.name.cmp(&right.name));
        for (clip, checked) in clips {
            let (mut stack, mut frame) =
                EvaluationStack::isolated(format!("named clip `{}`", clip.name), clip.span.clone());
            self.evaluate_body(
                &clip.body, checked, &mut scope, &mut stack, &mut frame, None,
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
            EvaluationStack::isolated("authored program", program.span().clone());
        let mut body_frame = EvaluationStack::<ValueRef>::enter_body(
            &parent,
            program.stack_access(),
            "source program",
            program.span().clone(),
        );
        self.evaluate_body(
            program.body(),
            &checked_program.body,
            &mut scope,
            &mut stack,
            &mut body_frame,
            None,
        )?;
        Ok(stack.finish_body(&body_frame))
    }

    fn fill_parameter_defaults(
        program: &SourceProgram,
        scope: &mut EvalScope,
        root: bool,
    ) -> Result<()> {
        for parameter in program.parameters() {
            if scope.parameters.contains_key(&parameter.name.value) {
                continue;
            }
            let Some(default) = &parameter.default else {
                return Err(Diagnostic::new(
                    if root {
                        "E_MISSING_ARGUMENT"
                    } else {
                        "E_INTERNAL_BINDING"
                    },
                    if root {
                        format!(
                            "root program is missing parameter `{}`",
                            parameter.name.value
                        )
                    } else {
                        format!(
                            "authored program parameter `{}` was not bound",
                            parameter.name.value
                        )
                    },
                    parameter.name.span.clone(),
                ));
            };
            let value = super::bind::bind_literal_value(
                "root",
                &parameter.name.value,
                &parameter.parameter_type,
                default,
            )?;
            scope.parameters.insert(
                parameter.name.value.clone(),
                crate::source::Spanned::new(value, default.span().clone()),
            );
        }
        Ok(())
    }

    fn collect_body_names(
        &mut self,
        body: &ProgramBody,
        checked: &CheckedBody,
        scope: &mut EvalScope,
    ) -> Result<()> {
        debug_assert_eq!(body.items.len(), checked.items.len());
        for (item, checked) in body.items.iter().zip(&checked.items) {
            self.collect_item_names(item, checked, scope)?;
        }
        Ok(())
    }

    fn collect_item_names(
        &mut self,
        item: &Item,
        checked: &CheckedItem,
        scope: &mut EvalScope,
    ) -> Result<()> {
        let output_types = match &item.kind {
            ItemKind::Reference(reference) => {
                vec![DeclaredValueType::AliasName(reference.name.value.clone())]
            }
            ItemKind::Invocation(_) => checked
                .output_types
                .iter()
                .copied()
                .map(DeclaredValueType::Known)
                .collect(),
        };
        match &item.output_bindings {
            OutputBindings::None => {}
            OutputBindings::One(id) => {
                let [output_type] = output_types.as_slice() else {
                    return Err(output_binding_count_error(
                        &item.kind,
                        output_types.len(),
                        "`id` requires exactly one output",
                        &id.span,
                    ));
                };
                self.add_scope_symbol(scope, &id.value, &id.span, output_type.clone())?;
            }
            OutputBindings::Many(ids, span) => {
                if output_types.len() <= 1 || ids.len() != output_types.len() {
                    return Err(output_binding_count_error(
                        &item.kind,
                        output_types.len(),
                        &format!("`ids` contains {} name(s)", ids.len()),
                        span,
                    ));
                }
                for (id, output_type) in ids.iter().zip(output_types) {
                    self.add_scope_symbol(scope, &id.value, &id.span, output_type)?;
                }
            }
        }
        if let (
            ItemKind::Invocation(invocation),
            CheckedItemKind::Invocation {
                body, input_bodies, ..
            },
        ) = (&item.kind, &checked.kind)
        {
            if let (Some(source), Some(checked)) = (&invocation.body, body.as_deref()) {
                self.collect_body_names(source, checked, scope)?;
            }
            for (name, argument) in &invocation.arguments {
                if let ArgumentValue::Body(source) = argument {
                    let checked = input_bodies
                        .get(name)
                        .expect("checked input body matches canonical source");
                    self.collect_body_names(source, checked, scope)?;
                }
            }
        }
        Ok(())
    }

    fn add_scope_symbol(
        &mut self,
        scope: &mut EvalScope,
        name: &str,
        span: &SourceSpan,
        declared_type: DeclaredValueType,
    ) -> Result<SymbolId> {
        if scope.parameters.contains_key(name) || scope.values.contains_key(name) {
            return Err(Diagnostic::new(
                "E_DUPLICATE_NAME",
                format!("duplicate local name `{name}`"),
                span.clone(),
            ));
        }
        let symbol = self.add_symbol(name, span, declared_type)?;
        scope.values.insert(name.to_owned(), symbol);
        if scope.public {
            self.public_symbols.insert(name.to_owned(), symbol);
        }
        Ok(symbol)
    }

    fn link_scope_aliases(&mut self, scope: &EvalScope) -> Result<()> {
        for symbol in scope.values.values() {
            let DeclaredValueType::AliasName(target_name) =
                self.symbols[symbol].declared_type.clone()
            else {
                continue;
            };
            let target = scope.values.get(&target_name).copied().ok_or_else(|| {
                Self::reference_lookup_error(scope, &target_name, &self.symbols[symbol].declared_at)
            })?;
            self.symbols
                .get_mut(symbol)
                .expect("scope symbol was collected")
                .declared_type = DeclaredValueType::Alias(target);
        }
        Ok(())
    }

    fn add_symbol(
        &mut self,
        name: &str,
        span: &SourceSpan,
        declared_type: DeclaredValueType,
    ) -> Result<SymbolId> {
        let symbol = SymbolId::new(u32::try_from(self.symbols.len()).map_err(|_| {
            Diagnostic::new(
                "E_GRAPH_TOO_LARGE",
                "too many named values were declared",
                span.clone(),
            )
        })?);
        self.symbol_order.push(symbol);
        self.symbols.insert(
            symbol,
            Symbol {
                name: name.to_owned(),
                declared_at: span.clone(),
                value: None,
                declared_type,
                value_type: None,
            },
        );
        Ok(symbol)
    }

    fn evaluate_body(
        &mut self,
        body: &ProgramBody,
        checked: &CheckedBody,
        scope: &mut EvalScope,
        stack: &mut EvaluationStack,
        frame: &mut StackFrame,
        requested_frames: Option<FrameCount>,
    ) -> Result<()> {
        debug_assert_eq!(body.items.len(), checked.items.len());
        for (item, checked) in body.items.iter().zip(&checked.items) {
            self.evaluate_item(item, checked, scope, stack, frame, requested_frames)?;
        }
        Ok(())
    }

    fn evaluate_item(
        &mut self,
        item: &Item,
        checked: &CheckedItem,
        scope: &mut EvalScope,
        stack: &mut EvaluationStack,
        frame: &mut StackFrame,
        requested_frames: Option<FrameCount>,
    ) -> Result<()> {
        let (outputs, construct) = match (&item.kind, &checked.kind) {
            (ItemKind::Reference(reference), CheckedItemKind::Reference) => (
                vec![self.evaluate_reference(reference, scope)?],
                "reference".to_owned(),
            ),
            (
                ItemKind::Invocation(invocation),
                CheckedItemKind::Invocation {
                    program,
                    signature,
                    access,
                    body,
                    input_bodies,
                },
            ) => (
                self.evaluate_invocation(
                    invocation,
                    *program,
                    signature,
                    *access,
                    body.as_deref(),
                    input_bodies,
                    scope,
                    stack,
                    frame,
                    requested_frames,
                    &item.span,
                )?,
                invocation.program.value.clone(),
            ),
            _ => unreachable!("checked item kind matches canonical source"),
        };
        let output_names = match &item.output_bindings {
            OutputBindings::None => vec![None; outputs.len()],
            OutputBindings::One(id) => vec![Some(id.value.clone())],
            OutputBindings::Many(ids, _) => ids
                .iter()
                .map(|id| Some(id.value.clone()))
                .collect::<Vec<_>>(),
        };
        debug_assert_eq!(outputs.len(), output_names.len());
        for (output, name) in outputs.iter().copied().zip(&output_names) {
            if let Some(name) = name {
                let key = scope
                    .values
                    .get(name)
                    .copied()
                    .expect("output names are collected before evaluation");
                self.bind_symbol(key, output)?;
            }
        }
        stack.extend(frame, outputs.iter().copied());
        self.surface.push(SurfaceRecord {
            construct,
            outputs: outputs
                .into_iter()
                .zip(output_names)
                .map(|(value, id)| SurfaceOutput { value, id })
                .collect(),
            span: item.span.clone(),
        });
        Ok(())
    }

    fn evaluate_reference(&mut self, reference: &Reference, scope: &EvalScope) -> Result<ValueRef> {
        self.evaluate_reference_name(&reference.name.value, &reference.name.span, scope)
    }

    fn evaluate_reference_name(
        &mut self,
        name: &str,
        span: &SourceSpan,
        scope: &EvalScope,
    ) -> Result<ValueRef> {
        if let Some(value) = scope
            .body_values
            .iter()
            .rev()
            .find_map(|values| values.get(name))
        {
            return Ok(*value);
        }
        let key = scope
            .values
            .get(name)
            .ok_or_else(|| Self::reference_lookup_error(scope, name, span))?;
        let symbol = self
            .symbols
            .get(key)
            .expect("scope value points to a collected symbol");
        let value_type = symbol
            .value_type
            .expect("symbol types are resolved before evaluation");
        let origin = SourceOrigin::new("reference", span.clone());
        GraphBuilder::for_program(&mut self.nodes, self.video, 1, origin)
            .reference(*key, value_type)
    }

    fn reference_lookup_error(scope: &EvalScope, name: &str, span: &SourceSpan) -> Diagnostic {
        if scope.parameters.contains_key(name) {
            Diagnostic::new(
                "E_PARAMETER_NOT_VALUE",
                format!("parameter `${name}` is not a graph value"),
                span.clone(),
            )
        } else {
            Diagnostic::new(
                "E_MISSING_REFERENCE",
                format!("reference `${name}` does not name a local input, clip, or id"),
                span.clone(),
            )
        }
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn evaluate_invocation(
        &mut self,
        invocation: &Invocation,
        program: crate::program::ProgramId,
        signature: &ResolvedSignature,
        access: crate::program::StackAccess,
        checked_body: Option<&CheckedBody>,
        input_bodies: &BTreeMap<String, CheckedBody>,
        scope: &mut EvalScope,
        stack: &mut EvaluationStack,
        frame: &mut StackFrame,
        requested_frames: Option<FrameCount>,
        span: &SourceSpan,
    ) -> Result<Vec<ValueRef>> {
        let registry = self.checked.registry.clone();
        let definition = registry.definition(program);
        let origin = SourceOrigin::new(invocation.program.value.clone(), span.clone());
        let parameter_bindings = scope.parameters.clone();
        let value_names = scope.values.keys().cloned().collect::<Vec<_>>();
        let call = super::bind::bind_call(
            definition,
            signature,
            invocation,
            super::bind::BindContext {
                stack,
                frame,
                access,
                requested_frames,
                origin: origin.clone(),
            },
            |expression, port| {
                self.evaluate_input_value(
                    expression,
                    port,
                    &invocation.program.value,
                    input_bodies.get(&port.name),
                    requested_frames,
                    scope,
                )
            },
            |reference, descriptor| {
                let value = parameter_bindings.get(&reference.value).ok_or_else(|| {
                    if value_names.contains(&reference.value) {
                        Diagnostic::new(
                            "E_INVALID_ARGUMENT_TYPE",
                            format!(
                                "graph value `${}` cannot be used as scalar parameter `{}.{}`",
                                reference.value, invocation.program.value, descriptor.name
                            ),
                            reference.span.clone(),
                        )
                    } else {
                        Diagnostic::new(
                            "E_MISSING_REFERENCE",
                            format!("unknown parameter reference `${}`", reference.value),
                            reference.span.clone(),
                        )
                    }
                })?;
                if !super::bind::parameter_value_matches(&descriptor.parameter_type, &value.value) {
                    return Err(Diagnostic::new(
                        "E_INVALID_ARGUMENT_TYPE",
                        format!(
                            "parameter `${}` is not compatible with `{}.{}`",
                            reference.value, invocation.program.value, descriptor.name
                        ),
                        reference.span.clone(),
                    ));
                }
                Ok(value.clone())
            },
        )?;

        let outputs = match definition.implementation {
            ProgramImplementation::Direct(lower) => {
                let mut builder = GraphBuilder::for_program(
                    &mut self.nodes,
                    self.video,
                    definition.descriptor.semantic_version,
                    origin,
                );
                lower(&call, &mut builder)?
            }
            ProgramImplementation::Body(prepare) => {
                let body = invocation
                    .body
                    .as_ref()
                    .expect("checked body program has canonical body");
                let checked_body =
                    checked_body.expect("checked body program has checked body metadata");
                let plan = {
                    let mut builder = GraphBuilder::for_program(
                        &mut self.nodes,
                        self.video,
                        definition.descriptor.semantic_version,
                        origin.clone(),
                    );
                    prepare(&call, &mut builder)?
                };
                let mut child = EvaluationStack::<ValueRef>::enter_body(
                    frame,
                    access,
                    definition.descriptor.name.clone(),
                    invocation.program.span.clone(),
                );
                stack.extend(&child, plan.initial_values);
                let body_values = signature
                    .inputs
                    .iter()
                    .filter(|port| matches!(port.cardinality, crate::program::Cardinality::One))
                    .map(|port| {
                        call.one_input(&port.name)
                            .map(|value| (port.name.clone(), value))
                    })
                    .collect::<Result<BTreeMap<_, _>>>()?;
                scope.body_values.push(body_values);
                self.evaluate_body(
                    body,
                    checked_body,
                    scope,
                    stack,
                    &mut child,
                    plan.requested_frames.or(requested_frames),
                )?;
                scope.body_values.pop().expect("body input scope");
                let owned = stack.finish_body(&child);
                let mut builder = GraphBuilder::for_program(
                    &mut self.nodes,
                    self.video,
                    definition.descriptor.semantic_version,
                    origin,
                );
                plan.finalizer.finish(owned, &mut builder)?
            }
            ProgramImplementation::Authored(unit) => {
                debug_assert!(invocation.body.is_none());
                self.evaluate_program(unit, Some(&call), false)?
            }
        };

        validate_program_outputs(definition, &signature.outputs, outputs, span)
    }

    fn evaluate_input_value(
        &mut self,
        value: &ArgumentValue,
        port: &ResolvedInputPort,
        program: &str,
        checked_body: Option<&CheckedBody>,
        requested_frames: Option<FrameCount>,
        scope: &mut EvalScope,
    ) -> Result<Vec<ValueRef>> {
        let values = match value {
            ArgumentValue::Reference(reference) => {
                vec![self.evaluate_reference_name(&reference.value, &reference.span, scope)?]
            }
            ArgumentValue::References(references, _) => references
                .iter()
                .map(|reference| {
                    self.evaluate_reference_name(&reference.value, &reference.span, scope)
                })
                .collect::<Result<Vec<_>>>()?,
            ArgumentValue::Body(body) => {
                let checked_body =
                    checked_body.expect("checked input body matches canonical source");
                let (mut local, mut frame) = EvaluationStack::isolated(
                    format!("inline input body for `{program}.{}`", port.name),
                    body.span.clone(),
                );
                self.evaluate_body(
                    body,
                    checked_body,
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
                        &body.span,
                    )
                    .note("combine multiple Videos explicitly with `concat` or a nested `glue`"));
                };
                vec![*result]
            }
            ArgumentValue::Literal(_) => {
                return Err(Diagnostic::new(
                    "E_INVALID_ARGUMENT_TYPE",
                    format!("input `{program}.{}` requires a graph input", port.name),
                    value.span().clone(),
                ));
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
                        "E_TYPE_MISMATCH",
                        format!(
                            "program `{program}` port `{}` expected {}, but the explicit value is {}",
                            port.name,
                            port.value_type,
                            value_ref.value_type()
                        ),
                        value.span().clone(),
                    ));
                }
                let origin = SourceOrigin::new("input adaptation", value.span().clone());
                let mut builder = GraphBuilder::for_program(&mut self.nodes, self.video, 1, origin);
                match (value_ref.value_type(), port.value_type) {
                    (ValueType::Video, ValueType::Audio) => builder.extract_audio(value_ref),
                    (ValueType::Audio, ValueType::Video) => builder.audio_on_black(value_ref),
                    _ => Err(Diagnostic::new(
                        "E_TYPE_MISMATCH",
                        format!(
                            "program `{program}` port `{}` expected {}, but the explicit value is {}",
                            port.name,
                            port.value_type,
                            value_ref.value_type()
                        ),
                        value.span().clone(),
                    )),
                }
            })
            .collect()
    }

    fn bind_symbol(&mut self, id: SymbolId, value: ValueRef) -> Result<()> {
        let symbol = self
            .symbols
            .get_mut(&id)
            .expect("all symbols are collected before evaluation");
        let declared_type = symbol
            .value_type
            .expect("symbol types are resolved before evaluation");
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

fn entrypoint_video_body(binding: &super::entrypoint::VideoInputBinding) -> ProgramBody {
    let span = binding.span.clone();
    ProgramBody {
        items: vec![Item {
            kind: ItemKind::Invocation(Invocation {
                program: Spanned::new("video".to_owned(), span.clone()),
                stack_access: None,
                arguments: BTreeMap::from([(
                    "path".to_owned(),
                    ArgumentValue::Literal(Literal::File(binding.path.clone(), span.clone())),
                )]),
                body: None,
            }),
            output_bindings: OutputBindings::None,
            span: span.clone(),
        }],
        span,
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

fn resolve_symbol_types(
    symbols: &mut BTreeMap<SymbolId, Symbol>,
    symbol_order: &[SymbolId],
) -> Result<()> {
    for symbol in symbol_order {
        resolve_symbol_type(*symbol, symbols)?;
    }
    Ok(())
}

fn resolve_symbol_type(
    symbol: SymbolId,
    symbols: &mut BTreeMap<SymbolId, Symbol>,
) -> Result<ValueType> {
    if let Some(value_type) = symbols.get(&symbol).and_then(|symbol| symbol.value_type) {
        return Ok(value_type);
    }

    let mut path = Vec::<SymbolId>::new();
    let mut positions = BTreeMap::<SymbolId, usize>::new();
    let mut current = symbol;
    let value_type = loop {
        if let Some(value_type) = symbols[&current].value_type {
            break value_type;
        }
        if let Some(start) = positions.get(&current).copied() {
            let mut cycle = path[start..]
                .iter()
                .map(|symbol| symbols[symbol].name.clone())
                .collect::<Vec<_>>();
            cycle.push(symbols[&current].name.clone());
            return Err(Diagnostic::new(
                "E_DEPENDENCY_CYCLE",
                format!("named-value dependency cycle: {}", cycle.join(" -> ")),
                symbols[&current].declared_at.clone(),
            ));
        }
        positions.insert(current, path.len());
        path.push(current);
        match symbols[&current].declared_type.clone() {
            DeclaredValueType::Known(value_type) => break value_type,
            DeclaredValueType::Alias(target) => current = target,
            DeclaredValueType::AliasName(_) => {
                unreachable!("scope aliases are linked before type resolution")
            }
        }
    };

    for entry in path {
        symbols
            .get_mut(&entry)
            .expect("alias path contains collected symbols")
            .value_type = Some(value_type);
    }
    Ok(value_type)
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

fn output_binding_count_error(
    kind: &ItemKind,
    output_count: usize,
    binding: &str,
    span: &SourceSpan,
) -> Diagnostic {
    let construct = match kind {
        ItemKind::Reference(_) => "reference",
        ItemKind::Invocation(invocation) => invocation.program.value.as_str(),
    };
    Diagnostic::new(
        "E_OUTPUT_BINDING_COUNT",
        format!("`{construct}` produces {output_count} value(s), but {binding}"),
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
        Ok(vec![builder.test_value()?])
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
            Ok(vec![builder.test_value()?])
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
        let body_contract = matches!(implementation, ProgramImplementation::Body(_)).then(|| {
            crate::program::BodyContract {
                initial_values: Vec::new(),
                outputs: crate::program::BodyOutputConstraint::Exactly(
                    outputs.iter().copied().map(Into::into).collect(),
                ),
                count_error_code: "E_BODY_OUTPUT_COUNT",
            }
        });
        ProgramDefinition {
            descriptor: ProgramDescriptor {
                name: name.to_owned(),
                semantic_version,
                default_stack_access,
                inputs,
                parameters: vec![],
                primary_parameter: None,
                type_parameter: None,
                outputs: outputs.into_iter().map(Into::into).collect(),
            },
            implementation,
            body_contract,
            postfix: None,
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
                ProgramImplementation::Body(prepare_root),
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
                ProgramImplementation::Body(prepare_wrong_body),
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
            ProgramImplementation::Body(prepare_versioned_body),
        );
        versioned_body
            .body_contract
            .as_mut()
            .expect("body contract")
            .initial_values = vec![ValueType::Video.into()];
        vec![
            definition(
                "glue",
                1,
                StackAccess::Owned,
                vec![],
                vec![ValueType::Video],
                ProgramImplementation::Body(prepare_root),
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
                ProgramImplementation::Body(prepare_root),
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

    #[test]
    fn resolves_a_deep_alias_chain_iteratively_with_path_compression() {
        const ALIASES: usize = 20_001;
        let span = SourceSpan::file_start("aliases.yaml");
        let mut symbols = BTreeMap::new();
        let mut order = Vec::with_capacity(ALIASES);
        for index in 0..ALIASES {
            let symbol = SymbolId::new(u32::try_from(index).expect("test symbol ID"));
            let name = format!("alias_{index:05}");
            let declared_type = if index + 1 == ALIASES {
                DeclaredValueType::Known(ValueType::Video)
            } else {
                DeclaredValueType::Alias(SymbolId::new(
                    u32::try_from(index + 1).expect("test target symbol ID"),
                ))
            };
            order.push(symbol);
            symbols.insert(
                symbol,
                Symbol {
                    name,
                    declared_at: span.clone(),
                    value: None,
                    declared_type,
                    value_type: None,
                },
            );
        }

        resolve_symbol_types(&mut symbols, &order).expect("deep aliases resolve");

        assert!(
            symbols
                .values()
                .all(|symbol| symbol.value_type == Some(ValueType::Video))
        );
    }
}
