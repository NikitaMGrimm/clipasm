#![allow(missing_docs)]

mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn compile_file(path: &Path) -> clipasm::diagnostic::Result<clipasm::compiler::CompiledProgram> {
    let source = clipasm::language::parse_file(path)?;
    clipasm::compiler::compile(&source)
}

fn compile_source(
    path: &Path,
    text: &str,
) -> clipasm::diagnostic::Result<clipasm::compiler::CompiledProgram> {
    let source = clipasm::language::parse_str(path, text)?;
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
    let output = run(&["inspect", workflow.to_str().expect("UTF-8 fixture path")]);
    assert!(
        output.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("compiled JSON")
}

#[cfg(unix)]
#[test]
fn non_utf8_bound_media_paths_have_distinct_semantic_identities() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;
    use std::path::PathBuf;

    let package = clipasm::language::parse_str(
        Path::new("template.clipasm"),
        "clipasm 1\ninput source: Video\n$source\n",
    )
    .expect("source");
    let compile = |bytes: &[u8]| {
        let mut bindings = clipasm::compiler::EntrypointBindings::new();
        bindings
            .bind_video_input(
                "source",
                PathBuf::from(OsString::from_vec(bytes.to_vec())),
                clipasm::source::SourceSpan::file_start("<test>"),
            )
            .expect("binding");
        clipasm::compiler::compile_with_bindings(&package, &bindings).expect("native path identity")
    };

    let first = compile(b"footage-\xff.mp4");
    let second = compile(b"footage-\xfe.mp4");

    assert_ne!(first.structure_hash(), second.structure_hash());
    assert_eq!(
        first
            .compiled_json()
            .expect_err("inspection JSON still requires representable paths")
            .code,
        "E_COMPILED_JSON"
    );
}

#[test]
fn source_program_body_returns_one_video_without_implicit_reduction() {
    let compiled = compile_source(
        Path::new("program.clipasm"),
        "clipasm 1\nimage(\"card.ppm\", 1s)\n",
    )
    .expect("source program result");

    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        30
    );
}

#[test]
fn source_program_allows_zero_or_multiple_outputs_without_publication() {
    let compiled = compile_source(Path::new("empty.clipasm"), "clipasm 1\n").expect("zero outputs");
    assert!(compiled.outputs().is_empty());

    let compiled = compile_source(
        Path::new("multiple.clipasm"),
        "clipasm 1\nimage(\"a.png\", 1s)\nimage(\"b.png\", 1s)\n",
    )
    .expect("multiple outputs");
    assert_eq!(compiled.outputs().len(), 2);
}

#[test]
fn source_output_publication_requires_exactly_one_video() {
    for (source, count) in [
        ("clipasm 1\nconfig { output = \"final.mp4\" }\n", 0),
        (
            "clipasm 1\nconfig { output = \"final.mp4\" }\nimage(\"a.png\", 1s)\nimage(\"b.png\", 1s)\n",
            2,
        ),
    ] {
        let error =
            compile_source(Path::new("publish.clipasm"), source).expect_err("invalid output count");
        assert_eq!(error.code, "E_ENTRYPOINT_OUTPUT_COUNT");
        assert!(error.message.contains(&count.to_string()));
    }
}

#[test]
fn native_version_and_declaration_order_are_enforced() {
    let cases = [
        ("image(\"card.png\", 1s)\n", "E_MISSING_VERSION"),
        ("clipasm 2\n", "E_UNSUPPORTED_VERSION"),
        (
            "clipasm 1\nimage(\"card.png\", 1s)\nparam count: Integer\n",
            "E_DECLARATION_AFTER_STATEMENT",
        ),
        (
            "clipasm 1\nconfig { unknown = 1 }\n",
            "E_UNKNOWN_CONFIG_FIELD",
        ),
    ];

    for (source, expected_code) in cases {
        let error = clipasm::language::parse_str(Path::new("invalid.clipasm"), source)
            .expect_err("invalid source program");
        assert_eq!(error.code, expected_code);
    }
}

#[test]
fn root_input_diagnostics_point_to_the_authored_declaration() {
    let missing = compile_source(
        Path::new("missing-input.clipasm"),
        "clipasm 1\ninput video: Video\n$video\n",
    )
    .expect_err("missing root input");
    assert_eq!(missing.code, "E_MISSING_REQUIRED_INPUT");
    assert_eq!((missing.span.line, missing.span.column), (2, 7));

    let duplicate = compile_source(
        Path::new("duplicate-input.clipasm"),
        "clipasm 1\ninput video: Video\ninput video: Video\n$video\n",
    )
    .expect_err("duplicate root input");
    assert_eq!(duplicate.code, "E_DUPLICATE_NAME");
    assert_eq!((duplicate.span.line, duplicate.span.column), (3, 7));
}

#[test]
fn stack_access_is_generic_invocation_metadata() {
    compile_source(
        Path::new("program.clipasm"),
        "clipasm 1\n@visible image(\"card.ppm\", 1s)\n",
    )
    .expect("no-op visible image");

    let error = clipasm::language::parse_str(
        Path::new("invalid.clipasm"),
        "clipasm 1\n@inherited image(\"card.ppm\", 1s)\n",
    )
    .expect_err("invalid stack access");
    assert_eq!(error.code, "E_INVALID_STACK_ACCESS");
}

#[test]
fn during_uses_timeline_as_its_single_input_name() {
    compile_source(
        Path::new("during-timeline.clipasm"),
        "clipasm 1\nclip { image(\"card.ppm\", 2s) } as selected\nduring(timeline=$selected, range=500ms..1500ms) { repeat(2) }\n",
    )
    .expect("during.timeline input");

    let error = clipasm::language::parse_str(
        Path::new("during-video.clipasm"),
        "clipasm 1\nclip { image(\"card.ppm\", 2s) } as selected\nduring(video=$selected, range=500ms..1500ms) { repeat(2) }\n",
    )
    .expect_err("obsolete during.video");
    assert_eq!(error.code, "E_UNKNOWN_PROGRAM_ARGUMENT");
}

#[test]
fn compiled_program_serializes_ordered_outputs() {
    let compiled = compile_source(
        Path::new("program.clipasm"),
        "clipasm 1\nimage(\"card.ppm\", 1s)\n",
    )
    .expect("compiled program");
    let document: serde_json::Value =
        serde_json::from_str(&compiled.compiled_json().expect("compiled JSON")).expect("JSON");

    assert_eq!(document["outputs"].as_array().expect("outputs").len(), 1);
    assert_eq!(document["format_version"], 22);
    assert_eq!(
        compiled.result_domain().expect("known result").frames().0,
        30
    );
}

#[test]
fn source_output_order_changes_compiled_identity() {
    let source = |first: &str, second: &str| {
        format!("clipasm 1\nimage(\"{first}\", 1s)\nimage(\"{second}\", 1s)\n")
    };
    let first = compile_source(
        Path::new("program.clipasm"),
        &source("first.png", "second.png"),
    )
    .expect("first order");
    let second = compile_source(
        Path::new("program.clipasm"),
        &source("second.png", "first.png"),
    )
    .expect("second order");

    assert_ne!(first.structure_hash(), second.structure_hash());
}

#[test]
fn parenthesized_output_bindings_require_multiple_names() {
    for (source, code) in [
        (
            "clipasm 1\nimage(\"card.png\", 1s) as (card)\n",
            "E_INVALID_OUTPUT_BINDING",
        ),
        (
            "clipasm 1\nimage(\"card.png\", 1s) as ()\n",
            "E_EXPECTED_TOKEN",
        ),
    ] {
        let error = clipasm::language::parse_str(Path::new("invalid.clipasm"), source)
            .expect_err("invalid output binding syntax");
        assert_eq!(error.code, code);
    }
}

#[test]
fn structural_block_bindings_validate_the_actual_output_sequence() {
    let empty = compile_source(Path::new("empty.clipasm"), "clipasm 1\n{} as empty\n")
        .expect_err("empty block cannot use one name");
    assert_eq!(empty.code, "E_OUTPUT_BINDING_COUNT");
    assert!(empty.message.contains("produces 0 value(s)"));

    let single = compile_source(
        Path::new("single.clipasm"),
        "clipasm 1\n{\n  image(\"morning.png\", 1s)\n  image(\"meadow.png\", 1s)\n  image(\"evening.png\", 1s)\n} as one_day\n",
    )
    .expect_err("three block outputs cannot use one name");
    assert_eq!(single.code, "E_OUTPUT_BINDING_COUNT");
    assert_eq!((single.span.line, single.span.column), (6, 6));
    assert_eq!(
        single.message,
        "`stack block` produces 3 value(s), but `as name` requires exactly one output"
    );

    let tuple = compile_source(
        Path::new("tuple.clipasm"),
        "clipasm 1\n{\n  image(\"morning.png\", 1s)\n  image(\"meadow.png\", 1s)\n  image(\"evening.png\", 1s)\n} as (first, second)\n",
    )
    .expect_err("tuple must name every block output");
    assert_eq!(tuple.code, "E_OUTPUT_BINDING_COUNT");
    assert!(tuple.message.contains("produces 3 value(s)"));
    assert!(tuple.message.contains("contains 2 name(s)"));

    compile_source(
        Path::new("single-valid.clipasm"),
        "clipasm 1\n{ image(\"card.png\", 1s) } as card\n",
    )
    .expect("single block output binding");

    compile_source(
        Path::new("complete.clipasm"),
        "clipasm 1\n{\n  image(\"morning.png\", 1s)\n  image(\"meadow.png\", 1s)\n  image(\"evening.png\", 1s)\n} as (morning, meadow, evening)\n",
    )
    .expect("complete tuple binding");
}

#[test]
fn explain_entries_expose_authored_names_and_source_locations() {
    let compiled = compile_source(
        Path::new("program.clipasm"),
        "clipasm 1\nimage(\"card.ppm\", 1s) as card\n",
    )
    .expect("compiled program");
    let entry = compiled.explain().last().expect("explain entry");

    assert_eq!(entry.construct(), "image");
    assert_eq!(entry.outputs().len(), 1);
    assert_eq!(entry.outputs()[0].id(), Some("card"));
    assert_eq!(entry.span().file(), Path::new("program.clipasm"));
    assert_eq!(entry.span().line, 2);
}

#[test]
fn entrypoint_output_does_not_change_compiled_semantics() {
    let source = |output: &str| {
        format!("clipasm 1\nconfig {{ output = \"{output}\" }}\nimage(\"card.ppm\", 1s)\n")
    };
    let first = compile_source(Path::new("program.clipasm"), &source("first.mp4"))
        .expect("first source program");
    let second = compile_source(Path::new("program.clipasm"), &source("second.mp4"))
        .expect("second source program");

    assert_eq!(first.structure_hash(), second.structure_hash());
}

#[test]
fn variadic_inputs_remain_reference_only() {
    let source = clipasm::language::parse_str(
        Path::new("program.clipasm"),
        "clipasm 1\nconcat(values={ image(\"card.ppm\", 1s) })\n",
    )
    .expect("canonical source");
    let error = clipasm::compiler::compile(&source).expect_err("variadic inline body");

    assert_eq!(error.code, "E_INVALID_ARGUMENT_TYPE");
    assert!(error.message.contains("variadic") && error.message.contains("references"));
}

#[test]
fn pure_compile_does_not_require_assets_to_exist() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig {\n  output = \"final.mp4\"\n}\n{\n  image(\"missing.png\", 1s)\n  concat\n}\n",
    )
    .expect("workflow");

    let output = run(&["inspect", workflow.to_str().expect("UTF-8 fixture path")]);
    assert!(
        output.status.success(),
        "pure compile unexpectedly accessed the asset: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    if common::media_tools_available() {
        let package = clipasm::language::parse_file(&workflow).expect("native source");
        let compiled = clipasm::compiler::compile(&package).expect("pure compile");
        let error = clipasm::preflight::preflight(&compiled).expect_err("missing asset preflight");
        assert_eq!(error.code, "E_MISSING_IMAGE_FILE");
    }
}

#[test]
fn unknown_named_program_arguments_are_rejected() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(&workflow, "clipasm 1\nimage(\"card.ppm\", 1s, sibling=1)\n").expect("workflow");

    let output = run(&["validate", workflow.to_str().expect("UTF-8 fixture path")]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[E_UNKNOWN_PROGRAM_ARGUMENT]"),
        "unexpected diagnostic: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn unknown_program_reports_a_diagnostic() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nnot_registered_program {\n  image(\"card.ppm\", 1s)\n}\n",
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
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nclip {\n  image(\"card.ppm\", 1s)\n} as card\n{\n  $card\n  concat\n}\n",
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
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig {\n  video {\n    width = 64\n    height = 64\n    fps = 60/2\n  }\n}\n{\n  image(\"card.ppm\", 1s)\n  concat\n}\n",
    )
    .expect("workflow");

    let plan = compile_json(&workflow);
    assert_eq!(plan["video"]["fps"]["numerator"], 30);
    assert_eq!(plan["video"]["fps"]["denominator"], 1);
}

#[test]
fn unused_imported_definitions_are_statically_checked() {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("invalid.clipasm"),
        "clipasm 1\nparam count: Integer = nope\nimage(\"unused.png\", 1s)\n",
    )
    .expect("invalid imported program");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nimport \"invalid.clipasm\" as invalid\nimage(\"used.png\", 1s)\n",
    )
    .expect("workflow");
    let error = compile_file(&workflow).expect_err("unused invalid import");
    assert_eq!(error.code, "E_INVALID_ARGUMENT_TYPE");
}

#[test]
fn video_sources_compile_purely_with_a_deferred_media_domain() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nvideo(\"missing.mp4\")\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("pure compile");
    assert!(compiled.result_domain().is_none());
    let document: serde_json::Value =
        serde_json::from_str(&compiled.compiled_json().expect("compiled JSON")).expect("JSON");
    assert_eq!(document["nodes"][0]["kind"]["operation"], "video_source");
    assert_eq!(document["nodes"][0]["kind"]["fit"], "cover");
    assert!(document["nodes"][0]["domain"].is_null());
}

#[test]
fn video_sources_do_not_accept_an_authored_duration() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nvideo(path=\"source.mp4\", duration=1s)\n",
    )
    .expect("workflow");

    let error = compile_file(&workflow).expect_err("duration argument");
    assert_eq!(error.code, "E_UNKNOWN_PROGRAM_ARGUMENT");
}

#[test]
fn compiler_owns_positive_project_dimension_validation() {
    for (field, source) in [
        ("width", "clipasm 1\nconfig { video { width = 0 } }\n"),
        ("height", "clipasm 1\nconfig { video { height = 0 } }\n"),
    ] {
        let package = clipasm::language::parse_str(Path::new("program.clipasm"), source)
            .expect("zero is representable in canonical source");
        let error = clipasm::compiler::compile(&package).expect_err("invalid project dimension");
        assert_eq!(error.code, "E_INVALID_VIDEO_SPEC");
        assert_eq!(
            error.message,
            format!("`{field}` must be greater than zero")
        );
    }
}

#[test]
fn project_audio_sample_rate_controls_audio_timeline_semantics() {
    let compiled = compile_source(
        Path::new("audio-rate.clipasm"),
        "clipasm 1\nconfig { audio { sample_rate = 44100 } }\naudio(\"missing.wav\")\ntrim(0s..1s)\n",
    )
    .expect("configured audio sample rate");

    assert_eq!(compiled.audio().sample_rate(), 44_100);
    let document: serde_json::Value =
        serde_json::from_str(&compiled.compiled_json().expect("compiled JSON"))
            .expect("compiled document");
    let slice = document["nodes"]
        .as_array()
        .expect("compiled nodes")
        .iter()
        .find(|node| {
            node["value_type"] == "audio"
                && node["kind"]["operation"] == "slice"
                && node["kind"]["unit"] == "samples"
        })
        .expect("audio slice");
    assert_eq!(slice["kind"]["range"]["start"], 0);
    assert_eq!(slice["kind"]["range"]["end"], 44_100);
}

#[test]
fn project_audio_sample_rate_must_be_positive() {
    let error = compile_source(
        Path::new("invalid-audio-rate.clipasm"),
        "clipasm 1\nconfig { audio { sample_rate = 0 } }\n",
    )
    .expect_err("zero sample rate");

    assert_eq!(error.code, "E_INVALID_AUDIO_SPEC");
}

#[test]
fn root_audio_inputs_use_the_native_audio_source_adapter() {
    let package = clipasm::language::parse_str(
        Path::new("audio-root.clipasm"),
        "clipasm 1\ninput soundtrack: Audio\n$soundtrack\n",
    )
    .expect("audio root source");
    let mut bindings = clipasm::compiler::EntrypointBindings::new();
    bindings
        .bind_audio_input(
            "soundtrack",
            "sound.wav",
            clipasm::source::SourceSpan::file_start("caller"),
        )
        .expect("audio binding");

    let compiled = clipasm::compiler::compile_with_bindings(&package, &bindings)
        .expect("audio root compilation");
    let document: serde_json::Value =
        serde_json::from_str(&compiled.compiled_json().expect("compiled JSON")).expect("JSON");
    assert_eq!(document["nodes"][0]["kind"]["operation"], "audio_source");
}
