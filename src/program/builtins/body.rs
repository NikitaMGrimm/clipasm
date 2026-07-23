use crate::diagnostic::{Diagnostic, Result, SourceSpan};
use crate::model::{ValueRef, ValueStack, ValueType};
use crate::program::{
    BodyFinalizer, BodyPlan, Cardinality, InputPort, ParameterDescriptor, ParameterType,
    PostfixSyntax, ProgramDefinition, ProgramDescriptor, ProgramImplementation, ResolvedCall,
};
use crate::semantic::{GraphBuilder, require_value_type};

const VIDEO: ValueType = ValueType::Video;
const THEN_INPUTS: &[InputPort] = &[InputPort {
    name: "input",
    value_type: VIDEO,
    cardinality: Cardinality::One,
}];
const JOIN_INPUTS: &[InputPort] = &[
    InputPort {
        name: "before",
        value_type: VIDEO,
        cardinality: Cardinality::One,
    },
    InputPort {
        name: "after",
        value_type: VIDEO,
        cardinality: Cardinality::One,
    },
];
const DURING_INPUTS: &[InputPort] = &[InputPort {
    name: "base",
    value_type: VIDEO,
    cardinality: Cardinality::One,
}];
const DURING_PARAMETERS: &[ParameterDescriptor] = &[ParameterDescriptor {
    name: "range",
    parameter_type: ParameterType::TimeRange,
    required: true,
}];
pub(crate) const THEN: ProgramDefinition = body(
    ProgramDescriptor {
        name: "then",
        semantic_version: 1,
        inputs: THEN_INPUTS,
        parameters: &[],
        primary_parameter: None,
        output: VIDEO,
    },
    prepare_then,
    None,
);

pub(crate) const JOIN: ProgramDefinition = body(
    ProgramDescriptor {
        name: "join",
        semantic_version: 1,
        inputs: JOIN_INPUTS,
        parameters: &[],
        primary_parameter: None,
        output: VIDEO,
    },
    prepare_join,
    None,
);

pub(crate) const TIMELINE: ProgramDefinition = body(
    ProgramDescriptor {
        name: "timeline",
        semantic_version: 1,
        inputs: &[],
        parameters: &[],
        primary_parameter: None,
        output: VIDEO,
    },
    prepare_timeline,
    None,
);

pub(crate) const DURING: ProgramDefinition = body(
    ProgramDescriptor {
        name: "during",
        semantic_version: 1,
        inputs: DURING_INPUTS,
        parameters: DURING_PARAMETERS,
        primary_parameter: Some("range"),
        output: VIDEO,
    },
    prepare_during,
    Some(PostfixSyntax { parameter: "range" }),
);

const fn body(
    descriptor: ProgramDescriptor,
    prepare: crate::program::BodyPrepareFn,
    postfix: Option<PostfixSyntax>,
) -> ProgramDefinition {
    ProgramDefinition {
        descriptor,
        implementation: ProgramImplementation::Body(prepare),
        postfix,
    }
}

fn prepare_then(call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
    Ok(BodyPlan {
        initial_stack: vec![call.one_input("input")?],
        requested_frames: call.requested_frames(),
        finalizer: Box::new(RequireOneVideo::for_call(call)),
    })
}

fn prepare_join(call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
    Ok(BodyPlan {
        initial_stack: vec![call.one_input("before")?, call.one_input("after")?],
        requested_frames: call.requested_frames(),
        finalizer: Box::new(RequireOneVideo::for_call(call)),
    })
}

#[allow(clippy::unnecessary_wraps)]
fn prepare_timeline(call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
    Ok(BodyPlan {
        initial_stack: Vec::new(),
        requested_frames: call.requested_frames(),
        finalizer: Box::new(FinalizeConcatBody::for_call(call, "E_EMPTY_TIMELINE")),
    })
}

fn prepare_during(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
    let base = call.one_input("base")?;
    let (range, span) = call.time_range_parameter("range")?;
    let range = range.to_frames(builder.video_spec().fps, span)?;
    let selected = builder.at_span(span.clone()).slice(base, range)?;
    Ok(BodyPlan {
        initial_stack: vec![selected],
        requested_frames: Some(range.frames()),
        finalizer: Box::new(FinalizeDuring {
            base,
            range,
            span: span.clone(),
        }),
    })
}

struct RequireOneVideo {
    owner: &'static str,
    span: SourceSpan,
}

impl RequireOneVideo {
    fn for_call(call: &ResolvedCall) -> Self {
        Self {
            owner: call.definition().descriptor.name,
            span: call.origin().span.clone(),
        }
    }
}

impl BodyFinalizer for RequireOneVideo {
    fn finish(
        self: Box<Self>,
        stack: ValueStack,
        _builder: &mut GraphBuilder<'_>,
    ) -> Result<ValueRef> {
        take_one_video(self.owner, stack, &self.span)
    }
}

struct FinalizeConcatBody {
    owner: &'static str,
    empty_code: &'static str,
    span: SourceSpan,
}

impl FinalizeConcatBody {
    fn for_call(call: &ResolvedCall, empty_code: &'static str) -> Self {
        Self {
            owner: call.definition().descriptor.name,
            empty_code,
            span: call.origin().span.clone(),
        }
    }
}

impl BodyFinalizer for FinalizeConcatBody {
    fn finish(
        self: Box<Self>,
        stack: ValueStack,
        builder: &mut GraphBuilder<'_>,
    ) -> Result<ValueRef> {
        if stack.is_empty() {
            return Err(Diagnostic::new(
                self.empty_code,
                format!("{} must produce at least one Video", self.owner),
                self.span,
            ));
        }
        builder.concat(stack)
    }
}

struct FinalizeDuring {
    base: ValueRef,
    range: crate::model::FrameRange,
    span: SourceSpan,
}

impl BodyFinalizer for FinalizeDuring {
    fn finish(
        self: Box<Self>,
        stack: ValueStack,
        builder: &mut GraphBuilder<'_>,
    ) -> Result<ValueRef> {
        let replacement = take_one_video("during", stack, &self.span)?;
        builder.replace_range(self.base, self.range, replacement)
    }
}

fn take_one_video(owner: &str, stack: ValueStack, span: &SourceSpan) -> Result<ValueRef> {
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
