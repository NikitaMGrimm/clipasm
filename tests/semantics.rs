#![allow(missing_docs)]

use std::fs;
use std::path::Path;

use clipasm::compiler;
use tempfile::TempDir;

fn project(source: &str) -> (TempDir, clipasm::syntax::Workflow) {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(directory.path().join("a.ppm"), b"P3\n1 1\n255\n255 0 0\n").expect("a image");
    fs::write(directory.path().join("b.ppm"), b"P3\n1 1\n255\n0 255 0\n").expect("b image");
    fs::write(directory.path().join("c.ppm"), b"P3\n1 1\n255\n0 0 255\n").expect("c image");
    fs::write(directory.path().join("x.ppm"), b"P3\n1 1\n255\n255 255 0\n").expect("x image");
    fs::write(directory.path().join("y.ppm"), b"P3\n1 1\n255\n0 255 255\n").expect("y image");
    let path = directory.path().join("workflow.yaml");
    fs::write(&path, source).expect("workflow");
    let workflow = clipasm::syntax::parse_file(&path).expect("parse workflow");
    (directory, workflow)
}

fn compiled_json(compiled: &compiler::CompiledWorkflow) -> serde_json::Value {
    serde_json::from_str(&compiled.canonical_json().expect("compiled JSON")).expect("JSON value")
}

#[test]
fn repeat_reuses_one_upstream_value() {
    let (_directory, workflow) = project(
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\nclips:\n  doubled:\n    - $later\n    - repeat: 3\n  later:\n    image:\n      path: a.ppm\n      duration: 1s\ntimeline:\n  - $doubled\n",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(compiled.root_domain().expect("known domain").frames.0, 30);
    let json = compiled_json(&compiled);
    let repeat = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["origin"]["construct"] == "repeat")
        .expect("repeat concat");
    let inputs = repeat["kind"]["inputs"].as_array().expect("inputs");
    assert_eq!(inputs.len(), 3);
    assert!(inputs.iter().all(|input| input == &inputs[0]));
}

#[test]
fn then_keeps_its_input_physically_present() {
    let (_directory, workflow) = project(
        "version: 1\ntimeline:\n  - image:\n      path: a.ppm\n      duration: 1s\n  - then:\n      - image:\n          path: b.ppm\n          duration: 1s\n",
    );
    let error = compiler::compile(&workflow).expect_err("two outputs");
    assert_eq!(error.code, "E_BODY_OUTPUT_COUNT");
}

#[test]
fn during_changes_duration() {
    let (_directory, workflow) = project(
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\ntimeline:\n  - image:\n      path: a.ppm\n      duration: 10s\n  - repeat: 2\n    during: 4s..6s\n",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(compiled.root_domain().expect("known domain").frames.0, 120);
    let json = compiled_json(&compiled);
    assert!(
        json["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .any(|node| node["kind"]["operation"] == "replace_range")
    );
}

#[test]
fn explicit_and_postfix_during_have_the_same_semantics() {
    let source = |during: &str| {
        format!(
            "version: 1\nproject:\n  video: {{width: 64, height: 64, fps: 10}}\ntimeline:\n  - image:\n      path: a.ppm\n      duration: 10s\n{during}\n"
        )
    };
    let (_postfix_directory, postfix) = project(&source("  - repeat: 2\n    during: 4s..6s"));
    let (_explicit_directory, explicit) = project(&source(
        "  - during:\n      range: 4s..6s\n      body:\n        - repeat: 2",
    ));
    assert_eq!(
        compiler::compile(&postfix)
            .expect("postfix")
            .structure_hash(),
        compiler::compile(&explicit)
            .expect("explicit")
            .structure_hash()
    );
}

#[test]
fn postfix_id_names_the_outer_during_result() {
    let (_directory, workflow) = project(
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\ntimeline:\n  - image:\n      path: a.ppm\n      duration: 10s\n  - repeat: 2\n    during: 4s..6s\n    id: edited\n",
    );
    let document = compiled_json(&compiler::compile(&workflow).expect("compile"));
    let edited = usize::try_from(
        document["named_values"]["edited"]["id"]
            .as_u64()
            .expect("edited node id"),
    )
    .expect("node id fits usize");
    assert_eq!(
        document["nodes"][edited]["kind"]["operation"],
        "replace_range"
    );
}

#[test]
fn during_does_not_hide_selected_input_from_a_source() {
    let (_directory, workflow) = project(
        "version: 1\ntimeline:\n  - image:\n      path: a.ppm\n      duration: 10s\n  - image:\n      path: b.ppm\n      duration: 2s\n    during: 4s..6s\n",
    );
    let error = compiler::compile(&workflow).expect_err("selected plus source");
    assert_eq!(error.code, "E_BODY_OUTPUT_COUNT");
}

#[test]
fn join_reduces_only_the_top_two_outer_values() {
    let (_directory, workflow) = project(
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\ntimeline:\n  - image:\n      path: a.ppm\n      duration: 1s\n  - image:\n      path: b.ppm\n      duration: 1s\n  - image:\n      path: c.ppm\n      duration: 1s\n  - join:\n      - concat\n",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(compiled.root_domain().expect("known domain").frames.0, 30);
    let json = compiled_json(&compiled);
    assert_eq!(
        json["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .filter(|node| node["kind"]["operation"] == "concat")
            .count(),
        2
    );
}

#[test]
fn explicit_inputs_do_not_consume_join_stack_occurrences() {
    let (_directory, workflow) = project(
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\nclips:\n  x: {image: {path: x.ppm, duration: 1s}}\n  y: {image: {path: y.ppm, duration: 1s}}\ntimeline:\n  - image:\n      path: a.ppm\n      duration: 1s\n  - image:\n      path: b.ppm\n      duration: 1s\n  - join:\n      - concat:\n          videos: [$x, $y]\n      - concat\n",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(compiled.root_domain().expect("known domain").frames.0, 40);
}

#[test]
fn explicit_join_inputs_preserve_the_outer_stack() {
    let (_directory, workflow) = project(
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\nclips:\n  x: {image: {path: x.ppm, duration: 1s}}\n  y: {image: {path: y.ppm, duration: 1s}}\ntimeline:\n  - image: {path: a.ppm, duration: 1s}\n  - image: {path: b.ppm, duration: 1s}\n  - join:\n      before: $x\n      after: $y\n      body:\n        - concat\n",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(compiled.root_domain().expect("known domain").frames.0, 40);
}

#[test]
fn partial_explicit_join_binding_uses_the_preceding_value() {
    let (_directory, workflow) = project(
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\nclips:\n  y:\n    image:\n      path: y.ppm\n      duration: 1s\ntimeline:\n  - image:\n      path: a.ppm\n      duration: 1s\n  - join:\n      after: $y\n      body:\n        - concat\n",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(compiled.root_domain().expect("known domain").frames.0, 20);
}

#[test]
fn named_clip_does_not_receive_timeline_finalization() {
    let (_directory, workflow) = project(
        "version: 1\nclips:\n  pair:\n    - {image: {path: a.ppm, duration: 1s}}\n    - {image: {path: b.ppm, duration: 1s}}\ntimeline:\n  - $pair\n",
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
fn mapping_order_does_not_change_compiled_structure() {
    let (_first_dir, first) = project(
        "version: 1\nclips:\n  a: {image: {path: a.ppm, duration: 1s}}\n  b: {image: {path: b.ppm, duration: 1s}}\ntimeline: [$a, $b]\n",
    );
    let (_second_dir, second) = project(
        "version: 1\nclips:\n  b: {image: {duration: 1s, path: b.ppm}}\n  a: {image: {duration: 1s, path: a.ppm}}\ntimeline: [$a, $b]\n",
    );
    let first_compiled = compiler::compile(&first).expect("first");
    let second_compiled = compiler::compile(&second).expect("second");
    assert_eq!(
        first_compiled.structure_hash(),
        second_compiled.structure_hash()
    );
}

#[test]
fn postfix_mapping_order_does_not_change_wrapper_direction() {
    let source = |item: &str| {
        format!(
            "version: 1\nproject:\n  video: {{width: 64, height: 64, fps: 10}}\ntimeline:\n  - image:\n      path: a.ppm\n      duration: 3s\n{item}\n"
        )
    };
    let (_first_directory, first) = project(&source("  - repeat: 2\n    during: 1s..2s"));
    let (_second_directory, second) = project(&source("  - during: 1s..2s\n    repeat: 2"));
    let first = compiler::compile(&first).expect("head first");
    let second = compiler::compile(&second).expect("wrapper first");
    assert_eq!(first.root_domain(), second.root_domain());
    assert_eq!(first.structure_hash(), second.structure_hash());
}

#[test]
fn root_and_nested_timeline_have_the_same_semantics() {
    let clips = "clips:\n  a: {image: {path: a.ppm, duration: 1s}}\n  b: {image: {path: b.ppm, duration: 1s}}\n";
    let (_root_directory, root) =
        project(&format!("version: 1\n{clips}timeline:\n  - $a\n  - $b\n"));
    let (_nested_directory, nested) = project(&format!(
        "version: 1\n{clips}timeline:\n  - timeline:\n      - $a\n      - $b\n"
    ));
    let root = compiler::compile(&root).expect("root timeline");
    let nested = compiler::compile(&nested).expect("nested timeline");
    assert_eq!(root.root_domain(), nested.root_domain());
    assert_eq!(root.structure_hash(), nested.structure_hash());
}

#[test]
fn compile_file_accepts_an_outputless_validation_workflow() {
    let (directory, _workflow) =
        project("version: 1\ntimeline:\n  - image:\n      path: a.ppm\n      duration: 1s\n");
    compiler::compile_file(&directory.path().join(Path::new("workflow.yaml"))).expect("compile");
}

#[test]
fn comments_do_not_change_structure_hash() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("workflow.yaml");
    let first = clipasm::syntax::parse_str(
        &path,
        "version: 1\ntimeline:\n  - image:\n      path: a.ppm\n      duration: 1s\n",
    )
    .expect("first parse");
    let second = clipasm::syntax::parse_str(
        &path,
        "# formatting is not semantic\nversion: 1\ntimeline:\n  - image:\n      duration: 1s\n      path: a.ppm\n",
    )
    .expect("second parse");
    assert_eq!(
        compiler::compile(&first).expect("first").structure_hash(),
        compiler::compile(&second).expect("second").structure_hash()
    );
}
