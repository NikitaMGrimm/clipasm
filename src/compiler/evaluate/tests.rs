use std::path::{Path, PathBuf};

use super::*;
use crate::model::{FrameCount, ImageFit, NativeRange};
use crate::program::{
    BodyFinalizer, BodyPlan, Cardinality, InputPort, ProgramDefinition, ProgramDescriptor,
    ProgramRegistry, ResolvedCall, StackAccess,
};

#[expect(
    clippy::unnecessary_wraps,
    reason = "test body preparers must match the fallible BodyPrepareFn signature"
)]
fn prepare_root(call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
    Ok(BodyPlan {
        initial_values: Vec::new(),
        requested_extent: call.requested_extent().cloned(),
        finalizer: Box::new(RootFinalizer),
    })
}

fn prepare_unexpected_initial_value(
    _call: &ResolvedCall,
    builder: &mut GraphBuilder<'_>,
) -> Result<BodyPlan> {
    Ok(BodyPlan {
        initial_values: vec![builder.image_video(
            PathBuf::from("unexpected.png"),
            FrameCount(1),
            ImageFit::Cover,
        )?],
        requested_extent: None,
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

fn lower_wrong_type(_call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<Vec<ValueRef>> {
    Ok(vec![builder.audio_source(PathBuf::from("wrong.wav"))?])
}

fn lower_two(_call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<Vec<ValueRef>> {
    Ok(vec![
        builder.image_video(PathBuf::from("first.png"), FrameCount(1), ImageFit::Cover)?,
        builder.image_video(PathBuf::from("second.png"), FrameCount(1), ImageFit::Cover)?,
    ])
}

fn lower_same_two(_call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<Vec<ValueRef>> {
    let value = builder.image_video(PathBuf::from("shared.png"), FrameCount(1), ImageFit::Cover)?;
    Ok(vec![value, value])
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "test direct lowerers must match the fallible DirectProgramFn signature"
)]
fn lower_zero(_call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<Vec<ValueRef>> {
    Ok(Vec::new())
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "test body preparers must match the fallible BodyPrepareFn signature"
)]
fn prepare_wrong_body(call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
    Ok(BodyPlan {
        initial_values: Vec::new(),
        requested_extent: call.requested_extent().cloned(),
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
        Ok(vec![builder.audio_source(PathBuf::from("wrong.wav"))?])
    }
}

fn prepare_versioned_body(call: &ResolvedCall, builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
    let prepared = builder.image_video(
        PathBuf::from("prepared.png"),
        FrameCount(1),
        ImageFit::Cover,
    )?;
    Ok(BodyPlan {
        initial_values: vec![prepared],
        requested_extent: call.requested_extent().cloned(),
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
    ProgramDefinition {
        descriptor: ProgramDescriptor {
            name: name.to_owned(),
            semantic_version,
            default_stack_access,
            inputs,
            parameters: vec![],
            outputs: outputs.into_iter().map(Into::into).collect(),
        },
        implementation,
        timeline_behavior: crate::program::TimelineBehavior::Fresh,
    }
}

fn output_programs() -> Vec<ProgramDefinition> {
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
            ProgramImplementation::Body {
                prepare: prepare_wrong_body,
                contract: crate::program::BodyContract {
                    initial_values: Vec::new(),
                    outputs: crate::program::BodyOutputConstraint::Exactly(vec![
                        ValueType::Video.into(),
                    ]),
                    count_diagnostic: crate::program::BodyCountDiagnostic::Builtin(
                        BuiltinDiagnostic::BodyOutputCount,
                    ),
                },
            },
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
        ProgramImplementation::Body {
            prepare: prepare_versioned_body,
            contract: crate::program::BodyContract {
                initial_values: Vec::new(),
                outputs: crate::program::BodyOutputConstraint::Exactly(vec![
                    ValueType::Video.into(),
                ]),
                count_diagnostic: crate::program::BodyCountDiagnostic::Builtin(
                    BuiltinDiagnostic::BodyOutputCount,
                ),
            },
        },
    );
    let ProgramImplementation::Body { contract, .. } = &mut versioned_body.implementation else {
        unreachable!("versioned body implementation")
    };
    contract.initial_values = vec![ValueType::Video.into()];
    vec![
        definition(
            "versioned_direct",
            11,
            StackAccess::Owned,
            vec![],
            vec![ValueType::Video],
            ProgramImplementation::Direct(lower_source),
        ),
        definition(
            "drop",
            1,
            StackAccess::Owned,
            vec![InputPort {
                name: "value".to_owned(),
                value_type: ValueType::Video.into(),
                cardinality: Cardinality::One,
            }],
            vec![],
            ProgramImplementation::Direct(lower_zero),
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
            ProgramImplementation::Body {
                prepare: prepare_root,
                contract: crate::program::BodyContract {
                    initial_values: Vec::new(),
                    outputs: crate::program::BodyOutputConstraint::Exactly(vec![
                        ValueType::Video.into(),
                    ]),
                    count_diagnostic: crate::program::BodyCountDiagnostic::Builtin(
                        BuiltinDiagnostic::BodyOutputCount,
                    ),
                },
            },
        ),
    ]
}

fn parse_with_registry(
    source: &str,
    definitions: Vec<ProgramDefinition>,
) -> (crate::source::SourcePackage, ProgramRegistry) {
    let registry = ProgramRegistry::from_definitions(definitions).expect("registry");
    let workflow =
        crate::language::parse_str_with_registry(Path::new("test.clipasm"), source, &registry)
            .expect("workflow");
    (workflow, registry)
}

fn parse_with_synthetic_outputs(source: &str) -> (crate::source::SourcePackage, ProgramRegistry) {
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
        "same_two_output",
        1,
        StackAccess::Owned,
        vec![],
        vec![ValueType::Video, ValueType::Video],
        ProgramImplementation::Direct(lower_same_two),
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
        "clipasm 1\nclip {\n  $before\n  $after\n  concat\n} as combined\ntwo_output as (before, after)\nconcat\n",
    );
    let compiled = crate::compiler::compile_with_registry(&workflow, &registry).expect("compile");

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
fn multiple_output_bindings_name_distinct_occurrences_even_when_media_is_shared() {
    let (workflow, registry) = parse_with_synthetic_outputs(
        "clipasm 1\nsame_two_output as (left, right)\nconcat as joined\ntrim(value=$joined, range=$joined::right)\n",
    );
    let compiled = crate::compiler::compile_with_registry(&workflow, &registry).expect("compile");

    let range = compiled
        .nodes()
        .iter()
        .find_map(|node| match node.kind() {
            crate::semantic::SemanticNodeKind::Slice {
                range: NativeRange::Frames(range),
                ..
            } => Some(*range),
            _ => None,
        })
        .expect("slice created from the right tuple output");
    assert_eq!(range.start(), 1);
    assert_eq!(range.end(), 2);
    assert_eq!(
        compiled.named_values()["left"],
        compiled.named_values()["right"]
    );
}

#[test]
fn multiple_output_bindings_reject_duplicate_names_within_one_tuple() {
    let (workflow, registry) =
        parse_with_synthetic_outputs("clipasm 1\ntwo_output as (same, same)\n");
    let error = crate::compiler::compile_with_registry(&workflow, &registry)
        .expect_err("duplicate tuple output names");
    assert_eq!(error.code, "E_DUPLICATE_NAME");
}

#[test]
fn zero_output_items_leave_the_stack_unchanged() {
    let (workflow, registry) =
        parse_with_synthetic_outputs("clipasm 1\nimage(\"card.png\", 1s)\nzero_output\n");
    let compiled = crate::compiler::compile_with_registry(&workflow, &registry).expect("compile");
    let entry = compiled
        .explain()
        .iter()
        .find(|entry| entry.construct() == "zero_output")
        .expect("zero-output explain entry");
    assert!(entry.outputs().is_empty());
}

#[test]
fn unnamed_multiple_outputs_are_appended_and_may_be_consumed() {
    let (workflow, registry) = parse_with_synthetic_outputs("clipasm 1\ntwo_output\nconcat\n");
    let compiled = crate::compiler::compile_with_registry(&workflow, &registry).expect("compile");
    assert_eq!(compiled.outputs().len(), 1);
}

#[test]
fn output_bindings_require_the_exact_supported_cardinality() {
    for (source, expected) in [
        (
            "clipasm 1\ntwo_output as pair\n",
            "`as name` requires exactly one output",
        ),
        (
            "clipasm 1\ntwo_output as (first, second, third)\n",
            "3 name(s)",
        ),
        (
            "clipasm 1\nimage(\"card.png\", 1s) as (card, extra)\n",
            "2 name(s)",
        ),
        ("clipasm 1\nzero_output as none\n", "produces 0 value(s)"),
    ] {
        let (workflow, registry) = parse_with_synthetic_outputs(source);
        let error = crate::compiler::compile_with_registry(&workflow, &registry)
            .expect_err("invalid output binding");
        assert_eq!(error.code, "E_OUTPUT_BINDING_COUNT");
        assert!(error.message.contains(expected), "{}", error.message);
    }
}

#[test]
fn direct_and_body_outputs_must_match_their_declarations() {
    for source in [
        "clipasm 1\nwrong_direct\n",
        "clipasm 1\nwrong_body { source }\n",
    ] {
        let (workflow, registry) = parse_with_registry(source, output_programs());
        let error = crate::compiler::compile_with_registry(&workflow, &registry).expect_err("type");
        assert_eq!(error.code, "E_PROGRAM_OUTPUT_TYPE");
    }
}

#[test]
fn program_output_count_must_match_its_declaration() {
    let (workflow, registry) = parse_with_registry("clipasm 1\nwrong_count\n", output_programs());
    let error =
        crate::compiler::compile_with_registry(&workflow, &registry).expect_err("output count");
    assert_eq!(error.code, "E_PROGRAM_OUTPUT_COUNT");
}

#[test]
fn scoped_builders_propagate_program_semantic_versions() {
    let (workflow, registry) = parse_with_registry(
        "clipasm 1\n@owned { versioned_direct } as unused\n@owned drop\nversioned_body {}\n",
        version_programs(),
    );
    let compiled = crate::compiler::compile_with_registry(&workflow, &registry).expect("compile");

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
        "clipasm 1\nsource\nvisible_body { visible_unary }\n",
        visible_default_programs(),
    );
    crate::compiler::compile_with_registry(&workflow, &registry)
        .expect("visible descriptor defaults capture the source");

    let (workflow, registry) = parse_with_registry(
        "clipasm 1\nsource\nvisible_body { @owned visible_unary }\n",
        visible_default_programs(),
    );
    let error = crate::compiler::compile_with_registry(&workflow, &registry)
        .expect_err("owned override blocks capture");
    assert_eq!(error.code, "E_STACK_UNDERFLOW");
    assert!(error.message.contains("only 0 owned"));
}
#[test]
fn body_prepare_values_must_match_the_declared_contract() {
    let mut programs = vec![
        definition(
            "source",
            1,
            StackAccess::Owned,
            vec![],
            vec![ValueType::Video],
            ProgramImplementation::Direct(lower_source),
        ),
        definition(
            "bad_body_plan",
            1,
            StackAccess::Owned,
            vec![],
            vec![ValueType::Video],
            ProgramImplementation::Body {
                prepare: prepare_unexpected_initial_value,
                contract: crate::program::BodyContract {
                    initial_values: vec![],
                    outputs: crate::program::BodyOutputConstraint::Exactly(vec![
                        ValueType::Video.into(),
                    ]),
                    count_diagnostic: crate::program::BodyCountDiagnostic::Builtin(
                        BuiltinDiagnostic::BodyOutputCount,
                    ),
                },
            },
        ),
    ];
    let (workflow, registry) = parse_with_registry(
        "clipasm 1\nbad_body_plan { source }\n",
        std::mem::take(&mut programs),
    );

    let error = crate::compiler::compile_with_registry(&workflow, &registry)
        .expect_err("prepare function must obey the body contract");
    assert_eq!(error.code, "E_INTERNAL_PROGRAM_CONTRACT");
    assert!(error.message.contains("prepared 1 initial value"));
}
