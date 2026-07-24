#![allow(missing_docs)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn compile_yaml(path: &Path) -> clipasm::diagnostic::Result<clipasm::compiler::CompiledProgram> {
    let source = clipasm::frontend::yaml::parse_file(path)?;
    clipasm::compiler::compile(&source)
}

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
fn source_program_body_returns_one_video_without_an_implicit_glue() {
    let source = "- program:\n    version: 1\n\n- image: {path: card.ppm, duration: 1s}\n";
    let program = clipasm::frontend::yaml::parse_str(Path::new("program.yaml"), source)
        .expect("source program syntax");
    let compiled = clipasm::compiler::compile(&program).expect("source program result");

    assert_eq!(compiled.result_domain().expect("known domain").frames.0, 30);
}

#[test]
fn source_program_allows_zero_or_multiple_outputs_without_publication() {
    let empty = clipasm::frontend::yaml::parse_str(Path::new("empty.yaml"), "- program:\n    version: 1\n")
        .expect("empty source syntax");
    let compiled = clipasm::compiler::compile(&empty).expect("zero outputs");
    assert!(compiled.outputs().is_empty());

    let multiple = clipasm::frontend::yaml::parse_str(
        Path::new("multiple.yaml"),
        "- program:\n    version: 1\n\n- image: {path: a.png, duration: 1s}\n- image: {path: b.png, duration: 1s}\n",
    )
    .expect("multiple-value source syntax");
    let compiled = clipasm::compiler::compile(&multiple).expect("multiple outputs");
    assert_eq!(compiled.outputs().len(), 2);
}

#[test]
fn source_output_publication_requires_exactly_one_video() {
    for (source, count) in [
        ("- program:\n    version: 1\n    output: final.mp4\n", 0),
        (
            "- program:\n    version: 1\n    output: final.mp4\n\n- image: {path: a.png, duration: 1s}\n- image: {path: b.png, duration: 1s}\n",
            2,
        ),
    ] {
        let program = clipasm::frontend::yaml::parse_str(Path::new("publish.yaml"), source)
            .expect("publication syntax");
        let error = clipasm::compiler::compile(&program).expect_err("invalid output count");
        assert_eq!(error.code, "E_ENTRYPOINT_OUTPUT_COUNT");
        assert!(error.message.contains(&count.to_string()));
    }
}

#[test]
fn source_program_header_is_required_first_and_rejects_unknown_fields() {
    let cases = [
        ("version: 1\nglue: []\n", "E_EXPECTED_SOURCE_PROGRAM"),
        ("- image: card.png\n", "E_MISSING_PROGRAM_HEADER"),
        (
            "- program:\n    version: 1\n\n- program:\n    version: 1\n",
            "E_MISPLACED_PROGRAM_HEADER",
        ),
        (
            "- program:\n    version: 1\n    unknown_field: true\n",
            "E_UNKNOWN_PROGRAM_HEADER_FIELD",
        ),
    ];

    for (source, expected_code) in cases {
        let error = clipasm::frontend::yaml::parse_str(Path::new("invalid.yaml"), source)
            .expect_err("invalid source program");
        assert_eq!(error.code, expected_code);
    }
}

#[test]
fn stack_access_is_generic_source_and_invocation_metadata() {
    let source = "- program:\n    version: 1\n    stack_access: visible\n\n- image:\n    path: card.ppm\n    duration: 1s\n    stack_access: visible\n";
    let program =
        clipasm::frontend::yaml::parse_str(Path::new("program.yaml"), source).expect("stack metadata");
    clipasm::compiler::compile(&program).expect("no-op visible image");

    for source in [
        "- program:\n    version: 1\n    stack_access: inherited\n",
        "- program:\n    version: 1\n\n- image:\n    path: card.ppm\n    duration: 1s\n    stack_access: inherited\n",
    ] {
        let error = clipasm::frontend::yaml::parse_str(Path::new("invalid.yaml"), source)
            .expect_err("invalid stack access");
        assert_eq!(error.code, "E_INVALID_STACK_ACCESS");
    }
}

#[test]
fn compiled_program_serializes_ordered_outputs() {
    let source = "- program:\n    version: 1\n\n- image: {path: card.ppm, duration: 1s}\n";
    let program =
        clipasm::frontend::yaml::parse_str(Path::new("program.yaml"), source).expect("source program");
    let compiled = clipasm::compiler::compile(&program).expect("compiled program");
    let document: serde_json::Value =
        serde_json::from_str(&compiled.canonical_json().expect("compiled JSON")).expect("JSON");

    assert_eq!(document["outputs"].as_array().expect("outputs").len(), 1);
    assert_eq!(document["format_version"], 8);
    assert_eq!(compiled.result_domain().expect("known result").frames.0, 30);
}

#[test]
fn source_output_order_changes_compiled_identity() {
    let source = |first: &str, second: &str| {
        format!(
            "- program:\n    version: 1\n\n- image: {{path: {first}, duration: 1s}}\n- image: {{path: {second}, duration: 1s}}\n"
        )
    };
    let first = clipasm::frontend::yaml::parse_str(
        Path::new("program.yaml"),
        &source("first.png", "second.png"),
    )
    .expect("first order");
    let second = clipasm::frontend::yaml::parse_str(
        Path::new("program.yaml"),
        &source("second.png", "first.png"),
    )
    .expect("second order");

    assert_ne!(
        clipasm::compiler::compile(&first)
            .expect("first compile")
            .structure_hash(),
        clipasm::compiler::compile(&second)
            .expect("second compile")
            .structure_hash()
    );
}

#[test]
fn id_and_ids_are_mutually_exclusive_and_ids_requires_a_sequence() {
    for (source, code) in [
        (
            "- program:\n    version: 1\n\n- image: {path: card.png, duration: 1s}\n  id: card\n  ids: [other]\n",
            "E_DUPLICATE_OUTPUT_BINDING",
        ),
        (
            "- program:\n    version: 1\n\n- image: {path: card.png, duration: 1s}\n  ids: card\n",
            "E_INVALID_OUTPUT_BINDING",
        ),
        (
            "- program:\n    version: 1\n\n- image: {path: card.png, duration: 1s}\n  ids: []\n",
            "E_INVALID_OUTPUT_BINDING",
        ),
    ] {
        let error = clipasm::frontend::yaml::parse_str(Path::new("invalid.yaml"), source)
            .expect_err("invalid output binding syntax");
        assert_eq!(error.code, code);
    }
}

#[test]
fn explain_entries_expose_authored_names_and_source_locations() {
    let source =
        "- program:\n    version: 1\n\n- image: {path: card.ppm, duration: 1s}\n  id: card\n";
    let program =
        clipasm::frontend::yaml::parse_str(Path::new("program.yaml"), source).expect("source program");
    let compiled = clipasm::compiler::compile(&program).expect("compiled program");
    let entry = compiled.explain().last().expect("explain entry");

    assert_eq!(entry.construct(), "image");
    assert_eq!(entry.outputs().len(), 1);
    assert_eq!(entry.outputs()[0].id(), Some("card"));
    assert_eq!(entry.span().file(), Path::new("program.yaml"));
    assert_eq!(entry.span().line, 4);
}

#[test]
fn entrypoint_output_does_not_change_compiled_semantics() {
    let source = |output: &str| {
        format!(
            "- program:\n    version: 1\n    output: {output}\n\n- image: {{path: card.ppm, duration: 1s}}\n"
        )
    };
    let first = clipasm::frontend::yaml::parse_str(Path::new("program.yaml"), &source("first.mp4"))
        .expect("first source program");
    let second = clipasm::frontend::yaml::parse_str(Path::new("program.yaml"), &source("second.mp4"))
        .expect("second source program");

    assert_eq!(
        clipasm::compiler::compile(&first)
            .expect("first compile")
            .structure_hash(),
        clipasm::compiler::compile(&second)
            .expect("second compile")
            .structure_hash()
    );
}

#[test]
fn variadic_inputs_remain_reference_only() {
    let source = clipasm::frontend::yaml::parse_str(
        Path::new("program.yaml"),
        "- program:\n    version: 1\n\n- concat:\n    videos:\n      - image: {path: card.ppm, duration: 1s}\n",
    )
    .expect("canonical source");
    let error = clipasm::compiler::compile(&source).expect_err("variadic inline body");

    assert_eq!(error.code, "E_INVALID_ARGUMENT_TYPE");
    assert!(error.message.contains("variadic") && error.message.contains("references"));
}

#[test]
fn pure_compile_does_not_require_assets_to_exist() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    output: final.mp4\n\n\n- glue:\n    - image:\n        path: missing.png\n        duration: 1s",
    )
    .expect("workflow");

    let output = run(&["compile", workflow.to_str().expect("UTF-8 fixture path")]);
    assert!(
        output.status.success(),
        "pure compile unexpectedly accessed the asset: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let compiled = compile_yaml(&workflow).expect("pure compile");
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
        "- program:\n    version: 1\n\n- glue:\n    - image: card.ppm\n      duration: 1s\n  ",
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
fn unknown_program_reports_a_diagnostic() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n\n- not_registered_program:\n    - image: {path: card.ppm, duration: 1s}\n",
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
        "- program:\n    version: 1\n    clips:\n      card:\n        image:\n          path: card.ppm\n          duration: 1s\n\n- glue:\n    - $card\n  ",
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
}

#[test]
fn reducible_frame_rate_is_canonical() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 60/2}\n\n- glue:\n    - image:\n        path: card.ppm\n        duration: 1s\n  ",
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
        "- program:\n    version: 1\n    clips:\n      invalid:\n        image: unused.png\n\n- glue:\n    - image:\n        path: used.png\n        duration: 1s\n  ",
    )
    .expect("workflow");
    let error = compile_yaml(&workflow).expect_err("unused invalid clip");
    assert_eq!(error.code, "E_MISSING_IMAGE_DURATION");
}

#[test]
fn video_sources_compile_purely_with_a_deferred_media_domain() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- glue:\n    - video: missing.mp4\n  ",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("pure compile");
    assert!(compiled.result_domain().is_none());
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
        "- program:\n    version: 1\n\n- glue:\n    - video:\n        path: source.mp4\n        duration: 1s\n  ",
    )
    .expect("workflow");

    let error = compile_yaml(&workflow).expect_err("duration argument");
    assert_eq!(error.code, "E_UNKNOWN_PROGRAM_ARGUMENT");
}
