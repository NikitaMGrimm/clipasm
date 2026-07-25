#![allow(missing_docs)]

use std::fs;
use std::path::Path;

use clipasm::compiler;
use tempfile::TempDir;

fn compile_yaml(path: &Path) -> clipasm::diagnostic::Result<compiler::CompiledProgram> {
    let source = clipasm::frontend::yaml::parse_file(path)?;
    compiler::compile(&source)
}

fn project(source: &str) -> (TempDir, clipasm::source::SourcePackage) {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(directory.path().join("a.ppm"), b"P3\n1 1\n255\n255 0 0\n").expect("a image");
    fs::write(directory.path().join("b.ppm"), b"P3\n1 1\n255\n0 255 0\n").expect("b image");
    fs::write(directory.path().join("c.ppm"), b"P3\n1 1\n255\n0 0 255\n").expect("c image");
    fs::write(directory.path().join("x.ppm"), b"P3\n1 1\n255\n255 255 0\n").expect("x image");
    fs::write(directory.path().join("y.ppm"), b"P3\n1 1\n255\n0 255 255\n").expect("y image");
    let path = directory.path().join("workflow.yaml");
    fs::write(&path, source).expect("workflow");
    let workflow = clipasm::frontend::yaml::parse_file(&path).expect("parse workflow");
    (directory, workflow)
}

fn compiled_json(compiled: &compiler::CompiledProgram) -> serde_json::Value {
    serde_json::from_str(&compiled.canonical_json().expect("compiled JSON")).expect("JSON value")
}

#[test]
fn repeat_reuses_one_upstream_value() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n    clips:\n      doubled:\n        - $later\n        - repeat: 3\n      later:\n        image:\n          path: a.ppm\n          duration: 1s\n\n- glue:\n    - $doubled\n  ",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        30
    );
    let json = compiled_json(&compiled);
    let repeat = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["origin"]["construct"] == "repeat")
        .expect("repeat node");
    assert_eq!(repeat["kind"]["operation"], "repeat");
    assert_eq!(repeat["kind"]["count"], 3);
    assert!(repeat["kind"]["input"].is_object());
}

#[test]
fn zoom_defaults_to_eight_percent_and_preserves_the_video_domain() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- glue:\n    - image:\n        path: a.ppm\n        duration: 1s\n    - zoom\n  ",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        10
    );

    let json = compiled_json(&compiled);
    let zoom = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["origin"]["construct"] == "zoom")
        .expect("zoom node");
    assert_eq!(zoom["kind"]["operation"], "zoom");
    assert_eq!(zoom["kind"]["percent"], 8);
    assert_eq!(zoom["domain"]["frames"], 10);
    assert_eq!(zoom["domain"]["width"], 64);
    assert_eq!(zoom["domain"]["height"], 64);
}

#[test]
fn omitted_and_explicit_default_zoom_have_equal_identity() {
    let source = |zoom: &str| {
        format!(
            "- program:\n    version: 1\n    project:\n      video: {{width: 64, height: 64, fps: 10}}\n\n- glue:\n    - image: {{path: a.ppm, duration: 1s}}\n    - {zoom}\n  "
        )
    };
    let (_omitted_directory, omitted) = project(&source("zoom"));
    let (_explicit_directory, explicit) = project(&source("zoom: 8"));
    let (_changed_directory, changed) = project(&source("zoom: 9"));

    let omitted = compiler::compile(&omitted).expect("omitted default");
    let explicit = compiler::compile(&explicit).expect("explicit default");
    let changed = compiler::compile(&changed).expect("changed percent");
    assert_eq!(omitted.structure_hash(), explicit.structure_hash());
    assert_ne!(omitted.structure_hash(), changed.structure_hash());
}

#[test]
fn zoom_rejects_nonpositive_or_unrepresentable_percentages() {
    for percent in [-1, 0, i64::from(u32::MAX) + 1] {
        let (_directory, workflow) = project(&format!(
            "- program:\n    version: 1\n\n- glue:\n    - image: {{path: a.ppm, duration: 1s}}\n    - zoom: {percent}\n  "
        ));
        let error = compiler::compile(&workflow).expect_err("invalid zoom percent");
        assert_eq!(error.code, "E_INVALID_ZOOM_PERCENT");
        assert!(error.message.contains("positive integer"));
    }
}

#[test]
fn zoom_consumes_only_the_top_video() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- glue:\n    - image: {path: a.ppm, duration: 1s}\n    - image: {path: b.ppm, duration: 1s}\n    - zoom: 12\n  ",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        20
    );

    let json = compiled_json(&compiled);
    let nodes = json["nodes"].as_array().expect("nodes");
    let zoom = nodes
        .iter()
        .find(|node| node["kind"]["operation"] == "zoom")
        .expect("zoom");
    assert_eq!(zoom["kind"]["input"]["id"], 1);
    assert_eq!(zoom["kind"]["percent"], 12);
    assert_eq!(nodes.last().expect("result")["kind"]["operation"], "concat");
}

#[test]
fn wobble_defaults_to_three_pixels_and_preserves_the_video_domain() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 48, fps: 10}\n\n- glue:\n    - image: {path: a.ppm, duration: 1s}\n    - wobble\n  ",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    let domain = compiled.result_domain().expect("known domain");
    assert_eq!(domain.frames().0, 10);
    assert_eq!(domain.width(), 64);
    assert_eq!(domain.height(), 48);

    let json = compiled_json(&compiled);
    let wobble = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["origin"]["construct"] == "wobble")
        .expect("wobble node");
    assert_eq!(wobble["kind"]["operation"], "wobble");
    assert_eq!(wobble["kind"]["pixels"], 3);
}

#[test]
fn wobble_default_normalizes_identity_and_changed_pixels_change_it() {
    let source = |wobble: &str| {
        format!(
            "- program:\n    version: 1\n    project:\n      video: {{width: 64, height: 48, fps: 10}}\n\n- glue:\n    - image: {{path: a.ppm, duration: 1s}}\n    - {wobble}\n  "
        )
    };
    let (_omitted_directory, omitted) = project(&source("wobble"));
    let (_explicit_directory, explicit) = project(&source("wobble: 3"));
    let (_changed_directory, changed) = project(&source("wobble: 4"));

    let omitted = compiler::compile(&omitted).expect("omitted default");
    let explicit = compiler::compile(&explicit).expect("explicit default");
    let changed = compiler::compile(&changed).expect("changed pixels");
    assert_eq!(omitted.structure_hash(), explicit.structure_hash());
    assert_ne!(omitted.structure_hash(), changed.structure_hash());
}

#[test]
fn wobble_rejects_invalid_or_dimension_unsafe_amplitudes() {
    for (width, height, pixels) in [
        (1280, 720, -1),
        (1280, 720, 0),
        (8, 48, 4),
        (64, 6, 3),
        (u32::MAX, u32::MAX, 1),
    ] {
        let (_directory, workflow) = project(&format!(
            "- program:\n    version: 1\n    project:\n      video: {{width: {width}, height: {height}, fps: 10}}\n\n- glue:\n    - image: {{path: a.ppm, duration: 1s}}\n    - wobble: {pixels}\n  "
        ));
        let error = compiler::compile(&workflow).expect_err("invalid wobble pixels");
        assert_eq!(error.code, "E_INVALID_WOBBLE_PIXELS");
        assert!(error.message.contains("both project dimensions"));
    }
}

#[test]
fn zoom_and_wobble_accept_values_above_the_old_policy_ceilings() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 1024, height: 768, fps: 10}\n\n- glue:\n    - image: {path: a.ppm, duration: 1s}\n    - zoom: 101\n    - wobble: 65\n  ",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        10
    );
}

#[test]
fn wobble_consumes_only_the_top_video() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 48, fps: 10}\n\n- glue:\n    - image: {path: a.ppm, duration: 1s}\n    - image: {path: b.ppm, duration: 1s}\n    - wobble: 4\n  ",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        20
    );

    let json = compiled_json(&compiled);
    let nodes = json["nodes"].as_array().expect("nodes");
    let wobble = nodes
        .iter()
        .find(|node| node["kind"]["operation"] == "wobble")
        .expect("wobble");
    assert_eq!(wobble["kind"]["input"]["id"], 1);
    assert_eq!(wobble["kind"]["pixels"], 4);
    assert_eq!(nodes.last().expect("result")["kind"]["operation"], "concat");
}

#[test]
fn flash_inside_join_binds_in_order_and_preserves_the_summed_domain() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 48, fps: 10}\n\n- glue:\n    - image: {path: a.ppm, duration: 1s}\n    - image: {path: b.ppm, duration: 1s}\n    - join:\n        - flash\n  ",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        20
    );

    let json = compiled_json(&compiled);
    let flash = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "flash_join")
        .expect("flash");
    assert_eq!(flash["kind"]["before"]["id"], 0);
    assert_eq!(flash["kind"]["after"]["id"], 1);
    assert_eq!(flash["kind"]["frames"], 2);
    assert_eq!(flash["domain"]["frames"], 20);
}

#[test]
fn explicit_flash_inputs_preserve_unrelated_stack_occurrences() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 48, fps: 10}\n    clips:\n      x: {image: {path: x.ppm, duration: 1s}}\n      y: {image: {path: y.ppm, duration: 1s}}\n\n- glue:\n    - image: {path: a.ppm, duration: 1s}\n    - image: {path: b.ppm, duration: 1s}\n    - flash:\n        before: $x\n        after: $y\n        frames: 2\n  ",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        40
    );

    let json = compiled_json(&compiled);
    let result = json["nodes"]
        .as_array()
        .expect("nodes")
        .last()
        .expect("result");
    assert_eq!(result["kind"]["operation"], "concat");
    assert_eq!(
        result["kind"]["inputs"].as_array().expect("inputs").len(),
        3
    );
}

#[test]
fn fixed_inputs_accept_isolated_inline_program_bodies() {
    let (_directory, program) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 48, fps: 10}\n\n- flash:\n    before:\n      - image: {path: a.ppm, duration: 1s}\n      - zoom\n    after:\n      image: {path: b.ppm, duration: 1s}\n    frames: 2\n",
    );

    let compiled = compiler::compile(&program).expect("inline fixed inputs");
    assert_eq!(
        compiled.result_domain().expect("known result").frames().0,
        20
    );
    let document = compiled_json(&compiled);
    let flash = document["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "flash_join")
        .expect("flash result");
    assert_eq!(flash["kind"]["before"]["id"], 1);
    assert_eq!(flash["kind"]["after"]["id"], 2);
}

#[test]
fn inline_input_bodies_start_empty_and_preserve_the_outer_stack() {
    let (_directory, isolated) = project(
        "- program:\n    version: 1\n\n- image: {path: a.ppm, duration: 1s}\n- repeat:\n    value:\n      - repeat: 2\n    count: 2\n",
    );
    let error = compiler::compile(&isolated).expect_err("isolated input stack");
    assert_eq!(error.code, "E_STACK_UNDERFLOW");

    let (_directory, preserved) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 48, fps: 10}\n\n- image: {path: a.ppm, duration: 1s}\n- repeat:\n    value:\n      image: {path: b.ppm, duration: 1s}\n    count: 2\n- concat\n",
    );
    let compiled = compiler::compile(&preserved).expect("preserved outer value");
    assert_eq!(
        compiled.result_domain().expect("known result").frames().0,
        30
    );
}

#[test]
fn inline_input_body_requires_exactly_one_value() {
    let (_directory, program) = project(
        "- program:\n    version: 1\n\n- repeat:\n    value:\n      - image: {path: a.ppm, duration: 1s}\n      - image: {path: b.ppm, duration: 1s}\n    count: 2\n",
    );
    let error = compiler::compile(&program).expect_err("two inline results");
    assert_eq!(error.code, "E_INPUT_BODY_OUTPUT_COUNT");
    assert!(error.message.contains("repeat.value"));
    assert!(error.message.contains("2 values remain"));
}

#[test]
fn inline_input_bodies_inherit_requested_frames() {
    let (_directory, program) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 48, fps: 10}\n\n- image: {path: a.ppm, duration: 2s}\n- during:\n    range: 500ms..1500ms\n    body:\n      - flash:\n          before:\n            image: b.ppm\n          after:\n            image: c.ppm\n      - concat\n",
    );
    let compiled = compiler::compile(&program).expect("requested inline duration");
    assert_eq!(
        compiled.result_domain().expect("known result").frames().0,
        40
    );
}

#[test]
fn inline_fixed_inputs_bind_in_descriptor_order_not_mapping_order() {
    let source = |inputs: &str| {
        format!(
            "- program:\n    version: 1\n    project:\n      video: {{width: 64, height: 48, fps: 10}}\n\n- flash:\n{inputs}    frames: 2\n"
        )
    };
    let before = "    before:\n      image: {path: a.ppm, duration: 1s}\n";
    let after = "    after:\n      image: {path: b.ppm, duration: 1s}\n";
    let (_first_directory, first) = project(&source(&format!("{before}{after}")));
    let (_second_directory, second) = project(&source(&format!("{after}{before}")));

    assert_eq!(
        compiler::compile(&first)
            .expect("declaration order")
            .structure_hash(),
        compiler::compile(&second)
            .expect("reverse mapping order")
            .structure_hash()
    );
}

#[test]
fn ids_inside_inline_inputs_use_the_global_named_value_namespace() {
    let (_directory, program) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 48, fps: 10}\n\n- flash:\n    before:\n      - image: {path: a.ppm, duration: 1s}\n        id: reusable\n      - zoom\n    after:\n      image: {path: b.ppm, duration: 1s}\n    frames: 2\n- $reusable\n- wobble\n- concat\n",
    );
    let compiled = compiler::compile(&program).expect("global inline id");
    assert_eq!(
        compiled.result_domain().expect("known result").frames().0,
        30
    );
    assert!(compiled_json(&compiled)["named_values"]["reusable"].is_object());
}

#[test]
fn ids_inside_named_clip_bodies_are_visible_to_the_source_body() {
    let (_directory, program) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 48, fps: 10}\n    clips:\n      decorated:\n        - image: {path: a.ppm, duration: 1s}\n          id: reusable\n        - zoom\n\n- $reusable\n- wobble\n",
    );
    let compiled = compiler::compile(&program).expect("global named-clip id");
    assert_eq!(
        compiled.result_domain().expect("known result").frames().0,
        10
    );
    assert!(compiled_json(&compiled)["named_values"]["reusable"].is_object());
}

#[test]
fn duplicate_names_cross_inline_input_boundaries() {
    let (_directory, duplicate) = project(
        "- program:\n    version: 1\n\n- flash:\n    before:\n      - image: {path: a.ppm, duration: 1s}\n        id: duplicate\n    after:\n      image: {path: b.ppm, duration: 1s}\n- image: {path: c.ppm, duration: 1s}\n  id: duplicate\n- concat\n",
    );
    assert_eq!(
        compiler::compile(&duplicate)
            .expect_err("duplicate inline id")
            .code,
        "E_DUPLICATE_NAME"
    );
}

#[test]
fn dependency_cycles_cross_inline_input_boundaries() {
    let (_directory, cycle) = project(
        "- program:\n    version: 1\n\n- flash:\n    before:\n      - $outer\n      - zoom:\n        id: inner\n    after:\n      - image: {path: b.ppm, duration: 1s}\n- $inner\n- zoom:\n  id: outer\n- concat\n",
    );
    assert_eq!(
        compiler::compile(&cycle)
            .expect_err("cross-boundary cycle")
            .code,
        "E_DEPENDENCY_CYCLE"
    );
}

#[test]
fn flash_identity_normalizes_the_default_and_preserves_order_and_frames() {
    let source = |before: &str, after: &str, frames: &str| {
        format!(
            "- program:\n    version: 1\n    project:\n      video: {{width: 64, height: 48, fps: 10}}\n    clips:\n      a: {{image: {{path: a.ppm, duration: 1s}}}}\n      b: {{image: {{path: b.ppm, duration: 1s}}}}\n\n- glue:\n    - flash:\n        before: ${before}\n        after: ${after}\n  {frames}"
        )
    };
    let (_omitted_directory, omitted) = project(&source("a", "b", ""));
    let (_explicit_directory, explicit) = project(&source("a", "b", "      frames: 2\n"));
    let (_changed_directory, changed) = project(&source("a", "b", "      frames: 3\n"));
    let (_reversed_directory, reversed) = project(&source("b", "a", "      frames: 2\n"));

    let omitted = compiler::compile(&omitted).expect("omitted default");
    let explicit = compiler::compile(&explicit).expect("explicit default");
    let changed = compiler::compile(&changed).expect("changed frames");
    let reversed = compiler::compile(&reversed).expect("reversed inputs");
    assert_eq!(omitted.structure_hash(), explicit.structure_hash());
    assert_ne!(omitted.structure_hash(), changed.structure_hash());
    assert_ne!(omitted.structure_hash(), reversed.structure_hash());
}

#[test]
fn flash_rejects_nonpositive_or_known_excessive_frame_counts() {
    for frames in [-1, 0, 11] {
        let (_directory, workflow) = project(&format!(
            "- program:\n    version: 1\n    project:\n      video: {{width: 64, height: 48, fps: 10}}\n\n- glue:\n    - image: {{path: a.ppm, duration: 1s}}\n    - image: {{path: b.ppm, duration: 1s}}\n    - flash: {frames}\n  "
        ));
        let error = compiler::compile(&workflow).expect_err("invalid flash frames");
        assert_eq!(error.code, "E_INVALID_FLASH_FRAMES");
    }
}

#[test]
fn default_flash_frames_are_the_smallest_count_covering_160_milliseconds() {
    for (fps, expected) in [("1", 1_u64), ("30000/1001", 5)] {
        let (_directory, workflow) = project(&format!(
            "- program:\n    version: 1\n    project:\n      video: {{width: 64, height: 48, fps: {fps}}}\n\n- glue:\n    - image: {{path: a.ppm, duration: 1001s}}\n    - image: {{path: b.ppm, duration: 1001s}}\n    - flash\n  "
        ));
        let compiled = compiler::compile(&workflow).expect("compile");
        let json = compiled_json(&compiled);
        let flash = json["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .find(|node| node["kind"]["operation"] == "flash_join")
            .expect("flash");
        assert_eq!(flash["kind"]["frames"], expected);
    }
}

#[test]
fn during_changes_duration() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- glue:\n    - image:\n        path: a.ppm\n        duration: 10s\n    - repeat: 2\n      during: 4s..6s\n  ",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        120
    );
    let json = compiled_json(&compiled);
    assert!(
        json["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .any(|node| node["kind"]["operation"] == "replace_range")
    );
}

#[test]
fn trim_selects_an_authored_time_range() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- glue:\n    - image:\n        path: a.ppm\n        duration: 3s\n    - trim: 1s..2s\n  ",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        10
    );

    let json = compiled_json(&compiled);
    let trim = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["origin"]["construct"] == "trim")
        .expect("trim slice");
    assert_eq!(trim["kind"]["operation"], "slice");
    assert_eq!(trim["kind"]["range"]["start"], 10);
    assert_eq!(trim["kind"]["range"]["end"], 20);
}

#[test]
fn empty_concat_bodies_report_their_owner() {
    let (_directory, workflow) = project("- program:\n    version: 1\n\n- glue: []\n");
    let error = compiler::compile(&workflow).expect_err("empty glue");
    assert_eq!(error.code, "E_EMPTY_GLUE");
    assert!(error.message.contains("glue"));
}

#[test]
fn nested_glue_starts_empty_and_does_not_consume_outer_values() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- glue:\n    - image: {path: a.ppm, duration: 1s}\n    - glue:\n        - image: {path: b.ppm, duration: 1s}\n        - image: {path: c.ppm, duration: 1s}\n  ",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        30
    );
}

#[test]
fn body_program_defaults_to_visible_and_can_capture_through_a_visible_operation() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- image: {path: a.ppm, duration: 1s}\n- glue:\n    - repeat:\n        count: 2\n        stack_access: visible\n",
    );
    let compiled = compiler::compile(&workflow).expect("default visible capture");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        20
    );
}

#[test]
fn default_visible_join_binds_its_inputs_from_the_visible_suffix() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- image: {path: a.ppm, duration: 1s}\n- image: {path: b.ppm, duration: 1s}\n- glue:\n    - join: []\n",
    );
    let compiled = compiler::compile(&workflow).expect("default visible join binding");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        20
    );
}

#[test]
fn default_visible_during_binds_its_video_from_the_visible_suffix() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- image: {path: a.ppm, duration: 2s}\n- glue:\n    - during:\n        range: 500ms..1500ms\n        body:\n          - repeat: 2\n",
    );
    let compiled = compiler::compile(&workflow).expect("default visible during binding");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        30
    );
}

#[test]
fn visible_body_does_not_make_its_children_visible() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n\n- image: {path: a.ppm, duration: 1s}\n- glue:\n    - repeat: 2\n",
    );
    let error = compiler::compile(&workflow).expect_err("owned repeat cannot capture");
    assert_eq!(error.code, "E_STACK_UNDERFLOW");
    assert!(error.message.contains("owned"));
    assert!(error.notes.iter().any(|note| {
        note.contains("additional Video value") && note.contains("stack_access: visible")
    }));
}

#[test]
fn owned_concat_reduces_only_values_captured_by_an_earlier_visible_operation() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- image: {path: a.ppm, duration: 1s}\n- image: {path: b.ppm, duration: 1s}\n- image: {path: c.ppm, duration: 2s}\n- during:\n    range: 500ms..1500ms\n    body:\n      - flash:\n          frames: 1\n          stack_access: visible\n      - image: {path: x.ppm, duration: 1s}\n      - concat\n- concat\n",
    );
    let compiled = compiler::compile(&workflow).expect("selective capture");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        50
    );

    let json = compiled_json(&compiled);
    let concat_inputs = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .filter(|node| node["kind"]["operation"] == "concat")
        .map(|node| {
            node["kind"]["inputs"]
                .as_array()
                .expect("concat inputs")
                .len()
        })
        .collect::<Vec<_>>();
    assert_eq!(concat_inputs, vec![2, 2]);
}

#[test]
fn visible_concat_deliberately_consumes_the_complete_visible_suffix() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- image: {path: a.ppm, duration: 1s}\n- glue:\n    - image: {path: b.ppm, duration: 1s}\n    - concat:\n        stack_access: visible\n",
    );
    let compiled = compiler::compile(&workflow).expect("visible concat");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        20
    );
}

#[test]
fn explicit_owned_body_boundary_blocks_a_visible_descendant() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n\n- image: {path: a.ppm, duration: 1s}\n- glue:\n    - image: {path: b.ppm, duration: 2s}\n    - during:\n        range: 500ms..1500ms\n        stack_access: owned\n        body:\n          - flash:\n              frames: 1\n              stack_access: visible\n",
    );
    let error = compiler::compile(&workflow).expect_err("during boundary");
    assert_eq!(error.code, "E_STACK_UNDERFLOW");
    assert!(error.message.contains("visible"));
    assert!(
        error
            .notes
            .iter()
            .any(|note| { note.contains("during") && note.contains("stack visibility boundary") })
    );
}

#[test]
fn omitted_body_stack_access_matches_explicit_visible_identity() {
    let source = |access: &str| {
        format!(
            "- program:\n    version: 1\n\n- image: {{path: a.ppm, duration: 1s}}\n- glue:{access}\n    body:\n      - repeat:\n          count: 2\n          stack_access: visible\n"
        )
    };
    let (_default_directory, default) = project(&source(""));
    let (_visible_directory, visible) = project(&source("\n    stack_access: visible"));

    assert_eq!(
        compiler::compile(&default)
            .expect("default body access")
            .structure_hash(),
        compiler::compile(&visible)
            .expect("explicit visible body access")
            .structure_hash()
    );
}

#[test]
fn no_op_stack_access_does_not_change_semantic_identity() {
    let (_owned_directory, owned) =
        project("- program:\n    version: 1\n\n- image: {path: a.ppm, duration: 1s}\n");
    let (_visible_directory, visible) = project(
        "- program:\n    version: 1\n    stack_access: visible\n\n- image:\n    path: a.ppm\n    duration: 1s\n    stack_access: visible\n",
    );
    assert_eq!(
        compiler::compile(&owned).expect("owned").structure_hash(),
        compiler::compile(&visible)
            .expect("visible")
            .structure_hash()
    );
}

#[test]
fn explicit_and_postfix_during_have_the_same_semantics() {
    let source = |during: &str| {
        format!(
            "- program:\n    version: 1\n    project:\n      video: {{width: 64, height: 64, fps: 10}}\n\n- glue:\n    - image:\n        path: a.ppm\n        duration: 10s\n{during}\n"
        )
    };
    let (_postfix_directory, postfix) = project(&source("    - repeat: 2\n      during: 4s..6s"));
    let (_explicit_directory, explicit) = project(&source(
        "    - during:\n        range: 4s..6s\n        body:\n          - repeat: 2",
    ));
    assert_eq!(
        compiler::compile(&postfix)
            .expect("postfix")
            .structure_hash(),
        compiler::compile(&explicit)
            .expect("explicit")
            .structure_hash()
    );
}

#[test]
fn postfix_id_names_the_outer_during_result() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- glue:\n    - image:\n        path: a.ppm\n        duration: 10s\n    - repeat: 2\n      during: 4s..6s\n      id: edited\n  ",
    );
    let document = compiled_json(&compiler::compile(&workflow).expect("compile"));
    let edited = usize::try_from(
        document["named_values"]["edited"]["id"]
            .as_u64()
            .expect("edited node id"),
    )
    .expect("node id fits usize");
    assert_eq!(
        document["nodes"][edited]["kind"]["operation"],
        "replace_range"
    );
}

#[test]
fn during_does_not_hide_selected_input_from_a_source() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n\n- glue:\n    - image:\n        path: a.ppm\n        duration: 10s\n    - image:\n        path: b.ppm\n        duration: 2s\n      during: 4s..6s\n  ",
    );
    let error = compiler::compile(&workflow).expect_err("selected plus source");
    assert_eq!(error.code, "E_BODY_OUTPUT_COUNT");
}

#[test]
fn join_concatenates_leftover_body_videos_in_order() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- glue:\n    - image: {path: a.ppm, duration: 1s}\n    - image: {path: b.ppm, duration: 1s}\n    - join:\n        - wobble\n  ",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        20
    );

    let json = compiled_json(&compiled);
    let nodes = json["nodes"].as_array().expect("nodes");
    let join_concat = nodes
        .iter()
        .find(|node| node["origin"]["construct"] == "join" && node["kind"]["operation"] == "concat")
        .expect("join finalization concat");
    assert_eq!(
        join_concat["kind"]["inputs"]
            .as_array()
            .expect("concat inputs")
            .len(),
        2
    );
}

#[test]
fn join_reduces_only_the_top_two_outer_values() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- glue:\n    - image:\n        path: a.ppm\n        duration: 1s\n    - image:\n        path: b.ppm\n        duration: 1s\n    - image:\n        path: c.ppm\n        duration: 1s\n    - join:\n        - concat\n  ",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        30
    );
    let json = compiled_json(&compiled);
    assert_eq!(
        json["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .filter(|node| node["kind"]["operation"] == "concat")
            .count(),
        2
    );
}

#[test]
fn explicit_inputs_do_not_consume_join_stack_occurrences() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n    clips:\n      x: {image: {path: x.ppm, duration: 1s}}\n      y: {image: {path: y.ppm, duration: 1s}}\n\n- glue:\n    - image:\n        path: a.ppm\n        duration: 1s\n    - image:\n        path: b.ppm\n        duration: 1s\n    - join:\n        - concat:\n            values: [$x, $y]\n        - concat\n  ",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        40
    );
}

#[test]
fn explicit_join_inputs_preserve_the_outer_stack() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n    clips:\n      x: {image: {path: x.ppm, duration: 1s}}\n      y: {image: {path: y.ppm, duration: 1s}}\n\n- glue:\n    - image: {path: a.ppm, duration: 1s}\n    - image: {path: b.ppm, duration: 1s}\n    - join:\n        before: $x\n        after: $y\n        body:\n          - concat\n  ",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        40
    );
}

#[test]
fn partial_explicit_join_binding_uses_the_preceding_value() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n    clips:\n      y:\n        image:\n          path: y.ppm\n          duration: 1s\n\n- glue:\n    - image:\n        path: a.ppm\n        duration: 1s\n    - join:\n        after: $y\n        body:\n          - concat\n  ",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        20
    );
}

#[test]
fn legacy_named_clip_lowers_to_owned_glue() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    clips:\n      pair:\n        - {image: {path: a.ppm, duration: 1s}}\n        - {image: {path: b.ppm, duration: 1s}}\n\n- glue:\n    - $pair\n  ",
    );
    let compiled = compiler::compile(&workflow).expect("compiled glue-backed clip");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        60
    );
}

#[test]
fn reports_readable_named_cycle() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    clips:\n      a: $b\n      b: $a\n\n- glue:\n    - $a\n  ",
    );
    let error = compiler::compile(&workflow).expect_err("cycle");
    assert_eq!(error.code, "E_DEPENDENCY_CYCLE");
    assert!(error.message.contains("a -> b -> a"));
}

#[test]
fn mapping_order_does_not_change_compiled_structure() {
    let (_first_dir, first) = project(
        "- program:\n    version: 1\n    clips:\n      a: {image: {path: a.ppm, duration: 1s}}\n      b: {image: {path: b.ppm, duration: 1s}}\n\n- glue: [$a, $b]\n",
    );
    let (_second_dir, second) = project(
        "- program:\n    version: 1\n    clips:\n      b: {image: {duration: 1s, path: b.ppm}}\n      a: {image: {duration: 1s, path: a.ppm}}\n\n- glue: [$a, $b]\n",
    );
    let first_compiled = compiler::compile(&first).expect("first");
    let second_compiled = compiler::compile(&second).expect("second");
    assert_eq!(
        first_compiled.structure_hash(),
        second_compiled.structure_hash()
    );
}

#[test]
fn postfix_mapping_order_does_not_change_wrapper_direction() {
    let source = |item: &str| {
        format!(
            "- program:\n    version: 1\n    project:\n      video: {{width: 64, height: 64, fps: 10}}\n\n- glue:\n    - image:\n        path: a.ppm\n        duration: 3s\n{item}\n"
        )
    };
    let (_first_directory, first) = project(&source("    - repeat: 2\n      during: 1s..2s"));
    let (_second_directory, second) = project(&source("    - during: 1s..2s\n      repeat: 2"));
    let first = compiler::compile(&first).expect("head first");
    let second = compiler::compile(&second).expect("wrapper first");
    assert_eq!(first.result_domain(), second.result_domain());
    assert_eq!(first.structure_hash(), second.structure_hash());
}

#[test]
fn explicit_concat_and_nested_glue_have_the_same_semantics() {
    let header = "- program:\n    version: 1\n    clips:\n      a: {image: {path: a.ppm, duration: 1s}}\n      b: {image: {path: b.ppm, duration: 1s}}\n\n";
    let (_concat_directory, concat) = project(&format!("{header}- $a\n- $b\n- concat\n"));
    let (_nested_directory, nested) = project(&format!("{header}- glue:\n    - $a\n    - $b\n"));
    let concat = compiler::compile(&concat).expect("explicit concat");
    let nested = compiler::compile(&nested).expect("nested glue");
    assert_eq!(concat.result_domain(), nested.result_domain());
    assert_eq!(concat.structure_hash(), nested.structure_hash());
}

#[test]
fn compile_file_accepts_an_outputless_validation_workflow() {
    let (directory, _workflow) = project(
        "- program:\n    version: 1\n\n- glue:\n    - image:\n        path: a.ppm\n        duration: 1s\n  ",
    );
    compile_yaml(&directory.path().join(Path::new("workflow.yaml"))).expect("compile");
}

#[test]
fn comments_do_not_change_structure_hash() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("workflow.yaml");
    let first = clipasm::frontend::yaml::parse_str(
        &path,
        "- program:\n    version: 1\n\n- glue:\n    - image:\n        path: a.ppm\n        duration: 1s\n  ",
    )
    .expect("first parse");
    let second = clipasm::frontend::yaml::parse_str(
        &path,
        "# formatting is not semantic\n- program:\n    version: 1\n\n- glue:\n    - image:\n        duration: 1s\n        path: a.ppm\n  ",
    )
    .expect("second parse");
    assert_eq!(
        compiler::compile(&first).expect("first").structure_hash(),
        compiler::compile(&second).expect("second").structure_hash()
    );
}

#[test]
fn authored_source_paths_change_compiled_identity() {
    let path = Path::new("workflow.yaml");
    for (program, first_path, second_path, suffix) in [
        ("image", "a.png", "b.png", ", duration: 1s"),
        ("video", "a.mp4", "b.mp4", ""),
    ] {
        let source = |asset: &str| {
            format!(
                "- program:\n    version: 1\n\n- glue:\n    - {program}: {{path: {asset}{suffix}}}\n  "
            )
        };
        let first = clipasm::frontend::yaml::parse_str(path, &source(first_path)).expect("first");
        let second =
            clipasm::frontend::yaml::parse_str(path, &source(second_path)).expect("second");
        assert_ne!(
            compiler::compile(&first).expect("first").structure_hash(),
            compiler::compile(&second).expect("second").structure_hash(),
            "{program} path must contribute to compiled identity"
        );
    }
}

#[test]
fn postfix_wrapper_accepts_stack_access_metadata() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- image: {path: a.ppm, duration: 2s}\n- repeat: 2\n  during:\n    range: 500ms..1s\n    stack_access: visible\n",
    );
    let compiled = compiler::compile(&workflow).expect("postfix mapping");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        25
    );
}

#[test]
fn body_program_inputs_are_available_as_local_references_after_stack_consumption() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- image: {path: a.ppm, duration: 1s}\n- image: {path: b.ppm, duration: 1s}\n- join:\n    - flash\n    - $before\n    - concat\n",
    );
    let compiled = compiler::compile(&workflow).expect("body input references");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        30
    );
}

#[test]
fn nested_body_arguments_resolve_before_inner_port_shadowing() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- image: {path: a.ppm, duration: 5s}\n- during:\n    range: 1s..3s\n    body:\n      - during:\n          video: $video\n          range: 2s..4s\n          body:\n            - repeat: 1\n      - concat\n",
    );
    let compiled = compiler::compile(&workflow).expect("nested shadowing");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        100
    );
}

#[test]
fn root_publication_ignores_auxiliary_audio_outputs() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    output: final.mp4\n\n- audio: missing.wav\n- image: {path: a.ppm, duration: 1s}\n",
    );
    let compiled = compiler::compile(&workflow).expect("one Video plus auxiliary Audio");
    assert_eq!(compiled.outputs().len(), 2);
    assert_eq!(
        compiled.result_domain().expect("Video result").frames().0,
        30
    );
}

#[test]
fn trim_uses_audio_natively_without_implicit_adaptation() {
    let (_directory, workflow) =
        project("- program:\n    version: 1\n\n- audio: missing.wav\n- trim: 0s..1s\n");
    let compiled = compiler::compile(&workflow).expect("native Audio trim");
    assert_eq!(
        compiled.outputs()[0].value_type(),
        clipasm::model::ValueType::Audio
    );
    let document = compiled_json(&compiled);
    let operations = document["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .filter_map(|node| node["kind"]["operation"].as_str())
        .collect::<Vec<_>>();
    assert!(operations.contains(&"audio_slice"));
    assert!(!operations.contains(&"audio_on_black"));
    assert!(!operations.contains(&"extract_audio"));
}

#[test]
fn nested_explicit_ports_compose_direct_audio_video_adaptations() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n\n- set_audio:\n    video:\n      image: {path: a.ppm, duration: 4s}\n    audio:\n      zoom:\n        video:\n          audio: missing.wav\n",
    );
    let compiled = compiler::compile(&workflow).expect("composed explicit adaptations");
    assert_eq!(
        compiled.result_domain().expect("Video result").frames().0,
        40
    );
    let document = compiled_json(&compiled);
    let operations = document["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .filter_map(|node| node["kind"]["operation"].as_str())
        .collect::<Vec<_>>();
    assert!(operations.contains(&"audio_on_black"));
    assert!(operations.contains(&"zoom"));
    assert!(operations.contains(&"extract_audio"));
    assert!(operations.contains(&"set_audio"));
}

#[test]
fn bare_concat_rejects_mixed_timeline_types() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n\n- image: {path: a.ppm, duration: 1s}\n- audio: missing.wav\n- concat\n",
    );
    let error = compiler::compile(&workflow).expect_err("mixed concat must select a type");
    assert_eq!(error.code, "E_AMBIGUOUS_GENERIC_TYPE");
    assert!(error.message.contains("type: Video"));
    assert!(error.message.contains("type: Audio"));
}

#[test]
fn concat_selector_reduces_only_the_selected_type() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n\n- image: {path: a.ppm, duration: 1s}\n- audio: first.wav\n- image: {path: b.ppm, duration: 1s}\n- audio: second.wav\n- concat: Video\n- concat: Audio\n",
    );
    let compiled = compiler::compile(&workflow).expect("selected concatenations");
    assert_eq!(compiled.outputs().len(), 2);
    assert_eq!(
        compiled.outputs()[0].value_type(),
        clipasm::model::ValueType::Video
    );
    assert_eq!(
        compiled.outputs()[1].value_type(),
        clipasm::model::ValueType::Audio
    );
    let document = compiled_json(&compiled);
    let operations = document["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .filter_map(|node| node["kind"]["operation"].as_str())
        .collect::<Vec<_>>();
    assert!(operations.contains(&"concat"));
    assert!(operations.contains(&"audio_concat"));
}

#[test]
fn generic_unary_programs_use_the_nearest_compatible_value() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n\n- image: {path: a.ppm, duration: 1s}\n- audio: missing.wav\n- repeat: 2\n- drop\n",
    );
    let compiled = compiler::compile(&workflow).expect("nearest Audio then drop it");
    assert_eq!(compiled.outputs().len(), 1);
    assert_eq!(
        compiled.outputs()[0].value_type(),
        clipasm::model::ValueType::Video
    );
    let document = compiled_json(&compiled);
    assert!(
        document["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .any(|node| node["kind"]["operation"] == "audio_repeat")
    );
}

#[test]
fn generic_explicit_inputs_must_be_homogeneous_without_coercion() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n    clips:\n      video: {image: {path: a.ppm, duration: 1s}}\n\n- audio: missing.wav\n  id: audio\n- concat:\n    values: [$video, $audio]\n",
    );
    let error = compiler::compile(&workflow).expect_err("mixed explicit generic input");
    assert_eq!(error.code, "E_GENERIC_TYPE_MISMATCH");
}

#[test]
fn glue_concatenates_homogeneous_audio() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n\n- glue:\n    - audio: first.wav\n    - audio: second.wav\n",
    );
    let compiled = compiler::compile(&workflow).expect("Audio glue");
    assert_eq!(compiled.outputs().len(), 1);
    assert_eq!(
        compiled.outputs()[0].value_type(),
        clipasm::model::ValueType::Audio
    );
    let document = compiled_json(&compiled);
    assert!(
        document["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .any(|node| node["kind"]["operation"] == "audio_concat")
    );
}

#[test]
fn join_concatenates_homogeneous_audio() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n\n- audio: first.wav\n- audio: second.wav\n- join:\n    - concat\n",
    );
    let compiled = compiler::compile(&workflow).expect("Audio join");
    assert_eq!(compiled.outputs().len(), 1);
    assert_eq!(
        compiled.outputs()[0].value_type(),
        clipasm::model::ValueType::Audio
    );
    let document = compiled_json(&compiled);
    assert!(
        document["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .any(|node| node["kind"]["operation"] == "audio_concat")
    );
}

#[test]
fn generic_body_programs_reject_mixed_outputs() {
    for program in [
        "- glue:\n    - audio: music.wav\n    - image: {path: a.ppm, duration: 1s}\n",
        "- audio: first.wav\n- audio: second.wav\n- join:\n    - image: {path: a.ppm, duration: 1s}\n",
    ] {
        let (_directory, workflow) = project(&format!("- program:\n    version: 1\n\n{program}"));
        let error = compiler::compile(&workflow).expect_err("mixed body output types");
        assert!(matches!(
            error.code,
            "E_GENERIC_TYPE_MISMATCH" | "E_TYPE_MISMATCH"
        ));
    }
}

#[test]
fn join_selector_chooses_one_homogeneous_stack_view() {
    let (_directory, ambiguous) = project(
        "- program:\n    version: 1\n\n- image: {path: a.ppm, duration: 1s}\n- image: {path: b.ppm, duration: 1s}\n- audio: first.wav\n- audio: second.wav\n- join:\n    - concat\n",
    );
    let error = compiler::compile(&ambiguous).expect_err("ambiguous join type");
    assert_eq!(error.code, "E_AMBIGUOUS_GENERIC_TYPE");

    let (_directory, selected) = project(
        "- program:\n    version: 1\n\n- image: {path: a.ppm, duration: 1s}\n- image: {path: b.ppm, duration: 1s}\n- audio: first.wav\n- audio: second.wav\n- join:\n    type: Audio\n    body:\n      - concat\n",
    );
    let compiled = compiler::compile(&selected).expect("selected Audio join");
    assert_eq!(compiled.outputs().len(), 3);
    assert_eq!(
        compiled.outputs()[2].value_type(),
        clipasm::model::ValueType::Audio
    );
}

#[test]
fn named_glue_infers_its_type_from_the_body() {
    let (_directory, inferred) = project(
        "- program:\n    version: 1\n\n- glue:\n    - audio: first.wav\n    - audio: second.wav\n  id: combined\n- $combined\n",
    );
    let inferred = compiler::compile(&inferred).expect("inferred named Audio glue");
    assert_eq!(
        inferred
            .outputs()
            .last()
            .expect("reference output")
            .value_type(),
        clipasm::model::ValueType::Audio
    );

    let (_directory, annotated) = project(
        "- program:\n    version: 1\n\n- glue:\n    type: Audio\n    body:\n      - audio: first.wav\n      - audio: second.wav\n  id: combined\n- $combined\n",
    );
    let annotated = compiler::compile(&annotated).expect("annotated named Audio glue");
    assert_eq!(inferred.structure_hash(), annotated.structure_hash());
}

#[test]
fn named_glue_type_inference_follows_forward_references() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n\n- $combined\n- glue:\n    - audio: first.wav\n    - audio: second.wav\n  id: combined\n",
    );
    let compiled = compiler::compile(&workflow).expect("forward inferred named glue");
    assert_eq!(compiled.outputs().len(), 2);
    assert!(
        compiled
            .outputs()
            .iter()
            .all(|output| output.value_type() == clipasm::model::ValueType::Audio)
    );
}

#[test]
fn named_glue_type_inference_resolves_dependency_chains() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n\n- glue:\n    - $later\n  id: earlier\n- glue:\n    - audio: first.wav\n    - audio: second.wav\n  id: later\n- $earlier\n",
    );
    let compiled = compiler::compile(&workflow).expect("inferred named glue chain");
    assert_eq!(
        compiled
            .outputs()
            .last()
            .expect("earlier reference")
            .value_type(),
        clipasm::model::ValueType::Audio
    );
}

#[test]
fn named_glue_type_inference_reports_dependency_cycles() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n\n- glue:\n    - $second\n  id: first\n- glue:\n    - $first\n  id: second\n",
    );
    let error = compiler::compile(&workflow).expect_err("named glue type cycle");
    assert_eq!(error.code, "E_DEPENDENCY_CYCLE");
    assert!(error.message.contains("first -> second -> first"));
}

#[test]
fn selected_named_glue_cycle_remains_a_dependency_cycle() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n\n- glue:\n    type: Audio\n    body:\n      - $second\n  id: first\n- glue:\n    type: Audio\n    body:\n      - $first\n  id: second\n",
    );
    let error = compiler::compile(&workflow).expect_err("selected named glue cycle");
    assert_eq!(error.code, "E_DEPENDENCY_CYCLE");
    assert!(error.message.contains("first -> second -> first"));
}

#[test]
fn self_dependent_stack_inference_reports_an_inference_dependency() {
    let (_directory, workflow) =
        project("- program:\n    version: 1\n\n- $future\n- repeat: 2\n  id: future\n");
    let error = compiler::compile(&workflow).expect_err("self-dependent generic stack binding");
    assert_eq!(error.code, "E_TYPE_INFERENCE_DEPENDENCY");
}

#[test]
fn named_glue_type_inference_respects_body_port_shadowing() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n\n- glue:\n    - image: {path: a.ppm, duration: 1s}\n    - image: {path: b.ppm, duration: 1s}\n    - join:\n        - drop\n        - drop\n        - $before\n  id: combined\n- $combined\n",
    );
    let compiled = compiler::compile(&workflow).expect("body-local port shadowing");
    assert_eq!(
        compiled
            .outputs()
            .last()
            .expect("combined reference")
            .value_type(),
        clipasm::model::ValueType::Video
    );
}

#[test]
fn named_generic_output_infers_from_the_same_stack_value_as_an_unnamed_call() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n\n- image: {path: a.ppm, duration: 1s}\n- repeat: 2\n  id: doubled\n",
    );
    let compiled = compiler::compile(&workflow).expect("named Video repeat");
    assert_eq!(compiled.outputs().len(), 1);
    assert_eq!(
        compiled.outputs()[0].value_type(),
        clipasm::model::ValueType::Video
    );
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        60
    );
}

#[test]
fn forward_reference_uses_the_type_inferred_from_a_named_calls_stack_input() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n\n- $doubled\n- image: {path: a.ppm, duration: 1s}\n- repeat: 2\n  id: doubled\n",
    );
    let compiled = compiler::compile(&workflow).expect("forward named Video repeat");
    assert_eq!(compiled.outputs().len(), 2);
    assert!(
        compiled
            .outputs()
            .iter()
            .all(|output| { output.value_type() == clipasm::model::ValueType::Video })
    );
}

#[test]
fn deferred_exact_binding_retries_after_a_forward_generic_type_resolves() {
    let (_directory, workflow) = project(
        "- program:\n    version: 1\n\n- $future\n- image: {path: a.ppm, duration: 1s}\n- zoom\n- audio: missing.wav\n- repeat: 2\n  id: future\n",
    );
    let compiled = compiler::compile(&workflow).expect("forward Audio above Video binding");
    assert_eq!(compiled.outputs().len(), 3);
    assert_eq!(
        compiled.outputs()[0].value_type(),
        clipasm::model::ValueType::Audio
    );
    assert_eq!(
        compiled.outputs()[1].value_type(),
        clipasm::model::ValueType::Video
    );
    assert_eq!(
        compiled.outputs()[2].value_type(),
        clipasm::model::ValueType::Audio
    );
}
