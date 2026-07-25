#![allow(missing_docs)]

use std::fs;
use std::process::Command;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("image");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nglue {\n  image(\"card.ppm\", 1s)\n}\n",
    )
    .expect("workflow");
    (directory, workflow)
}

#[test]
fn compile_prints_machine_readable_plan() {
    let (_directory, workflow) = fixture();
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args(["compile", workflow.to_str().expect("UTF-8 path")])
        .output()
        .expect("run clipasm");
    assert!(output.status.success());
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("plan JSON");
    assert!(plan["structure_hash"].as_str().is_some());
    assert_eq!(plan["nodes"][0]["kind"]["operation"], "image_video");
}

#[test]
fn compile_writes_an_explicit_plan_path() {
    let (directory, workflow) = fixture();
    let plan_path = directory.path().join("plan.json");
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args([
            "compile",
            workflow.to_str().expect("UTF-8 path"),
            "--output",
            plan_path.to_str().expect("UTF-8 path"),
        ])
        .output()
        .expect("run clipasm");
    assert!(output.status.success());
    assert!(plan_path.is_file());
}

#[test]
fn compile_refuses_to_replace_an_existing_file() {
    let (_directory, workflow) = fixture();
    let original = fs::read(&workflow).expect("original workflow");
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args([
            "compile",
            workflow.to_str().expect("UTF-8 path"),
            "--output",
            workflow.to_str().expect("UTF-8 path"),
        ])
        .output()
        .expect("run clipasm");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[E_PLAN_EXISTS]"));
    assert_eq!(fs::read(&workflow).expect("preserved workflow"), original);
}

#[test]
fn compile_preserves_an_existing_plan_destination() {
    let (directory, workflow) = fixture();
    let plan = directory.path().join("plan.json");
    fs::write(&plan, b"existing plan").expect("existing plan");

    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args([
            "compile",
            workflow.to_str().expect("UTF-8 path"),
            "--output",
            plan.to_str().expect("UTF-8 path"),
        ])
        .output()
        .expect("run clipasm");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[E_PLAN_EXISTS]"));
    assert_eq!(fs::read(&plan).expect("preserved plan"), b"existing plan");
}

#[test]
fn diagnostics_produce_a_failure_exit_code() {
    let (directory, workflow) = fixture();
    fs::write(&workflow, "clipasm 1\nglue {\n  repeat(2)\n}\n").expect("invalid workflow");
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args(["validate", workflow.to_str().expect("UTF-8 path")])
        .output()
        .expect("run clipasm");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[E_STACK_UNDERFLOW]"));
    drop(directory);
}

#[test]
fn cli_rejects_non_clipasm_source_paths() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.txt");
    fs::write(&workflow, "clipasm 1\n").expect("source");

    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args(["validate", workflow.to_str().expect("UTF-8 path")])
        .output()
        .expect("run clipasm");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[E_SOURCE_EXTENSION]"));
}

#[test]
fn validate_reports_a_deferred_video_duration_without_opening_the_asset() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nglue {\n  video(\"missing.mp4\")\n}\n",
    )
    .expect("workflow");

    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args(["validate", workflow.to_str().expect("UTF-8 path")])
        .output()
        .expect("run clipasm");
    assert!(
        output.status.success(),
        "validation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("duration resolves during preflight"));
}

#[test]
fn compile_binds_root_video_inputs_and_typed_parameters() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("template.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\ninput source: Video\nparam range: TimeRange\nparam count: Integer\ntrim($source, $range)\nrepeat($count)\n",
    )
    .expect("template");

    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .current_dir(directory.path())
        .args([
            "compile",
            "template.clipasm",
            "--input",
            "source=footage.mp4",
            "--arg",
            "range=1s..2s",
            "--arg",
            "count=2",
        ])
        .output()
        .expect("run clipasm");
    assert!(
        output.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("plan JSON");
    let operations = plan["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|node| node["kind"]["operation"].as_str().expect("operation"))
        .collect::<Vec<_>>();
    assert_eq!(
        operations,
        vec!["video_source", "reference", "slice", "repeat"]
    );
    assert_eq!(plan["nodes"][0]["kind"]["path"], "footage.mp4");
}

#[test]
fn root_cli_bindings_reject_unknown_and_duplicate_names() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("template.clipasm");
    fs::write(&workflow, "clipasm 1\ninput source: Video\n$source\n").expect("template");

    let unknown = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .current_dir(directory.path())
        .args([
            "validate",
            "template.clipasm",
            "--input",
            "other=footage.mp4",
        ])
        .output()
        .expect("run clipasm");
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("[E_UNKNOWN_PROGRAM_ARGUMENT]"));

    let duplicate = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .current_dir(directory.path())
        .args([
            "validate",
            "template.clipasm",
            "--input",
            "source=first.mp4",
            "--input",
            "source=second.mp4",
        ])
        .output()
        .expect("run clipasm");
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("[E_DUPLICATE_ARGUMENT]"));
}

#[test]
fn render_accepts_caller_relative_input_and_output_bindings() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("template.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\ninput source: Video\nparam count: Integer\nparam overlay: File\nrepeat($source, $count)\nimage($overlay, 1s)\nconcat\n",
    )
    .expect("template");
    fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/assets/gentle-motion.mkv"),
        directory.path().join("input.mkv"),
    )
    .expect("copy video fixture");
    fs::write(
        directory.path().join("overlay.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("overlay fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .current_dir(directory.path())
        .args([
            "render",
            "template.clipasm",
            "--input",
            "source=input.mkv",
            "--arg",
            "count=1",
            "--arg",
            "overlay=overlay.ppm",
            "--output",
            "result.mp4",
        ])
        .output()
        .expect("run clipasm");
    assert!(
        output.status.success(),
        "render failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(directory.path().join("result.mp4").is_file());
    assert!(directory.path().join("result.mp4.manifest.json").is_file());
}
