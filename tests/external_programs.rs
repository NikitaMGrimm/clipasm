#![allow(missing_docs)]

mod common;

use std::fs;

use clipasm::compiler::EntrypointBindings;
use clipasm::source::SourceSpan;
use clipasm::{compiler, language, preflight, render};

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
fn compilation_links_external_programs_without_resolving_the_executable() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_external_program(directory.path(), "./missing-script");
    fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/assets/morning.png"),
        directory.path().join("card.png"),
    )
    .expect("image");
    let workflow = write_workflow(directory.path());

    let package = language::parse_file(&workflow).expect("parse external program");
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
        "clipasm 1\ninput video: Video\nparam duration: Duration\nexternal {\n  command = \"./effect\"\n  semantic_version = 1\n  preserve = video\n}\n",
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

#[cfg(unix)]
#[test]
fn publication_paths_cannot_collide_with_external_executables() {
    use std::os::unix::fs::PermissionsExt as _;

    if !common::media_tools_available() {
        return;
    }

    for (executable_name, output, expected_code) in [
        ("effect.mp4", "effect.mp4", "E_OUTPUT_COLLISION"),
        (
            "result.mp4.manifest.json",
            "result.mp4",
            "E_MANIFEST_COLLISION",
        ),
    ] {
        let directory = tempfile::tempdir().expect("temporary directory");
        let executable = directory.path().join(executable_name);
        fs::write(&executable, "#!/bin/sh\nexit 0\n").expect("external executable");
        let mut permissions = fs::metadata(&executable).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("executable permissions");
        fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/assets/morning.png"),
            directory.path().join("card.png"),
        )
        .expect("image");
        let command = format!("./{executable_name}");
        fs::write(
            directory.path().join("effect.clipasm"),
            format!(
                "clipasm 1\ninput video: Video\nexternal {{\n  command = {command:?}\n  semantic_version = 1\n  preserve = video\n}}\n"
            ),
        )
        .expect("external source");
        let workflow = directory.path().join("workflow.clipasm");
        fs::write(
            &workflow,
            format!(
                "clipasm 1\nconfig {{ output = {output:?} }}\nimport \"effect.clipasm\" as effect\nimage(\"card.png\", 1s)\neffect\n"
            ),
        )
        .expect("workflow");

        let package = language::parse_file(&workflow).expect("external package");
        let compiled = compiler::compile(&package).expect("pure compilation");
        let error = preflight::preflight(&compiled).expect_err("executable collision");
        assert_eq!(error.code, expected_code);
        assert!(executable.is_file(), "preflight must preserve executable");
    }
}

#[cfg(unix)]
#[test]
fn external_file_parameters_are_resolved_and_hashed_during_preflight() {
    use std::os::unix::fs::PermissionsExt as _;

    if !common::media_tools_available() {
        eprintln!("skipping external file preflight test because FFmpeg is unavailable");
        return;
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    let executable = directory.path().join("effect.sh");
    fs::write(&executable, "#!/bin/sh\nexit 0\n").expect("external executable");
    let mut permissions = fs::metadata(&executable).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).expect("executable permissions");
    fs::write(directory.path().join("lut.bin"), b"lookup table").expect("file parameter");
    fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/assets/morning.png"),
        directory.path().join("card.png"),
    )
    .expect("image");
    fs::write(
        directory.path().join("effect.clipasm"),
        "clipasm 1\ninput video: Video\nparam lut: File = \"lut.bin\"\nexternal {\n  command = \"./effect.sh\"\n  semantic_version = 1\n  preserve = video\n}\n",
    )
    .expect("external source");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { output = \"result.mp4\" }\nimport \"effect.clipasm\" as effect\nimage(\"card.png\", 1s)\neffect\n",
    )
    .expect("workflow");

    let package = language::parse_file(&workflow).expect("external package");
    let compiled = compiler::compile(&package).expect("pure compilation");
    let prepared = preflight::preflight(&compiled).expect("prepared external file");
    let Some(preflight::PreparedVideoKind::ExternalVideo { parameters, .. }) =
        prepared.nodes().last().and_then(|node| node.video_kind())
    else {
        panic!("external prepared node");
    };
    let Some(preflight::PreparedExternalParameterValue::File(asset)) = parameters.get("lut") else {
        panic!("prepared file parameter");
    };
    assert_eq!(asset.source_path(), directory.path().join("lut.bin"));
    assert!(!asset.content_hash().is_empty());

    fs::write(directory.path().join("lut.bin"), b"changed lookup table")
        .expect("change file parameter");
    let error = render::render(&prepared).expect_err("changed file parameter");
    assert_eq!(error.code, "E_ASSET_CHANGED");
}
