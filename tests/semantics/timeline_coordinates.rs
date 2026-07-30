use std::fmt::Write as _;

use clipasm::compiler;

use super::support::*;

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

    assert_eq!(
        compiled_document(&compiled)
            .operation("replace_range")
            .range(),
        (10, 30)
    );
}
#[test]
fn timeline_placement_selector_is_a_complete_closed_open_range() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"a.ppm\", 1s) as intro\n  image(\"b.ppm\", 2s) as credits\n} as edit\n$edit\nduring($edit::credits) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("complete placement range");
    assert_eq!(
        compiled_document(&compiled)
            .operation("replace_range")
            .range(),
        (10, 30)
    );
}

#[test]
fn unique_reference_marker_survives_identity_timeline_programs() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as interview\nclip {\n  $interview\n  zoom_in(8%)\n} as edit\n$edit\nduring($edit::interview::start..$edit::interview::end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("identity-preserved marker");
    assert_eq!(
        compiled_document(&compiled)
            .operation("replace_range")
            .range(),
        (0, 10)
    );
}

#[test]
fn marker_selector_uses_the_bound_timeline_as_inference_context() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"interview.ppm\", 2s) as interview\nclip {\n  image(\"intro.ppm\", 1s)\n  $interview\n} as edit\nduring(timeline=$edit, range=$interview::start..$interview::end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("contextual marker root");
    assert_eq!(
        compiled_document(&compiled)
            .operation("replace_range")
            .range(),
        (10, 30)
    );
}

#[test]
fn nested_clip_placements_form_explicit_selector_paths() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"a.ppm\", 1s) as intro\n  image(\"b.ppm\", 2s) as interview\n} as chapter\nclip {\n  $chapter\n  image(\"c.ppm\", 1s) as credits\n} as edit\n$edit\nduring($edit::chapter::interview::start..$edit::chapter::interview::end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("nested marker path");
    assert_eq!(
        compiled_document(&compiled)
            .operation("replace_range")
            .range(),
        (10, 30)
    );
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
    assert_eq!(
        compiled_document(&compiled)
            .operation("replace_range")
            .range(),
        (30, 60)
    );
}

#[test]
fn timeline_coordinates_support_exact_addition_and_scaling() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"a.ppm\", 1s) as first\n  image(\"b.ppm\", 2s) as second\n} as edit\n$edit\nduring(50% * ($edit::first::start + $edit::second::start)..$edit::second::end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("coordinate arithmetic");
    assert_eq!(
        compiled_document(&compiled)
            .operation("replace_range")
            .range(),
        (5, 30)
    );
}

#[test]
fn timeline_region_middle_is_an_exact_coordinate() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"section.ppm\", 2s) as section\n} as edit\n$edit\nduring($edit::section::middle..$edit::section::end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("exact region middle");
    assert_eq!(
        compiled_document(&compiled)
            .operation("replace_range")
            .range(),
        (10, 20)
    );
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
    assert_eq!(
        compiled_document(&compiled)
            .operation("replace_range")
            .range(),
        (5, 30)
    );
}

#[test]
fn scalar_aliases_support_forward_references() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"a.ppm\", 1s) as first\n  image(\"b.ppm\", 2s) as second\n} as edit\nrange_end = $range_start + 1s\nrange_start = $edit::second::start\n$edit\nduring($range_start..$range_end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("forward scalar aliases");
    assert_eq!(
        compiled_document(&compiled)
            .operation("replace_range")
            .range(),
        (10, 20)
    );
}

#[test]
fn unused_out_of_bounds_scalar_alias_is_harmless() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s) as edit\nunused = $edit::end + 500s\n$edit\n",
    );

    let compiled = compiler::compile(&workflow).expect("unused coordinate is not consumed");
    let _document = compiled_document(&compiled);
}

#[test]
fn scalar_aliases_support_complex_exact_marker_arithmetic() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nclip {\n  image(\"intro.ppm\", 1s) as intro\n  image(\"main.ppm\", 2s) as main\n} as edit\nhalf = 1 / 2\nmidpoint = $half * ($edit::main::start + $edit::main::end)\nrange_start = $midpoint - 500ms\nrange_end = $midpoint + 500ms\n$edit\nduring($range_start..$range_end) {\n  zoom_in(2%)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("complex marker aliases");
    assert_eq!(
        compiled_document(&compiled)
            .operation("replace_range")
            .range(),
        (15, 25)
    );
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
    assert_eq!(
        compiled_document(&compiled)
            .operation("replace_range")
            .range(),
        (0, 5)
    );
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
    let _document = compiled_document(&compiled);
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
