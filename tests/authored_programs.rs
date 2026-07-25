//! Contracts for native authored program packages.

use std::fs;
use std::path::Path;

use clipasm::{compiler, language};

fn write(path: &Path, name: &str, source: &str) {
    fs::write(path.join(name), source).expect("write source program");
}

#[test]
fn native_programs_call_through_three_files() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "leaf.clipasm",
        "clipasm 1\ninput video: Video\nparam count: Integer\nrepeat($video, $count)\n",
    );
    write(
        directory.path(),
        "middle.clipasm",
        "clipasm 1\nimport \"leaf.clipasm\" as leaf\ninput video: Video\nparam count: Integer\nleaf($video, $count)\n",
    );
    write(
        directory.path(),
        "root.clipasm",
        "clipasm 1\nimport \"middle.clipasm\" as middle\nimage(\"missing.png\", 1s)\nmiddle(2)\n",
    );

    let package = language::parse_file(&directory.path().join("root.clipasm"))
        .expect("linked native package");
    let compiled = compiler::compile(&package).expect("compiled authored calls");

    assert_eq!(compiled.outputs().len(), 1);
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        60
    );
}

#[test]
fn repeated_calls_isolate_local_ids_and_apply_defaults() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "effect.clipasm",
        "clipasm 1\ninput video: Video\nparam count: Integer = 2\nrepeat($video, $count) as temporary\n",
    );
    write(
        directory.path(),
        "root.clipasm",
        "clipasm 1\nimport \"effect.clipasm\" as effect\nimage(\"first.png\", 1s)\neffect\nimage(\"second.png\", 1s)\neffect\nconcat\n",
    );

    let package = language::parse_file(&directory.path().join("root.clipasm"))
        .expect("linked native package");
    let compiled = compiler::compile(&package).expect("isolated authored calls");

    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        120
    );
}

#[test]
fn two_aliases_may_reference_the_same_source_program() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "effect.clipasm",
        "clipasm 1\ninput video: Video\nrepeat($video, 2) as local\n",
    );
    write(
        directory.path(),
        "root.clipasm",
        "clipasm 1\nimport \"effect.clipasm\" as name1\nimport \"effect.clipasm\" as name2\nimage(\"first.png\", 1s)\nname1\nimage(\"second.png\", 1s)\nname2\nconcat\n",
    );

    let package = language::parse_file(&directory.path().join("root.clipasm"))
        .expect("deduplicated source definition");
    let compiled = compiler::compile(&package).expect("separate calls through two aliases");

    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        120
    );
}

#[test]
fn triangle_import_cycle_is_rejected_before_compilation() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "first.clipasm",
        "clipasm 1\nimport \"second.clipasm\" as second\nsecond\n",
    );
    write(
        directory.path(),
        "second.clipasm",
        "clipasm 1\nimport \"third.clipasm\" as third\nthird\n",
    );
    write(
        directory.path(),
        "third.clipasm",
        "clipasm 1\nimport \"first.clipasm\" as first\nfirst\n",
    );

    let error =
        language::parse_file(&directory.path().join("first.clipasm")).expect_err("triangle cycle");

    assert_eq!(error.code, "E_PROGRAM_IMPORT_CYCLE");
    assert!(error.message.contains("first.clipasm"));
    assert!(error.message.contains("second.clipasm"));
    assert!(error.message.contains("third.clipasm"));
}

#[test]
fn scalar_parameters_and_graph_values_do_not_collide_silently() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "bad.clipasm",
        "clipasm 1\nparam count: Integer = 2\n$count\n",
    );
    let package = language::parse_file(&directory.path().join("bad.clipasm"))
        .expect("parsed parameter program");
    let error = compiler::compile(&package).expect_err("scalar used as graph value");
    assert_eq!(error.code, "E_PARAMETER_NOT_VALUE");

    write(
        directory.path(),
        "bad-value.clipasm",
        "clipasm 1\nimage(\"missing.png\", 1s) as video\nrepeat(value=$video, count=$video)\n",
    );
    let package = language::parse_file(&directory.path().join("bad-value.clipasm"))
        .expect("parsed value program");
    let error = compiler::compile(&package).expect_err("graph value used as scalar");
    assert_eq!(error.code, "E_INVALID_ARGUMENT_TYPE");
}

#[test]
fn imported_programs_reject_root_only_settings() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "imported.clipasm",
        "clipasm 1\nconfig {\n  video {\n    fps = 24\n  }\n}\nimage(\"missing.png\", 1s)\n",
    );
    write(
        directory.path(),
        "root.clipasm",
        "clipasm 1\nimport \"imported.clipasm\" as imported\nimported\n",
    );

    let error = language::parse_file(&directory.path().join("root.clipasm"))
        .expect_err("imported project settings");
    assert_eq!(error.code, "E_IMPORTED_PROJECT_SETTINGS");
}

#[test]
fn import_aliases_do_not_change_semantic_identity() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "effect.clipasm",
        "clipasm 1\ninput video: Video\nrepeat($video, 2)\n",
    );
    for (file, alias) in [
        ("first.clipasm", "first_name"),
        ("second.clipasm", "renamed"),
    ] {
        write(
            directory.path(),
            file,
            &format!(
                "clipasm 1\nimport \"effect.clipasm\" as {alias}\nimage(\"missing.png\", 1s)\n{alias}\n"
            ),
        );
    }

    let compile = |file: &str| {
        let package = language::parse_file(&directory.path().join(file)).expect("package");
        compiler::compile(&package).expect("compile")
    };
    assert_eq!(
        compile("first.clipasm").structure_hash(),
        compile("second.clipasm").structure_hash()
    );
}

#[test]
fn file_parameter_origins_follow_the_authored_value() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "effect.clipasm",
        "clipasm 1\nparam path: File = \"effect.png\"\nimage($path, 1s)\n",
    );
    write(
        directory.path(),
        "default.clipasm",
        "clipasm 1\nimport \"effect.clipasm\" as effect\neffect\n",
    );
    write(
        directory.path(),
        "caller.clipasm",
        "clipasm 1\nimport \"effect.clipasm\" as effect\neffect(path=\"caller.png\")\n",
    );

    let origin_file = |file: &str| {
        let package = language::parse_file(&directory.path().join(file)).expect("package");
        let compiled = compiler::compile(&package).expect("compile");
        let document: serde_json::Value =
            serde_json::from_str(&compiled.compiled_json().expect("JSON")).expect("document");
        document["nodes"][0]["origin"]["span"]["file"]
            .as_str()
            .expect("origin file")
            .to_owned()
    };

    assert!(origin_file("default.clipasm").ends_with("effect.clipasm"));
    assert!(origin_file("caller.clipasm").ends_with("caller.clipasm"));
}

#[test]
fn imported_multiple_outputs_use_normal_output_binding() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "two.clipasm",
        "clipasm 1\nimage(\"first.png\", 1s)\nimage(\"second.png\", 1s)\n",
    );
    write(
        directory.path(),
        "root.clipasm",
        "clipasm 1\nimport \"two.clipasm\" as two\ntwo as (first, second)\nconcat\n",
    );

    let package = language::parse_file(&directory.path().join("root.clipasm"))
        .expect("linked native package");
    let compiled = compiler::compile(&package).expect("compiled multiple outputs");

    assert_eq!(compiled.outputs().len(), 1);
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        60
    );
}

#[test]
fn imported_local_references_serialize_resolved_targets() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "effect.clipasm",
        "clipasm 1\ninput video: Video\nrepeat($video, 1) as local\n$local\n",
    );
    write(
        directory.path(),
        "root.clipasm",
        "clipasm 1\nimport \"effect.clipasm\" as effect\nimage(\"missing.png\", 1s)\neffect\n",
    );

    let package = language::parse_file(&directory.path().join("root.clipasm"))
        .expect("linked native package");
    let compiled = compiler::compile(&package).expect("compiled local references");
    let document: serde_json::Value =
        serde_json::from_str(&compiled.compiled_json().expect("JSON")).expect("document");
    let reference = document["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "reference")
        .expect("reference node");

    assert!(reference["kind"].get("target").is_some());
    assert!(reference["kind"].get("symbol").is_none());
}

#[test]
fn unused_imported_programs_validate_scalar_defaults() {
    let cases = [
        (
            "Duration",
            "definitely-not-a-duration",
            "E_INVALID_DURATION",
        ),
        (
            "TimeRange",
            "definitely-not-a-range",
            "E_INVALID_TIME_RANGE",
        ),
        (
            "Keyword(inside, beside)",
            "outside",
            "E_INVALID_ARGUMENT_VALUE",
        ),
    ];

    for (parameter_type, default, expected_code) in cases {
        let directory = tempfile::tempdir().expect("temporary directory");
        write(
            directory.path(),
            "bad.clipasm",
            &format!(
                "clipasm 1\nparam value: {parameter_type} = {default}\nimage(\"unused.ppm\", 1s)\n"
            ),
        );
        write(
            directory.path(),
            "root.clipasm",
            "clipasm 1\nimport \"bad.clipasm\" as bad\nimage(\"card.ppm\", 1s)\n",
        );

        let package = language::parse_file(&directory.path().join("root.clipasm"))
            .expect("linked native package");
        let error = compiler::compile(&package).expect_err("invalid imported default");

        assert_eq!(error.code, expected_code, "parameter type {parameter_type}");
        assert!(
            error.span.file().ends_with("bad.clipasm"),
            "parameter type {parameter_type}: {}",
            error.span.file().display()
        );
    }
}

#[test]
fn unused_imported_programs_accept_valid_scalar_defaults() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "defaults.clipasm",
        "clipasm 1\nparam count: Integer = 2\nparam path: File = \"unused.ppm\"\nparam duration: Duration = 1s\nparam range: TimeRange = 0s..1s\nparam fit: Keyword(cover, contain) = cover\nimage(\"unused.ppm\", 1s)\n",
    );
    write(
        directory.path(),
        "root.clipasm",
        "clipasm 1\nimport \"defaults.clipasm\" as defaults\nimage(\"card.ppm\", 1s)\n",
    );

    let package = language::parse_file(&directory.path().join("root.clipasm"))
        .expect("linked native package");
    let compiled = compiler::compile(&package).expect("valid imported defaults");

    assert_eq!(compiled.outputs().len(), 1);
}
