use std::collections::BTreeMap;

use crate::compiler::{
    DeclaredValueType, Evaluation, GraphBuilder, ResolvedCall, SourceOrigin, SurfaceRecord, Symbol,
    require_value_type,
};
use crate::diagnostic::{Diagnostic, Result, SourceSpan};
use crate::model::{FrameCount, ValueRef, ValueStack, ValueType, VideoSpec};
use crate::program::{Cardinality, InputPort, ProgramDefinition, ProgramRegistry};
use crate::syntax::{Argument, Item, ItemKind, Reference, Workflow};

pub(super) fn evaluate(
    workflow: &Workflow,
    video: &VideoSpec,
    registry: ProgramRegistry,
) -> Result<Evaluation> {
    let mut evaluator = Evaluator {
        workflow,
        video,
        registry,
        nodes: Vec::new(),
        symbols: BTreeMap::new(),
        symbol_order: Vec::new(),
        surface: Vec::new(),
    };
    evaluator.collect_names()?;
    let root = evaluator.evaluate_all()?;
    Ok(Evaluation {
        nodes: evaluator.nodes,
        symbols: evaluator.symbols,
        symbol_order: evaluator.symbol_order,
        surface: evaluator.surface,
        root,
    })
}

struct Evaluator<'a> {
    workflow: &'a Workflow,
    video: &'a VideoSpec,
    registry: ProgramRegistry,
    nodes: Vec<super::DraftNode>,
    symbols: BTreeMap<String, Symbol>,
    symbol_order: Vec<String>,
    surface: Vec<SurfaceRecord>,
}

impl Evaluator<'_> {
    fn collect_names(&mut self) -> Result<()> {
        for clip in self.workflow.clips() {
            self.add_symbol(
                &clip.name,
                &clip.span,
                DeclaredValueType::Known(ValueType::Video),
            )?;
        }
        for clip in self.workflow.clips() {
            self.collect_item_names(&clip.body)?;
        }
        self.collect_item_names(self.workflow.timeline())?;
        resolve_symbol_types(&mut self.symbols, &self.symbol_order)?;
        self.symbol_order.sort();
        Ok(())
    }

    fn collect_item_names(&mut self, items: &[Item]) -> Result<()> {
        for item in items {
            if let Some(id) = &item.id {
                let declared_type = if item.during.is_some() {
                    DeclaredValueType::Known(ValueType::Video)
                } else {
                    self.item_output_type(&item.kind)?
                };
                self.add_symbol(&id.value, &id.span, declared_type)?;
            }
            match &item.kind {
                ItemKind::Then(body) | ItemKind::Join(body) | ItemKind::Timeline(body) => {
                    self.collect_item_names(body)?;
                }
                ItemKind::Reference(_) | ItemKind::Invocation(_) => {}
            }
        }
        Ok(())
    }

    fn item_output_type(&self, kind: &ItemKind) -> Result<DeclaredValueType> {
        match kind {
            ItemKind::Invocation(invocation) => self
                .registry
                .get(&invocation.program.value)
                .map(|definition| DeclaredValueType::Known(definition.descriptor.output))
                .ok_or_else(|| {
                    Diagnostic::new(
                        "E_UNKNOWN_PROGRAM",
                        format!("unknown program `{}`", invocation.program.value),
                        invocation.program.span.clone(),
                    )
                }),
            ItemKind::Reference(reference) => {
                Ok(DeclaredValueType::Alias(reference.name.value.clone()))
            }
            ItemKind::Then(_) | ItemKind::Join(_) | ItemKind::Timeline(_) => {
                Ok(DeclaredValueType::Known(ValueType::Video))
            }
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
        let mut clips = self.workflow.clips().iter().collect::<Vec<_>>();
        clips.sort_by(|left, right| left.name.cmp(&right.name));
        for clip in clips {
            let mut stack = ValueStack::new();
            self.evaluate_body(&clip.body, &mut stack, None)?;
            if stack.len() != 1 {
                return Err(output_count_error(
                    "E_CLIP_OUTPUT_COUNT",
                    &format!("named clip `{}`", clip.name),
                    stack.len(),
                    &clip.span,
                ));
            }
            require_value_type(
                stack[0],
                ValueType::Video,
                "named clip",
                "output",
                &clip.span,
            )?;
            self.bind_symbol(&clip.name, stack[0])?;
            self.surface.push(SurfaceRecord {
                construct: "named clip".to_owned(),
                value: stack[0],
                id: Some(clip.name.clone()),
                span: clip.span.clone(),
            });
        }

        let mut root_stack = ValueStack::new();
        self.evaluate_body(self.workflow.timeline(), &mut root_stack, None)?;
        self.finalize_timeline(
            root_stack,
            SourceSpan::file_start(self.workflow.source_path()),
            "root timeline",
        )
    }

    fn evaluate_body(
        &mut self,
        items: &[Item],
        stack: &mut ValueStack,
        requested_frames: Option<FrameCount>,
    ) -> Result<()> {
        for item in items {
            let output = if let Some(range) = &item.during {
                let base = pop_one(stack, "during", &item.span)?;
                require_value_type(base, ValueType::Video, "during", "base", &item.span)?;
                let frame_range = range.value.to_frames(self.video.fps, &range.span)?;
                let selected = GraphBuilder::new(&mut self.nodes, self.video).slice(
                    base,
                    frame_range,
                    SourceOrigin {
                        construct: "during selection".to_owned(),
                        span: range.span.clone(),
                    },
                )?;
                let mut local = vec![selected];
                let processed = self.evaluate_item_kind(
                    &item.kind,
                    &item.span,
                    &mut local,
                    Some(frame_range.frames()),
                )?;
                local.push(processed);
                if local.len() != 1 {
                    return Err(output_count_error(
                        "E_DURING_OUTPUT_COUNT",
                        "`during` body",
                        local.len(),
                        &item.span,
                    ));
                }
                require_value_type(
                    local[0],
                    ValueType::Video,
                    "during",
                    "processed",
                    &item.span,
                )?;
                GraphBuilder::new(&mut self.nodes, self.video).during(
                    base,
                    local[0],
                    frame_range,
                    SourceOrigin {
                        construct: "during".to_owned(),
                        span: item.span.clone(),
                    },
                )?
            } else {
                self.evaluate_item_kind(&item.kind, &item.span, stack, requested_frames)?
            };

            stack.push(output);
            if let Some(id) = &item.id {
                self.bind_symbol(&id.value, output)?;
            }
            self.surface.push(SurfaceRecord {
                construct: if item.during.is_some() {
                    "during".to_owned()
                } else {
                    item_construct(&item.kind)
                },
                value: output,
                id: item.id.as_ref().map(|id| id.value.clone()),
                span: item.span.clone(),
            });
        }
        Ok(())
    }

    fn evaluate_item_kind(
        &mut self,
        kind: &ItemKind,
        span: &SourceSpan,
        outer_stack: &mut ValueStack,
        requested_frames: Option<FrameCount>,
    ) -> Result<ValueRef> {
        match kind {
            ItemKind::Reference(reference) => self.evaluate_reference(reference),
            ItemKind::Invocation(invocation) => {
                let definition = self
                    .registry
                    .get(&invocation.program.value)
                    .ok_or_else(|| {
                        Diagnostic::new(
                            "E_UNKNOWN_PROGRAM",
                            format!("unknown program `{}`", invocation.program.value),
                            invocation.program.span.clone(),
                        )
                    })?;
                self.evaluate_invocation(
                    definition,
                    &invocation.arguments,
                    outer_stack,
                    requested_frames,
                    span,
                )
            }
            ItemKind::Then(body) => {
                let input = pop_one(outer_stack, "then", span)?;
                require_value_type(input, ValueType::Video, "then", "input", span)?;
                let mut local = vec![input];
                self.evaluate_body(body, &mut local, requested_frames)?;
                if local.len() != 1 {
                    return Err(output_count_error(
                        "E_THEN_OUTPUT_COUNT",
                        "`then` body",
                        local.len(),
                        span,
                    ));
                }
                require_value_type(local[0], ValueType::Video, "then", "output", span)?;
                Ok(local[0])
            }
            ItemKind::Join(body) => {
                if outer_stack.len() < 2 {
                    return Err(stack_underflow("join", 2, outer_stack.len(), span));
                }
                let split = outer_stack.len() - 2;
                let mut local = outer_stack.split_off(split);
                for (port, value) in [("before", local[0]), ("after", local[1])] {
                    require_value_type(value, ValueType::Video, "join", port, span)?;
                }
                self.evaluate_body(body, &mut local, requested_frames)?;
                if local.len() != 1 {
                    return Err(output_count_error(
                        "E_JOIN_OUTPUT_COUNT",
                        "`join` body",
                        local.len(),
                        span,
                    ));
                }
                require_value_type(local[0], ValueType::Video, "join", "output", span)?;
                Ok(local[0])
            }
            ItemKind::Timeline(body) => {
                let mut local = ValueStack::new();
                self.evaluate_body(body, &mut local, requested_frames)?;
                self.finalize_timeline(local, span.clone(), "timeline")
            }
        }
    }

    fn evaluate_reference(&mut self, reference: &Reference) -> Result<ValueRef> {
        let symbol = self.symbols.get(&reference.name.value).ok_or_else(|| {
            Diagnostic::new(
                "E_MISSING_REFERENCE",
                format!(
                    "reference `${}` does not name any clip or invocation id",
                    reference.name.value
                ),
                reference.name.span.clone(),
            )
        })?;
        GraphBuilder::new(&mut self.nodes, self.video).reference(
            reference.name.value.clone(),
            symbol
                .value_type
                .expect("symbol types are resolved before evaluation"),
            SourceOrigin {
                construct: "reference".to_owned(),
                span: reference.name.span.clone(),
            },
        )
    }

    fn evaluate_invocation(
        &mut self,
        definition: &'static ProgramDefinition,
        arguments: &BTreeMap<String, Argument>,
        stack: &mut ValueStack,
        requested_frames: Option<FrameCount>,
        span: &SourceSpan,
    ) -> Result<ValueRef> {
        let inputs = self.bind_inputs(definition, arguments, stack, span)?;
        let call = ResolvedCall {
            definition,
            inputs,
            parameters: arguments,
            requested_frames,
            origin: SourceOrigin {
                construct: definition.descriptor.name.to_owned(),
                span: span.clone(),
            },
        };
        let output =
            (definition.lower)(&call, &mut GraphBuilder::new(&mut self.nodes, self.video))?;
        if output.value_type() != definition.descriptor.output {
            return Err(Diagnostic::new(
                "E_PROGRAM_OUTPUT_TYPE",
                format!(
                    "program `{}` declares output {}, but its lowerer returned {}",
                    definition.descriptor.name,
                    definition.descriptor.output,
                    output.value_type()
                ),
                span.clone(),
            ));
        }
        Ok(output)
    }

    fn bind_inputs(
        &mut self,
        definition: &'static ProgramDefinition,
        arguments: &BTreeMap<String, Argument>,
        stack: &mut ValueStack,
        span: &SourceSpan,
    ) -> Result<BTreeMap<&'static str, Vec<ValueRef>>> {
        let descriptor = &definition.descriptor;
        let mut slots = vec![None; descriptor.inputs.len()];
        for (index, port) in descriptor.inputs.iter().enumerate() {
            if let Some(argument) = arguments.get(port.name) {
                let values = self.resolve_explicit_input(descriptor.name, argument, port)?;
                slots[index] = Some(values);
            }
        }

        bind_missing_fixed(descriptor.name, descriptor.inputs, &mut slots, stack, span)?;

        for (index, port) in descriptor.inputs.iter().enumerate() {
            if slots[index].is_some() {
                continue;
            }
            if let Cardinality::Variadic { min } = port.cardinality {
                if stack.len() < min {
                    let code = if descriptor.name == "concat" {
                        "E_EMPTY_CONCAT"
                    } else {
                        "E_MISSING_REQUIRED_INPUT"
                    };
                    return Err(Diagnostic::new(
                        code,
                        format!(
                            "`{}.{}` needs at least {min} {} value(s), but the local stack has {}",
                            descriptor.name,
                            port.name,
                            port.value_type,
                            stack.len()
                        ),
                        span.clone(),
                    ));
                }
                let values = std::mem::take(stack);
                for value in &values {
                    require_value_type(*value, port.value_type, descriptor.name, port.name, span)?;
                }
                slots[index] = Some(values);
            }
        }

        let mut bound = BTreeMap::new();
        for (port, values) in descriptor.inputs.iter().zip(slots) {
            let values = values.ok_or_else(|| {
                Diagnostic::new(
                    "E_MISSING_REQUIRED_INPUT",
                    format!(
                        "program `{}` is missing input `{}`",
                        descriptor.name, port.name
                    ),
                    span.clone(),
                )
            })?;
            bound.insert(port.name, values);
        }
        Ok(bound)
    }

    fn resolve_explicit_input(
        &mut self,
        program: &str,
        argument: &Argument,
        port: &InputPort,
    ) -> Result<Vec<ValueRef>> {
        let references = match argument {
            Argument::Reference(name, span) => vec![(name, span)],
            Argument::List(values, _) => values
                .iter()
                .map(|value| match value {
                    Argument::Reference(name, span) => Ok((name, span)),
                    _ => Err(Diagnostic::new(
                        "E_INVALID_ARGUMENT_TYPE",
                        "explicit graph inputs must be `$name` references",
                        value.span().clone(),
                    )),
                })
                .collect::<Result<Vec<_>>>()?,
            _ => {
                return Err(Diagnostic::new(
                    "E_INVALID_ARGUMENT_TYPE",
                    "explicit graph inputs must be `$name` references",
                    argument.span().clone(),
                ));
            }
        };
        let mut values = Vec::with_capacity(references.len());
        for (name, reference_span) in references {
            let value = self.evaluate_reference(&Reference {
                name: crate::diagnostic::Spanned::new(name.clone(), reference_span.clone()),
            })?;
            require_value_type(value, port.value_type, program, port.name, reference_span)?;
            values.push(value);
        }
        if let Cardinality::Variadic { min } = port.cardinality
            && values.len() < min
        {
            return Err(Diagnostic::new(
                "E_MISSING_REQUIRED_INPUT",
                format!("input `{}` requires at least {min} values", port.name),
                argument.span().clone(),
            ));
        }
        Ok(values)
    }

    fn finalize_timeline(
        &mut self,
        values: ValueStack,
        span: SourceSpan,
        construct: &str,
    ) -> Result<ValueRef> {
        if values.is_empty() {
            return Err(Diagnostic::new(
                "E_EMPTY_TIMELINE",
                "timeline must produce at least one Video",
                span,
            ));
        }
        for value in &values {
            require_value_type(*value, ValueType::Video, "timeline", "output", &span)?;
        }
        GraphBuilder::new(&mut self.nodes, self.video).concat(
            values,
            1,
            SourceOrigin {
                construct: construct.to_owned(),
                span,
            },
        )
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

fn resolve_symbol_types(
    symbols: &mut BTreeMap<String, Symbol>,
    symbol_order: &[String],
) -> Result<()> {
    let mut states = BTreeMap::<String, u8>::new();
    let mut path = Vec::new();
    for name in symbol_order {
        resolve_symbol_type(name, symbols, &mut states, &mut path)?;
    }
    Ok(())
}

fn resolve_symbol_type(
    name: &str,
    symbols: &mut BTreeMap<String, Symbol>,
    states: &mut BTreeMap<String, u8>,
    path: &mut Vec<String>,
) -> Result<ValueType> {
    if let Some(value_type) = symbols.get(name).and_then(|symbol| symbol.value_type) {
        return Ok(value_type);
    }
    if states.get(name) == Some(&1) {
        let start = path.iter().position(|entry| entry == name).unwrap_or(0);
        let mut cycle = path[start..].to_vec();
        cycle.push(name.to_owned());
        return Err(Diagnostic::new(
            "E_DEPENDENCY_CYCLE",
            format!("named-value dependency cycle: {}", cycle.join(" -> ")),
            symbols[name].declared_at.clone(),
        ));
    }

    states.insert(name.to_owned(), 1);
    path.push(name.to_owned());
    let declared_type = symbols[name].declared_type.clone();
    let value_type = match declared_type {
        DeclaredValueType::Known(value_type) => value_type,
        DeclaredValueType::Alias(target) => {
            if !symbols.contains_key(&target) {
                return Err(Diagnostic::new(
                    "E_MISSING_REFERENCE",
                    format!("reference `${target}` does not name any clip or invocation id"),
                    symbols[name].declared_at.clone(),
                ));
            }
            resolve_symbol_type(&target, symbols, states, path)?
        }
    };
    path.pop();
    states.insert(name.to_owned(), 2);
    symbols.get_mut(name).expect("collected symbol").value_type = Some(value_type);
    Ok(value_type)
}

fn bind_missing_fixed(
    program: &str,
    ports: &[InputPort],
    slots: &mut [Option<Vec<ValueRef>>],
    stack: &mut ValueStack,
    span: &SourceSpan,
) -> Result<()> {
    let missing = ports
        .iter()
        .enumerate()
        .filter(|(index, port)| {
            slots[*index].is_none() && matches!(port.cardinality, Cardinality::One)
        })
        .collect::<Vec<_>>();
    if stack.len() < missing.len() {
        return Err(stack_underflow(program, missing.len(), stack.len(), span));
    }
    let start = stack.len() - missing.len();
    let implicit = stack.split_off(start);
    for ((index, port), value) in missing.into_iter().zip(implicit) {
        require_value_type(value, port.value_type, program, port.name, span)?;
        slots[index] = Some(vec![value]);
    }
    Ok(())
}

fn pop_one(stack: &mut ValueStack, program: &str, span: &SourceSpan) -> Result<ValueRef> {
    stack
        .pop()
        .ok_or_else(|| stack_underflow(program, 1, 0, span))
}

fn stack_underflow(
    program: &str,
    required: usize,
    available: usize,
    span: &SourceSpan,
) -> Diagnostic {
    Diagnostic::new(
        "E_STACK_UNDERFLOW",
        format!(
            "`{program}` needs {required} preceding value(s), but the local stack has {available}"
        ),
        span.clone(),
    )
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

fn item_construct(kind: &ItemKind) -> String {
    match kind {
        ItemKind::Reference(_) => "reference".to_owned(),
        ItemKind::Invocation(invocation) => invocation.program.value.clone(),
        ItemKind::Then(_) => "then".to_owned(),
        ItemKind::Join(_) => "join".to_owned(),
        ItemKind::Timeline(_) => "timeline".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::model::ValueId;
    use crate::program::{IMAGE, ProgramDescriptor, ProgramRegistry};

    const TEST_INPUTS: &[InputPort] = &[InputPort {
        name: "video",
        value_type: ValueType::Video,
        cardinality: Cardinality::One,
    }];

    fn lower_test_value(
        call: &ResolvedCall<'_>,
        builder: &mut GraphBuilder<'_>,
    ) -> Result<ValueRef> {
        let _ = call.one_input("video")?;
        builder.reference(
            "test_value".to_owned(),
            ValueType::Test,
            call.origin().clone(),
        )
    }

    const TEST_VALUE: ProgramDefinition = ProgramDefinition {
        descriptor: ProgramDescriptor {
            name: "test_value",
            version: 1,
            inputs: TEST_INPUTS,
            parameters: &[],
            primary_parameter: None,
            output: ValueType::Test,
        },
        lower: lower_test_value,
    };
    static TEST_PROGRAMS: [ProgramDefinition; 2] = [IMAGE, TEST_VALUE];

    const TWO_TEST_INPUTS: &[InputPort] = &[
        InputPort {
            name: "original",
            value_type: ValueType::Test,
            cardinality: Cardinality::One,
        },
        InputPort {
            name: "alias",
            value_type: ValueType::Test,
            cardinality: Cardinality::One,
        },
    ];

    fn lower_test_source(
        call: &ResolvedCall<'_>,
        builder: &mut GraphBuilder<'_>,
    ) -> Result<ValueRef> {
        builder.push(
            crate::compiler::SemanticNodeKind::ImageVideo {
                path: "test.value".into(),
                frames: FrameCount(1),
                fit: crate::model::ImageFit::Cover,
            },
            ValueType::Test,
            1,
            call.origin().clone(),
        )
    }

    fn lower_test_sink(
        call: &ResolvedCall<'_>,
        builder: &mut GraphBuilder<'_>,
    ) -> Result<ValueRef> {
        let _ = call.one_input("original")?;
        let _ = call.one_input("alias")?;
        builder.image_video(
            "result.png".into(),
            FrameCount(1),
            crate::model::ImageFit::Cover,
            1,
            call.origin().clone(),
        )
    }

    const TEST_SOURCE: ProgramDefinition = ProgramDefinition {
        descriptor: ProgramDescriptor {
            name: "test_source",
            version: 1,
            inputs: &[],
            parameters: &[],
            primary_parameter: None,
            output: ValueType::Test,
        },
        lower: lower_test_source,
    };
    const TEST_SINK: ProgramDefinition = ProgramDefinition {
        descriptor: ProgramDescriptor {
            name: "test_sink",
            version: 1,
            inputs: TWO_TEST_INPUTS,
            parameters: &[],
            primary_parameter: None,
            output: ValueType::Video,
        },
        lower: lower_test_sink,
    };
    static ALIAS_TEST_PROGRAMS: [ProgramDefinition; 2] = [TEST_SOURCE, TEST_SINK];

    const VIDEO_PORTS_2: [InputPort; 2] = [
        InputPort {
            name: "before",
            value_type: ValueType::Video,
            cardinality: Cardinality::One,
        },
        InputPort {
            name: "after",
            value_type: ValueType::Video,
            cardinality: Cardinality::One,
        },
    ];
    const VIDEO_PORTS_3: [InputPort; 3] = [
        InputPort {
            name: "first",
            value_type: ValueType::Video,
            cardinality: Cardinality::One,
        },
        InputPort {
            name: "middle",
            value_type: ValueType::Video,
            cardinality: Cardinality::One,
        },
        InputPort {
            name: "last",
            value_type: ValueType::Video,
            cardinality: Cardinality::One,
        },
    ];

    fn span() -> SourceSpan {
        SourceSpan::file_start("test.yaml")
    }

    fn video(id: u32) -> ValueRef {
        ValueRef::new(ValueId::new(id), ValueType::Video)
    }

    #[test]
    fn two_fixed_implicit_ports_preserve_signature_order() {
        let mut slots = vec![None, None];
        let mut stack = vec![video(1), video(2)];
        bind_missing_fixed("combine", &VIDEO_PORTS_2, &mut slots, &mut stack, &span())
            .expect("bind");
        assert_eq!(slots[0].as_ref().expect("before")[0].id().get(), 1);
        assert_eq!(slots[1].as_ref().expect("after")[0].id().get(), 2);
    }

    #[test]
    fn mixed_explicit_and_implicit_ports_preserve_order() {
        let mut first_explicit = vec![Some(vec![video(9)]), None];
        let mut first_stack = vec![video(2)];
        bind_missing_fixed(
            "combine",
            &VIDEO_PORTS_2,
            &mut first_explicit,
            &mut first_stack,
            &span(),
        )
        .expect("bind after");
        assert_eq!(
            first_explicit[1].as_ref().expect("implicit after")[0].id(),
            ValueId::new(2)
        );

        let mut second_explicit = vec![None, Some(vec![video(9)])];
        let mut second_stack = vec![video(1)];
        bind_missing_fixed(
            "combine",
            &VIDEO_PORTS_2,
            &mut second_explicit,
            &mut second_stack,
            &span(),
        )
        .expect("bind before");
        assert_eq!(
            second_explicit[0].as_ref().expect("implicit before")[0].id(),
            ValueId::new(1)
        );
    }

    #[test]
    fn three_ports_with_explicit_middle_preserve_relative_order() {
        let mut slots = vec![None, Some(vec![video(9)]), None];
        let mut stack = vec![video(1), video(3)];
        bind_missing_fixed("combine", &VIDEO_PORTS_3, &mut slots, &mut stack, &span())
            .expect("bind");
        assert_eq!(slots[0].as_ref().expect("first")[0].id().get(), 1);
        assert_eq!(slots[2].as_ref().expect("last")[0].id().get(), 3);
    }

    #[test]
    fn incompatible_top_value_is_not_skipped() {
        let ports = [InputPort {
            name: "video",
            value_type: ValueType::Video,
            cardinality: Cardinality::One,
        }];
        let mut slots = vec![None];
        let mut stack = vec![video(1), ValueRef::new(ValueId::new(2), ValueType::Test)];
        let error = bind_missing_fixed("consume", &ports, &mut slots, &mut stack, &span())
            .expect_err("type");
        assert_eq!(error.code, "E_TYPE_MISMATCH");
        assert!(error.message.contains("expected Video"));
        assert!(error.message.contains("Test"));
    }

    #[test]
    fn then_rejects_a_non_video_input() {
        let workflow =
            crate::syntax::parse_str(Path::new("test.yaml"), "version: 1\ntimeline: []\n")
                .expect("workflow");
        let video = VideoSpec::default();
        let mut evaluator = Evaluator {
            workflow: &workflow,
            video: &video,
            registry: ProgramRegistry::default(),
            nodes: Vec::new(),
            symbols: BTreeMap::new(),
            symbol_order: Vec::new(),
            surface: Vec::new(),
        };
        let mut stack = vec![ValueRef::new(ValueId::new(0), ValueType::Test)];

        let error = evaluator
            .evaluate_item_kind(&ItemKind::Then(Vec::new()), &span(), &mut stack, None)
            .expect_err("then input type");
        assert_eq!(error.code, "E_TYPE_MISMATCH");
        assert!(error.message.contains("program `then` port `input`"));
    }

    #[test]
    fn then_rejects_a_non_video_output() {
        let registry = ProgramRegistry::from_definitions(&TEST_PROGRAMS).expect("registry");
        let workflow = crate::syntax::parse_str_with_registry(
            Path::new("test.yaml"),
            "version: 1\ntimeline:\n  - image: {path: a.png, duration: 1s}\n  - then: [test_value]\n",
            registry,
        )
        .expect("workflow");

        let error = crate::compiler::compile_with_registry(&workflow, registry)
            .expect_err("then output type");
        assert_eq!(error.code, "E_TYPE_MISMATCH");
        assert!(error.message.contains("program `then` port `output`"));
    }

    #[test]
    fn reference_alias_preserves_a_non_video_value_type() {
        let registry = ProgramRegistry::from_definitions(&ALIAS_TEST_PROGRAMS).expect("registry");
        let workflow = crate::syntax::parse_str_with_registry(
            Path::new("test.yaml"),
            "version: 1\ntimeline:\n  - test_source:\n    id: original\n  - ref: $original\n    id: alias\n  - test_sink\n",
            registry,
        )
        .expect("workflow");

        let compiled =
            crate::compiler::compile_with_registry(&workflow, registry).expect("typed alias");
        let document: serde_json::Value =
            serde_json::from_str(&compiled.canonical_json().expect("compiled JSON")).expect("JSON");
        assert_eq!(document["named_values"]["original"]["value_type"], "test");
        assert_eq!(document["named_values"]["alias"]["value_type"], "test");
    }

    #[test]
    fn reference_alias_cycles_use_the_dependency_cycle_diagnostic() {
        let registry = ProgramRegistry::from_definitions(&ALIAS_TEST_PROGRAMS).expect("registry");
        let workflow = crate::syntax::parse_str_with_registry(
            Path::new("test.yaml"),
            "version: 1\ntimeline:\n  - ref: $second\n    id: first\n  - ref: $first\n    id: second\n",
            registry,
        )
        .expect("workflow");

        let error =
            crate::compiler::compile_with_registry(&workflow, registry).expect_err("alias cycle");
        assert_eq!(error.code, "E_DEPENDENCY_CYCLE");
        assert!(error.message.contains("first -> second -> first"));
    }
}
