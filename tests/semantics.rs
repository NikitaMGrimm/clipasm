use std::fs;
use std::path::Path;

use rhythmcut::compiler::{self, PrimitiveNodeKind};
use tempfile::TempDir;

fn project(source: &str) -> (TempDir, rhythmcut::syntax::Workflow) {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(directory.path().join("a.ppm"), b"P3\n1 1\n255\n255 0 0\n").expect("a image");
    fs::write(directory.path().join("b.ppm"), b"P3\n1 1\n255\n0 255 0\n").expect("b image");
    fs::write(directory.path().join("c.ppm"), b"P3\n1 1\n255\n0 0 255\n").expect("c image");
    fs::write(directory.path().join("x.ppm"), b"P3\n1 1\n255\n255 255 0\n").expect("x image");
    fs::write(directory.path().join("y.ppm"), b"P3\n1 1\n255\n0 255 255\n").expect("y image");
    let path = directory.path().join("workflow.yaml");
    fs::write(&path, source).expect("workflow");
    let workflow = rhythmcut::syntax::parse_file(&path).expect("parse workflow");
    (directory, workflow)
}

fn frames(plan: &rhythmcut::compiler::CompiledPlan) -> u64 {
    plan.nodes[plan.root.0 as usize].frames.0
}

#[test]
fn repeat_reuses_one_upstream_node() {
    let (_directory, workflow) = project(
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\nclips:\n  doubled:\n    - $later\n    - repeat: 3\n  later:\n    image: a.ppm\n    duration: 1s\ntimeline:\n  - $doubled\n",
    );
    let plan = compiler::compile(&workflow).expect("compile");
    assert_eq!(frames(&plan), 30);
    let repeat = plan
        .nodes
        .iter()
        .find(|node| node.origin.construct == "repeat")
        .expect("repeat concat");
    let PrimitiveNodeKind::Concat { inputs } = &repeat.kind else {
        panic!("repeat must lower to concat");
    };
    assert_eq!(inputs.len(), 3);
    assert!(inputs.iter().all(|input| *input == inputs[0]));
}

#[test]
fn then_keeps_its_input_physically_present() {
    let (_directory, workflow) = project(
        "version: 1\ntimeline:\n  - image: a.ppm\n    duration: 1s\n  - then:\n      - image: b.ppm\n        duration: 1s\n",
    );
    let error = compiler::compile(&workflow).expect_err("two outputs");
    assert_eq!(error.code, "E_THEN_OUTPUT_COUNT");
}

#[test]
fn during_changes_duration_and_lowers_to_primitives() {
    let (_directory, workflow) = project(
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\ntimeline:\n  - image: a.ppm\n    duration: 10s\n  - repeat: 2\n    during: 4s..6s\n",
    );
    let plan = compiler::compile(&workflow).expect("compile");
    assert_eq!(frames(&plan), 120);
    assert!(plan.nodes.iter().all(|node| matches!(
        node.kind,
        PrimitiveNodeKind::ImageVideo { .. }
            | PrimitiveNodeKind::Slice { .. }
            | PrimitiveNodeKind::Concat { .. }
    )));
    assert_eq!(
        plan.nodes
            .iter()
            .filter(|node| matches!(node.kind, PrimitiveNodeKind::Slice { .. }))
            .count(),
        3
    );
}

#[test]
fn during_does_not_hide_selected_input_from_a_source() {
    let (_directory, workflow) = project(
        "version: 1\ntimeline:\n  - image: a.ppm\n    duration: 10s\n  - image: b.ppm\n    duration: 2s\n    during: 4s..6s\n",
    );
    let error = compiler::compile(&workflow).expect_err("selected plus source");
    assert_eq!(error.code, "E_DURING_OUTPUT_COUNT");
}

#[test]
fn join_reduces_only_the_top_two_outer_values() {
    let (_directory, workflow) = project(
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\ntimeline:\n  - image: a.ppm\n    duration: 1s\n  - image: b.ppm\n    duration: 1s\n  - image: c.ppm\n    duration: 1s\n  - join:\n      - concat\n",
    );
    let plan = compiler::compile(&workflow).expect("compile");
    assert_eq!(frames(&plan), 30);
    assert_eq!(
        plan.nodes
            .iter()
            .filter(|node| matches!(node.kind, PrimitiveNodeKind::Concat { .. }))
            .count(),
        2
    );
}

#[test]
fn explicit_inputs_do_not_consume_join_stack_occurrences() {
    let (_directory, workflow) = project(
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\nclips:\n  x: {image: x.ppm, duration: 1s}\n  y: {image: y.ppm, duration: 1s}\ntimeline:\n  - image: a.ppm\n    duration: 1s\n  - image: b.ppm\n    duration: 1s\n  - join:\n      - concat: [$x, $y]\n      - concat\n",
    );
    let plan = compiler::compile(&workflow).expect("compile");
    assert_eq!(frames(&plan), 40);
}

#[test]
fn named_clip_does_not_receive_timeline_finalization() {
    let (_directory, workflow) = project(
        "version: 1\nclips:\n  pair:\n    - {image: a.ppm, duration: 1s}\n    - {image: b.ppm, duration: 1s}\ntimeline:\n  - $pair\n",
    );
    let error = compiler::compile(&workflow).expect_err("leftovers");
    assert_eq!(error.code, "E_CLIP_OUTPUT_COUNT");
}

#[test]
fn reports_readable_named_cycle() {
    let (_directory, workflow) =
        project("version: 1\nclips:\n  a: $b\n  b: $a\ntimeline:\n  - $a\n");
    let error = compiler::compile(&workflow).expect_err("cycle");
    assert_eq!(error.code, "E_DEPENDENCY_CYCLE");
    assert!(error.message.contains("a -> b -> a"));
}

#[test]
fn mapping_order_does_not_change_compiled_plan() {
    let (first_dir, first) = project(
        "version: 1\nclips:\n  a: {image: a.ppm, duration: 1s}\n  b: {image: b.ppm, duration: 1s}\ntimeline: [$a, $b]\n",
    );
    let (second_dir, second) = project(
        "version: 1\nclips:\n  b: {image: b.ppm, duration: 1s}\n  a: {image: a.ppm, duration: 1s}\ntimeline: [$a, $b]\n",
    );
    let first_plan = compiler::compile(&first).expect("first plan");
    let second_plan = compiler::compile(&second).expect("second plan");
    // Source paths differ between temp directories; compare semantic structure and domains.
    assert_eq!(frames(&first_plan), frames(&second_plan));
    let first_ops = operations(&first_plan);
    let second_ops = operations(&second_plan);
    assert_eq!(first_ops, second_ops);
    drop((first_dir, second_dir));
}

fn operations(plan: &rhythmcut::compiler::CompiledPlan) -> Vec<&'static str> {
    plan.nodes
        .iter()
        .map(|node| match node.kind {
            PrimitiveNodeKind::ImageVideo { .. } => "image",
            PrimitiveNodeKind::Slice { .. } => "slice",
            PrimitiveNodeKind::Concat { .. } => "concat",
        })
        .collect()
}

#[test]
fn compile_file_accepts_an_outputless_validation_workflow() {
    let (directory, _workflow) =
        project("version: 1\ntimeline:\n  - image: a.ppm\n    duration: 1s\n");
    compiler::compile_file(&directory.path().join(Path::new("workflow.yaml"))).expect("compile");
}

#[test]
fn comments_and_invocation_mapping_order_do_not_change_plan_hash() {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(directory.path().join("a.ppm"), b"P3\n1 1\n255\n255 0 0\n").expect("image");
    let path = directory.path().join("workflow.yaml");
    let first = rhythmcut::syntax::parse_str(
        &path,
        "version: 1\ntimeline:\n  - image: a.ppm\n    duration: 1s\n",
    )
    .expect("first parse");
    let second = rhythmcut::syntax::parse_str(
        &path,
        "# formatting is not semantic\nversion: 1\ntimeline:\n  - duration: 1s\n    image: a.ppm\n",
    )
    .expect("second parse");
    assert_eq!(
        compiler::compile(&first).expect("first plan").plan_hash,
        compiler::compile(&second).expect("second plan").plan_hash
    );
}
