use std::collections::BTreeMap;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, ValueRef, ValueType, VideoSpec};
use crate::program::{InputPort, ProgramDefinition, ProgramImplementation, ProgramRegistry};
use crate::semantic::{DraftNode, GraphBuilder, SourceOrigin, require_value_type};
use crate::source::SourceSpan;
use crate::source::{
    ArgumentValue, Invocation, Item, ItemKind, OutputBindings, ProgramBody, Reference,
    SourceProgram,
};

use super::stack::{EvaluationStack, StackFrame};

#[derive(Clone, Debug)]
pub(super) enum DeclaredValueType {
    Known(ValueType),
    Alias(String),
}

#[derive(Clone, Debug)]
pub(super) struct Symbol {
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
    pub(super) symbols: BTreeMap<String, Symbol>,
    pub(super) symbol_order: Vec<String>,
    pub(super) surface: Vec<SurfaceRecord>,
    pub(super) outputs: Vec<ValueRef>,
}

pub(super) fn evaluate(
    program: &SourceProgram,
    video: &VideoSpec,
    registry: ProgramRegistry,
) -> Result<Evaluation> {
    let mut evaluator = Evaluator {
        program,
        video,
        registry,
        nodes: Vec::new(),
        symbols: BTreeMap::new(),
        symbol_order: Vec::new(),
        surface: Vec::new(),
    };
    evaluator.collect_names()?;
    let outputs = evaluator.evaluate_all()?;
    Ok(Evaluation {
        nodes: evaluator.nodes,
        symbols: evaluator.symbols,
        symbol_order: evaluator.symbol_order,
        surface: evaluator.surface,
        outputs,
    })
}

struct Evaluator<'a> {
    program: &'a SourceProgram,
    video: &'a VideoSpec,
    registry: ProgramRegistry,
    nodes: Vec<DraftNode>,
    symbols: BTreeMap<String, Symbol>,
    symbol_order: Vec<String>,
    surface: Vec<SurfaceRecord>,
}

impl Evaluator<'_> {
    fn collect_names(&mut self) -> Result<()> {
        for clip in self.program.clips() {
            self.add_symbol(
                &clip.name,
                &clip.span,
                DeclaredValueType::Known(ValueType::Video),
            )?;
        }
        for clip in self.program.clips() {
            self.collect_body_names(&clip.body)?;
        }
        self.collect_body_names(self.program.body())?;
        resolve_symbol_types(&mut self.symbols, &self.symbol_order)?;
        self.symbol_order.sort();
        Ok(())
    }

    fn collect_body_names(&mut self, body: &ProgramBody) -> Result<()> {
        for item in &body.items {
            self.collect_item_names(item)?;
        }
        Ok(())
    }

    fn collect_item_names(&mut self, item: &Item) -> Result<()> {
        let output_types = self.item_output_types(&item.kind)?;
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
                self.add_symbol(&id.value, &id.span, output_type.clone())?;
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
                    self.add_symbol(&id.value, &id.span, output_type)?;
                }
            }
        }
        if let ItemKind::Invocation(invocation) = &item.kind {
            if let Some(body) = &invocation.body {
                self.collect_body_names(body)?;
            }
            for argument in invocation.arguments.values() {
                if let ArgumentValue::Body(body) = argument {
                    self.collect_body_names(body)?;
                }
            }
        }
        Ok(())
    }

    fn item_output_types(&self, kind: &ItemKind) -> Result<Vec<DeclaredValueType>> {
        match kind {
            ItemKind::Reference(reference) => {
                Ok(vec![DeclaredValueType::Alias(reference.name.value.clone())])
            }
            ItemKind::Invocation(invocation) => self
                .registry
                .get(&invocation.program.value)
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

    fn add_symbol(
        &mut self,
        name: &str,
        span: &SourceSpan,
        declared_type: DeclaredValueType,
    ) -> Result<()> {
        if let Some(previous) = self.symbols.get(name) {
            return Err(Diagnostic::new(
                "E_DUPLICATE_NAME",
                format!("duplicate user-visible name `{name}`"),
                span.clone(),
            )
            .note(format!(
                "the first `{name}` was declared at {}:{}:{}",
                previous.declared_at.file().display(),
                previous.declared_at.line,
                previous.declared_at.column
            )));
        }
        self.symbol_order.push(name.to_owned());
        self.symbols.insert(
            name.to_owned(),
            Symbol {
                declared_at: span.clone(),
                value: None,
                declared_type,
                value_type: None,
            },
        );
        Ok(())
    }

    fn evaluate_all(&mut self) -> Result<Vec<ValueRef>> {
        let mut clips = self.program.clips().iter().collect::<Vec<_>>();
        clips.sort_by(|left, right| left.name.cmp(&right.name));
        for clip in clips {
            let (mut stack, mut frame) =
                EvaluationStack::isolated(format!("named clip `{}`", clip.name), clip.span.clone());
            self.evaluate_body(&clip.body, &mut stack, &mut frame, None)?;
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
            self.bind_symbol(&clip.name, *output)?;
            self.surface.push(SurfaceRecord {
                construct: "named clip".to_owned(),
                outputs: vec![SurfaceOutput {
                    value: *output,
                    id: Some(clip.name.clone()),
                }],
                span: clip.span.clone(),
            });
        }

        let (mut stack, mut entrypoint) =
            EvaluationStack::isolated("entrypoint", self.program.span().clone());
        let mut source = stack.enter_body(
            &entrypoint,
            self.program.stack_access(),
            "source program",
            self.program.span().clone(),
        );
        self.evaluate_body(self.program.body(), &mut stack, &mut source, None)?;
        Ok(stack.finish_body(&mut entrypoint, source))
    }

    fn evaluate_body(
        &mut self,
        body: &ProgramBody,
        stack: &mut EvaluationStack,
        frame: &mut StackFrame,
        requested_frames: Option<FrameCount>,
    ) -> Result<()> {
        for item in &body.items {
            self.evaluate_item(item, stack, frame, requested_frames)?;
        }
        Ok(())
    }

    fn evaluate_item(
        &mut self,
        item: &Item,
        stack: &mut EvaluationStack,
        frame: &mut StackFrame,
        requested_frames: Option<FrameCount>,
    ) -> Result<()> {
        let (outputs, construct) = match &item.kind {
            ItemKind::Reference(reference) => (
                vec![self.evaluate_reference(reference)?],
                "reference".to_owned(),
            ),
            ItemKind::Invocation(invocation) => (
                self.evaluate_invocation(invocation, stack, frame, requested_frames, &item.span)?,
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
                self.bind_symbol(name, output)?;
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

    fn evaluate_reference(&mut self, reference: &Reference) -> Result<ValueRef> {
        self.evaluate_reference_name(&reference.name.value, &reference.name.span)
    }

    fn evaluate_reference_name(&mut self, name: &str, span: &SourceSpan) -> Result<ValueRef> {
        let symbol = self.symbols.get(name).ok_or_else(|| {
            Diagnostic::new(
                "E_MISSING_REFERENCE",
                format!("reference `${name}` does not name any clip or invocation id"),
                span.clone(),
            )
        })?;
        let value_type = symbol
            .value_type
            .expect("symbol types are resolved before evaluation");
        let origin = SourceOrigin::new("reference", span.clone());
        GraphBuilder::for_program(&mut self.nodes, self.video, 1, origin)
            .reference(name.to_owned(), value_type)
    }

    fn evaluate_invocation(
        &mut self,
        invocation: &Invocation,
        stack: &mut EvaluationStack,
        frame: &mut StackFrame,
        requested_frames: Option<FrameCount>,
        span: &SourceSpan,
    ) -> Result<Vec<ValueRef>> {
        let definition = self
            .registry
            .get(&invocation.program.value)
            .ok_or_else(|| unknown_program(invocation))?;
        let origin = SourceOrigin::new(definition.descriptor.name, span.clone());
        let access = invocation
            .stack_access
            .as_ref()
            .map_or(definition.descriptor.default_stack_access, |access| {
                access.value
            });
        let call = super::bind::bind_call(
            definition,
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
                    definition.descriptor.name,
                    requested_frames,
                )
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
                    definition.descriptor.name,
                    invocation.program.span.clone(),
                );
                stack.extend(plan.initial_values);
                self.evaluate_body(
                    body,
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
        };

        validate_program_outputs(definition, outputs, span)
    }

    fn evaluate_input_value(
        &mut self,
        value: &ArgumentValue,
        port: &InputPort,
        program: &str,
        requested_frames: Option<FrameCount>,
    ) -> Result<Vec<ValueRef>> {
        match value {
            ArgumentValue::Reference(reference) => {
                Ok(vec![self.evaluate_reference_name(
                    &reference.value,
                    &reference.span,
                )?])
            }
            ArgumentValue::References(references, _) => references
                .iter()
                .map(|reference| self.evaluate_reference_name(&reference.value, &reference.span))
                .collect(),
            ArgumentValue::Body(body) => {
                let (mut local, mut frame) = EvaluationStack::isolated(
                    format!("inline input body for `{program}.{}`", port.name),
                    body.span.clone(),
                );
                self.evaluate_body(body, &mut local, &mut frame, requested_frames)?;
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

    fn bind_symbol(&mut self, name: &str, value: ValueRef) -> Result<()> {
        let symbol = self
            .symbols
            .get_mut(name)
            .expect("all symbols are collected before evaluation");
        let declared_type = symbol
            .value_type
            .expect("symbol types are resolved before evaluation");
        if declared_type != value.value_type() {
            return Err(Diagnostic::new(
                "E_TYPE_MISMATCH",
                format!(
                    "name `{name}` was declared as {}, but its value is {}",
                    declared_type,
                    value.value_type()
                ),
                symbol.declared_at.clone(),
            ));
        }
        if symbol.value.replace(value).is_some() {
            return Err(Diagnostic::new(
                "E_DUPLICATE_NAME",
                format!("name `{name}` was bound more than once"),
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
    definition: &'static ProgramDefinition,
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
        .zip(definition.descriptor.outputs)
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
    symbols: &mut BTreeMap<String, Symbol>,
    symbol_order: &[String],
) -> Result<()> {
    for name in symbol_order {
        resolve_symbol_type(name, symbols)?;
    }
    Ok(())
}

fn resolve_symbol_type(name: &str, symbols: &mut BTreeMap<String, Symbol>) -> Result<ValueType> {
    if let Some(value_type) = symbols.get(name).and_then(|symbol| symbol.value_type) {
        return Ok(value_type);
    }

    let mut path = Vec::<String>::new();
    let mut positions = BTreeMap::<String, usize>::new();
    let mut current = name.to_owned();
    let value_type = loop {
        if let Some(value_type) = symbols[&current].value_type {
            break value_type;
        }
        if let Some(start) = positions.get(&current).copied() {
            let mut cycle = path[start..].to_vec();
            cycle.push(current.clone());
            return Err(Diagnostic::new(
                "E_DEPENDENCY_CYCLE",
                format!("named-value dependency cycle: {}", cycle.join(" -> ")),
                symbols[&current].declared_at.clone(),
            ));
        }
        positions.insert(current.clone(), path.len());
        path.push(current.clone());
        match symbols[&current].declared_type.clone() {
            DeclaredValueType::Known(value_type) => break value_type,
            DeclaredValueType::Alias(target) => {
                if !symbols.contains_key(&target) {
                    return Err(Diagnostic::new(
                        "E_MISSING_REFERENCE",
                        format!("reference `${target}` does not name any clip or invocation id"),
                        symbols[&current].declared_at.clone(),
                    ));
                }
                current = target;
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

    const ONE_VIDEO: &[InputPort] = &[InputPort {
        name: "video",
        value_type: ValueType::Video,
        cardinality: Cardinality::One,
    }];

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

    const ROOT: ProgramDefinition = ProgramDefinition {
        descriptor: ProgramDescriptor {
            name: "glue",
            semantic_version: 1,
            default_stack_access: StackAccess::Owned,
            inputs: &[],
            parameters: &[],
            primary_parameter: None,
            outputs: &[ValueType::Video],
        },
        implementation: ProgramImplementation::Body(prepare_root),
        postfix: None,
    };
    const SOURCE: ProgramDefinition = ProgramDefinition {
        descriptor: ProgramDescriptor {
            name: "source",
            semantic_version: 3,
            default_stack_access: StackAccess::Owned,
            inputs: &[],
            parameters: &[],
            primary_parameter: None,
            outputs: &[ValueType::Video],
        },
        implementation: ProgramImplementation::Direct(lower_source),
        postfix: None,
    };
    const WRONG_DIRECT: ProgramDefinition = ProgramDefinition {
        descriptor: ProgramDescriptor {
            name: "wrong_direct",
            semantic_version: 5,
            default_stack_access: StackAccess::Owned,
            inputs: &[],
            parameters: &[],
            primary_parameter: None,
            outputs: &[ValueType::Video],
        },
        implementation: ProgramImplementation::Direct(lower_wrong_type),
        postfix: None,
    };
    const WRONG_BODY: ProgramDefinition = ProgramDefinition {
        descriptor: ProgramDescriptor {
            name: "wrong_body",
            semantic_version: 7,
            default_stack_access: StackAccess::Owned,
            inputs: &[],
            parameters: &[],
            primary_parameter: None,
            outputs: &[ValueType::Video],
        },
        implementation: ProgramImplementation::Body(prepare_wrong_body),
        postfix: None,
    };
    const WRONG_COUNT: ProgramDefinition = ProgramDefinition {
        descriptor: ProgramDescriptor {
            name: "wrong_count",
            semantic_version: 1,
            default_stack_access: StackAccess::Owned,
            inputs: &[],
            parameters: &[],
            primary_parameter: None,
            outputs: &[ValueType::Video, ValueType::Video],
        },
        implementation: ProgramImplementation::Direct(lower_source),
        postfix: None,
    };
    const VERSIONED_DIRECT: ProgramDefinition = ProgramDefinition {
        descriptor: ProgramDescriptor {
            name: "versioned_direct",
            semantic_version: 11,
            default_stack_access: StackAccess::Owned,
            inputs: &[],
            parameters: &[],
            primary_parameter: None,
            outputs: &[ValueType::Video],
        },
        implementation: ProgramImplementation::Direct(lower_source),
        postfix: None,
    };
    const VERSIONED_BODY: ProgramDefinition = ProgramDefinition {
        descriptor: ProgramDescriptor {
            name: "versioned_body",
            semantic_version: 17,
            default_stack_access: StackAccess::Owned,
            inputs: &[],
            parameters: &[],
            primary_parameter: None,
            outputs: &[ValueType::Video],
        },
        implementation: ProgramImplementation::Body(prepare_versioned_body),
        postfix: None,
    };
    const VISIBLE_UNARY: ProgramDefinition = ProgramDefinition {
        descriptor: ProgramDescriptor {
            name: "visible_unary",
            semantic_version: 1,
            default_stack_access: StackAccess::Visible,
            inputs: ONE_VIDEO,
            parameters: &[],
            primary_parameter: None,
            outputs: &[ValueType::Video],
        },
        implementation: ProgramImplementation::Direct(lower_alias),
        postfix: None,
    };
    const VISIBLE_BODY: ProgramDefinition = ProgramDefinition {
        descriptor: ProgramDescriptor {
            name: "visible_body",
            semantic_version: 1,
            default_stack_access: StackAccess::Visible,
            inputs: &[],
            parameters: &[],
            primary_parameter: None,
            outputs: &[ValueType::Video],
        },
        implementation: ProgramImplementation::Body(prepare_root),
        postfix: None,
    };
    const TWO_OUTPUT: ProgramDefinition = ProgramDefinition {
        descriptor: ProgramDescriptor {
            name: "two_output",
            semantic_version: 1,
            default_stack_access: StackAccess::Owned,
            inputs: &[],
            parameters: &[],
            primary_parameter: None,
            outputs: &[ValueType::Video, ValueType::Video],
        },
        implementation: ProgramImplementation::Direct(lower_two),
        postfix: None,
    };
    const ZERO_OUTPUT: ProgramDefinition = ProgramDefinition {
        descriptor: ProgramDescriptor {
            name: "zero_output",
            semantic_version: 1,
            default_stack_access: StackAccess::Owned,
            inputs: &[],
            parameters: &[],
            primary_parameter: None,
            outputs: &[],
        },
        implementation: ProgramImplementation::Direct(lower_zero),
        postfix: None,
    };

    static OUTPUT_PROGRAMS: &[ProgramDefinition] =
        &[ROOT, SOURCE, WRONG_DIRECT, WRONG_BODY, WRONG_COUNT];
    static VERSION_PROGRAMS: &[ProgramDefinition] = &[ROOT, VERSIONED_DIRECT, VERSIONED_BODY];
    static VISIBLE_DEFAULT_PROGRAMS: &[ProgramDefinition] = &[SOURCE, VISIBLE_UNARY, VISIBLE_BODY];

    fn parse_with_registry(
        source: &str,
        definitions: &'static [ProgramDefinition],
    ) -> (crate::source::SourceEntryPoint, ProgramRegistry) {
        let registry = ProgramRegistry::from_definitions(definitions).expect("registry");
        let language = Language::new(registry).expect("language");
        let workflow = crate::frontend::yaml::parse_str_with_language(
            Path::new("test.yaml"),
            source,
            language,
        )
        .expect("workflow");
        (workflow, registry)
    }

    fn parse_with_synthetic_outputs(
        source: &str,
    ) -> (crate::source::SourceEntryPoint, ProgramRegistry) {
        let definitions = Box::leak(
            crate::program::BUILTIN_PROGRAMS
                .iter()
                .copied()
                .chain([TWO_OUTPUT, ZERO_OUTPUT])
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
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
            let (workflow, registry) = parse_with_registry(source, OUTPUT_PROGRAMS);
            let error =
                crate::compiler::compile_with_registry(&workflow, registry).expect_err("type");
            assert_eq!(error.code, "E_PROGRAM_OUTPUT_TYPE");
        }
    }

    #[test]
    fn program_output_count_must_match_its_declaration() {
        let (workflow, registry) = parse_with_registry(
            "- program:\n    version: 1\n\n- wrong_count\n",
            OUTPUT_PROGRAMS,
        );
        let error =
            crate::compiler::compile_with_registry(&workflow, registry).expect_err("output count");
        assert_eq!(error.code, "E_PROGRAM_OUTPUT_COUNT");
    }

    #[test]
    fn scoped_builders_propagate_program_semantic_versions() {
        let (workflow, registry) = parse_with_registry(
            "- program:\n    version: 1\n    clips:\n      unused: versioned_direct\n\n- versioned_body: []\n",
            VERSION_PROGRAMS,
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
            VISIBLE_DEFAULT_PROGRAMS,
        );
        crate::compiler::compile_with_registry(&workflow, registry)
            .expect("visible descriptor defaults capture the source");

        let (workflow, registry) = parse_with_registry(
            "- program:\n    version: 1\n\n- source\n- visible_body:\n    - visible_unary:\n        stack_access: owned\n",
            VISIBLE_DEFAULT_PROGRAMS,
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
            let name = format!("alias_{index:05}");
            let declared_type = if index + 1 == ALIASES {
                DeclaredValueType::Known(ValueType::Video)
            } else {
                DeclaredValueType::Alias(format!("alias_{:05}", index + 1))
            };
            order.push(name.clone());
            symbols.insert(
                name,
                Symbol {
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
