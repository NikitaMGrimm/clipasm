use crate::diagnostic::{Diagnostic, Result};
use crate::model::{ValueRef, ValueType};
use crate::program::{
    BodyFinalizer, BodyPlan, Cardinality, InputPort, ParameterDescriptor, ParameterType,
    PostfixSyntax, ProgramDefinition, ProgramDescriptor, ProgramImplementation, ProgramOutputs,
    ResolvedCall, StackAccess,
};
use crate::semantic::{GraphBuilder, require_value_type};
use crate::source::SourceSpan;

const VIDEO: ValueType = ValueType::Video;

pub(crate) fn join() -> ProgramDefinition {
    body(
        descriptor(
            "join",
            2,
            vec![input("before"), input("after")],
            vec![],
            None,
        ),
        prepare_join,
        None,
    )
}

pub(crate) fn glue() -> ProgramDefinition {
    body(
        descriptor("glue", 1, vec![], vec![], None),
        prepare_glue,
        None,
    )
}

pub(crate) fn during() -> ProgramDefinition {
    body(
        descriptor(
            "during",
            1,
            vec![input("base")],
            vec![ParameterDescriptor {
                name: "range".to_owned(),
                parameter_type: ParameterType::TimeRange,
                required: true,
            }],
            Some("range"),
        ),
        prepare_during,
        Some(PostfixSyntax {
            parameter: "range".to_owned(),
        }),
    )
}

fn descriptor(
    name: &str,
    semantic_version: u32,
    inputs: Vec<InputPort>,
    parameters: Vec<ParameterDescriptor>,
    primary_parameter: Option<&str>,
) -> ProgramDescriptor {
    ProgramDescriptor {
        name: name.to_owned(),
        semantic_version,
        default_stack_access: StackAccess::Owned,
        inputs,
        parameters,
        primary_parameter: primary_parameter.map(str::to_owned),
        outputs: vec![VIDEO],
    }
}

fn input(name: &str) -> InputPort {
    InputPort {
        name: name.to_owned(),
        value_type: VIDEO,
        cardinality: Cardinality::One,
    }
}

fn body(
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

fn prepare_join(call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
    Ok(BodyPlan {
        initial_values: vec![call.one_input("before")?, call.one_input("after")?],
        requested_frames: call.requested_frames(),
        finalizer: Box::new(FinalizeConcatBody::for_call(call, "E_EMPTY_JOIN")),
    })
}

#[allow(clippy::unnecessary_wraps)]
fn prepare_glue(call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
    Ok(BodyPlan {
        initial_values: Vec::new(),
        requested_frames: call.requested_frames(),
        finalizer: Box::new(FinalizeConcatBody::for_call(call, "E_EMPTY_GLUE")),
    })
}

fn prepare_during(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
    let base = call.one_input("base")?;
    let (range, span) = call.time_range_parameter("range")?;
    let range = range.to_frames(builder.video_spec().fps, span)?;
    let selected = builder.at_span(span.clone()).slice(base, range)?;
    Ok(BodyPlan {
        initial_values: vec![selected],
        requested_frames: Some(range.frames()),
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
                format!("{} must produce at least one Video", self.owner),
                self.span,
            ));
        }
        Ok(vec![builder.concat(stack)?])
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
        stack: Vec<ValueRef>,
        builder: &mut GraphBuilder<'_>,
    ) -> Result<ProgramOutputs> {
        let replacement = take_one_video("during", stack, &self.span)?;
        Ok(vec![builder.replace_range(
            self.base,
            self.range,
            replacement,
        )?])
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
