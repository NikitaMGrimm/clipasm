#![allow(missing_docs)]

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use clipasm::compiler;
use tempfile::TempDir;

fn compile_file(path: &Path) -> clipasm::diagnostic::Result<compiler::CompiledProgram> {
    let source = clipasm::language::parse_file(path)?;
    compiler::compile(&source)
}

fn project(source: &str) -> (TempDir, clipasm::source::SourcePackage) {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(directory.path().join("a.ppm"), b"P3\n1 1\n255\n255 0 0\n").expect("a image");
    fs::write(directory.path().join("b.ppm"), b"P3\n1 1\n255\n0 255 0\n").expect("b image");
    fs::write(directory.path().join("c.ppm"), b"P3\n1 1\n255\n0 0 255\n").expect("c image");
    fs::write(directory.path().join("x.ppm"), b"P3\n1 1\n255\n255 255 0\n").expect("x image");
    fs::write(directory.path().join("y.ppm"), b"P3\n1 1\n255\n0 255 255\n").expect("y image");
    let path = directory.path().join("workflow.clipasm");
    fs::write(&path, source).expect("workflow");
    let workflow = clipasm::language::parse_file(&path).expect("parse workflow");
    (directory, workflow)
}

fn compiled_json(compiled: &compiler::CompiledProgram) -> serde_json::Value {
    serde_json::from_str(&compiled.compiled_json().expect("compiled JSON")).expect("JSON value")
}

fn assert_last_slice_range(compiled: &compiler::CompiledProgram, start: u64, end: u64) {
    let json = compiled_json(compiled);
    let slice = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .rev()
        .find(|node| {
            node["value_type"] == "video"
                && node["kind"]["operation"] == "slice"
                && node["kind"]["unit"] == "frames"
        })
        .expect("trim slice");
    assert_eq!(slice["kind"]["range"]["start"], start);
    assert_eq!(slice["kind"]["range"]["end"], end);
}

fn assert_last_audio_slice_range(compiled: &compiler::CompiledProgram, start: u64, end: u64) {
    let json = compiled_json(compiled);
    let slice = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .rev()
        .find(|node| {
            node["value_type"] == "audio"
                && node["kind"]["operation"] == "slice"
                && node["kind"]["unit"] == "samples"
        })
        .expect("audio trim slice");
    assert_eq!(slice["kind"]["range"]["start"], start);
    assert_eq!(slice["kind"]["range"]["end"], end);
}

#[test]
fn explicitly_rooted_clip_placement_selects_its_exact_range() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"a.ppm\", 1s) as intro\n  image(\"b.ppm\", 2s) as credits\n} as edit\n$edit\nduring($edit::credits::start..$edit::credits::end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("marker-rooted during range");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        30
    );

    let json = compiled_json(&compiled);
    let replacement = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "replace_range")
        .expect("during replacement");
    assert_eq!(replacement["kind"]["range"]["start"], 10);
    assert_eq!(replacement["kind"]["range"]["end"], 30);
}

#[test]
fn timeline_placement_selector_is_a_complete_closed_open_range() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"a.ppm\", 1s) as intro\n  image(\"b.ppm\", 2s) as credits\n} as edit\n$edit\nduring($edit::credits) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("complete placement range");
    let json = compiled_json(&compiled);
    let replacement = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "replace_range")
        .expect("during replacement");
    assert_eq!(replacement["kind"]["range"]["start"], 10);
    assert_eq!(replacement["kind"]["range"]["end"], 30);
}

#[test]
fn unique_reference_marker_survives_identity_timeline_programs() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as interview\nclip {\n  $interview\n  zoom_in(8%)\n} as edit\n$edit\nduring($edit::interview::start..$edit::interview::end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("identity-preserved marker");
    let json = compiled_json(&compiled);
    let replacement = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "replace_range")
        .expect("during replacement");
    assert_eq!(replacement["kind"]["range"]["start"], 0);
    assert_eq!(replacement["kind"]["range"]["end"], 10);
}

#[test]
fn marker_selector_uses_the_bound_timeline_as_inference_context() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"interview.ppm\", 2s) as interview\nclip {\n  image(\"intro.ppm\", 1s)\n  $interview\n} as edit\nduring(timeline=$edit, range=$interview::start..$interview::end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("contextual marker root");
    let json = compiled_json(&compiled);
    let replacement = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "replace_range")
        .expect("during replacement");
    assert_eq!(replacement["kind"]["range"]["start"], 10);
    assert_eq!(replacement["kind"]["range"]["end"], 30);
}

#[test]
fn nested_clip_placements_form_explicit_selector_paths() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"a.ppm\", 1s) as intro\n  image(\"b.ppm\", 2s) as interview\n} as chapter\nclip {\n  $chapter\n  image(\"c.ppm\", 1s) as credits\n} as edit\n$edit\nduring($edit::chapter::interview::start..$edit::chapter::interview::end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("nested marker path");
    let json = compiled_json(&compiled);
    let replacement = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "replace_range")
        .expect("during replacement");
    assert_eq!(replacement["kind"]["range"]["start"], 10);
    assert_eq!(replacement["kind"]["range"]["end"], 30);
}

#[test]
fn duplicate_implicit_reference_markers_require_explicit_aliases() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as interview\nclip {\n  $interview\n  $interview\n} as edit\n$edit\nduring($edit::interview::start..$edit::interview::end) {\n  zoom_in(2%)\n}\n",
    );

    let error = compiler::compile(&workflow).expect_err("duplicate implicit marker");
    assert_eq!(error.code, "E_AMBIGUOUS_TIMELINE_PLACEMENT");
    assert!(error.message.contains("interview"));
    let layout = error.notes.join("\n");
    assert_eq!(
        layout
            .matches("interview (not directly addressable)")
            .count(),
        2
    );
}

#[test]
fn marker_ranges_must_belong_to_the_consumed_timeline() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"a.ppm\", 1s) as intro\n  image(\"b.ppm\", 1s) as credits\n} as edit\nimage(\"c.ppm\", 2s) as other\nduring(timeline=$other, range=$edit::credits::start..$edit::credits::end) {\n  zoom_in(2%)\n}\n",
    );

    let error = compiler::compile(&workflow).expect_err("marker root mismatch");
    assert_eq!(error.code, "E_TIMELINE_ROOT_MISMATCH");
    assert!(error.message.contains("does not belong"));
    assert!(
        error
            .notes
            .iter()
            .any(|note| note.contains("marker range root"))
    );
    assert!(
        error
            .notes
            .iter()
            .any(|note| note.contains("bound input 1"))
    );
}

#[test]
fn timeline_layout_diagnostics_are_bounded() {
    let mut source =
        String::from("clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n");
    for index in 0..70 {
        writeln!(source, "  image(\"{index}.ppm\", 100ms) as item_{index}")
            .expect("write source fixture");
    }
    source.push_str("} as edit\ntrim(value=$edit, range=$edit::missing)\n");
    let (_directory, workflow) = project(&source);

    let error = compiler::compile(&workflow).expect_err("missing marker in large layout");
    assert_eq!(error.code, "E_UNKNOWN_TIMELINE_PLACEMENT");
    let layout = error.notes.join("\n");
    assert!(layout.contains("timeline layout truncated"));
    assert!(layout.len() < 8_000);
}

#[test]
fn marker_ranges_remain_on_the_exact_project_frame_grid() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 30000 / 1001 } }\nclip {\n  image(\"a.ppm\", 1001ms) as first\n  image(\"b.ppm\", 1001ms) as second\n} as edit\n$edit\nduring($edit::second::start..$edit::second::end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("frame-native marker range");
    let json = compiled_json(&compiled);
    let replacement = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "replace_range")
        .expect("during replacement");
    assert_eq!(replacement["kind"]["range"]["start"], 30);
    assert_eq!(replacement["kind"]["range"]["end"], 60);
}

#[test]
fn timeline_coordinates_support_exact_addition_and_scaling() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"a.ppm\", 1s) as first\n  image(\"b.ppm\", 2s) as second\n} as edit\n$edit\nduring(50% * ($edit::first::start + $edit::second::start)..$edit::second::end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("coordinate arithmetic");
    let json = compiled_json(&compiled);
    let replacement = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "replace_range")
        .expect("during replacement");
    assert_eq!(replacement["kind"]["range"]["start"], 5);
    assert_eq!(replacement["kind"]["range"]["end"], 30);
}

#[test]
fn timeline_region_middle_is_an_exact_coordinate() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"section.ppm\", 2s) as section\n} as edit\n$edit\nduring($edit::section::middle..$edit::section::end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("exact region middle");
    let json = compiled_json(&compiled);
    let replacement = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "replace_range")
        .expect("during replacement");
    assert_eq!(replacement["kind"]["range"]["start"], 10);
    assert_eq!(replacement["kind"]["range"]["end"], 20);
}

#[test]
fn boundary_words_remain_addressable_as_placement_names_with_an_explicit_boundary() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"middle.ppm\", 1s) as middle\n} as edit\n$edit\ntrim(range=$edit::middle::start..$edit::middle::end)\n",
    );

    let compiled = compiler::compile(&workflow).expect("explicit boundary disambiguates placement");
    assert_last_slice_range(&compiled, 0, 10);
}

#[test]
fn unaligned_timeline_middle_fails_only_when_used_as_a_frame_boundary() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"section.ppm\", 500ms) as section\n} as edit\n$edit\nduring($edit::start..$edit::section::middle) {\n  zoom_in(2%)\n}\n",
    );

    let error = compiler::compile(&workflow).expect_err("half-frame middle must be rejected");
    assert_eq!(error.code, "E_TIME_NOT_FRAME_ALIGNED");
}

#[test]
fn timeline_coordinate_bounds_are_checked_only_at_parameter_use() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"a.ppm\", 1s) as first\n} as edit\n$edit\nduring($edit::start..($edit::end + 1s)) {\n  zoom_in(2%)\n}\n",
    );

    let error = compiler::compile(&workflow).expect_err("out-of-bounds final coordinate");
    assert_eq!(error.code, "E_INVALID_TIME_RANGE");
    assert!(error.message.contains("outside"));
}

#[test]
fn scalar_alias_infers_a_rooted_coordinate_and_can_be_reused() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"intro.ppm\", 1s) as intro\n  image(\"credits.ppm\", 2s) as credits\n} as edit\ncredits_lead_in = $edit::credits::start - 500ms\n$edit\nduring($credits_lead_in..$edit::credits::end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("inferred scalar alias");
    let json = compiled_json(&compiled);
    let replacement = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "replace_range")
        .expect("during replacement");
    assert_eq!(replacement["kind"]["range"]["start"], 5);
    assert_eq!(replacement["kind"]["range"]["end"], 30);
}

#[test]
fn scalar_aliases_support_forward_references() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"a.ppm\", 1s) as first\n  image(\"b.ppm\", 2s) as second\n} as edit\nrange_end = $range_start + 1s\nrange_start = $edit::second::start\n$edit\nduring($range_start..$range_end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("forward scalar aliases");
    let json = compiled_json(&compiled);
    let replacement = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "replace_range")
        .expect("during replacement");
    assert_eq!(replacement["kind"]["range"]["start"], 10);
    assert_eq!(replacement["kind"]["range"]["end"], 20);
}

#[test]
fn unused_out_of_bounds_scalar_alias_is_harmless() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as edit\nunused = $edit::end + 500s\n$edit\n",
    );

    let compiled = compiler::compile(&workflow).expect("unused coordinate is not consumed");
    assert!(!compiled.compiled_json().expect("compiled JSON").is_empty());
}

#[test]
fn scalar_aliases_support_complex_exact_marker_arithmetic() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"intro.ppm\", 1s) as intro\n  image(\"main.ppm\", 2s) as main\n} as edit\nhalf = 1 / 2\nmidpoint = $half * ($edit::main::start + $edit::main::end)\nrange_start = $midpoint - 500ms\nrange_end = $midpoint + 500ms\n$edit\nduring($range_start..$range_end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("complex marker aliases");
    let json = compiled_json(&compiled);
    let replacement = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "replace_range")
        .expect("during replacement");
    assert_eq!(replacement["kind"]["range"]["start"], 15);
    assert_eq!(replacement["kind"]["range"]["end"], 25);
}

#[test]
fn unused_negative_duration_alias_is_valid() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nsmall = 250ms\nbig = 1s\nnegative = $small - $big\nimage(\"a.ppm\", 1s)\n",
    );

    let compiled = compiler::compile(&workflow).expect("unused negative Duration");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        10
    );
}

#[test]
fn negative_duration_alias_can_be_brought_back_into_parameter_range() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nsmall = 250ms\nbig = 1s\nnegative = $small - $big\npositive = $negative + 1250ms\nimage(\"a.ppm\", $positive)\n",
    );

    let compiled = compiler::compile(&workflow).expect("corrected Duration alias");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        5
    );
}

#[test]
fn negative_duration_alias_fails_when_consumed_as_duration() {
    let (_directory, workflow) = project(
        "clipasm 1\nsmall = 250ms\nbig = 1s\nnegative = $small - $big\nimage(\"a.ppm\", $negative)\n",
    );

    let error = compiler::compile(&workflow).expect_err("negative Duration parameter");
    assert_eq!(error.code, "E_INVALID_DURATION");
    assert!(error.message.contains("negative"));
}

#[test]
fn out_of_bounds_coordinate_alias_fails_when_consumed_by_during() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as edit\nafter_end = $edit::end + 1s\n$edit\nduring($edit::start..$after_end) {\n  zoom_in(2%)\n}\n",
    );

    let error = compiler::compile(&workflow).expect_err("consumed out-of-bounds alias");
    assert_eq!(error.code, "E_INVALID_TIME_RANGE");
    assert!(error.message.contains("outside"));
}

#[test]
fn out_of_bounds_coordinate_alias_can_be_brought_back_in_range() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as edit\ntoo_early = $edit::start - 2s\nrange_start = $too_early + 2s\nrange_end = $range_start + 500ms\n$edit\nduring($range_start..$range_end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("corrected coordinate alias");
    let json = compiled_json(&compiled);
    let replacement = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "replace_range")
        .expect("during replacement");
    assert_eq!(replacement["kind"]["range"]["start"], 0);
    assert_eq!(replacement["kind"]["range"]["end"], 5);
}

#[test]
fn scalar_alias_declared_in_nested_body_does_not_escape() {
    let (_directory, workflow) = project(
        "clipasm 1\nimage(\"a.ppm\", 2s)\nduring(0s..1s) {\n  amount = 2%\n  zoom_in($amount)\n}\nzoom_in($amount)\n",
    );

    let error = compiler::compile(&workflow).expect_err("nested alias must not escape");
    assert_eq!(error.code, "E_MISSING_REFERENCE");
}

#[test]
fn unused_scalar_aliases_are_eagerly_checked_for_structure() {
    let cases = [
        (
            "missing reference",
            "clipasm 1\nmissing = $does_not_exist + 1s\nimage(\"a.ppm\", 1s)\n",
            "E_MISSING_REFERENCE",
        ),
        (
            "invalid operator",
            "clipasm 1\ninvalid = 1s * 2\nimage(\"a.ppm\", 1s)\n",
            "E_INVALID_SCALAR_OPERATION",
        ),
        (
            "graph value",
            "clipasm 1\nimage(\"a.ppm\", 1s) as picture\ninvalid = $picture\n",
            "E_INVALID_ARGUMENT_TYPE",
        ),
        (
            "dependency cycle",
            "clipasm 1\ncycle_a = $cycle_b + 1s\ncycle_b = $cycle_a + 1s\nimage(\"a.ppm\", 1s)\n",
            "E_DEPENDENCY_CYCLE",
        ),
    ];

    for (name, source, expected) in cases {
        let (_directory, workflow) = project(source);
        let error = compiler::compile(&workflow).expect_err(name);
        assert_eq!(error.code, expected, "{name}: {}", error.message);
    }
}

#[test]
fn unused_value_dependent_scalar_alias_errors_remain_inert() {
    let (_directory, workflow) = project(
        "clipasm 1\nimage(\"a.ppm\", 1s) as first\nimage(\"b.ppm\", 1s) as second\ndivision = 1 / 0\nmixed_roots = $first::start + $second::end\n",
    );

    let compiled = compiler::compile(&workflow).expect("unused values are not evaluated");
    assert!(!compiled.compiled_json().expect("compiled JSON").is_empty());
}

#[test]
fn reached_invalid_scalar_aliases_report_their_errors() {
    let cases = [
        (
            "division by zero",
            "clipasm 1\nbad = 1 / 0\nimage(\"a.ppm\", 1s)\nzoom_in($bad)\n",
            "E_DIVISION_BY_ZERO",
        ),
        (
            "missing reference",
            "clipasm 1\nbad = $missing + 1s\nimage(\"a.ppm\", $bad)\n",
            "E_MISSING_REFERENCE",
        ),
        (
            "invalid operator",
            "clipasm 1\nbad = 1s * 2\nimage(\"a.ppm\", $bad)\n",
            "E_INVALID_SCALAR_OPERATION",
        ),
        (
            "wrong final type",
            "clipasm 1\nbad = 1 / 2\nimage(\"a.ppm\", $bad)\n",
            "E_INVALID_ARGUMENT_TYPE",
        ),
        (
            "mixed timeline roots",
            "clipasm 1\nimage(\"a.ppm\", 1s) as first\nimage(\"b.ppm\", 1s) as second\nbad = $first::start + $second::end\ntrim(value=$first, range=$first::start..$bad)\n",
            "E_TIMELINE_ROOT_MISMATCH",
        ),
    ];

    for (name, source, expected) in cases {
        let (_directory, workflow) = project(source);
        let error = compiler::compile(&workflow).expect_err(name);
        assert_eq!(error.code, expected, "{name}: {}", error.message);
    }
}

#[test]
fn graph_values_are_rejected_during_eager_alias_checking() {
    let (_directory, workflow) = project(
        "clipasm 1\nimage(\"a.ppm\", 1s) as picture\ninvalid = $picture\nimage(\"b.ppm\", $invalid)\n",
    );

    let error = compiler::compile(&workflow).expect_err("graph value in scalar alias");
    assert_eq!(error.code, "E_INVALID_ARGUMENT_TYPE");
    assert!(error.message.contains("graph value `$picture`"));
}

#[test]
fn scalar_and_body_input_names_are_resolved_by_context() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nvideo = 2%\nimage(\"base.ppm\", 2s)\nduring(0s..1s) {\n  drop<Video>\n  zoom_in(video=$timeline, by=$video)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("context separates graph and scalar names");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        30
    );
}

#[test]
fn nested_aliases_may_capture_body_inputs() {
    let (_directory, workflow) = project(
        "clipasm 1\nimage(\"base.ppm\", 2s)\nduring(0s..1s) {\n  body_start = $timeline::start\n  body_end = $timeline::end\n  drop<Video>\n  trim(value=$timeline, range=$body_start..$body_end)\n}\n",
    );

    compiler::compile(&workflow).expect("body-input aliases are lexical captures");
}

#[test]
fn parent_scalar_aliases_are_visible_in_nested_bodies() {
    let (_directory, workflow) = project(
        "clipasm 1\namount = 2%\nimage(\"base.ppm\", 2s)\nduring(0s..1s) {\n  zoom_in($amount)\n}\n",
    );

    compiler::compile(&workflow).expect("parent alias is visible in child body");
}

#[test]
fn sibling_bodies_may_reuse_scalar_alias_names() {
    let (_directory, workflow) = project(
        "clipasm 1\nset_audio(\n  video={\n    count = 1\n    image(\"base.ppm\", 1s)\n    repeat($count)\n  },\n  audio={\n    count = 1\n    audio(\"missing.wav\")\n    repeat($count)\n  },\n)\n",
    );

    compiler::compile(&workflow).expect("sibling scalar scopes are independent");
}

#[test]
fn nested_scalar_aliases_cannot_shadow_visible_aliases() {
    let (_directory, workflow) = project(
        "clipasm 1\namount = 2%\nimage(\"base.ppm\", 1s)\nduring(0s..1s) {\n  amount = 3%\n  zoom_in($amount)\n}\n",
    );

    let error = compiler::compile(&workflow).expect_err("visible alias shadowing");
    assert_eq!(error.code, "E_DUPLICATE_NAME");
    assert!(error.message.contains("shadows a visible alias"));
}

#[test]
fn scalar_alias_cannot_be_used_as_a_graph_statement() {
    let (_directory, workflow) = project("clipasm 1\nlength = 1s\n$length\n");

    let error = compiler::compile(&workflow).expect_err("scalar alias as graph value");
    assert_eq!(error.code, "E_SCALAR_NOT_VALUE");
}

#[test]
fn alias_does_not_retroactively_merge_original_roots_after_join() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as a\nimage(\"b.ppm\", 1s) as b\njoin as joined\nbad = $a::start + $b::end\nduring(timeline=$joined, range=$joined::start..$bad) { zoom_in(2%) }\n",
    );

    let error = compiler::compile(&workflow).expect_err("original roots remain distinct");
    assert_eq!(error.code, "E_TIMELINE_ROOT_MISMATCH");
    assert!(
        error
            .notes
            .iter()
            .any(|note| note.contains("left coordinate root") && note.contains("`$a` [0s..1s)"))
    );
    assert!(
        error
            .notes
            .iter()
            .any(|note| note.contains("right coordinate root") && note.contains("`$b` [0s..1s)"))
    );
}

#[test]
fn unknown_placement_reports_the_real_canonical_layout_tree() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s)\nimage(\"b.ppm\", 1s) as b\nconcat as joined\ntrim(value=$joined, range=$joined::missing)\n",
    );

    let error = compiler::compile(&workflow).expect_err("canonical layout diagnostic");
    assert_eq!(error.code, "E_UNKNOWN_TIMELINE_PLACEMENT");
    let layout = error.notes.join("\n");
    assert!(layout.contains("timeline layout for `$joined`"));
    assert!(layout.contains("├── <unnamed> (not directly addressable) [0s..1s)"));
    assert!(layout.contains("└── b [1s..2s)"));
}

#[test]
fn explicit_and_implicit_placement_names_do_not_shadow_each_other() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as a\n$a\nconcat as joined\ntrim(value=$joined, range=$joined::a)\n",
    );

    let error = compiler::compile(&workflow).expect_err("same spelling is ambiguous");
    assert_eq!(error.code, "E_AMBIGUOUS_TIMELINE_PLACEMENT");
    assert!(error.message.contains("2 placements named `a`"));
    assert_eq!(
        error
            .notes
            .join("\n")
            .matches("a (not directly addressable)")
            .count(),
        2
    );
}

#[test]
fn operation_owned_placement_names_reject_surviving_collisions() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"a.ppm\", 1s) as replacement\n  image(\"b.ppm\", 1s) as target\n} as edit\n$edit\nduring($edit::target) {\n  drop<Video>\n  image(\"c.ppm\", 500ms)\n} as revised\n",
    );

    let error = compiler::compile(&workflow).expect_err("reserved placement collision");
    assert_eq!(error.code, "E_TIMELINE_PLACEMENT_CONFLICT");
    assert!(error.message.contains("replacement"));
    assert!(error.notes.join("\n").contains("base timeline"));
}

#[test]
fn operation_owned_placement_name_may_replace_a_removed_collision() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"a.ppm\", 1s) as replacement\n  image(\"b.ppm\", 1s) as tail\n} as edit\n$edit\nduring($edit::replacement) {\n  drop<Video>\n  image(\"c.ppm\", 500ms)\n} as revised\ntrim(value=$revised, range=$revised::replacement)\n",
    );

    let compiled = compiler::compile(&workflow).expect("removed collision does not survive");
    assert_last_slice_range(&compiled, 0, 5);
}

#[test]
fn alias_can_address_joined_children_through_a_named_parent_root() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as a\nimage(\"b.ppm\", 1s) as b\njoin as joined\nend = $joined::a::start + $joined::b::end\nduring(timeline=$joined, range=$joined::start..$end) { zoom_in(2%) }\n",
    );

    let compiled = compiler::compile(&workflow).expect("joined child alias");
    let json = compiled_json(&compiled);
    let replacement = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "replace_range")
        .expect("during replacement");
    assert_eq!(replacement["kind"]["range"]["start"], 0);
    assert_eq!(replacement["kind"]["range"]["end"], 20);
}

#[test]
fn unnamed_composite_supports_direct_contextual_child_selectors() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\n{\n  image(\"a.ppm\", 1s) as a\n  image(\"b.ppm\", 1s) as b\n  concat\n}\nduring(range=$a::start..$b::end) { zoom_in(2%) }\n",
    );

    let compiled = compiler::compile(&workflow).expect("contextual unnamed composite selectors");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        20
    );
}

#[test]
fn unnamed_join_result_supplies_context_to_the_next_timeline_consumer() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as a\nimage(\"b.ppm\", 1s) as b\njoin { concat }\ntrim(range=$a::start..$b::end)\n",
    );

    let compiled = compiler::compile(&workflow).expect("unnamed joined timeline context");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        20
    );
}

#[test]
fn join_without_body_concat_still_supplies_context_to_a_named_consumer() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as a\nimage(\"b.ppm\", 1s) as b\njoin as joined\ntrim(value=$joined, range=$a::start..$b::end)\n",
    );

    let compiled = compiler::compile(&workflow).expect("body-concat finalizer supplies context");
    assert_last_slice_range(&compiled, 0, 20);
}

#[test]
fn join_body_has_no_aggregate_timeline_before_finalization() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as a\nimage(\"b.ppm\", 1s) as b\njoin {\n  point = $a::start + $b::start\n  concat\n  trim(range=$a::start..$point)\n} as joined\n",
    );

    let error = compiler::compile(&workflow).expect_err("join body inputs remain separate roots");
    assert_eq!(error.code, "E_TIMELINE_ROOT_MISMATCH");
}

#[test]
fn join_body_can_create_a_contextual_root_before_finalization() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as a\nimage(\"b.ppm\", 1s) as b\njoin {\n  concat\n  trim(range=$a::start..$b::end)\n} as joined\n",
    );

    let compiled = compiler::compile(&workflow).expect("body-created contextual root");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        20
    );
}

#[test]
fn contextual_selector_can_skip_unique_named_ancestors() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as a\nimage(\"b.ppm\", 1s) as b\nconcat as pair\nimage(\"c.ppm\", 1s) as c\nconcat as edit\ntrim(value=$edit, range=$a::start..$c::end)\n",
    );

    let compiled = compiler::compile(&workflow).expect("unique descendant shorthand");
    assert_last_slice_range(&compiled, 0, 30);
}

#[test]
fn contextual_selector_can_match_a_unique_nested_suffix_path() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as a\nimage(\"b.ppm\", 1s) as b\nconcat as pair\nimage(\"x.ppm\", 1s) as x\nconcat as section\nimage(\"c.ppm\", 1s) as c\nconcat as edit\ntrim(value=$edit, range=$pair::a::start..$c::end)\n",
    );

    let compiled = compiler::compile(&workflow).expect("unique nested suffix shorthand");
    assert_last_slice_range(&compiled, 0, 40);
}

#[test]
fn contextual_selector_rejects_an_ambiguous_descendant_name() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as a\nclip { $a } as first_pair\nclip { $a } as second_pair\n{\n  $first_pair\n  $second_pair\n  concat\n} as edit\ntrim(value=$edit, range=$a::start..$edit::end)\n",
    );

    let error = compiler::compile(&workflow).expect_err("ambiguous descendant shorthand");
    assert_eq!(error.code, "E_AMBIGUOUS_TIMELINE_PLACEMENT");
    assert!(error.message.contains('a'));
}

#[test]
fn contextual_selector_handles_shared_layout_dags_without_expanding_occurrences() {
    let mut source = String::from(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"leaf.ppm\", 100ms) as leaf\ndrop<Video>\n",
    );
    let mut previous = "leaf".to_owned();
    for depth in 0..32 {
        let next = format!("level_{depth}");
        writeln!(
            source,
            "clip {{\n  ${previous}\n  ${previous}\n}} as {next}"
        )
        .expect("write shared layout level");
        previous = next;
    }
    writeln!(source, "${previous}").expect("write final reference");
    writeln!(
        source,
        "trim(value=${previous}, range=$leaf::start..$leaf::end)"
    )
    .expect("write contextual selector");
    let (_directory, workflow) = project(&source);

    let error = compiler::compile(&workflow).expect_err("shared leaf occurs many times");
    assert_eq!(error.code, "E_AMBIGUOUS_TIMELINE_PLACEMENT");
    assert!(error.message.contains("multiple placements"));
}

#[test]
fn contextual_selector_rejects_same_level_duplicate_names() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as a\n$a\nconcat as edit\ntrim(value=$edit, range=$a::start..$edit::end)\n",
    );

    let error = compiler::compile(&workflow).expect_err("same-level contextual ambiguity");
    assert_eq!(error.code, "E_AMBIGUOUS_TIMELINE_PLACEMENT");
    assert!(error.message.contains("matches multiple placements"));
}

#[test]
fn alias_cannot_borrow_an_unnamed_composite_as_its_parent_root() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\n{\n  image(\"a.ppm\", 1s) as a\n  image(\"b.ppm\", 1s) as b\n  concat\n}\nbad = $a::start + $b::end\nduring(range=$a::start..$bad) { zoom_in(2%) }\n",
    );

    let error = compiler::compile(&workflow).expect_err("aliases require an explicit parent root");
    assert_eq!(error.code, "E_TIMELINE_ROOT_MISMATCH");
}

#[test]
fn anonymous_concat_layers_do_not_change_named_child_paths() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as a\nimage(\"b.ppm\", 1s) as b\nconcat\nconcat as joined\ntrim(value=$joined, range=$joined::a::start..$joined::b::end)\n",
    );

    let compiled = compiler::compile(&workflow).expect("transparent anonymous concat");
    assert_last_slice_range(&compiled, 0, 20);
}

#[test]
fn join_body_anonymous_concat_does_not_change_named_child_paths() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as a\nimage(\"b.ppm\", 1s) as b\njoin {\n  concat\n} as joined\ntrim(value=$joined, range=$joined::a::start..$joined::b::end)\n",
    );

    let compiled = compiler::compile(&workflow).expect("transparent join body concat");
    assert_last_slice_range(&compiled, 0, 20);
}

#[test]
fn anonymous_concat_regrouping_is_associative_for_layout_paths() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as a\nimage(\"b.ppm\", 1s) as b\nconcat\nimage(\"c.ppm\", 1s) as c\nconcat as joined\ntrim(value=$joined, range=$joined::a::start..$joined::c::end)\n",
    );

    let compiled = compiler::compile(&workflow).expect("associative anonymous concat layout");
    assert_last_slice_range(&compiled, 0, 30);
}

#[test]
fn explicit_named_concat_remains_a_selector_boundary() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as a\nimage(\"b.ppm\", 1s) as b\nconcat as pair\nimage(\"c.ppm\", 1s) as c\nconcat as joined\ntrim(value=$joined, range=$joined::pair::a::start..$joined::c::end)\n",
    );

    let compiled = compiler::compile(&workflow).expect("named concat boundary");
    assert_last_slice_range(&compiled, 0, 30);
}

#[test]
fn explicit_named_concat_cannot_be_skipped_in_a_selector_path() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as a\nimage(\"b.ppm\", 1s) as b\nconcat as pair\nimage(\"c.ppm\", 1s) as c\nconcat as joined\ntrim(value=$joined, range=$joined::a)\n",
    );

    let error = compiler::compile(&workflow).expect_err("named boundary must not flatten");
    assert_eq!(error.code, "E_UNKNOWN_TIMELINE_PLACEMENT");
}

#[test]
fn join_body_names_work_on_either_side_of_anonymous_concat() {
    for body in ["concat as j\n  concat", "concat\n  concat as j"] {
        let source = format!(
            "clipasm 1\nconfig {{ video {{ width = 64\nheight = 64\nfps = 10 }} }}\nimage(\"a.ppm\", 1s) as a\nimage(\"b.ppm\", 1s) as b\njoin {{\n  {body}\n}}\nend = $j::a::start + $j::b::end\ntrim(value=$j, range=$j::start..$end)\n"
        );
        let (_directory, workflow) = project(&source);
        let compiled = compiler::compile(&workflow).expect("stable join body name");
        assert_last_slice_range(&compiled, 0, 20);
    }
}

#[test]
fn anonymous_transition_layout_flattens_into_an_outer_concat() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s)\nimage(\"b.ppm\", 1s)\ncrossfade(400ms)\nimage(\"c.ppm\", 1s)\nconcat as edit\ntrim(value=$edit, range=$edit::overlap)\n",
    );

    let compiled = compiler::compile(&workflow).expect("transparent unnamed transition");
    assert_last_slice_range(&compiled, 6, 10);
}

#[test]
fn named_transition_layout_remains_nested_in_an_outer_concat() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s)\nimage(\"b.ppm\", 1s)\ncrossfade(400ms) as transition\nimage(\"c.ppm\", 1s)\nconcat as edit\ntrim(value=$edit, range=$edit::transition::overlap)\n",
    );

    let compiled = compiler::compile(&workflow).expect("named transition boundary");
    assert_last_slice_range(&compiled, 6, 10);
}

#[test]
fn anonymous_replacement_layout_flattens_into_an_outer_concat() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 2s)\nduring(1s..2s) {\n  drop<Video>\n  image(\"b.ppm\", 500ms)\n}\nimage(\"c.ppm\", 1s)\nconcat as edit\ntrim(value=$edit, range=$edit::replacement)\n",
    );

    let compiled = compiler::compile(&workflow).expect("transparent unnamed replacement");
    assert_last_slice_range(&compiled, 10, 15);
}

#[test]
fn media_dependent_marker_range_compiles_without_reading_the_video() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nvideo(\"missing.mkv\") as source\ntrim(range=($source::start + 200ms)..($source::end - 300ms))\n",
    );

    let compiled = compiler::compile(&workflow).expect("deferred marker compilation");
    assert!(compiled.result_domain().is_none());
    let json = compiled_json(&compiled);
    assert!(
        json["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .any(|node| node["kind"]["operation"] == "slice")
    );
}

#[test]
fn media_dependent_during_inherits_its_requested_extent_without_reading_media() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nvideo(\"missing.mkv\") as source\nduring(range=($source::start + 200ms)..($source::end - 300ms)) {\n  drop<Video>\n  image(\"replacement.ppm\")\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("deferred during compilation");
    assert!(compiled.result_domain().is_none());
    let document = compiled_json(&compiled);
    let operations = document["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|node| node["kind"]["operation"].as_str().expect("operation"))
        .collect::<Vec<_>>();
    assert!(operations.contains(&"deferred_image_video"));
    assert!(operations.contains(&"replace_range"));
}

#[test]
fn crossfade_exposes_before_after_and_overlap_regions() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s)\nimage(\"b.ppm\", 1s)\ncrossfade(400ms) as transition\ntrim(range=$transition::overlap)\n",
    );

    let compiled = compiler::compile(&workflow).expect("crossfade overlap marker");
    assert_eq!(
        compiled.result_domain().expect("known overlap").frames().0,
        4
    );
    let json = compiled_json(&compiled);
    let slice = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "slice")
        .expect("overlap slice");
    assert_eq!(slice["kind"]["range"]["start"], 6);
    assert_eq!(slice["kind"]["range"]["end"], 10);
}

#[test]
fn crossfade_overlap_middle_uses_exact_coordinate_arithmetic() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s)\nimage(\"b.ppm\", 1s)\ncrossfade(400ms) as transition\ntrim(range=$transition::overlap::middle..$transition::overlap::end)\n",
    );

    let compiled = compiler::compile(&workflow).expect("crossfade overlap midpoint");
    assert_eq!(
        compiled
            .result_domain()
            .expect("known half-overlap")
            .frames()
            .0,
        2
    );
}

#[test]
fn crossfade_input_regions_retain_nested_marker_layouts() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"a.ppm\", 1s) as title\n} as first\n$first\nimage(\"b.ppm\", 1s)\ncrossfade(400ms) as transition\ntrim(range=$transition::before::title)\n",
    );

    let compiled = compiler::compile(&workflow).expect("nested crossfade input marker");
    assert_eq!(
        compiled.result_domain().expect("known title").frames().0,
        10
    );
}

#[test]
fn flash_cut_exposes_sequential_before_and_after_regions() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s)\nimage(\"b.ppm\", 2s)\nflash_cut(400ms) as transition\ntrim(range=$transition::after)\n",
    );

    let compiled = compiler::compile(&workflow).expect("flash-cut after marker");
    assert_eq!(
        compiled.result_domain().expect("known after").frames().0,
        20
    );
}

#[test]
fn during_splices_unaffected_base_placements_and_rebases_the_suffix() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"intro.ppm\", 1s) as intro\n  image(\"main.ppm\", 2s) as main\n  image(\"credits.ppm\", 1s) as credits\n} as edit\n$edit\nduring(range=$edit::main) {\n  drop<Video>\n  image(\"replacement.ppm\", 1s)\n} as revised\ntrim(range=$revised::credits)\n",
    );

    let compiled = compiler::compile(&workflow).expect("spliced suffix marker");
    assert_eq!(
        compiled.result_domain().expect("known credits").frames().0,
        10
    );
    let json = compiled_json(&compiled);
    let slice = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .rev()
        .find(|node| node["kind"]["operation"] == "slice")
        .expect("credits slice");
    assert_eq!(slice["kind"]["range"]["start"], 20);
    assert_eq!(slice["kind"]["range"]["end"], 30);
}

#[test]
fn during_drops_base_placements_intersecting_the_replaced_range() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"intro.ppm\", 1s) as intro\n  image(\"main.ppm\", 2s) as main\n  image(\"credits.ppm\", 1s) as credits\n} as edit\n$edit\nduring(range=$edit::main) {\n  drop<Video>\n  image(\"replacement.ppm\", 1s)\n} as revised\ntrim(range=$revised::main)\n",
    );

    let error = compiler::compile(&workflow).expect_err("replaced placement must disappear");
    assert_eq!(error.code, "E_UNKNOWN_TIMELINE_PLACEMENT");
}

#[test]
fn during_body_seed_preserves_the_selected_nested_layout() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"intro.ppm\", 1s) as intro\n  clip {\n    image(\"a.ppm\", 1s) as a\n    image(\"b.ppm\", 1s) as b\n  } as main\n  $main\n  image(\"outro.ppm\", 1s) as outro\n} as edit\n$edit\nduring(range=$edit::main) {\n  trim(range=$b::start..$b::end)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("selected layout is available in the body");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        30
    );
}

#[test]
fn no_op_during_preserves_selected_children_under_replacement() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"intro.ppm\", 1s) as intro\n  image(\"main.ppm\", 2s) as main\n  image(\"outro.ppm\", 1s) as outro\n} as edit\n$edit\nduring(range=$edit::main) as revised\ndrop<Video>\ntrim(value=$revised, range=$revised::replacement::main)\n",
    );

    let compiled = compiler::compile(&workflow).expect("selected child survives under replacement");
    assert_eq!(
        compiled
            .result_domain()
            .expect("known selected child")
            .frames()
            .0,
        20
    );
}

#[test]
fn during_layout_orders_replacement_between_surviving_siblings() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"intro.ppm\", 1s) as intro\n  image(\"main.ppm\", 2s) as main\n  image(\"outro.ppm\", 1s) as outro\n} as edit\n$edit\nduring(range=$edit::main) as revised\ntrim(value=$revised, range=$revised::missing)\n",
    );

    let error = compiler::compile(&workflow).expect_err("unknown placement shows canonical order");
    assert_eq!(error.code, "E_UNKNOWN_TIMELINE_PLACEMENT");
    let layout = error
        .notes
        .iter()
        .find(|note| note.contains("timeline layout for `$revised`"))
        .expect("layout note");
    let intro = layout.find("intro").expect("intro placement");
    let replacement = layout.find("replacement").expect("replacement placement");
    let outro = layout.find("outro").expect("outro placement");
    assert!(intro < replacement && replacement < outro, "{layout}");
}

#[test]
fn during_exposes_the_replacement_and_its_nested_layout() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"base.ppm\", 3s)\nduring(1s..2s) {\n  drop<Video>\n  clip {\n    image(\"lead.ppm\", 500ms) as lead\n    image(\"body.ppm\", 1500ms) as body\n  } as inserted\n  $inserted\n} as revised\ntrim(range=$revised::replacement::lead)\n",
    );

    let compiled = compiler::compile(&workflow).expect("nested replacement marker");
    assert_eq!(compiled.result_domain().expect("known lead").frames().0, 5);
}

#[test]
fn during_symbolic_replacement_rebases_later_placements_exactly() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"intro.ppm\", 1s) as intro\n  video(\"missing.mkv\") as main\n  image(\"credits.ppm\", 1s) as credits\n} as edit\n$edit\nduring(range=$edit::main) {\n  drop<Video>\n  image(\"replacement.ppm\", 1s)\n} as revised\ntrim(range=$revised::credits)\n",
    );

    let compiled = compiler::compile(&workflow).expect("symbolic replacement splice");
    let json = compiled_json(&compiled);
    let slice = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .rev()
        .find(|node| node["kind"]["operation"] == "slice")
        .expect("credits slice");
    assert_eq!(slice["kind"]["range"]["start"], 20);
    assert_eq!(slice["kind"]["range"]["end"], 30);
}

#[test]
fn scalar_alias_cycles_report_the_complete_path() {
    let (_directory, workflow) =
        project("clipasm 1\na = $b + 1s\nb = $a + 1s\nimage(\"a.ppm\", $a)\n");

    let error = compiler::compile(&workflow).expect_err("scalar cycle must fail");
    assert_eq!(error.code, "E_DEPENDENCY_CYCLE");
    assert!(error.message.contains("a -> b -> a"));
}

#[test]
fn scalar_alias_selector_does_not_borrow_later_invocation_context() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"interview.ppm\", 2s) as interview\nclip {\n  image(\"intro.ppm\", 1s)\n  $interview\n} as edit\nstart = $interview::start\nduring(timeline=$edit, range=$start..($start + 1s)) {\n  zoom_in(2%)\n}\n",
    );

    let error = compiler::compile(&workflow).expect_err("alias root must remain explicit");
    assert_eq!(error.code, "E_TIMELINE_ROOT_MISMATCH");
}

#[test]
fn scalar_aliases_infer_number_and_duration_types() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nlength = 500ms\ncount = 6 / 2\nimage(\"a.ppm\", $length)\nrepeat($count)\n",
    );

    let compiled = compiler::compile(&workflow).expect("ordinary scalar aliases");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        15
    );
}

#[test]
fn scalar_aliases_cannot_collide_with_named_graph_values() {
    let (_directory, workflow) =
        project("clipasm 1\nimage(\"a.ppm\", 1s) as shared\nshared = 500ms\n$shared\n");

    let error = compiler::compile(&workflow).expect_err("duplicate local name must fail");
    assert_eq!(error.code, "E_DUPLICATE_NAME");
}

#[test]
fn duplicate_scalar_aliases_in_one_body_are_rejected() {
    let (_directory, workflow) =
        project("clipasm 1\namount = 2%\namount = 3%\nimage(\"a.ppm\", 1s)\n");

    let error = compiler::compile(&workflow).expect_err("duplicate scalar alias");
    assert_eq!(error.code, "E_DUPLICATE_NAME");
    assert!(error.message.contains("same body"));
}

#[test]
fn scalar_alias_declarations_have_no_stack_effect() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s)\nunused = 500ms\nimage(\"b.ppm\", 2s)\nconcat\n",
    );

    let compiled = compiler::compile(&workflow).expect("zero-stack scalar declaration");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        30
    );
}

#[test]
fn repeat_one_preserves_timeline_layout_as_a_true_identity() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"a.ppm\", 1s) as a\n  image(\"b.ppm\", 1s) as b\n} as edit\n$edit\nrepeat(1) as same\ntrim(value=$same, range=$same::b)\n",
    );

    let compiled = compiler::compile(&workflow).expect("repeat one preserves markers");
    assert_last_slice_range(&compiled, 10, 20);
}

#[test]
fn repeat_multiple_keeps_a_fresh_unindexed_timeline() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"a.ppm\", 1s) as a\n  image(\"b.ppm\", 1s) as b\n} as edit\n$edit\nrepeat(2) as repeated\ntrim(value=$repeated, range=$repeated::b)\n",
    );

    let error =
        compiler::compile(&workflow).expect_err("repeated children need indexing semantics");
    assert_eq!(error.code, "E_UNKNOWN_TIMELINE_PLACEMENT");
}

#[test]
fn repeat_multiple_preserves_an_exact_root_extent_without_child_markers() {
    let (_video_directory, video_workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s)\nrepeat(2) as repeated\ndrop<Video>\ntrim(value=$repeated, range=$repeated::start..$repeated::end)\n",
    );
    let video = compiler::compile(&video_workflow).expect("exact repeated Video extent");
    assert_last_slice_range(&video, 0, 20);

    let (_audio_directory, audio_workflow) = project(
        "clipasm 1\naudio(\"missing.wav\")\ntrim(0s..1s)\nrepeat(2) as repeated\ndrop<Audio>\ntrim(value=$repeated, range=$repeated::start..$repeated::end)\n",
    );
    let audio = compiler::compile(&audio_workflow).expect("exact repeated Audio extent");
    assert_last_audio_slice_range(&audio, 0, 96_000);
}

#[test]
fn repeat_reuses_one_upstream_value() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  $later\n  repeat(3)\n} as doubled\nclip { image(\"a.ppm\", 1s) } as later\n$doubled\n",
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
fn zoom_in_defaults_to_eight_percent_and_preserves_the_video_domain() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\n{\n  image(\"a.ppm\", 1s)\n  zoom_in\n  concat\n}\n",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        10
    );

    let json = compiled_json(&compiled);
    let zoom_in = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["origin"]["construct"] == "zoom_in")
        .expect("zoom_in node");
    assert_eq!(zoom_in["kind"]["operation"], "zoom_in");
    assert_eq!(zoom_in["kind"]["by"], "2/25");
    assert_eq!(zoom_in["domain"]["frames"], 10);
    assert_eq!(zoom_in["domain"]["width"], 64);
    assert_eq!(zoom_in["domain"]["height"], 64);
}

#[test]
fn equivalent_zoom_in_numbers_have_equal_identity() {
    let source = |zoom_in: &str| {
        format!(
            "clipasm 1\nconfig {{ video {{ width = 64\nheight = 64\nfps = 10 }} }}\n{{\n  image(\"a.ppm\", 1s)\n  {zoom_in}\n  concat\n}}\n"
        )
    };
    let (_omitted_directory, omitted) = project(&source("zoom_in"));
    let (_percent_directory, percent) = project(&source("zoom_in(8%)"));
    let (_decimal_directory, decimal) = project(&source("zoom_in(0.08)"));
    let (_fraction_directory, fraction) = project(&source("zoom_in(2 / 25)"));
    let (_repeated_postfix_directory, repeated_postfix) = project(&source("zoom_in(800%%)"));
    let (_changed_directory, changed) = project(&source("zoom_in(9%)"));

    let omitted = compiler::compile(&omitted).expect("omitted default");
    let percent = compiler::compile(&percent).expect("percentage");
    let decimal = compiler::compile(&decimal).expect("decimal");
    let fraction = compiler::compile(&fraction).expect("fraction");
    let repeated_postfix =
        compiler::compile(&repeated_postfix).expect("repeated percentage postfix");
    let changed = compiler::compile(&changed).expect("changed amount");
    assert_eq!(omitted.structure_hash(), percent.structure_hash());
    assert_eq!(omitted.structure_hash(), decimal.structure_hash());
    assert_eq!(omitted.structure_hash(), fraction.structure_hash());
    assert_eq!(omitted.structure_hash(), repeated_postfix.structure_hash());
    assert_ne!(omitted.structure_hash(), changed.structure_hash());
}

#[test]
fn empty_argument_delimiters_do_not_change_invocation_semantics() {
    let source = |invocation: &str| {
        format!(
            "clipasm 1\nconfig {{ video {{ width = 64\nheight = 64\nfps = 10 }} }}\nimage(\"a.ppm\", 1s)\n{invocation}\n"
        )
    };
    let (_bare_directory, bare) = project(&source("zoom_in"));
    let (_parenthesized_directory, parenthesized) = project(&source("zoom_in()"));
    assert_eq!(
        compiler::compile(&bare)
            .expect("bare invocation")
            .structure_hash(),
        compiler::compile(&parenthesized)
            .expect("parenthesized invocation")
            .structure_hash()
    );
}

#[test]
fn exact_number_expressions_refine_to_integer_at_the_parameter_boundary() {
    let (_directory, accepted) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s)\nrepeat(6 / 2)\n",
    );
    let compiled = compiler::compile(&accepted).expect("exact integer result");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        30
    );

    let (_directory, rejected) = project("clipasm 1\nimage(\"a.ppm\", 1s)\nrepeat(5 / 2)\n");
    let error = compiler::compile(&rejected).expect_err("fraction is not Integer");
    assert_eq!(error.code, "E_INVALID_ARGUMENT_TYPE");
    assert!(error.message.contains("evaluates to 2.5"));
    assert_eq!(error.notes, ["exact value: 5/2"]);

    let (_directory, referenced) =
        project("clipasm 1\nparam count: Number = 5 / 2\nimage(\"a.ppm\", 1s)\nrepeat($count)\n");
    let error = compiler::compile(&referenced).expect_err("referenced fraction is not Integer");
    assert!(
        error
            .notes
            .iter()
            .any(|note| note == "scalar parameter `$count` evaluated to 2.5 (exactly 5/2)")
    );
}

#[test]
fn duration_arithmetic_and_integer_unit_postfixes_are_exact() {
    let (_directory, subtraction) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 100s - 100ms)\n",
    );
    let compiled = compiler::compile(&subtraction).expect("duration subtraction");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        999
    );

    let (_directory, converted) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 1000 } }\nimage(\"a.ppm\", (6 / 2)ms)\n",
    );
    let compiled = compiler::compile(&converted).expect("integer expression in milliseconds");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        3
    );
}

#[test]
fn duration_unit_and_mixed_arithmetic_errors_follow_types_and_precedence() {
    let (_directory, fractional_unit) = project("clipasm 1\nimage(\"a.ppm\", (5 / 2)ms)\n");
    let error = compiler::compile(&fractional_unit).expect_err("ms requires Integer");
    assert_eq!(error.code, "E_INVALID_ARGUMENT_TYPE");
    assert!(error.message.contains("evaluates to 2.5"));

    let (_directory, mixed_division) = project("clipasm 1\nimage(\"a.ppm\", 5 / 2ms)\n");
    let error = compiler::compile(&mixed_division).expect_err("Number / Duration is undefined");
    assert_eq!(error.code, "E_INVALID_SCALAR_OPERATION");
    assert!(error.message.contains("got Number and Duration"));
}

#[test]
fn scalar_parameter_references_participate_in_exact_expressions() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nparam marker: Duration = 100s - 100ms\nparam by: Number = 800%%\nimage(\"a.ppm\", $marker)\nzoom_in($by)\n",
    );
    let compiled = compiler::compile(&workflow).expect("parameter expressions");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        999
    );
    let json = compiled_json(&compiled);
    let zoom_in = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "zoom_in")
        .expect("zoom_in");
    assert_eq!(zoom_in["kind"]["by"], "2/25");
}

#[test]
fn zoom_in_rejects_nonpositive_amounts() {
    for amount in [-1, 0] {
        let (_directory, workflow) = project(&format!(
            "clipasm 1\n{{\n  image(\"a.ppm\", 1s)\n  zoom_in({amount})\n  concat\n}}\n"
        ));
        let error = compiler::compile(&workflow).expect_err("invalid zoom_in amount");
        assert_eq!(error.code, "E_INVALID_ZOOM_AMOUNT");
        assert!(error.message.contains("positive"));
    }
}

#[test]
fn zoom_in_consumes_only_the_top_video() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\n{\n  image(\"a.ppm\", 1s)\n  image(\"b.ppm\", 1s)\n  zoom_in(12%)\n  concat\n}\n",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        20
    );

    let json = compiled_json(&compiled);
    let nodes = json["nodes"].as_array().expect("nodes");
    let zoom_in = nodes
        .iter()
        .find(|node| node["kind"]["operation"] == "zoom_in")
        .expect("zoom_in");
    assert_eq!(zoom_in["kind"]["input"]["id"], 1);
    assert_eq!(zoom_in["kind"]["by"], "3/25");
    assert_eq!(nodes.last().expect("result")["kind"]["operation"], "concat");
}

#[test]
fn flash_cut_inside_join_binds_in_order_and_preserves_the_summed_domain() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 } }\n{\n  image(\"a.ppm\", 1s)\n  image(\"b.ppm\", 1s)\n  join { flash_cut }\n  concat\n}\n",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        20
    );

    let json = compiled_json(&compiled);
    let flash_cut = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "flash_cut")
        .expect("flash_cut");
    assert_eq!(flash_cut["kind"]["before"]["id"], 0);
    assert_eq!(flash_cut["kind"]["after"]["id"], 1);
    assert_eq!(flash_cut["kind"]["frames"], 2);
    assert_eq!(flash_cut["domain"]["frames"], 20);
}

#[test]
fn explicit_flash_cut_inputs_preserve_unrelated_stack_occurrences() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 } }\nclip { image(\"x.ppm\", 1s) } as x\nclip { image(\"y.ppm\", 1s) } as y\n{\n  image(\"a.ppm\", 1s)\n  image(\"b.ppm\", 1s)\n  flash_cut(before=$x, after=$y, duration=200ms)\n  concat\n}\n",
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
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 } }\nflash_cut(\n  before={\n    image(\"a.ppm\", 1s)\n    zoom_in\n  },\n  after=image(\"b.ppm\", 1s),\n  duration=200ms,\n)\n",
    );

    let compiled = compiler::compile(&program).expect("inline fixed inputs");
    assert_eq!(
        compiled.result_domain().expect("known result").frames().0,
        20
    );
    let document = compiled_json(&compiled);
    let flash_cut = document["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "flash_cut")
        .expect("flash_cut result");
    assert_eq!(flash_cut["kind"]["before"]["id"], 1);
    assert_eq!(flash_cut["kind"]["after"]["id"], 2);
}

#[test]
fn inline_input_bodies_start_empty_and_preserve_the_outer_stack() {
    let (_directory, isolated) =
        project("clipasm 1\nimage(\"a.ppm\", 1s)\nrepeat(value={ repeat(2) }, count=2)\n");
    let error = compiler::compile(&isolated).expect_err("isolated input stack");
    assert_eq!(error.code, "E_STACK_UNDERFLOW");

    let (_directory, preserved) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 } }\nimage(\"a.ppm\", 1s)\nrepeat(value=image(\"b.ppm\", 1s), count=2)\nconcat\n",
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
        "clipasm 1\nrepeat(value={\n  image(\"a.ppm\", 1s)\n  image(\"b.ppm\", 1s)\n}, count=2)\n",
    );
    let error = compiler::compile(&program).expect_err("two inline results");
    assert_eq!(error.code, "E_INPUT_BODY_OUTPUT_COUNT");
    assert!(error.message.contains("repeat.value"));
    assert!(error.message.contains("2 values remain"));
}

#[test]
fn inline_input_bodies_inherit_requested_frames() {
    let (_directory, program) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 } }\nimage(\"a.ppm\", 2s)\nduring(500ms..1500ms) {\n  flash_cut(before=image(\"b.ppm\"), after=image(\"c.ppm\"))\n  concat\n}\n",
    );
    let compiled = compiler::compile(&program).expect("requested inline duration");
    assert_eq!(
        compiled.result_domain().expect("known result").frames().0,
        40
    );
}

#[test]
fn inline_fixed_inputs_bind_in_descriptor_order_not_argument_order() {
    let source = |inputs: &str| {
        format!(
            "clipasm 1\nconfig {{ video {{ width = 64\nheight = 48\nfps = 10 }} }}\nflash_cut({inputs}duration=200ms)\n"
        )
    };
    let before = "before=image(\"a.ppm\", 1s), ";
    let after = "after=image(\"b.ppm\", 1s), ";
    let (_first_directory, first) = project(&source(&format!("{before}{after}")));
    let (_second_directory, second) = project(&source(&format!("{after}{before}")));

    assert_eq!(
        compiler::compile(&first)
            .expect("declaration order")
            .structure_hash(),
        compiler::compile(&second)
            .expect("reverse argument order")
            .structure_hash()
    );
}

#[test]
fn ids_inside_inline_inputs_use_the_global_named_value_namespace() {
    let (_directory, program) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 } }\nflash_cut(\n  before={\n    image(\"a.ppm\", 1s) as reusable\n    zoom_in\n  },\n  after=image(\"b.ppm\", 1s),\n  duration=200ms,\n)\n$reusable\nzoom_in\nconcat\n",
    );
    let compiled = compiler::compile(&program).expect("global inline output binding");
    assert_eq!(
        compiled.result_domain().expect("known result").frames().0,
        30
    );
    assert!(compiled_json(&compiled)["named_values"]["reusable"].is_object());
}

#[test]
fn ids_inside_clip_bodies_are_visible_to_the_source_body() {
    let (_directory, program) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 } }\nclip {\n  image(\"a.ppm\", 1s) as reusable\n  zoom_in\n} as decorated\n$reusable\nzoom_in\n",
    );
    let compiled = compiler::compile(&program).expect("global clip-body output binding");
    assert_eq!(
        compiled.result_domain().expect("known result").frames().0,
        10
    );
    assert!(compiled_json(&compiled)["named_values"]["reusable"].is_object());
}

#[test]
fn duplicate_names_cross_inline_input_boundaries() {
    let (_directory, duplicate) = project(
        "clipasm 1\nflash_cut(\n  before={ image(\"a.ppm\", 1s) as duplicate },\n  after=image(\"b.ppm\", 1s),\n)\nimage(\"c.ppm\", 1s) as duplicate\nconcat\n",
    );
    assert_eq!(
        compiler::compile(&duplicate)
            .expect_err("duplicate inline output binding")
            .code,
        "E_DUPLICATE_NAME"
    );
}

#[test]
fn dependency_cycles_cross_inline_input_boundaries() {
    let (_directory, cycle) = project(
        "clipasm 1\nflash_cut(\n  before={\n    $outer\n    zoom_in as inner\n  },\n  after={ image(\"b.ppm\", 1s) },\n)\n$inner\nzoom_in as outer\nconcat\n",
    );
    assert_eq!(
        compiler::compile(&cycle)
            .expect_err("cross-boundary cycle")
            .code,
        "E_DEPENDENCY_CYCLE"
    );
}

#[test]
fn flash_cut_identity_normalizes_the_default_and_preserves_order_and_duration() {
    let source = |before: &str, after: &str, duration: &str| {
        format!(
            "clipasm 1\nconfig {{ video {{ width = 64\nheight = 48\nfps = 10 }} }}\nclip {{ image(\"a.ppm\", 1s) }} as a\nclip {{ image(\"b.ppm\", 1s) }} as b\nflash_cut(before=${before}, after=${after}{duration})\n"
        )
    };
    let (_omitted_directory, omitted) = project(&source("a", "b", ""));
    let (_explicit_directory, explicit) = project(&source("a", "b", ", duration=160ms"));
    let (_changed_directory, changed) = project(&source("a", "b", ", duration=300ms"));
    let (_reversed_directory, reversed) = project(&source("b", "a", ", duration=160ms"));

    let omitted = compiler::compile(&omitted).expect("omitted default");
    let explicit = compiler::compile(&explicit).expect("explicit default");
    let changed = compiler::compile(&changed).expect("changed duration");
    let reversed = compiler::compile(&reversed).expect("reversed inputs");
    assert_eq!(omitted.structure_hash(), explicit.structure_hash());
    assert_ne!(omitted.structure_hash(), changed.structure_hash());
    assert_ne!(omitted.structure_hash(), reversed.structure_hash());
}

#[test]
fn flash_cut_rejects_empty_or_known_excessive_duration() {
    for duration in ["0ms", "1100ms"] {
        let (_directory, workflow) = project(&format!(
            "clipasm 1\nconfig {{ video {{ width = 64\nheight = 48\nfps = 10 }} }}\n{{\n  image(\"a.ppm\", 1s)\n  image(\"b.ppm\", 1s)\n  flash_cut({duration})\n  concat\n}}\n"
        ));
        let error = compiler::compile(&workflow).expect_err("invalid flash_cut duration");
        assert_eq!(error.code, "E_INVALID_FLASH_CUT_DURATION");
    }
}

#[test]
fn flash_cut_duration_uses_the_smallest_covering_project_frame_count() {
    for (fps, expected) in [("1", 1_u64), ("30000/1001", 5)] {
        let (_directory, workflow) = project(&format!(
            "clipasm 1\nconfig {{ video {{ width = 64\nheight = 48\nfps = {fps} }} }}\n{{\n  image(\"a.ppm\", 1001s)\n  image(\"b.ppm\", 1001s)\n  flash_cut(160ms)\n  concat\n}}\n"
        ));
        let compiled = compiler::compile(&workflow).expect("compile");
        let json = compiled_json(&compiled);
        let flash_cut = json["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .find(|node| node["kind"]["operation"] == "flash_cut")
            .expect("flash_cut");
        assert_eq!(flash_cut["kind"]["frames"], expected);
    }
}

#[test]
fn during_changes_duration() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\n{\n  image(\"a.ppm\", 10s)\n  during(4s..6s) { repeat(2) }\n  concat\n}\n",
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
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\n{\n  image(\"a.ppm\", 3s)\n  trim(1s..2s)\n  concat\n}\n",
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
fn trim_accepts_a_rooted_timeline_marker_range() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"intro.ppm\", 1s) as intro\n  image(\"main.ppm\", 2s) as main\n} as edit\ntrim(value=$edit, range=$edit::main)\n",
    );

    let compiled = compiler::compile(&workflow).expect("marker-based trim");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        20
    );
}

#[test]
fn trim_rebases_fully_contained_placements() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"intro.ppm\", 1s) as intro\n  image(\"main.ppm\", 2s) as main\n  image(\"credits.ppm\", 1s) as credits\n} as edit\n$edit\ntrim(range=$edit::main::start..$edit::end) as tail\ntrim(range=$tail::credits)\n",
    );

    let compiled = compiler::compile(&workflow).expect("rebased trim markers");
    assert_eq!(
        compiled.result_domain().expect("known credits").frames().0,
        10
    );
}

#[test]
fn trim_drops_partially_surviving_placements() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"intro.ppm\", 1s) as intro\n  image(\"main.ppm\", 2s) as main\n  image(\"credits.ppm\", 1s) as credits\n} as edit\n$edit\ntrim(range=$edit::main::middle..$edit::end) as tail\ntrim(range=$tail::main)\n",
    );

    let error = compiler::compile(&workflow).expect_err("partial placement must disappear");
    assert_eq!(error.code, "E_UNKNOWN_TIMELINE_PLACEMENT");
}

#[test]
fn trim_preserves_the_occurrence_label_for_a_later_clip() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"interview.ppm\", 2s) as interview\nclip {\n  $interview\n  trim(range=500ms..1500ms)\n} as edit\ntrim(value=$edit, range=$edit::interview)\n",
    );

    let compiled = compiler::compile(&workflow).expect("trimmed implicit placement");
    let json = compiled_json(&compiled);
    let range = json["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .rev()
        .find(|node| node["kind"]["operation"] == "slice")
        .expect("placement slice");
    assert_eq!(range["kind"]["range"]["start"], 0);
    assert_eq!(range["kind"]["range"]["end"], 10);
}

#[test]
fn trim_preserves_a_symbolically_selected_complete_placement() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"intro.ppm\", 1s) as intro\n  video(\"missing.mkv\") as main\n} as edit\n$edit\ntrim(range=$edit::main) as selected\ntrim(range=$selected::main)\n",
    );

    let compiled = compiler::compile(&workflow).expect("symbolic full placement crop");
    assert!(compiled.result_domain().is_none());
}

#[test]
fn nested_stack_blocks_with_concat_preserve_composition_order() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\n{\n  image(\"a.ppm\", 1s)\n  {\n    image(\"b.ppm\", 1s)\n    image(\"c.ppm\", 1s)\n    concat\n  }\n  concat\n}\n",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        30
    );
}

#[test]
fn stack_block_is_transparent_to_explicit_visible_consumers_by_default() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s)\n{ @visible repeat(2) }\nconcat\n",
    );
    let compiled = compiler::compile(&workflow).expect("default visible stack block");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        20
    );
}

#[test]
fn default_visible_body_program_can_bind_through_a_default_stack_block() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 2s)\n{ during(500ms..1500ms) { repeat(2) } }\n",
    );
    let compiled = compiler::compile(&workflow).expect("body program through stack block");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        30
    );
}

#[test]
fn owned_stack_block_still_blocks_explicit_visible_consumers() {
    let (_directory, workflow) =
        project("clipasm 1\nimage(\"a.ppm\", 1s)\n@owned { @visible repeat(2) }\n");
    let error = compiler::compile(&workflow).expect_err("owned stack block boundary");
    assert_eq!(error.code, "E_STACK_UNDERFLOW");
    assert!(error.message.contains("preceding Video or Audio value"));
}

#[test]
fn default_visible_join_binds_its_inputs_from_the_visible_suffix() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s)\nimage(\"b.ppm\", 1s)\n{ join {}\nconcat }\n",
    );
    let compiled = compiler::compile(&workflow).expect("default visible join binding");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        20
    );
}

#[test]
fn omitted_join_body_is_the_same_as_an_empty_body() {
    let source = |join: &str| {
        format!(
            "clipasm 1\nconfig {{ video {{ width = 64\nheight = 64\nfps = 10 }} }}\nimage(\"a.ppm\", 1s)\nimage(\"b.ppm\", 1s)\n{join}\n"
        )
    };
    let mut hashes = Vec::new();
    for invocation in ["join", "join()", "join {}", "join() {}"] {
        let (_directory, workflow) = project(&source(invocation));
        let compiled = compiler::compile(&workflow).expect("join invocation");
        assert_eq!(
            compiled.result_domain().expect("known domain").frames().0,
            20
        );
        hashes.push(compiled.structure_hash().to_owned());
    }
    assert!(hashes.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn omitted_during_body_is_the_same_as_an_empty_body() {
    let source = |during: &str| {
        format!(
            "clipasm 1\nconfig {{ video {{ width = 64\nheight = 64\nfps = 10 }} }}\nimage(\"a.ppm\", 2s)\n{during}\n"
        )
    };
    let (_omitted_directory, omitted) = project(&source("during(500ms..1500ms)"));
    let (_empty_directory, empty) = project(&source("during(500ms..1500ms) {}"));
    let omitted = compiler::compile(&omitted).expect("omitted during body");
    let empty = compiler::compile(&empty).expect("empty during body");
    assert_eq!(omitted.structure_hash(), empty.structure_hash());
    assert_eq!(
        omitted.result_domain().expect("known domain").frames().0,
        20
    );
}

#[test]
fn omitted_body_uses_the_existing_input_diagnostic() {
    let (_omitted_directory, omitted) = project("clipasm 1\njoin\n");
    let (_empty_directory, empty) = project("clipasm 1\njoin {}\n");
    let omitted = compiler::compile(&omitted).expect_err("missing join inputs");
    let empty = compiler::compile(&empty).expect_err("missing join inputs");
    assert_eq!(omitted.code, empty.code);
    assert_eq!(omitted.message, empty.message);
}

#[test]
fn default_visible_during_binds_its_timeline_from_the_visible_suffix() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 2s)\n{ during(500ms..1500ms) { repeat(2) }\nconcat }\n",
    );
    let compiled = compiler::compile(&workflow).expect("default visible during binding");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        30
    );
}

#[test]
fn visible_body_does_not_make_its_children_visible() {
    let (_directory, workflow) =
        project("clipasm 1\nimage(\"a.ppm\", 1s)\n{ repeat<Video>(2)\nconcat<Video> }\n");
    let error = compiler::compile(&workflow).expect_err("owned repeat cannot capture");
    assert_eq!(error.code, "E_STACK_UNDERFLOW");
    assert!(error.message.contains("owned"));
    assert!(
        error
            .notes
            .iter()
            .any(|note| { note.contains("additional Video value") && note.contains("@visible") })
    );
}

#[test]
fn owned_concat_reduces_only_values_captured_by_an_earlier_visible_operation() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s)\nimage(\"b.ppm\", 1s)\nimage(\"c.ppm\", 2s)\nduring(500ms..1500ms) {\n  @visible flash_cut(100ms)\n  image(\"x.ppm\", 1s)\n  concat\n}\nconcat\n",
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
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s)\n{\n  image(\"b.ppm\", 1s)\n  @visible concat\n}\n",
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
        "clipasm 1\nimage(\"a.ppm\", 1s)\n{\n  image(\"b.ppm\", 2s)\n  @owned during(500ms..1500ms) { @visible flash_cut(100ms) }\n  concat\n}\n",
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
fn omitted_stack_block_access_matches_explicit_visible_identity() {
    let source = |access: &str| {
        format!("clipasm 1\nimage(\"a.ppm\", 1s)\n{access}{{ @visible repeat(2)\nconcat }}\n")
    };
    let (_default_directory, default) = project(&source(""));
    let (_visible_directory, visible) = project(&source("@visible "));

    assert_eq!(
        compiler::compile(&default)
            .expect("default block access")
            .structure_hash(),
        compiler::compile(&visible)
            .expect("explicit visible block access")
            .structure_hash()
    );
}

#[test]
fn no_op_stack_access_does_not_change_semantic_identity() {
    let (_owned_directory, owned) = project("clipasm 1\nimage(\"a.ppm\", 1s)\n");
    let (_visible_directory, visible) = project("clipasm 1\n@visible image(\"a.ppm\", 1s)\n");
    assert_eq!(
        compiler::compile(&owned).expect("owned").structure_hash(),
        compiler::compile(&visible)
            .expect("visible")
            .structure_hash()
    );
}

#[test]
fn positional_and_named_during_ranges_have_the_same_semantics() {
    let source = |during: &str| {
        format!(
            "clipasm 1\nconfig {{ video {{ width = 64\nheight = 64\nfps = 10 }} }}\n{{\n  image(\"a.ppm\", 10s)\n{during}\n  concat\n}}\n"
        )
    };
    let (_positional_directory, positional) = project(&source("  during(4s..6s) { repeat(2) }"));
    let (_explicit_directory, explicit) = project(&source("  during(range=4s..6s) { repeat(2) }"));
    assert_eq!(
        compiler::compile(&positional)
            .expect("positional")
            .structure_hash(),
        compiler::compile(&explicit)
            .expect("explicit")
            .structure_hash()
    );
}

#[test]
fn output_binding_names_the_during_result() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\n{\n  image(\"a.ppm\", 10s)\n  during(4s..6s) { repeat(2) } as edited\n  concat\n}\n",
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
        "clipasm 1\n{\n  image(\"a.ppm\", 10s)\n  during(4s..6s) { image(\"b.ppm\", 2s) }\n  concat\n}\n",
    );
    let error = compiler::compile(&workflow).expect_err("selected plus source");
    assert_eq!(error.code, "E_BODY_OUTPUT_COUNT");
}

#[test]
fn join_preserves_nested_markers_from_untouched_inputs() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip { image(\"a.ppm\", 1s) as title } as first\nclip { image(\"b.ppm\", 2s) as body } as second\n$first\n$second\njoin {} as joined\ntrim(value=$joined, range=$joined::first::title)\n",
    );

    let compiled = compiler::compile(&workflow).expect("joined input marker");
    let document = compiled_json(&compiled);
    let slice = document["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "slice")
        .expect("title slice");
    assert_eq!(slice["kind"]["range"]["start"], 0);
    assert_eq!(slice["kind"]["range"]["end"], 10);
}

#[test]
fn join_exposes_named_values_created_by_its_body() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s)\nimage(\"b.ppm\", 1s)\njoin { image(\"bridge.ppm\", 500ms) as bridge } as joined\ntrim(value=$joined, range=$joined::bridge)\n",
    );

    let compiled = compiler::compile(&workflow).expect("joined body marker");
    let document = compiled_json(&compiled);
    let slice = document["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "slice")
        .expect("bridge slice");
    assert_eq!(slice["kind"]["range"]["start"], 20);
    assert_eq!(slice["kind"]["range"]["end"], 25);
}

#[test]
fn join_concatenates_leftover_body_videos_in_order() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\n{\n  image(\"a.ppm\", 1s)\n  image(\"b.ppm\", 1s)\n  join { zoom_in }\n  concat\n}\n",
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
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\n{\n  image(\"a.ppm\", 1s)\n  image(\"b.ppm\", 1s)\n  image(\"c.ppm\", 1s)\n  join { concat }\n  concat\n}\n",
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
fn owned_stack_block_preserves_join_stack_occurrences() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip { image(\"x.ppm\", 1s) } as x\nclip { image(\"y.ppm\", 1s) } as y\n{\n  image(\"a.ppm\", 1s)\n  image(\"b.ppm\", 1s)\n  join {\n    {\n      $x\n      $y\n      concat\n    }\n    concat\n  }\n  concat\n}\n",
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
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip { image(\"x.ppm\", 1s) } as x\nclip { image(\"y.ppm\", 1s) } as y\n{\n  image(\"a.ppm\", 1s)\n  image(\"b.ppm\", 1s)\n  join(before=$x, after=$y) { concat }\n  concat\n}\n",
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
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip { image(\"y.ppm\", 1s) } as y\n{\n  image(\"a.ppm\", 1s)\n  join(after=$y) { concat }\n  concat\n}\n",
    );
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        20
    );
}

#[test]
fn clip_sugar_is_visible_and_hides_cleanup() {
    let (_directory, workflow) = project(
        "clipasm 1\nclip {\n  image(\"a.ppm\", 1s)\n  image(\"b.ppm\", 1s)\n} as pair\n$pair\n",
    );
    let compiled = compiler::compile(&workflow).expect("compiled native clip");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        60
    );
    let constructs = compiled
        .explain()
        .iter()
        .map(clipasm::compiler::ExplainEntry::construct)
        .collect::<Vec<_>>();
    assert_eq!(
        constructs
            .iter()
            .filter(|construct| **construct == "clip")
            .count(),
        1
    );
    assert!(!constructs.contains(&"drop"));
}

#[test]
fn generated_clip_diagnostics_use_the_authored_construct() {
    let (_directory, workflow) =
        project("clipasm 1\nclip {\n  image(\"a.ppm\", 1s)\n  audio(\"a.wav\")\n} as mixed\n");
    let error = compiler::compile(&workflow).expect_err("mixed clip body");
    assert!(error.message.contains("`clip`"), "{}", error.message);
    assert!(!error.message.contains("`concat`"), "{}", error.message);
}

#[test]
fn reports_readable_named_cycle() {
    let (_directory, workflow) = project("clipasm 1\nclip { $b } as a\nclip { $a } as b\n$a\n");
    let error = compiler::compile(&workflow).expect_err("cycle");
    assert_eq!(error.code, "E_DEPENDENCY_CYCLE");
    assert!(error.message.contains("a -> b -> a"));
}

#[test]
fn named_argument_order_does_not_change_compiled_structure() {
    let (_first_dir, first) = project(
        "clipasm 1\nclip { image(path=\"a.ppm\", duration=1s) } as a\nclip { image(path=\"b.ppm\", duration=1s) } as b\n{ $a\n$b\nconcat }\n",
    );
    let (_second_dir, second) = project(
        "clipasm 1\nclip { image(duration=1s, path=\"a.ppm\") } as a\nclip { image(duration=1s, path=\"b.ppm\") } as b\n{ $a\n$b\nconcat }\n",
    );
    let first_compiled = compiler::compile(&first).expect("first");
    let second_compiled = compiler::compile(&second).expect("second");
    assert_eq!(
        first_compiled.structure_hash(),
        second_compiled.structure_hash()
    );
}

#[test]
fn explicit_concat_and_nested_stack_block_have_the_same_semantics() {
    let header =
        "clipasm 1\nclip { image(\"a.ppm\", 1s) } as a\nclip { image(\"b.ppm\", 1s) } as b\n";
    let (_concat_directory, concat) = project(&format!("{header}$a\n$b\nconcat\n"));
    let (_nested_directory, nested) = project(&format!("{header}{{ $a\n$b\nconcat }}\n"));
    let concat = compiler::compile(&concat).expect("explicit concat");
    let nested = compiler::compile(&nested).expect("nested stack block");
    assert_eq!(concat.result_domain(), nested.result_domain());
    assert_eq!(concat.structure_hash(), nested.structure_hash());
}

#[test]
fn compile_file_accepts_an_outputless_validation_workflow() {
    let (directory, _workflow) = project("clipasm 1\nimage(\"a.ppm\", 1s)\n");
    compile_file(&directory.path().join(Path::new("workflow.clipasm"))).expect("compile");
}

#[test]
fn comments_do_not_change_structure_hash() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("workflow.clipasm");
    let first = clipasm::language::parse_str(&path, "clipasm 1\nimage(\"a.ppm\", 1s)\n")
        .expect("first parse");
    let second = clipasm::language::parse_str(
        &path,
        "# formatting is not semantic\nclipasm 1\nimage(duration=1s, path=\"a.ppm\")\n",
    )
    .expect("second parse");
    assert_eq!(
        compiler::compile(&first).expect("first").structure_hash(),
        compiler::compile(&second).expect("second").structure_hash()
    );
}

#[test]
fn authored_source_paths_change_compiled_identity() {
    let path = Path::new("workflow.clipasm");
    for (program, first_path, second_path, suffix) in [
        ("image", "a.png", "b.png", ", 1s"),
        ("video", "a.mp4", "b.mp4", ""),
    ] {
        let source = |asset: &str| format!("clipasm 1\n{program}(\"{asset}\"{suffix})\n");
        let first = clipasm::language::parse_str(path, &source(first_path)).expect("first");
        let second = clipasm::language::parse_str(path, &source(second_path)).expect("second");
        assert_ne!(
            compiler::compile(&first).expect("first").structure_hash(),
            compiler::compile(&second).expect("second").structure_hash(),
            "{program} path must contribute to compiled identity"
        );
    }
}

#[test]
fn during_accepts_explicit_visible_access() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 2s)\n@visible during(500ms..1s) { repeat(2) }\n",
    );
    let compiled = compiler::compile(&workflow).expect("visible during");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        25
    );
}

#[test]
fn body_program_inputs_are_available_as_local_references_after_stack_consumption() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s)\nimage(\"b.ppm\", 1s)\njoin {\n  flash_cut\n  $before\n  concat\n}\n",
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
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 5s)\nduring(1s..3s) {\n  during(timeline=$timeline, range=2s..4s) { repeat(1) }\n  concat\n}\n",
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
        "clipasm 1\nconfig { output = \"final.mp4\" }\naudio(\"missing.wav\")\nimage(\"a.ppm\", 1s)\n",
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
    let (_directory, workflow) = project("clipasm 1\naudio(\"missing.wav\")\ntrim(0s..1s)\n");
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
    assert!(
        document["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .any(|node| {
                node["value_type"] == "audio"
                    && node["kind"]["operation"] == "slice"
                    && node["kind"]["unit"] == "samples"
            })
    );
    assert!(!operations.contains(&"audio_on_black"));
    assert!(!operations.contains(&"extract_audio"));
}

#[test]
fn audio_markers_resolve_immediately_when_sample_extents_are_known() {
    let (_directory, workflow) = project(
        "clipasm 1\naudio(\"first.wav\")\ntrim(0s..1s) as first\naudio(\"second.wav\")\ntrim(0s..2s) as second\nconcat as mix\ntrim(value=$mix, range=$mix::second)\n",
    );

    let compiled = compiler::compile(&workflow).expect("known Audio marker range");
    assert_last_audio_slice_range(&compiled, 48_000, 144_000);
}

#[test]
fn audio_markers_remain_deferred_until_source_sample_counts_are_known() {
    let (_directory, workflow) = project(
        "clipasm 1\naudio(\"first.wav\") as first\naudio(\"second.wav\") as second\njoin as mix\ntrim(value=$mix, range=$second::start..$second::end)\n",
    );

    let compiled = compiler::compile(&workflow).expect("deferred Audio marker range");
    let document = compiled_json(&compiled);
    let slice = document["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .rev()
        .find(|node| node["value_type"] == "audio" && node["kind"]["operation"] == "slice")
        .expect("deferred Audio slice");
    assert_eq!(
        slice["kind"]["range"]["start"]["terms"]
            .as_array()
            .expect("start terms")
            .len(),
        1
    );
    assert_eq!(
        slice["kind"]["range"]["end"]["terms"]
            .as_array()
            .expect("end terms")
            .len(),
        2
    );
}

#[test]
fn audio_trim_preserves_a_symbolically_selected_complete_placement() {
    let (_directory, workflow) = project(
        "clipasm 1\naudio(\"first.wav\") as first\naudio(\"second.wav\") as second\njoin as mix\ntrim(value=$mix, range=$mix::second) as selected\ntrim(value=$selected, range=$selected::second)\n",
    );

    let compiled = compiler::compile(&workflow).expect("symbolic Audio placement crop");
    assert_eq!(
        compiled_json(&compiled)["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .filter(|node| {
                node["value_type"] == "audio" && node["kind"]["operation"] == "slice"
            })
            .count(),
        2
    );
}

#[test]
fn during_infers_audio_and_emits_a_native_sample_replacement() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { audio { sample_rate = 1000 } }\naudio(\"missing.wav\")\nduring(100ms..300ms) { repeat(2) }\n",
    );

    let compiled = compiler::compile(&workflow).expect("native Audio during");
    assert_eq!(
        compiled.outputs()[0].value_type(),
        clipasm::model::ValueType::Audio
    );
    let document = compiled_json(&compiled);
    let replacement = document["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| {
            node["value_type"] == "audio"
                && node["kind"]["operation"] == "replace_range"
                && node["kind"]["unit"] == "samples"
        })
        .expect("Audio replacement");
    assert_eq!(replacement["kind"]["range"]["start"], 100);
    assert_eq!(replacement["kind"]["range"]["end"], 300);
}

#[test]
fn during_exposes_the_complete_audio_input_as_timeline() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { audio { sample_rate = 1000 } }\naudio(\"missing.wav\") as song\nduring(timeline=$song, range=100ms..200ms) {\n  drop<Audio>\n  trim(value=$timeline, range=0ms..50ms)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("Audio body timeline alias");
    assert!(
        compiled_json(&compiled)["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .any(|node| {
                node["value_type"] == "audio"
                    && node["kind"]["operation"] == "slice"
                    && node["kind"]["unit"] == "samples"
                    && node["kind"]["range"]["start"] == 0
                    && node["kind"]["range"]["end"] == 50
            })
    );
}

#[test]
fn audio_during_markers_remain_deferred_until_source_domains_are_known() {
    let (_directory, workflow) = project(
        "clipasm 1\naudio(\"first.wav\") as first\naudio(\"second.wav\") as second\njoin as mix\nduring(timeline=$mix, range=$mix::second) { repeat(2) }\n",
    );

    let compiled = compiler::compile(&workflow).expect("deferred Audio during");
    let document = compiled_json(&compiled);
    let replacement = document["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["value_type"] == "audio" && node["kind"]["operation"] == "replace_range")
        .expect("deferred Audio replacement");
    assert_eq!(
        replacement["kind"]["range"]["start"]["terms"]
            .as_array()
            .expect("start terms")
            .len(),
        1
    );
    assert_eq!(
        replacement["kind"]["range"]["end"]["terms"]
            .as_array()
            .expect("end terms")
            .len(),
        2
    );
}

#[test]
fn audio_during_rebases_later_placements_to_exact_samples_when_extents_are_known() {
    let (_directory, workflow) = project(
        "clipasm 1\naudio(\"first.wav\")\ntrim(0s..1s) as intro\naudio(\"second.wav\")\ntrim(0s..2s) as section\naudio(\"third.wav\")\ntrim(0s..1s) as outro\nconcat as song\nduring(timeline=$song, range=$song::section) { repeat(2) } as revised\ntrim(value=$revised, range=$revised::outro)\n",
    );

    let compiled = compiler::compile(&workflow).expect("exact Audio replacement layout shift");
    assert_last_audio_slice_range(&compiled, 240_000, 288_000);
}

#[test]
fn no_op_audio_during_preserves_selected_children_under_replacement() {
    let (_directory, workflow) = project(
        "clipasm 1\naudio(\"first.wav\")\ntrim(0s..1s) as intro\naudio(\"second.wav\")\ntrim(0s..2s) as main\naudio(\"third.wav\")\ntrim(0s..1s) as outro\nconcat as mix\nduring(timeline=$mix, range=$mix::main) as revised\ndrop<Audio>\ntrim(value=$revised, range=$revised::replacement::main)\n",
    );

    let compiled = compiler::compile(&workflow).expect("Audio selected child survives replacement");
    assert_last_audio_slice_range(&compiled, 48_000, 144_000);
}

#[test]
fn audio_during_requires_an_audio_body_result() {
    let (_directory, workflow) = project(
        "clipasm 1\naudio(\"missing.wav\")\nduring<Audio>(0s..1s) {\n  drop<Audio>\n  image(\"a.ppm\", 1s)\n}\n",
    );

    let error = compiler::compile(&workflow).expect_err("mismatched Audio during body");
    assert_eq!(error.code, "E_GENERIC_TYPE_MISMATCH");
}

#[test]
fn duplicate_audio_placement_names_are_ambiguous() {
    let (_directory, workflow) = project(
        "clipasm 1\naudio(\"tone.wav\") as tone\n$tone\nconcat as mix\ntrim(value=$mix, range=$mix::tone)\n",
    );

    let error = compiler::compile(&workflow).expect_err("duplicate Audio placement");
    assert_eq!(error.code, "E_AMBIGUOUS_TIMELINE_PLACEMENT");
}

#[test]
fn audio_trim_rejects_a_video_marker_root() {
    let (_directory, workflow) = project(
        "clipasm 1\nimage(\"a.ppm\", 1s) as picture\naudio(\"tone.wav\") as sound\ntrim(value=$sound, range=$picture::start..$picture::end)\n",
    );

    let error = compiler::compile(&workflow).expect_err("cross-media marker root");
    assert_eq!(error.code, "E_TIMELINE_ROOT_MISMATCH");
    assert!(error.message.contains("does not belong"));
}

#[test]
fn nested_explicit_ports_compose_direct_audio_video_adaptations() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nset_audio(\n  video=image(\"a.ppm\", 4s),\n  audio=zoom_in(video=audio(\"missing.wav\")),\n)\n",
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
    assert!(operations.contains(&"zoom_in"));
    assert!(operations.contains(&"extract_audio"));
    assert!(operations.contains(&"set_audio"));
}

#[test]
fn nested_invocation_resolutions_are_consumed_by_id() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nset_audio(\n  audio={ audio(\"missing.wav\")\nrepeat(3) },\n  video={ image(\"a.ppm\", 1s)\nrepeat(2) },\n)\n",
    );
    let compiled = compiler::compile(&workflow).expect("nested invocation resolutions");
    assert_eq!(
        compiled.result_domain().expect("Video result").frames().0,
        20
    );
    let document = compiled_json(&compiled);
    let nodes = document["nodes"].as_array().expect("nodes");
    assert!(
        nodes
            .iter()
            .any(|node| node["kind"]["operation"] == "repeat" && node["kind"]["count"] == 2)
    );
    assert!(nodes.iter().any(|node| {
        node["value_type"] == "audio"
            && node["kind"]["operation"] == "repeat"
            && node["kind"]["count"] == 3
    }));
}

#[test]
fn nested_stack_blocks_keep_distinct_resolved_output_sequences() {
    let (_directory, workflow) = project(
        "clipasm 1\n@owned {\n  @owned {\n    image(\"a.ppm\", 1s)\n    audio(\"missing.wav\")\n  }\n  image(\"b.ppm\", 1s)\n} as (first, sound, last)\n",
    );
    let compiled = compiler::compile(&workflow).expect("nested stack-block resolutions");
    assert_eq!(
        compiled
            .outputs()
            .iter()
            .map(|output| output.value_type())
            .collect::<Vec<_>>(),
        vec![
            clipasm::model::ValueType::Video,
            clipasm::model::ValueType::Audio,
            clipasm::model::ValueType::Video,
        ]
    );
}

#[test]
fn parenthesized_output_bindings_name_each_timeline_occurrence_in_order() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\n@owned {\n  image(\"a.ppm\", 1s)\n  image(\"b.ppm\", 2s)\n  image(\"c.ppm\", 3s)\n} as (first, middle, last)\nconcat as edit\ntrim(value=$edit, range=$edit::middle::start..$edit::last::end)\n",
    );

    let compiled = compiler::compile(&workflow).expect("ordered tuple placement labels");
    assert_last_slice_range(&compiled, 10, 60);
}

#[test]
fn parenthesized_output_names_follow_reordered_reference_occurrences() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\n@owned {\n  image(\"a.ppm\", 1s)\n  image(\"b.ppm\", 2s)\n  image(\"c.ppm\", 3s)\n} as (first, middle, last)\nclip {\n  $last\n  $first\n  $middle\n} as edit\ntrim(value=$edit, range=$edit::first::start..$edit::middle::end)\n",
    );

    let compiled = compiler::compile(&workflow).expect("reordered tuple placement labels");
    assert_last_slice_range(&compiled, 30, 60);
}

#[test]
fn body_input_ids_preserve_same_typed_descriptor_slots() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s)\nimage(\"b.ppm\", 2s)\njoin {\n  drop\n  drop\n  $before\n  repeat(2)\n  $after\n  repeat(3)\n  concat\n}\n",
    );
    let compiled = compiler::compile(&workflow).expect("ordered body-input identities");
    assert_eq!(
        compiled.result_domain().expect("Video result").frames().0,
        80
    );
}

#[test]
fn bare_concat_rejects_mixed_timeline_types() {
    let (_directory, workflow) =
        project("clipasm 1\nimage(\"a.ppm\", 1s)\naudio(\"missing.wav\")\nconcat\n");
    let error = compiler::compile(&workflow).expect_err("mixed concat must select a type");
    assert_eq!(error.code, "E_AMBIGUOUS_GENERIC_TYPE");
    assert!(error.message.contains("<Video>"));
    assert!(error.message.contains("<Audio>"));
}

#[test]
fn concat_selector_reduces_only_the_selected_type() {
    let (_directory, workflow) = project(
        "clipasm 1\nimage(\"a.ppm\", 1s)\naudio(\"first.wav\")\nimage(\"b.ppm\", 1s)\naudio(\"second.wav\")\nconcat<Video>\nconcat<Audio>\n",
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
    assert_eq!(
        operations
            .iter()
            .filter(|operation| **operation == "concat")
            .count(),
        2
    );
    assert!(
        document["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .any(|node| node["value_type"] == "audio" && node["kind"]["operation"] == "concat")
    );
}

#[test]
fn generic_unary_programs_use_the_nearest_compatible_value() {
    let (_directory, workflow) =
        project("clipasm 1\nimage(\"a.ppm\", 1s)\naudio(\"missing.wav\")\nrepeat(2)\ndrop\n");
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
            .any(|node| { node["value_type"] == "audio" && node["kind"]["operation"] == "repeat" })
    );
}

#[test]
fn stack_block_with_concat_concatenates_homogeneous_audio() {
    let (_directory, workflow) =
        project("clipasm 1\n{\n  audio(\"first.wav\")\n  audio(\"second.wav\")\n  concat\n}\n");
    let compiled = compiler::compile(&workflow).expect("Audio stack block");
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
            .any(|node| { node["value_type"] == "audio" && node["kind"]["operation"] == "concat" })
    );
}

#[test]
fn join_concatenates_homogeneous_audio() {
    let (_directory, workflow) =
        project("clipasm 1\naudio(\"first.wav\")\naudio(\"second.wav\")\njoin { concat }\n");
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
            .any(|node| { node["value_type"] == "audio" && node["kind"]["operation"] == "concat" })
    );
}

#[test]
fn generic_join_rejects_mixed_outputs() {
    let (_directory, workflow) = project(
        "clipasm 1\naudio(\"first.wav\")\naudio(\"second.wav\")\njoin { image(\"a.ppm\", 1s) }\n",
    );
    let error = compiler::compile(&workflow).expect_err("mixed body output types");
    assert!(matches!(
        error.code,
        "E_GENERIC_TYPE_MISMATCH" | "E_TYPE_MISMATCH"
    ));
}

#[test]
fn join_selector_chooses_one_homogeneous_stack_view() {
    let (_directory, ambiguous) = project(
        "clipasm 1\nimage(\"a.ppm\", 1s)\nimage(\"b.ppm\", 1s)\naudio(\"first.wav\")\naudio(\"second.wav\")\njoin { concat }\n",
    );
    let error = compiler::compile(&ambiguous).expect_err("ambiguous join type");
    assert_eq!(error.code, "E_AMBIGUOUS_GENERIC_TYPE");

    let (_directory, selected) = project(
        "clipasm 1\nimage(\"a.ppm\", 1s)\nimage(\"b.ppm\", 1s)\naudio(\"first.wav\")\naudio(\"second.wav\")\njoin<Audio> { concat }\n",
    );
    let compiled = compiler::compile(&selected).expect("selected Audio join");
    assert_eq!(compiled.outputs().len(), 3);
    assert_eq!(
        compiled.outputs()[2].value_type(),
        clipasm::model::ValueType::Audio
    );
}

#[test]
fn named_stack_block_infers_its_concat_type_from_the_body() {
    let (_directory, inferred) = project(
        "clipasm 1\n{\n  audio(\"first.wav\")\n  audio(\"second.wav\")\n  concat\n} as combined\n$combined\n",
    );
    let inferred = compiler::compile(&inferred).expect("inferred named Audio block");
    assert_eq!(
        inferred
            .outputs()
            .last()
            .expect("reference output")
            .value_type(),
        clipasm::model::ValueType::Audio
    );

    let (_directory, annotated) = project(
        "clipasm 1\n{\n  audio(\"first.wav\")\n  audio(\"second.wav\")\n  concat<Audio>\n} as combined\n$combined\n",
    );
    let annotated = compiler::compile(&annotated).expect("annotated named Audio block");
    assert_eq!(inferred.structure_hash(), annotated.structure_hash());
}

#[test]
fn named_stack_block_type_inference_follows_forward_references() {
    let (_directory, workflow) = project(
        "clipasm 1\n$combined\n{\n  audio(\"first.wav\")\n  audio(\"second.wav\")\n  concat\n} as combined\n",
    );
    let compiled = compiler::compile(&workflow).expect("forward inferred named block");
    assert_eq!(compiled.outputs().len(), 2);
    assert!(
        compiled
            .outputs()
            .iter()
            .all(|output| output.value_type() == clipasm::model::ValueType::Audio)
    );
}

#[test]
fn named_stack_block_type_inference_resolves_dependency_chains() {
    let (_directory, workflow) = project(
        "clipasm 1\n{ $later\nconcat } as earlier\n{\n  audio(\"first.wav\")\n  audio(\"second.wav\")\n  concat\n} as later\n$earlier\n",
    );
    let compiled = compiler::compile(&workflow).expect("inferred named block chain");
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
fn named_stack_block_type_inference_reports_dependency_cycles() {
    let (_directory, workflow) =
        project("clipasm 1\n{ $second\nconcat } as first\n{ $first\nconcat } as second\n");
    let error = compiler::compile(&workflow).expect_err("named block type cycle");
    assert_eq!(error.code, "E_DEPENDENCY_CYCLE");
    assert!(error.message.contains("first -> second -> first"));
}

#[test]
fn selected_named_stack_block_cycle_remains_a_dependency_cycle() {
    let (_directory, workflow) = project(
        "clipasm 1\n{ $second\nconcat<Audio> } as first\n{ $first\nconcat<Audio> } as second\n",
    );
    let error = compiler::compile(&workflow).expect_err("selected named block cycle");
    assert_eq!(error.code, "E_DEPENDENCY_CYCLE");
    assert!(error.message.contains("first -> second -> first"));
}

#[test]
fn self_dependent_stack_inference_reports_an_inference_dependency() {
    let (_directory, workflow) = project("clipasm 1\n$future\nrepeat(2) as future\n");
    let error = compiler::compile(&workflow).expect_err("self-dependent generic stack binding");
    assert_eq!(error.code, "E_TYPE_INFERENCE_DEPENDENCY");
}

#[test]
fn named_stack_block_type_inference_respects_body_port_shadowing() {
    let (_directory, workflow) = project(
        "clipasm 1\n{\n  image(\"a.ppm\", 1s)\n  image(\"b.ppm\", 1s)\n  join {\n    drop\n    drop\n    $before\n  }\n  concat\n} as combined\n$combined\n",
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
    let (_directory, workflow) = project("clipasm 1\nimage(\"a.ppm\", 1s)\nrepeat(2) as doubled\n");
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
    let (_directory, workflow) =
        project("clipasm 1\n$doubled\nimage(\"a.ppm\", 1s)\nrepeat(2) as doubled\n");
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
        "clipasm 1\n$future\nimage(\"a.ppm\", 1s)\nzoom_in\naudio(\"missing.wav\")\nrepeat(2) as future\n",
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
