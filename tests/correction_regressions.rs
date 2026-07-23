use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rhythmcut::preflight::PreparedNodeKind;
fn write_image(directory: &Path, name: &str, color: &str) {
    fs::write(directory.join(name), format!("P3\n1 1\n255\n{color}\n"))
        .expect("write image fixture");
}

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rhythmcut"))
        .args(arguments)
        .output()
        .expect("run rhythmcut")
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
    let compiled = rhythmcut::compiler::compile_file(&workflow).expect("pure compile");
    let error = rhythmcut::preflight::preflight(&compiled).expect_err("missing asset preflight");
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
fn relocated_identical_projects_have_equal_semantic_hashes() {
    let first = tempfile::tempdir().expect("first directory");
    let second = tempfile::tempdir().expect("second directory");
    for directory in [first.path(), second.path()] {
        write_image(directory, "card.ppm", "255 0 0");
        fs::write(
            directory.join("workflow.yaml"),
            "version: 1\ntimeline:\n  - image:\n      path: card.ppm\n      duration: 1s\noutput: final.mp4\n",
        )
        .expect("workflow");
    }

    let first_compiled =
        rhythmcut::compiler::compile_file(&first.path().join("workflow.yaml")).expect("compile");
    let second_compiled =
        rhythmcut::compiler::compile_file(&second.path().join("workflow.yaml")).expect("compile");
    let first_prepared = rhythmcut::preflight::preflight(&first_compiled).expect("preflight");
    let second_prepared = rhythmcut::preflight::preflight(&second_compiled).expect("preflight");
    assert_eq!(
        first_prepared.semantic_hash(),
        second_prepared.semantic_hash()
    );
}

#[test]
fn unused_named_values_are_absent_from_executable_nodes() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "used.ppm", "255 0 0");
    write_image(directory.path(), "unused.ppm", "0 255 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "version: 1\nclips:\n  unused:\n    image:\n      path: unused.ppm\n      duration: 1s\ntimeline:\n  - image:\n      path: used.ppm\n      duration: 1s\noutput: final.mp4\n",
    )
    .expect("workflow");

    let compiled = rhythmcut::compiler::compile_file(&workflow).expect("compile");
    let plan = rhythmcut::preflight::preflight(&compiled).expect("preflight");
    let image_nodes = plan
        .nodes()
        .iter()
        .filter(|node| matches!(node.kind(), PreparedNodeKind::ImageVideo { .. }))
        .count();
    assert_eq!(image_nodes, 1);
}

#[test]
fn preflight_hashes_assets_and_render_rejects_later_changes() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\ntimeline:\n  - image:\n      path: card.ppm\n      duration: 1s\noutput: final.mp4\n",
    )
    .expect("workflow");
    let compiled = rhythmcut::compiler::compile_file(&workflow).expect("compile");
    let prepared = rhythmcut::preflight::preflight(&compiled).expect("preflight");
    let PreparedNodeKind::ImageVideo { asset, .. } = prepared.nodes()[0].kind() else {
        panic!("prepared image");
    };
    assert_eq!(asset.content_hash().len(), 64);

    fs::write(directory.path().join("card.ppm"), b"changed").expect("change asset");
    fs::write(directory.path().join("final.mp4"), b"existing output").expect("existing output");
    let error = rhythmcut::render::render(&prepared).expect_err("changed asset");
    assert_eq!(error.code, "E_ASSET_CHANGED");
    assert_eq!(
        fs::read(directory.path().join("final.mp4")).expect("preserved output"),
        b"existing output"
    );
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
    let error = rhythmcut::compiler::compile_file(&workflow).expect_err("unused invalid clip");
    assert_eq!(error.code, "E_MISSING_IMAGE_DURATION");
}

#[test]
fn backend_export_constraints_do_not_leak_into_pure_compilation() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "version: 1\nproject:\n  video: {width: 63, height: 65, fps: 10}\ntimeline:\n  - image:\n      path: card.ppm\n      duration: 1s\noutput: final.mp4\n",
    )
    .expect("workflow");
    let compiled = rhythmcut::compiler::compile_file(&workflow).expect("pure compile");
    let error = rhythmcut::preflight::preflight(&compiled).expect_err("export dimensions");
    assert_eq!(error.code, "E_EXPORT_DIMENSIONS");
}

#[test]
fn output_extension_is_strictly_mp4() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "version: 1\ntimeline:\n  - image:\n      path: missing.png\n      duration: 1s\noutput: final.mov\n",
    )
    .expect("workflow");
    let compiled = rhythmcut::compiler::compile_file(&workflow).expect("compile");
    let error = rhythmcut::preflight::preflight(&compiled).expect_err("extension");
    assert_eq!(error.code, "E_INVALID_OUTPUT_EXTENSION");
}

#[test]
fn output_path_fixture_supports_non_utf8_components_without_display_roundtrip() {
    #[cfg(unix)]
    {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let name = OsString::from_vec(b"video-\xFF.mp4".to_vec());
        let path = PathBuf::from(name);
        assert_eq!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("mp4")
        );
    }
}
