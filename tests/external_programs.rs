#![allow(missing_docs)]

use std::fs;

use clipasm::{compiler, language, preflight};

fn write_manifest(directory: &std::path::Path, command: &str) {
    fs::write(
        directory.join("effect.json"),
        format!(
            r#"{{
  "format_version": 2,
  "protocol_version": 1,
  "semantic_version": 1,
  "command": {command:?},
  "inputs": [{{"name": "video", "type": "Video"}}],
  "parameters": [{{"name": "amount", "type": "Integer", "required": true}}],
  "output": {{"type": "Video", "preserve": "video"}}
}}"#
        ),
    )
    .expect("manifest");
}

fn write_workflow(directory: &std::path::Path) -> std::path::PathBuf {
    let workflow = directory.join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig {\n  output = \"result.mp4\"\n}\nexternal \"effect.json\" as effect\nimage(\"card.png\", 1s)\neffect(12)\n",
    )
    .expect("workflow");
    workflow
}

#[test]
fn compilation_registers_external_programs_without_resolving_the_executable() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_manifest(directory.path(), "./missing-script");
    fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/assets/morning.png"),
        directory.path().join("card.png"),
    )
    .expect("image");
    let workflow = write_workflow(directory.path());

    let package = language::parse_file(&workflow).expect("parse external registration");
    let compiled = compiler::compile(&package).expect("pure external compilation");
    let document: serde_json::Value =
        serde_json::from_str(&compiled.canonical_json().expect("compiled JSON"))
            .expect("JSON document");
    assert_eq!(document["nodes"][1]["kind"]["operation"], "external_video");
    assert_eq!(document["nodes"][1]["kind"]["parameters"]["amount"], 12);

    let error = preflight::preflight(&compiled).expect_err("missing executable");
    assert_eq!(error.code, "E_EXTERNAL_EXECUTABLE");
}

#[test]
fn string_parsing_rejects_external_manifest_loading() {
    let source = "clipasm 1\nexternal \"effect.json\" as effect\n";
    let error = language::parse_str(std::path::Path::new("workflow.clipasm"), source)
        .expect_err("external registration needs a file base");
    assert_eq!(error.code, "E_EXTERNAL_REQUIRES_FILE");
}
