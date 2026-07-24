use std::collections::BTreeMap;

use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, SourceTime, SourceTimeRange, ValueRef};
use crate::program::{
    BoundParameters, Cardinality, InputPort, ParameterDescriptor, ParameterType, ParameterValue,
    ProgramDefinition, ResolvedCall, StackAccess,
};
use crate::semantic::{SourceOrigin, require_value_type};
use crate::source::{ArgumentValue, Invocation, Literal};
use crate::source::{SourceSpan, Spanned};

use super::stack::{EvaluationStack, StackFrame};

pub(super) struct BindContext<'a> {
    pub(super) stack: &'a mut EvaluationStack,
    pub(super) frame: &'a mut StackFrame,
    pub(super) access: StackAccess,
    pub(super) requested_frames: Option<FrameCount>,
    pub(super) origin: SourceOrigin,
}

pub(super) fn bind_call(
    definition: &ProgramDefinition,
    invocation: &Invocation,
    context: BindContext<'_>,
    mut resolve_input_value: impl FnMut(&ArgumentValue, &InputPort) -> Result<Vec<ValueRef>>,
) -> Result<ResolvedCall> {
    let BindContext {
        stack,
        frame,
        access,
        requested_frames,
        origin,
    } = context;
    let descriptor = &definition.descriptor;
    for (name, argument) in &invocation.arguments {
        if !descriptor.inputs.iter().any(|port| port.name == *name)
            && !descriptor
                .parameters
                .iter()
                .any(|parameter| parameter.name == *name)
        {
            return Err(Diagnostic::new(
                "E_UNKNOWN_PROGRAM_ARGUMENT",
                format!(
                    "unknown argument `{name}` for program `{}`",
                    descriptor.name
                ),
                argument.span().clone(),
            ));
        }
    }

    let mut slots = vec![None; descriptor.inputs.len()];
    for (index, port) in descriptor.inputs.iter().enumerate() {
        if let Some(argument) = invocation.arguments.get(&port.name) {
            slots[index] = Some(resolve_explicit_input(
                &descriptor.name,
                argument,
                port,
                &mut resolve_input_value,
            )?);
        }
    }

    bind_missing_fixed(
        &descriptor.name,
        &descriptor.inputs,
        &mut slots,
        stack,
        frame,
        access,
        &origin.span,
    )?;
    for (index, port) in descriptor.inputs.iter().enumerate() {
        if slots[index].is_some() {
            continue;
        }
        let Cardinality::Variadic { min } = port.cardinality else {
            continue;
        };
        let values = stack.take_variadic(
            frame,
            access,
            min,
            &descriptor.name,
            &port.name,
            &origin.span,
        )?;
        for value in &values {
            require_value_type(
                *value,
                port.value_type,
                &descriptor.name,
                &port.name,
                &origin.span,
            )?;
        }
        slots[index] = Some(values);
    }

    let inputs = descriptor
        .inputs
        .iter()
        .zip(slots)
        .map(|(port, values)| {
            values
                .map(|values| (port.name.clone(), values))
                .ok_or_else(|| {
                    Diagnostic::new(
                        "E_MISSING_REQUIRED_INPUT",
                        format!(
                            "program `{}` is missing input `{}`",
                            descriptor.name, port.name
                        ),
                        origin.span.clone(),
                    )
                })
        })
        .collect::<Result<BTreeMap<_, _>>>()?;

    let parameters = bind_parameters(definition, invocation)?;
    Ok(ResolvedCall::new(
        descriptor.name.clone(),
        inputs,
        parameters,
        requested_frames,
        origin,
    ))
}

fn bind_parameters(
    definition: &ProgramDefinition,
    invocation: &Invocation,
) -> Result<BoundParameters> {
    let mut parameters = BTreeMap::new();
    for descriptor in &definition.descriptor.parameters {
        if let Some(argument) = invocation.arguments.get(&descriptor.name) {
            parameters.insert(
                descriptor.name.clone(),
                bind_parameter(&definition.descriptor.name, descriptor, argument)?,
            );
        } else if descriptor.required {
            return Err(Diagnostic::new(
                "E_MISSING_ARGUMENT",
                format!(
                    "missing required parameter `{}.{}`",
                    definition.descriptor.name, descriptor.name
                ),
                invocation.program.span.clone(),
            ));
        }
    }
    Ok(parameters)
}

fn bind_parameter(
    program: &str,
    descriptor: &ParameterDescriptor,
    argument: &ArgumentValue,
) -> Result<Spanned<ParameterValue>> {
    let ArgumentValue::Literal(argument) = argument else {
        return Err(Diagnostic::new(
            "E_INVALID_ARGUMENT_TYPE",
            format!(
                "parameter `{}.{}` requires a scalar value",
                program, descriptor.name
            ),
            argument.span().clone(),
        ));
    };
    let value = match (&descriptor.parameter_type, argument) {
        (ParameterType::Integer, Literal::Integer(value, _)) => ParameterValue::Integer(*value),
        (ParameterType::File, Literal::String(value, _)) => ParameterValue::File(value.into()),
        (ParameterType::Duration, Literal::String(value, span)) => {
            ParameterValue::Duration(SourceTime::parse(value, span)?)
        }
        (ParameterType::TimeRange, Literal::String(value, span)) => {
            ParameterValue::TimeRange(SourceTimeRange::parse(value, span)?)
        }
        (ParameterType::Keyword(allowed), Literal::String(value, span)) => {
            let matched = allowed
                .iter()
                .find(|candidate| candidate.as_str() == value)
                .ok_or_else(|| {
                    Diagnostic::new(
                        "E_INVALID_ARGUMENT_VALUE",
                        format!(
                            "parameter `{program}.{}` must be one of: {}",
                            descriptor.name,
                            allowed.join(", ")
                        ),
                        span.clone(),
                    )
                })?;
            ParameterValue::Keyword(matched.clone())
        }
        _ => {
            return Err(Diagnostic::new(
                "E_INVALID_ARGUMENT_TYPE",
                format!(
                    "parameter `{}.{}` has the wrong value type",
                    program, descriptor.name
                ),
                argument.span().clone(),
            ));
        }
    };
    Ok(Spanned::new(value, argument.span().clone()))
}

fn resolve_explicit_input(
    program: &str,
    argument: &ArgumentValue,
    port: &InputPort,
    resolve_input_value: &mut impl FnMut(&ArgumentValue, &InputPort) -> Result<Vec<ValueRef>>,
) -> Result<Vec<ValueRef>> {
    if matches!(port.cardinality, Cardinality::Variadic { .. })
        && !matches!(
            argument,
            ArgumentValue::Reference(_) | ArgumentValue::References(_, _)
        )
    {
        return Err(Diagnostic::new(
            "E_INVALID_ARGUMENT_TYPE",
            format!(
                "explicit variadic input `{program}.{}` must use `$name` references",
                port.name
            ),
            argument.span().clone(),
        ));
    }
    let values = resolve_input_value(argument, port)?;
    match port.cardinality {
        Cardinality::One if values.len() != 1 => {
            return Err(Diagnostic::new(
                "E_INVALID_ARGUMENT_TYPE",
                format!("input `{program}.{}` requires exactly one value", port.name),
                argument.span().clone(),
            ));
        }
        Cardinality::Variadic { min } if values.len() < min => {
            return Err(Diagnostic::new(
                "E_MISSING_REQUIRED_INPUT",
                format!("input `{}` requires at least {min} values", port.name),
                argument.span().clone(),
            ));
        }
        _ => {}
    }
    for value in &values {
        require_value_type(
            *value,
            port.value_type,
            program,
            &port.name,
            argument.span(),
        )?;
    }
    Ok(values)
}

fn bind_missing_fixed(
    program: &str,
    ports: &[InputPort],
    slots: &mut [Option<Vec<ValueRef>>],
    stack: &mut EvaluationStack,
    frame: &mut StackFrame,
    access: StackAccess,
    span: &SourceSpan,
) -> Result<()> {
    let missing = ports
        .iter()
        .enumerate()
        .filter(|(index, port)| {
            slots[*index].is_none() && matches!(port.cardinality, Cardinality::One)
        })
        .collect::<Vec<_>>();
    let implicit = stack.take_fixed(frame, access, missing.len(), program, span)?;
    for ((index, port), value) in missing.into_iter().zip(implicit) {
        require_value_type(value, port.value_type, program, &port.name, span)?;
        slots[index] = Some(vec![value]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ValueId, ValueType};
    use crate::program::{ProgramDescriptor, ProgramImplementation};
    use std::path::Path;

    fn ports() -> Vec<InputPort> {
        ["first", "middle", "last"]
            .into_iter()
            .map(|name| InputPort {
                name: name.to_owned(),
                value_type: ValueType::Video,
                cardinality: Cardinality::One,
            })
            .collect()
    }

    fn parameters() -> Vec<ParameterDescriptor> {
        vec![
            ParameterDescriptor {
                name: "count".to_owned(),
                parameter_type: ParameterType::Integer,
                required: true,
            },
            ParameterDescriptor {
                name: "path".to_owned(),
                parameter_type: ParameterType::File,
                required: true,
            },
            ParameterDescriptor {
                name: "duration".to_owned(),
                parameter_type: ParameterType::Duration,
                required: true,
            },
            ParameterDescriptor {
                name: "range".to_owned(),
                parameter_type: ParameterType::TimeRange,
                required: true,
            },
            ParameterDescriptor {
                name: "fit".to_owned(),
                parameter_type: ParameterType::Keyword(
                    ["cover", "contain", "stretch"]
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                ),
                required: true,
            },
        ]
    }

    fn lower_stub(
        _call: &ResolvedCall,
        _builder: &mut crate::semantic::GraphBuilder<'_>,
    ) -> Result<Vec<ValueRef>> {
        unreachable!("binding tests do not lower programs")
    }

    fn typed_program() -> ProgramDefinition {
        ProgramDefinition {
            descriptor: ProgramDescriptor {
                name: "typed".to_owned(),
                semantic_version: 1,
                default_stack_access: StackAccess::Owned,
                inputs: vec![],
                parameters: parameters(),
                primary_parameter: None,
                outputs: vec![ValueType::Video],
            },
            implementation: ProgramImplementation::Direct(lower_stub),
            postfix: None,
        }
    }

    fn video(id: u32) -> ValueRef {
        ValueRef::new(ValueId::new(id), ValueType::Video)
    }

    fn span() -> SourceSpan {
        SourceSpan::file_start("test.yaml")
    }

    fn invocation(
        arguments: impl IntoIterator<Item = (&'static str, ArgumentValue)>,
    ) -> Invocation {
        Invocation {
            program: Spanned::new("typed".to_owned(), span()),
            stack_access: None,
            arguments: arguments
                .into_iter()
                .map(|(name, argument)| (name.to_owned(), argument))
                .collect(),
            body: None,
        }
    }

    fn bind(invocation: &Invocation) -> Result<ResolvedCall> {
        let (mut stack, mut frame) =
            EvaluationStack::isolated("test", SourceSpan::file_start("test.yaml"));
        let definition = typed_program();
        bind_call(
            &definition,
            invocation,
            BindContext {
                stack: &mut stack,
                frame: &mut frame,
                access: StackAccess::Owned,
                requested_frames: None,
                origin: SourceOrigin::new("typed", span()),
            },
            |_, _| unreachable!("typed program has no inputs"),
        )
    }

    #[test]
    fn fixed_suffix_preserves_descriptor_order() {
        let ports = ports();
        let mut slots = vec![None, Some(vec![video(9)]), None];
        let (mut stack, mut frame) =
            EvaluationStack::isolated("test", SourceSpan::file_start("test.yaml"));
        stack.extend([video(1), video(3)]);
        bind_missing_fixed(
            "combine",
            &ports,
            &mut slots,
            &mut stack,
            &mut frame,
            StackAccess::Owned,
            &SourceSpan::file_start("test.yaml"),
        )
        .expect("bind");
        assert_eq!(slots[0].as_ref().expect("first")[0].id().get(), 1);
        assert_eq!(slots[2].as_ref().expect("last")[0].id().get(), 3);
    }

    #[test]
    fn incompatible_top_value_is_not_skipped() {
        let ports = ports();
        let mut slots = vec![None];
        let (mut stack, mut frame) =
            EvaluationStack::isolated("test", SourceSpan::file_start("test.yaml"));
        stack.extend([video(1), ValueRef::new(ValueId::new(2), ValueType::Test)]);
        let error = bind_missing_fixed(
            "consume",
            &ports[..1],
            &mut slots,
            &mut stack,
            &mut frame,
            StackAccess::Owned,
            &SourceSpan::file_start("test.yaml"),
        )
        .expect_err("type mismatch");
        assert_eq!(error.code, "E_TYPE_MISMATCH");
    }

    #[test]
    fn fixed_explicit_input_rejects_reference_lists() {
        let ports = ports();
        let error = resolve_explicit_input(
            "combine",
            &ArgumentValue::References(
                vec![
                    Spanned::new("first".to_owned(), span()),
                    Spanned::new("second".to_owned(), span()),
                ],
                span(),
            ),
            &ports[0],
            &mut |_, _| Ok(vec![video(1), video(2)]),
        )
        .expect_err("fixed input list");
        assert_eq!(error.code, "E_INVALID_ARGUMENT_TYPE");
    }

    #[test]
    fn converts_every_declared_parameter_type() {
        let call = bind(&invocation([
            ("count", ArgumentValue::Literal(Literal::Integer(3, span()))),
            (
                "path",
                ArgumentValue::Literal(Literal::String("card.png".to_owned(), span())),
            ),
            (
                "duration",
                ArgumentValue::Literal(Literal::String("500ms".to_owned(), span())),
            ),
            (
                "range",
                ArgumentValue::Literal(Literal::String("1s..3s".to_owned(), span())),
            ),
            (
                "fit",
                ArgumentValue::Literal(Literal::String("contain".to_owned(), span())),
            ),
        ]))
        .expect("bind");

        assert_eq!(call.integer_parameter("count").expect("count").0, 3);
        assert_eq!(
            call.file_parameter("path").expect("path").0,
            Path::new("card.png")
        );
        assert_eq!(
            call.optional_duration_parameter("duration")
                .expect("duration")
                .expect("present")
                .0,
            SourceTime::parse("500ms", &span()).expect("expected duration")
        );
        assert_eq!(
            call.time_range_parameter("range").expect("range").0,
            SourceTimeRange::parse("1s..3s", &span()).expect("expected range")
        );
        let keyword = call
            .optional_keyword_parameter("fit")
            .expect("fit")
            .expect("present")
            .0;
        assert_eq!(keyword, "contain");
    }

    #[test]
    fn rejects_invalid_keyword() {
        let parameters = parameters();
        let error = bind_parameter(
            "typed",
            &parameters[4],
            &ArgumentValue::Literal(Literal::String("crop".to_owned(), span())),
        )
        .expect_err("invalid keyword");
        assert_eq!(error.code, "E_INVALID_ARGUMENT_VALUE");
        assert!(error.message.contains("cover, contain, stretch"));
    }

    #[test]
    fn rejects_wrong_parameter_representation() {
        let parameters = parameters();
        let error = bind_parameter(
            "typed",
            &parameters[1],
            &ArgumentValue::Literal(Literal::Integer(3, span())),
        )
        .expect_err("wrong representation");
        assert_eq!(error.code, "E_INVALID_ARGUMENT_TYPE");
    }

    #[test]
    fn rejects_missing_required_parameter() {
        let error = bind(&invocation([])).expect_err("missing parameter");
        assert_eq!(error.code, "E_MISSING_ARGUMENT");
        assert!(error.message.contains("typed.count"));
    }

    #[test]
    fn rejects_unknown_argument() {
        let error = bind(&invocation([(
            "mystery",
            ArgumentValue::Literal(Literal::String("value".to_owned(), span())),
        )]))
        .expect_err("unknown argument");
        assert_eq!(error.code, "E_UNKNOWN_PROGRAM_ARGUMENT");
    }
}
