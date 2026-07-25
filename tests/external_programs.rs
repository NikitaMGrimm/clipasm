#![allow(missing_docs)]

use std::fs;

use clipasm::compiler::EntrypointBindings;
use clipasm::source::SourceSpan;
use clipasm::{compiler, language, preflight};

fn write_external_program(directory: &std::path::Path, command: &str) {
    fs::write(
        directory.join("effect.clipasm"),
        format!(
            "clipasm 1\ninput video: Video\nparam amount: Integer\nexternal {{\n  command = {command:?}\n  semantic_version = 1\n  preserve = video\n}}\n"
        ),
    )
    .expect("external program");
}

fn write_workflow(directory: &std::path::Path) -> std::path::PathBuf {
    let workflow = directory.join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig {\n  output = \"result.mp4\"\n}\nimport \"effect.clipasm\" as effect\nimage(\"card.png\", 1s)\neffect(12)\n",
    )
    .expect("workflow");
    workflow
}

#[test]
fn compilation_registers_external_programs_without_resolving_the_executable() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_external_program(directory.path(), "./missing-script");
    fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/assets/morning.png"),
        directory.path().join("card.png"),
    )
    .expect("image");
    let workflow = write_workflow(directory.path());

    let package = language::parse_file(&workflow).expect("parse external registration");
    let compiled = compiler::compile(&package).expect("pure external compilation");
    let document: serde_json::Value =
        serde_json::from_str(&compiled.compiled_json().expect("compiled JSON"))
            .expect("JSON document");
    assert_eq!(document["nodes"][1]["kind"]["operation"], "external_video");
    assert_eq!(document["nodes"][1]["kind"]["parameters"]["amount"], 12);

    let error = preflight::preflight(&compiled).expect_err("missing executable");
    assert_eq!(error.code, "E_EXTERNAL_EXECUTABLE");
}

#[test]
fn string_parsing_accepts_external_program_definitions() {
    let source = "clipasm 1\ninput video: Video\nexternal {\n  command = \"./effect\"\n  semantic_version = 1\n  preserve = video\n}\n";
    language::parse_str(std::path::Path::new("effect.clipasm"), source)
        .expect("native external program");
}

#[test]
fn external_program_defaults_are_passed_to_the_runtime_invocation() {
    let source = "clipasm 1\ninput video: Video\nparam amount: Integer = 15\nexternal {\n  command = \"./effect\"\n  semantic_version = 1\n  preserve = video\n}\n";
    let package = language::parse_str(std::path::Path::new("effect.clipasm"), source)
        .expect("native external program");
    let mut bindings = EntrypointBindings::new();
    bindings
        .bind_video_input(
            "video",
            "input.mp4",
            SourceSpan::file_start("caller.clipasm"),
        )
        .expect("root video binding");

    let compiled =
        compiler::compile_with_bindings(&package, &bindings).expect("external root compilation");
    let document: serde_json::Value =
        serde_json::from_str(&compiled.compiled_json().expect("compiled JSON"))
            .expect("JSON document");
    assert_eq!(document["nodes"][1]["kind"]["operation"], "external_video");
    assert_eq!(document["nodes"][1]["kind"]["parameters"]["amount"], 15);
}

#[test]
fn external_programs_reject_bodies_unknown_preserve_inputs_and_unsupported_parameters() {
    let body = language::parse_str(
        std::path::Path::new("effect.clipasm"),
        "clipasm 1\ninput video: Video\nexternal {\n  command = \"./effect\"\n  semantic_version = 1\n  preserve = video\n}\n$video\n",
    )
    .expect_err("external body");
    assert_eq!(body.code, "E_EXTERNAL_WITH_BODY");

    let unknown = language::parse_str(
        std::path::Path::new("effect.clipasm"),
        "clipasm 1\ninput video: Video\nexternal {\n  command = \"./effect\"\n  semantic_version = 1\n  preserve = missing\n}\n",
    )
    .expect("syntax and lowering");
    let unknown = compiler::compile(&unknown).expect_err("unknown preserve input");
    assert_eq!(unknown.code, "E_INVALID_EXTERNAL_PROGRAM");

    let unsupported = language::parse_str(
        std::path::Path::new("effect.clipasm"),
        "clipasm 1\ninput video: Video\nparam lut: File\nexternal {\n  command = \"./effect\"\n  semantic_version = 1\n  preserve = video\n}\n",
    )
    .expect("syntax and lowering");
    let unsupported = compiler::compile(&unsupported).expect_err("unsupported parameter");
    assert_eq!(unsupported.code, "E_INVALID_EXTERNAL_PROGRAM");
}

#[test]
fn external_programs_reject_imports_in_favor_of_wrapper_programs() {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(directory.path().join("helper.clipasm"), "clipasm 1\n").expect("helper program");
    let effect = directory.path().join("effect.clipasm");
    fs::write(
        &effect,
        "clipasm 1\nimport \"helper.clipasm\" as helper\ninput video: Video\nexternal {\n  command = \"./effect\"\n  semantic_version = 1\n  preserve = video\n}\n",
    )
    .expect("external program");

    let error = language::parse_file(&effect).expect_err("external imports");
    assert_eq!(error.code, "E_EXTERNAL_WITH_IMPORTS");
}
