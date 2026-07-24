use std::collections::BTreeMap;

use crate::diagnostic::{Diagnostic, Result, SourceSpan};
use crate::model::{FrameCount, ValueRef, ValueType, VideoSpec};
use crate::program::{InputPort, ProgramImplementation, ProgramRegistry};
use crate::semantic::{DraftNode, GraphBuilder, SourceOrigin, require_value_type};
use crate::syntax::{
    Argument, InputExpression, Invocation, Item, ItemKind, ProgramBody, Reference, SourceProgram,
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
    pub(super) value: ValueRef,
    pub(super) id: Option<String>,
    pub(super) span: SourceSpan,
}

pub(super) struct Evaluation {
    pub(super) nodes: Vec<DraftNode>,
    pub(super) symbols: BTreeMap<String, Symbol>,
    pub(super) symbol_order: Vec<String>,
    pub(super) surface: Vec<SurfaceRecord>,
    pub(super) result: ValueRef,
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
    let result = evaluator.evaluate_all()?;
    Ok(Evaluation {
        nodes: evaluator.nodes,
        symbols: evaluator.symbols,
        symbol_order: evaluator.symbol_order,
        surface: evaluator.surface,
        result,
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
        if let Some(id) = &item.id {
            self.add_symbol(&id.value, &id.span, self.item_output_type(&item.kind)?)?;
        }
        if let ItemKind::Invocation(invocation) = &item.kind {
            if let Some(body) = &invocation.body {
                self.collect_body_names(body)?;
            }
            for argument in invocation.arguments.values() {
                if let Argument::Input(InputExpression::Body(body)) = argument {
                    self.collect_body_names(body)?;
                }
            }
        }
        Ok(())
    }

    fn item_output_type(&self, kind: &ItemKind) -> Result<DeclaredValueType> {
        match kind {
            ItemKind::Reference(reference) => {
                Ok(DeclaredValueType::Alias(reference.name.value.clone()))
            }
            ItemKind::Invocation(invocation) => self
                .registry
                .get(&invocation.program.value)
                .and_then(|definition| match definition.descriptor.outputs {
                    [output] => Some(DeclaredValueType::Known(*output)),
                    _ => None,
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
                previous.declared_at.file.display(),
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

    fn evaluate_all(&mut self) -> Result<ValueRef> {
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
                value: *output,
                id: Some(clip.name.clone()),
                span: clip.span.clone(),
            });
        }

        let (mut stack, mut entrypoint) =
            EvaluationStack::isolated("entrypoint", self.program.header_span().clone());
        let mut source = stack.enter_body(
            &entrypoint,
            self.program.stack_access(),
            "source program",
            self.program.header_span().clone(),
        );
        self.evaluate_body(self.program.body(), &mut stack, &mut source, None)?;
        let values = stack.finish_body(&mut entrypoint, source);
        let [result] = values.as_slice() else {
            return Err(output_count_error(
                "E_SOURCE_PROGRAM_OUTPUT_COUNT",
                "source program",
                values.len(),
                self.program.header_span(),
            )
            .note("combine multiple Videos explicitly with `concat` or a nested `glue`"));
        };
        require_value_type(
            *result,
            ValueType::Video,
            "source program",
            "result",
            self.program.header_span(),
        )?;
        Ok(*result)
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
        let [output] = outputs.as_slice() else {
            return Err(Diagnostic::new(
                "E_ITEM_OUTPUT_COUNT",
                format!(
                    "items currently require exactly one output, but `{construct}` produced {}",
                    outputs.len()
                ),
                item.span.clone(),
            ));
        };
        stack.push(*output);
        if let Some(id) = &item.id {
            self.bind_symbol(&id.value, *output)?;
        }
        self.surface.push(SurfaceRecord {
            construct,
            value: *output,
            id: item.id.as_ref().map(|id| id.value.clone()),
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
        let origin = SourceOrigin {
            construct: "reference",
            span: span.clone(),
        };
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
        let origin = SourceOrigin {
            construct: definition.descriptor.name,
            span: span.clone(),
        };
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
                self.evaluate_input_expression(
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

    fn evaluate_input_expression(
        &mut self,
        expression: &InputExpression,
        port: &InputPort,
        program: &str,
        requested_frames: Option<FrameCount>,
    ) -> Result<Vec<ValueRef>> {
        match expression {
            InputExpression::Reference(reference) => {
                Ok(vec![self.evaluate_reference_name(
                    &reference.value,
                    &reference.span,
                )?])
            }
            InputExpression::ReferenceList(references, _) => references
                .iter()
                .map(|reference| self.evaluate_reference_name(&reference.value, &reference.span))
                .collect(),
            InputExpression::Body(body) => {
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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::language::Language;
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

    static OUTPUT_PROGRAMS: &[ProgramDefinition] = &[ROOT, SOURCE, WRONG_DIRECT, WRONG_BODY];
    static VERSION_PROGRAMS: &[ProgramDefinition] = &[ROOT, VERSIONED_DIRECT, VERSIONED_BODY];
    static VISIBLE_DEFAULT_PROGRAMS: &[ProgramDefinition] = &[SOURCE, VISIBLE_UNARY, VISIBLE_BODY];

    fn parse_with_registry(
        source: &str,
        definitions: &'static [ProgramDefinition],
    ) -> (SourceProgram, ProgramRegistry) {
        let registry = ProgramRegistry::from_definitions(definitions).expect("registry");
        let language = Language::new(registry).expect("language");
        let workflow =
            crate::syntax::parse_str_with_language(Path::new("test.yaml"), source, language)
                .expect("workflow");
        (workflow, registry)
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
