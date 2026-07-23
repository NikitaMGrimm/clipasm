use std::fs;
use std::process::Command;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("image");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "version: 1\ntimeline:\n  - image:\n      path: card.ppm\n      duration: 1s\n",
    )
    .expect("workflow");
    (directory, workflow)
}

#[test]
fn compile_prints_machine_readable_plan() {
    let (_directory, workflow) = fixture();
    let output = Command::new(env!("CARGO_BIN_EXE_rhythmcut"))
        .args(["compile", workflow.to_str().expect("UTF-8 path")])
        .output()
        .expect("run rhythmcut");
    assert!(output.status.success());
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("plan JSON");
    assert!(plan["structure_hash"].as_str().is_some());
    assert_eq!(plan["nodes"][0]["kind"]["operation"], "image_video");
}

#[test]
fn compile_writes_an_explicit_plan_path() {
    let (directory, workflow) = fixture();
    let plan_path = directory.path().join("plan.json");
    let output = Command::new(env!("CARGO_BIN_EXE_rhythmcut"))
        .args([
            "compile",
            workflow.to_str().expect("UTF-8 path"),
            "--output",
            plan_path.to_str().expect("UTF-8 path"),
        ])
        .output()
        .expect("run rhythmcut");
    assert!(output.status.success());
    assert!(plan_path.is_file());
}

#[test]
fn diagnostics_produce_a_failure_exit_code() {
    let (directory, workflow) = fixture();
    fs::write(&workflow, "version: 1\ntimeline:\n  - repeat: 2\n").expect("invalid workflow");
    let output = Command::new(env!("CARGO_BIN_EXE_rhythmcut"))
        .args(["validate", workflow.to_str().expect("UTF-8 path")])
        .output()
        .expect("run rhythmcut");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[E_STACK_UNDERFLOW]"));
    drop(directory);
}
