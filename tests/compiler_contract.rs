#![allow(missing_docs)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn write_image(directory: &Path, name: &str, color: &str) {
    fs::write(directory.join(name), format!("P3\n1 1\n255\n{color}\n"))
        .expect("write image fixture");
}

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args(arguments)
        .output()
        .expect("run clipasm")
}

fn compile_json(workflow: &Path) -> serde_json::Value {
    let output = run(&["compile", workflow.to_str().expect("UTF-8 fixture path")]);
    assert!(
        output.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("compiled JSON")
}

#[test]
fn pure_compile_does_not_require_assets_to_exist() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "version: 1\ntimeline:\n  - image:\n      path: missing.png\n      duration: 1s\noutput: final.mp4\n",
    )
    .expect("workflow");

    let output = run(&["compile", workflow.to_str().expect("UTF-8 fixture path")]);
    assert!(
        output.status.success(),
        "pure compile unexpectedly accessed the asset: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let compiled = clipasm::compiler::compile_file(&workflow).expect("pure compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("missing asset preflight");
    assert_eq!(error.code, "E_MISSING_IMAGE_FILE");
}

#[test]
fn sibling_program_parameters_are_rejected() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "version: 1\ntimeline:\n  - image: card.ppm\n    duration: 1s\n",
    )
    .expect("workflow");

    let output = run(&["validate", workflow.to_str().expect("UTF-8 fixture path")]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[E_UNKNOWN_INVOCATION_FIELD]"),
        "unexpected diagnostic: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn clip_is_not_a_public_program() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "version: 1\nclips:\n  card:\n    image:\n      path: card.ppm\n      duration: 1s\ntimeline:\n  - clip: $card\n",
    )
    .expect("workflow");

    let output = run(&["validate", workflow.to_str().expect("UTF-8 fixture path")]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[E_UNKNOWN_PROGRAM]"));
}

#[test]
fn references_are_explained_as_references() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "version: 1\nclips:\n  card:\n    image:\n      path: card.ppm\n      duration: 1s\ntimeline:\n  - $card\n",
    )
    .expect("workflow");

    let plan = compile_json(&workflow);
    let constructs = plan["explain"]
        .as_array()
        .expect("explain array")
        .iter()
        .filter_map(|entry| entry["construct"].as_str())
        .collect::<Vec<_>>();
    assert!(constructs.contains(&"reference"));
    assert!(!constructs.contains(&"clip"));
}

#[test]
fn reducible_frame_rate_is_canonical() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 60/2}\ntimeline:\n  - image:\n      path: card.ppm\n      duration: 1s\n",
    )
    .expect("workflow");

    let plan = compile_json(&workflow);
    assert_eq!(plan["video"]["fps"]["numerator"], 30);
    assert_eq!(plan["video"]["fps"]["denominator"], 1);
}

#[test]
fn unused_definitions_are_still_compiled_and_validated() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "version: 1\nclips:\n  invalid:\n    image: unused.png\ntimeline:\n  - image:\n      path: used.png\n      duration: 1s\n",
    )
    .expect("workflow");
    let error = clipasm::compiler::compile_file(&workflow).expect_err("unused invalid clip");
    assert_eq!(error.code, "E_MISSING_IMAGE_DURATION");
}

#[test]
fn video_sources_compile_purely_with_a_deferred_media_domain() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\ntimeline:\n  - video: missing.mp4\n",
    )
    .expect("workflow");

    let compiled = clipasm::compiler::compile_file(&workflow).expect("pure compile");
    assert!(compiled.root_domain().is_none());
    let document: serde_json::Value =
        serde_json::from_str(&compiled.canonical_json().expect("compiled JSON")).expect("JSON");
    assert_eq!(document["nodes"][0]["kind"]["operation"], "video_source");
    assert_eq!(document["nodes"][0]["kind"]["fit"], "cover");
    assert!(document["nodes"][0]["domain"].is_null());
}

#[test]
fn video_sources_do_not_accept_an_authored_duration() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "version: 1\ntimeline:\n  - video:\n      path: source.mp4\n      duration: 1s\n",
    )
    .expect("workflow");

    let error = clipasm::compiler::compile_file(&workflow).expect_err("duration argument");
    assert_eq!(error.code, "E_UNKNOWN_PROGRAM_ARGUMENT");
}
