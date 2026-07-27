use crate::diagnostic::{Diagnostic, Result};
use crate::model::{NativeRange, ValueRef, ValueType};
use crate::program::{
    BodyContract, BodyFinalizer, BodyOutputConstraint, BodyPlan, Cardinality, InputPort,
    NativeTimeRange, ParameterDescriptor, ParameterType, ProgramDefinition, ProgramDescriptor,
    ProgramImplementation, ProgramOutputs, RequestedVideoExtent, ResolvedCall, StackAccess,
    ValueTypeSpec,
};
use crate::semantic::{GraphBuilder, require_value_type};
use crate::source::SourceSpan;

const JOIN_INPUTS: [crate::program::InputSlot; 2] = [
    crate::program::InputSlot::new(0),
    crate::program::InputSlot::new(1),
];

pub(crate) fn join() -> ProgramDefinition {
    body(
        generic_descriptor(
            "join",
            3,
            vec![generic_input("before"), generic_input("after")],
            vec![],
        ),
        prepare_join,
        BodyContract {
            initial_values: vec![ValueTypeSpec::Generic, ValueTypeSpec::Generic],
            outputs: BodyOutputConstraint::Variadic {
                value_type: ValueTypeSpec::Generic,
                min: 1,
            },
            count_error_code: "E_EMPTY_JOIN",
        },
        crate::program::TimelineBehavior::BodyConcat {
            inputs: &JOIN_INPUTS,
        },
    )
}

pub(crate) fn during() -> ProgramDefinition {
    body(
        generic_descriptor(
            "during",
            3,
            vec![generic_input("timeline")],
            vec![ParameterDescriptor {
                name: "range".to_owned(),
                parameter_type: ParameterType::TimeRange,
                required: true,
            }],
        ),
        prepare_during,
        BodyContract {
            initial_values: vec![ValueTypeSpec::Generic],
            outputs: BodyOutputConstraint::Exactly(vec![ValueTypeSpec::Generic]),
            count_error_code: "E_BODY_OUTPUT_COUNT",
        },
        crate::program::TimelineBehavior::Replace {
            base: crate::program::InputSlot::new(0),
        },
    )
}

fn generic_descriptor(
    name: &str,
    semantic_version: u32,
    inputs: Vec<InputPort>,
    parameters: Vec<ParameterDescriptor>,
) -> ProgramDescriptor {
    ProgramDescriptor {
        name: name.to_owned(),
        semantic_version,
        default_stack_access: StackAccess::Visible,
        inputs,
        parameters,
        outputs: vec![ValueTypeSpec::Generic],
    }
}

fn generic_input(name: &str) -> InputPort {
    InputPort {
        name: name.to_owned(),
        value_type: ValueTypeSpec::Generic,
        cardinality: Cardinality::One,
    }
}

fn body(
    descriptor: ProgramDescriptor,
    prepare: crate::program::BodyPrepareFn,
    body_contract: BodyContract,
    timeline_behavior: crate::program::TimelineBehavior,
) -> ProgramDefinition {
    ProgramDefinition {
        descriptor,
        implementation: ProgramImplementation::Body {
            prepare,
            contract: body_contract,
        },
        timeline_behavior,
    }
}

fn prepare_join(call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
    Ok(BodyPlan {
        initial_values: vec![call.one_input("before")?, call.one_input("after")?],
        requested_extent: call.requested_extent().cloned(),
        finalizer: Box::new(FinalizeConcatBody::for_call(call, "E_EMPTY_JOIN")),
    })
}

fn prepare_during(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
    let base = call.one_input("timeline")?;
    let (authored_range, span) = call.time_range_parameter("range")?;
    let range = authored_range.to_native_range(
        base.value_type(),
        builder.video_spec().fps(),
        builder.audio_spec().sample_rate(),
        span,
    )?;
    let selected = select_range(&range, base, builder, span)?;
    let requested_extent = match base.value_type() {
        ValueType::Video => Some(requested_video_extent(&range)),
        ValueType::Audio => call.requested_extent().cloned(),
    };
    Ok(BodyPlan {
        initial_values: vec![selected],
        requested_extent,
        finalizer: Box::new(FinalizeDuring {
            base,
            range,
            span: span.clone(),
        }),
    })
}

fn select_range(
    range: &NativeTimeRange,
    base: ValueRef,
    builder: &mut GraphBuilder<'_>,
    span: &SourceSpan,
) -> Result<ValueRef> {
    let mut builder = builder.at_span(span.clone());
    match range {
        NativeTimeRange::Concrete(range) => builder.slice(base, *range),
        NativeTimeRange::Deferred(range) => builder.deferred_slice(base, range.clone()),
    }
}

fn requested_video_extent(range: &NativeTimeRange) -> RequestedVideoExtent {
    match range {
        NativeTimeRange::Concrete(NativeRange::Frames(range)) => {
            RequestedVideoExtent::Concrete(range.frames())
        }
        NativeTimeRange::Deferred(range) => {
            RequestedVideoExtent::Deferred(range.end.subtract(&range.start))
        }
        NativeTimeRange::Concrete(NativeRange::Samples(_)) => {
            unreachable!("Audio range cannot request a Video extent")
        }
    }
}

struct FinalizeConcatBody {
    owner: String,
    empty_code: &'static str,
    span: SourceSpan,
}

impl FinalizeConcatBody {
    fn for_call(call: &ResolvedCall, empty_code: &'static str) -> Self {
        Self {
            owner: call.program_name().to_owned(),
            empty_code,
            span: call.origin().span.clone(),
        }
    }
}

impl BodyFinalizer for FinalizeConcatBody {
    fn finish(
        self: Box<Self>,
        stack: Vec<ValueRef>,
        builder: &mut GraphBuilder<'_>,
    ) -> Result<ProgramOutputs> {
        if stack.is_empty() {
            return Err(Diagnostic::new(
                self.empty_code,
                format!("{} must produce at least one Video or Audio", self.owner),
                self.span,
            ));
        }
        Ok(vec![builder.concat(stack)?])
    }
}

struct FinalizeDuring {
    base: ValueRef,
    range: NativeTimeRange,
    span: SourceSpan,
}

impl BodyFinalizer for FinalizeDuring {
    fn finish(
        self: Box<Self>,
        stack: Vec<ValueRef>,
        builder: &mut GraphBuilder<'_>,
    ) -> Result<ProgramOutputs> {
        let replacement = take_one_timeline("during", stack, self.base.value_type(), &self.span)?;
        Ok(vec![match self.range {
            NativeTimeRange::Concrete(range) => {
                builder.replace_range(self.base, range, replacement)?
            }
            NativeTimeRange::Deferred(range) => {
                builder.deferred_replace_range(self.base, range, replacement)?
            }
        }])
    }
}

fn take_one_timeline(
    owner: &str,
    stack: Vec<ValueRef>,
    expected: ValueType,
    span: &SourceSpan,
) -> Result<ValueRef> {
    if stack.len() != 1 {
        return Err(Diagnostic::new(
            "E_BODY_OUTPUT_COUNT",
            format!(
                "`{owner}` body must leave exactly one value, but {} values remain",
                stack.len()
            ),
            span.clone(),
        ));
    }
    let output = stack.into_iter().next().expect("one checked body output");
    require_value_type(output, expected, owner, "output", span)?;
    Ok(output)
}
