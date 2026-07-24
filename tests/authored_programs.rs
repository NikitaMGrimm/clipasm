//! Contracts for representation-neutral authored program packages loaded by YAML.

use std::fs;
use std::path::Path;

use clipasm::{compiler, frontend};

fn write(path: &Path, name: &str, source: &str) {
    fs::write(path.join(name), source).expect("write source program");
}

#[test]
fn yaml_programs_call_through_three_files() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "leaf.yaml",
        "- program:\n    version: 1\n    inputs:\n      - video: Video\n    parameters:\n      count: Integer\n\n- repeat:\n    video: $video\n    count: $count\n",
    );
    write(
        directory.path(),
        "middle.yaml",
        "- program:\n    version: 1\n    imports:\n      leaf: ./leaf.yaml\n    inputs:\n      - video: Video\n    parameters:\n      count: Integer\n\n- leaf:\n    video: $video\n    count: $count\n",
    );
    write(
        directory.path(),
        "root.yaml",
        "- program:\n    version: 1\n    imports:\n      middle: ./middle.yaml\n\n- image: {path: missing.png, duration: 1s}\n- middle:\n    count: 2\n",
    );

    let package = frontend::yaml::parse_file(&directory.path().join("root.yaml"))
        .expect("linked YAML package");
    let compiled = compiler::compile(&package).expect("compiled authored calls");

    assert_eq!(compiled.outputs().len(), 1);
    assert_eq!(compiled.result_domain().expect("known domain").frames.0, 60);
}

#[test]
fn repeated_calls_isolate_local_ids_and_apply_defaults() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "effect.yaml",
        "- program:\n    version: 1\n    inputs:\n      - video: Video\n    parameters:\n      count:\n        type: Integer\n        default: 2\n\n- repeat:\n    video: $video\n    count: $count\n  id: temporary\n",
    );
    write(
        directory.path(),
        "root.yaml",
        "- program:\n    version: 1\n    imports:\n      effect: ./effect.yaml\n\n- image: {path: first.png, duration: 1s}\n- effect\n- image: {path: second.png, duration: 1s}\n- effect\n- concat\n",
    );

    let package = frontend::yaml::parse_file(&directory.path().join("root.yaml"))
        .expect("linked YAML package");
    let compiled = compiler::compile(&package).expect("isolated authored calls");

    assert_eq!(
        compiled.result_domain().expect("known domain").frames.0,
        120
    );
}

#[test]
fn two_aliases_may_reference_the_same_source_program() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "effect.yaml",
        "- program:\n    version: 1\n    inputs:\n      - video: Video\n\n- repeat:\n    video: $video\n    count: 2\n  id: local\n",
    );
    write(
        directory.path(),
        "root.yaml",
        "- program:\n    version: 1\n    imports:\n      name1: ./effect.yaml\n      name2: ./effect.yaml\n\n- image: {path: first.png, duration: 1s}\n- name1\n- image: {path: second.png, duration: 1s}\n- name2\n- concat\n",
    );

    let package = frontend::yaml::parse_file(&directory.path().join("root.yaml"))
        .expect("deduplicated source definition");
    let compiled = compiler::compile(&package).expect("separate calls through two aliases");

    assert_eq!(
        compiled.result_domain().expect("known domain").frames.0,
        120
    );
}

#[test]
fn triangle_import_cycle_is_rejected_before_compilation() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "yaml1.yaml",
        "- program:\n    version: 1\n    imports:\n      yaml2: ./yaml2.yaml\n\n- yaml2\n",
    );
    write(
        directory.path(),
        "yaml2.yaml",
        "- program:\n    version: 1\n    imports:\n      yaml3: ./yaml3.yaml\n\n- yaml3\n",
    );
    write(
        directory.path(),
        "yaml3.yaml",
        "- program:\n    version: 1\n    imports:\n      yaml1: ./yaml1.yaml\n\n- yaml1\n",
    );

    let error = frontend::yaml::parse_file(&directory.path().join("yaml1.yaml"))
        .expect_err("triangle cycle");

    assert_eq!(error.code, "E_PROGRAM_IMPORT_CYCLE");
    assert!(error.message.contains("yaml1.yaml"));
    assert!(error.message.contains("yaml2.yaml"));
    assert!(error.message.contains("yaml3.yaml"));
}

#[test]
fn scalar_parameters_and_graph_values_do_not_collide_silently() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "bad.yaml",
        "- program:\n    version: 1\n    parameters:\n      count:\n        type: Integer\n        default: 2\n\n- $count\n",
    );
    let package = frontend::yaml::parse_file(&directory.path().join("bad.yaml"))
        .expect("parsed parameter program");
    let error = compiler::compile(&package).expect_err("scalar used as graph value");
    assert_eq!(error.code, "E_PARAMETER_NOT_VALUE");

    write(
        directory.path(),
        "bad-value.yaml",
        "- program:\n    version: 1\n\n- image: {path: missing.png, duration: 1s}\n  id: video\n- repeat:\n    video: $video\n    count: $video\n",
    );
    let package = frontend::yaml::parse_file(&directory.path().join("bad-value.yaml"))
        .expect("parsed value program");
    let error = compiler::compile(&package).expect_err("graph value used as scalar");
    assert_eq!(error.code, "E_INVALID_ARGUMENT_TYPE");
}

#[test]
fn imported_programs_reject_root_only_settings() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "imported.yaml",
        "- program:\n    version: 1\n    project:\n      video: {fps: 24}\n\n- image: {path: missing.png, duration: 1s}\n",
    );
    write(
        directory.path(),
        "root.yaml",
        "- program:\n    version: 1\n    imports:\n      imported: ./imported.yaml\n\n- imported\n",
    );

    let error = frontend::yaml::parse_file(&directory.path().join("root.yaml"))
        .expect_err("imported project settings");
    assert_eq!(error.code, "E_IMPORTED_PROJECT_SETTINGS");
}

#[test]
fn import_aliases_do_not_change_semantic_identity() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "effect.yaml",
        "- program:\n    version: 1\n    inputs:\n      - video: Video\n\n- repeat:\n    video: $video\n    count: 2\n",
    );
    for (file, alias) in [("first.yaml", "first_name"), ("second.yaml", "renamed")] {
        write(
            directory.path(),
            file,
            &format!(
                "- program:\n    version: 1\n    imports:\n      {alias}: ./effect.yaml\n\n- image: {{path: missing.png, duration: 1s}}\n- {alias}\n"
            ),
        );
    }

    let compile = |file: &str| {
        let package = frontend::yaml::parse_file(&directory.path().join(file)).expect("package");
        compiler::compile(&package).expect("compile")
    };
    assert_eq!(
        compile("first.yaml").structure_hash(),
        compile("second.yaml").structure_hash()
    );
}

#[test]
fn file_parameter_origins_follow_the_authored_value() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "effect.yaml",
        "- program:\n    version: 1\n    parameters:\n      path:\n        type: File\n        default: effect.png\n\n- image:\n    path: $path\n    duration: 1s\n",
    );
    write(
        directory.path(),
        "default.yaml",
        "- program:\n    version: 1\n    imports:\n      effect: ./effect.yaml\n\n- effect\n",
    );
    write(
        directory.path(),
        "caller.yaml",
        "- program:\n    version: 1\n    imports:\n      effect: ./effect.yaml\n\n- effect:\n    path: caller.png\n",
    );

    let origin_file = |file: &str| {
        let package = frontend::yaml::parse_file(&directory.path().join(file)).expect("package");
        let compiled = compiler::compile(&package).expect("compile");
        let document: serde_json::Value =
            serde_json::from_str(&compiled.canonical_json().expect("JSON")).expect("document");
        document["nodes"][0]["origin"]["span"]["file"]
            .as_str()
            .expect("origin file")
            .to_owned()
    };

    assert!(origin_file("default.yaml").ends_with("effect.yaml"));
    assert!(origin_file("caller.yaml").ends_with("caller.yaml"));
}

#[test]
fn imported_multiple_outputs_use_normal_ids_binding() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "two.yaml",
        "- program:\n    version: 1\n\n- image: {path: first.png, duration: 1s}\n- image: {path: second.png, duration: 1s}\n",
    );
    write(
        directory.path(),
        "root.yaml",
        "- program:\n    version: 1\n    imports:\n      two: ./two.yaml\n\n- two:\n  ids: [first, second]\n- concat\n",
    );

    let package = frontend::yaml::parse_file(&directory.path().join("root.yaml"))
        .expect("linked YAML package");
    let compiled = compiler::compile(&package).expect("compiled multiple outputs");

    assert_eq!(compiled.outputs().len(), 1);
    assert_eq!(compiled.result_domain().expect("known domain").frames.0, 60);
}

#[test]
fn imported_local_references_serialize_resolved_targets() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write(
        directory.path(),
        "effect.yaml",
        "- program:\n    version: 1\n    inputs:\n      - video: Video\n\n- repeat:\n    video: $video\n    count: 1\n  id: local\n- $local\n",
    );
    write(
        directory.path(),
        "root.yaml",
        "- program:\n    version: 1\n    imports:\n      effect: ./effect.yaml\n\n- image: {path: missing.png, duration: 1s}\n- effect\n",
    );

    let package = frontend::yaml::parse_file(&directory.path().join("root.yaml"))
        .expect("linked YAML package");
    let compiled = compiler::compile(&package).expect("compiled local references");
    let document: serde_json::Value =
        serde_json::from_str(&compiled.canonical_json().expect("JSON")).expect("document");
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
            "",
            "E_INVALID_DURATION",
        ),
        (
            "TimeRange",
            "definitely-not-a-range",
            "",
            "E_INVALID_TIME_RANGE",
        ),
        (
            "Keyword",
            "outside",
            "        values: [inside, beside]\n",
            "E_INVALID_ARGUMENT_VALUE",
        ),
    ];

    for (parameter_type, default, type_options, expected_code) in cases {
        let directory = tempfile::tempdir().expect("temporary directory");
        write(
            directory.path(),
            "bad.yaml",
            &format!(
                "- program:\n    version: 1\n    parameters:\n      value:\n        type: {parameter_type}\n{type_options}        default: {default}\n\n- image: {{path: unused.ppm, duration: 1s}}\n"
            ),
        );
        write(
            directory.path(),
            "root.yaml",
            "- program:\n    version: 1\n    imports:\n      bad: ./bad.yaml\n\n- image: {path: card.ppm, duration: 1s}\n",
        );

        let package = frontend::yaml::parse_file(&directory.path().join("root.yaml"))
            .expect("linked YAML package");
        let error = compiler::compile(&package).expect_err("invalid imported default");

        assert_eq!(error.code, expected_code, "parameter type {parameter_type}");
        assert!(
            error.span.file().ends_with("bad.yaml"),
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
        "defaults.yaml",
        "- program:\n    version: 1\n    parameters:\n      count:\n        type: Integer\n        default: 2\n      path:\n        type: File\n        default: unused.ppm\n      duration:\n        type: Duration\n        default: 1s\n      range:\n        type: TimeRange\n        default: 0s..1s\n      fit:\n        type: Keyword\n        values: [cover, contain]\n        default: cover\n\n- image: {path: unused.ppm, duration: 1s}\n",
    );
    write(
        directory.path(),
        "root.yaml",
        "- program:\n    version: 1\n    imports:\n      defaults: ./defaults.yaml\n\n- image: {path: card.ppm, duration: 1s}\n",
    );

    let package = frontend::yaml::parse_file(&directory.path().join("root.yaml"))
        .expect("linked YAML package");
    let compiled = compiler::compile(&package).expect("valid imported defaults");

    assert_eq!(compiled.outputs().len(), 1);
}
