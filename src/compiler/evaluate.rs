use std::collections::BTreeMap;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, ValueRef, ValueType, VideoSpec};
use crate::program::{
    BoundParameters, InputPort, ProgramDefinition, ProgramImplementation, ProgramRegistry,
    ResolvedCall,
};
use crate::semantic::{DraftNode, GraphBuilder, SourceOrigin, SymbolId, require_value_type};
use crate::source::SourceSpan;
use crate::source::{
    ArgumentValue, Invocation, Item, ItemKind, OutputBindings, ProgramBody, Reference,
    SourcePackage, SourceProgram, SourceUnitId,
};

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
    registry: ProgramRegistry,
) -> Result<Evaluation> {
    let mut evaluator = Evaluator {
        package,
        video,
        registry,
        nodes: Vec::new(),
        symbols: BTreeMap::new(),
        symbol_order: Vec::new(),
        public_symbols: BTreeMap::new(),
        surface: Vec::new(),
    };
    let outputs = evaluator.evaluate_program(package.root, None, true)?;
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
    registry: ProgramRegistry,
    nodes: Vec<DraftNode>,
    symbols: BTreeMap<SymbolId, Symbol>,
    symbol_order: Vec<SymbolId>,
    public_symbols: BTreeMap<String, SymbolId>,
    surface: Vec<SurfaceRecord>,
}

struct EvalScope {
    unit: SourceUnitId,
    values: BTreeMap<String, SymbolId>,
    parameters: BoundParameters,
    public: bool,
}

impl Evaluator<'_> {
    #[allow(clippy::too_many_lines)]
    fn evaluate_program(
        &mut self,
        unit: SourceUnitId,
        call: Option<&ResolvedCall>,
        public: bool,
    ) -> Result<Vec<ValueRef>> {
        let program = self.package.units()[unit.0].program().clone();
        let mut scope = EvalScope {
            unit,
            values: BTreeMap::new(),
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
                    DeclaredValueType::Known(input.value_type),
                )?;
                self.symbols
                    .get_mut(&key)
                    .expect("new input symbol")
                    .value_type = Some(input.value_type);
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
        for clip in program.clips() {
            self.collect_body_names(&clip.body, &mut scope)?;
        }
        self.collect_body_names(program.body(), &mut scope)?;
        self.link_scope_aliases(&scope)?;
        resolve_symbol_types(&mut self.symbols, &self.symbol_order)?;

        let mut clips = program.clips().iter().collect::<Vec<_>>();
        clips.sort_by(|left, right| left.name.cmp(&right.name));
        for clip in clips {
            let (mut stack, mut frame) =
                EvaluationStack::isolated(format!("named clip `{}`", clip.name), clip.span.clone());
            self.evaluate_body(&clip.body, &mut scope, &mut stack, &mut frame, None)?;
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

        let (mut stack, mut parent) =
            EvaluationStack::isolated("authored program", program.span().clone());
        let mut body_frame = stack.enter_body(
            &parent,
            program.stack_access(),
            "source program",
            program.span().clone(),
        );
        self.evaluate_body(
            program.body(),
            &mut scope,
            &mut stack,
            &mut body_frame,
            None,
        )?;
        Ok(stack.finish_body(&mut parent, body_frame))
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

    fn collect_body_names(&mut self, body: &ProgramBody, scope: &mut EvalScope) -> Result<()> {
        for item in &body.items {
            self.collect_item_names(item, scope)?;
        }
        Ok(())
    }

    fn collect_item_names(&mut self, item: &Item, scope: &mut EvalScope) -> Result<()> {
        match &item.output_bindings {
            OutputBindings::None => {}
            OutputBindings::One(id) => {
                let output_types = self.item_output_types(&item.kind, scope)?;
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
                let output_types = self.item_output_types(&item.kind, scope)?;
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
        if let ItemKind::Invocation(invocation) = &item.kind {
            if let Some(body) = &invocation.body {
                self.collect_body_names(body, scope)?;
            }
            for argument in invocation.arguments.values() {
                if let ArgumentValue::Body(body) = argument {
                    self.collect_body_names(body, scope)?;
                }
            }
        }
        Ok(())
    }

    fn item_output_types(
        &self,
        kind: &ItemKind,
        scope: &EvalScope,
    ) -> Result<Vec<DeclaredValueType>> {
        match kind {
            ItemKind::Reference(reference) => Ok(vec![DeclaredValueType::AliasName(
                reference.name.value.clone(),
            )]),
            ItemKind::Invocation(invocation) => self
                .registry
                .get_for(scope.unit, &invocation.program.value)
                .map(|definition| {
                    definition
                        .descriptor
                        .outputs
                        .iter()
                        .copied()
                        .map(DeclaredValueType::Known)
                        .collect()
                })
                .ok_or_else(|| unknown_program(invocation)),
        }
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
        scope: &mut EvalScope,
        stack: &mut EvaluationStack,
        frame: &mut StackFrame,
        requested_frames: Option<FrameCount>,
    ) -> Result<()> {
        for item in &body.items {
            self.evaluate_item(item, scope, stack, frame, requested_frames)?;
        }
        Ok(())
    }

    fn evaluate_item(
        &mut self,
        item: &Item,
        scope: &mut EvalScope,
        stack: &mut EvaluationStack,
        frame: &mut StackFrame,
        requested_frames: Option<FrameCount>,
    ) -> Result<()> {
        let (outputs, construct) = match &item.kind {
            ItemKind::Reference(reference) => (
                vec![self.evaluate_reference(reference, scope)?],
                "reference".to_owned(),
            ),
            ItemKind::Invocation(invocation) => (
                self.evaluate_invocation(
                    invocation,
                    scope,
                    stack,
                    frame,
                    requested_frames,
                    &item.span,
                )?,
                invocation.program.value.clone(),
            ),
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
        stack.extend(outputs.iter().copied());
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

    #[allow(clippy::too_many_lines)]
    fn evaluate_invocation(
        &mut self,
        invocation: &Invocation,
        scope: &mut EvalScope,
        stack: &mut EvaluationStack,
        frame: &mut StackFrame,
        requested_frames: Option<FrameCount>,
        span: &SourceSpan,
    ) -> Result<Vec<ValueRef>> {
        let definition = self
            .registry
            .get_for(scope.unit, &invocation.program.value)
            .cloned()
            .ok_or_else(|| unknown_program(invocation))?;
        let origin = SourceOrigin::new(invocation.program.value.clone(), span.clone());
        let access = invocation
            .stack_access
            .as_ref()
            .map_or(definition.descriptor.default_stack_access, |access| {
                access.value
            });
        let parameter_bindings = scope.parameters.clone();
        let value_names = scope.values.keys().cloned().collect::<Vec<_>>();
        let call = super::bind::bind_call(
            &definition,
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
        );
        let call = call?;

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
                let body = invocation.body.as_ref().ok_or_else(|| {
                    Diagnostic::new(
                        "E_MISSING_PROGRAM_BODY",
                        format!(
                            "body program `{}` requires a `body`",
                            definition.descriptor.name
                        ),
                        invocation.program.span.clone(),
                    )
                })?;
                let plan = {
                    let mut builder = GraphBuilder::for_program(
                        &mut self.nodes,
                        self.video,
                        definition.descriptor.semantic_version,
                        origin.clone(),
                    );
                    prepare(&call, &mut builder)?
                };
                let mut child = stack.enter_body(
                    frame,
                    access,
                    definition.descriptor.name.clone(),
                    invocation.program.span.clone(),
                );
                stack.extend(plan.initial_values);
                self.evaluate_body(
                    body,
                    scope,
                    stack,
                    &mut child,
                    plan.requested_frames.or(requested_frames),
                )?;
                let owned = stack.finish_body(frame, child);
                let mut builder = GraphBuilder::for_program(
                    &mut self.nodes,
                    self.video,
                    definition.descriptor.semantic_version,
                    origin,
                );
                plan.finalizer.finish(owned, &mut builder)?
            }
            ProgramImplementation::Authored(unit) => {
                if invocation.body.is_some() {
                    return Err(Diagnostic::new(
                        "E_UNEXPECTED_PROGRAM_BODY",
                        format!(
                            "authored program `{}` does not accept a caller-supplied body",
                            invocation.program.value
                        ),
                        invocation.program.span.clone(),
                    ));
                }
                self.evaluate_program(unit, Some(&call), false)?
            }
        };

        validate_program_outputs(&definition, outputs, span)
    }

    fn evaluate_input_value(
        &mut self,
        value: &ArgumentValue,
        port: &InputPort,
        program: &str,
        requested_frames: Option<FrameCount>,
        scope: &mut EvalScope,
    ) -> Result<Vec<ValueRef>> {
        match value {
            ArgumentValue::Reference(reference) => Ok(vec![self.evaluate_reference_name(
                &reference.value,
                &reference.span,
                scope,
            )?]),
            ArgumentValue::References(references, _) => references
                .iter()
                .map(|reference| {
                    self.evaluate_reference_name(&reference.value, &reference.span, scope)
                })
                .collect(),
            ArgumentValue::Body(body) => {
                let (mut local, mut frame) = EvaluationStack::isolated(
                    format!("inline input body for `{program}.{}`", port.name),
                    body.span.clone(),
                );
                self.evaluate_body(body, scope, &mut local, &mut frame, requested_frames)?;
                let [result] = local.values() else {
                    return Err(output_count_error(
                        "E_INPUT_BODY_OUTPUT_COUNT",
                        &format!("inline input body for `{program}.{}`", port.name),
                        local.len(),
                        &body.span,
                    )
                    .note("combine multiple Videos explicitly with `concat` or a nested `glue`"));
                };
                Ok(vec![*result])
            }
            ArgumentValue::Literal(_) => Err(Diagnostic::new(
                "E_INVALID_ARGUMENT_TYPE",
                format!("input `{program}.{}` requires a graph input", port.name),
                value.span().clone(),
            )),
        }
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

fn unknown_program(invocation: &Invocation) -> Diagnostic {
    Diagnostic::new(
        "E_UNKNOWN_PROGRAM",
        format!("unknown program `{}`", invocation.program.value),
        invocation.program.span.clone(),
    )
}

fn validate_program_outputs(
    definition: &ProgramDefinition,
    outputs: Vec<ValueRef>,
    span: &SourceSpan,
) -> Result<Vec<ValueRef>> {
    if outputs.len() != definition.descriptor.outputs.len() {
        return Err(Diagnostic::new(
            "E_PROGRAM_OUTPUT_COUNT",
            format!(
                "program `{}` declares {} output(s), but its implementation returned {}",
                definition.descriptor.name,
                definition.descriptor.outputs.len(),
                outputs.len()
            ),
            span.clone(),
        ));
    }
    for (index, (output, expected)) in outputs
        .iter()
        .zip(&definition.descriptor.outputs)
        .enumerate()
    {
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
        ResolvedCall, StackAccess,
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
                outputs: crate::program::BodyOutputConstraint::Exactly(outputs.clone()),
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
                outputs,
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
            definition(
                "versioned_body",
                17,
                StackAccess::Owned,
                vec![],
                vec![ValueType::Video],
                ProgramImplementation::Body(prepare_versioned_body),
            ),
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
                    value_type: ValueType::Video,
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
