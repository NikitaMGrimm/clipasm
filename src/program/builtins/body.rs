use crate::diagnostic::{Diagnostic, Result};
use crate::model::{ValueRef, ValueType};
use crate::program::{
    BodyContract, BodyFinalizer, BodyOutputConstraint, BodyPlan, Cardinality, InputPort,
    ParameterDescriptor, ParameterType, ProgramDefinition, ProgramDescriptor,
    ProgramImplementation, ProgramOutputs, RequestedVideoExtent, ResolvedCall, StackAccess,
    ValueTypeSpec, VideoTimeRange,
};
use crate::semantic::{GraphBuilder, require_value_type};
use crate::source::SourceSpan;

const VIDEO: ValueType = ValueType::Video;
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
        descriptor(
            "during",
            2,
            vec![input("video")],
            vec![ParameterDescriptor {
                name: "range".to_owned(),
                parameter_type: ParameterType::TimeRange,
                required: true,
            }],
        ),
        prepare_during,
        BodyContract {
            initial_values: vec![VIDEO.into()],
            outputs: BodyOutputConstraint::Exactly(vec![VIDEO.into()]),
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
) -> ProgramDescriptor {
    ProgramDescriptor {
        name: name.to_owned(),
        semantic_version,
        default_stack_access: StackAccess::Visible,
        inputs,
        parameters: vec![],
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

fn descriptor(
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
        outputs: vec![VIDEO.into()],
    }
}

fn input(name: &str) -> InputPort {
    InputPort {
        name: name.to_owned(),
        value_type: VIDEO.into(),
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
    let base = call.one_input("video")?;
    let (range, span) = call.time_range_parameter("range")?;
    let range = range.to_video_range(builder.video_spec().fps(), span)?;
    let selected = match &range {
        VideoTimeRange::Concrete(range) => builder.at_span(span.clone()).slice(base, *range)?,
        VideoTimeRange::Deferred(range) => builder
            .at_span(span.clone())
            .deferred_slice(base, range.clone())?,
    };
    Ok(BodyPlan {
        initial_values: vec![selected],
        requested_extent: Some(RequestedVideoExtent::from_range(range.clone())),
        finalizer: Box::new(FinalizeDuring {
            base,
            range,
            span: span.clone(),
        }),
    })
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
    range: VideoTimeRange,
    span: SourceSpan,
}

impl BodyFinalizer for FinalizeDuring {
    fn finish(
        self: Box<Self>,
        stack: Vec<ValueRef>,
        builder: &mut GraphBuilder<'_>,
    ) -> Result<ProgramOutputs> {
        let replacement = take_one_video("during", stack, &self.span)?;
        Ok(vec![match self.range {
            VideoTimeRange::Concrete(range) => {
                builder.replace_range(self.base, range, replacement)?
            }
            VideoTimeRange::Deferred(range) => {
                builder.deferred_replace_range(self.base, range, replacement)?
            }
        }])
    }
}

fn take_one_video(owner: &str, stack: Vec<ValueRef>, span: &SourceSpan) -> Result<ValueRef> {
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
    require_value_type(output, VIDEO, owner, "output", span)?;
    Ok(output)
}
