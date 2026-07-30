use clipasm::compiler;

use super::support::*;

#[test]
fn trim_uses_audio_natively_without_implicit_adaptation() {
    let (_directory, workflow) = project("clipasm 1\naudio(\"missing.wav\")\ntrim(0s..1s)\n");
    let compiled = compiler::compile(&workflow).expect("native Audio trim");
    assert_eq!(
        compiled.outputs()[0].value_type(),
        clipasm::model::ValueType::Audio
    );
    let document = compiled_document(&compiled);
    let operations = document.operation_names();
    assert_eq!(
        document
            .typed_operation("audio", "slice")
            .string_parameter("unit"),
        "samples"
    );
    assert!(!operations.contains(&"audio_on_black"));
    assert!(!operations.contains(&"extract_audio"));
}

#[test]
fn project_frame_literals_drive_video_durations_transitions_and_ranges_exactly() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 30 } }\nimage(\"a.ppm\", 17f)\nimage(\"b.ppm\", 13f)\nflash_cut(3f)\ntrim(2f..27f)\n",
    );

    let compiled = compiler::compile(&workflow).expect("project-frame-authored edit");

    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        25
    );
    assert_last_slice_range(&compiled, 2, 27);
    assert_eq!(
        compiled_document(&compiled)
            .operation("flash_cut")
            .integer_parameter("frames"),
        3
    );
}

#[test]
fn project_frame_ranges_map_to_the_project_audio_sample_grid() {
    for (rate, numerator, denominator) in [
        ("23", 23_u64, 1_u64),
        ("24", 24, 1),
        ("25", 25, 1),
        ("29", 29, 1),
        ("30", 30, 1),
        ("50", 50, 1),
        ("59", 59, 1),
        ("60", 60, 1),
        ("30000/1001", 30_000, 1_001),
    ] {
        let source = format!(
            "clipasm 1\nconfig {{\n  video {{ fps = {rate} }}\n  audio {{ sample_rate = 48000 }}\n}}\naudio(\"missing.wav\")\ntrim(3f..8f)\n"
        );
        let (_directory, workflow) = project(&source);
        let compiled = compiler::compile(&workflow).expect("frame-addressed Audio trim");
        let boundary = |frame: u64| {
            let scaled = frame * 48_000 * denominator;
            scaled.div_ceil(numerator)
        };

        assert_last_audio_slice_range(&compiled, boundary(3), boundary(8));
    }
}

#[test]
fn project_frame_duration_arithmetic_preserves_signed_intermediates() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { fps = 30 } }\noffset = -5f\nduration = $offset + 20f\nimage(\"a.ppm\", $duration)\n",
    );
    let compiled = compiler::compile(&workflow).expect("signed frame arithmetic");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        15
    );

    let (_directory, fractional) = project("clipasm 1\nimage(\"a.ppm\", (5 / 2)f)\n");
    let error = compiler::compile(&fractional).expect_err("fractional frame duration");
    assert_eq!(error.code, "E_INVALID_ARGUMENT_TYPE");
    assert!(error.message.contains("requires Integer"));
    assert!(error.message.contains("2.5"));
}

#[test]
fn duration_unit_families_reject_mixing_explicitly() {
    for expression in ["1f + 1s", "1f..1s"] {
        let (_directory, workflow) =
            project(&format!("clipasm 1\nimage(\"a.ppm\", {expression})\n"));
        let error = compiler::compile(&workflow).expect_err("mixed duration families");
        assert_eq!(error.code, "E_INVALID_SCALAR_OPERATION");
        assert!(error.message.contains("matching Duration families"));
        assert!(error.message.contains("project-frame Duration"));
        assert!(error.message.contains("wall-clock Duration"));
    }

    let (_directory, parameter) =
        project("clipasm 1\nparam duration: Duration = 15f\nimage(\"a.ppm\", $duration + 1s)\n");
    let error = compiler::compile(&parameter).expect_err("runtime parameter family mismatch");
    assert!(error.message.contains("matching Duration families"));
    assert!(
        error
            .notes
            .iter()
            .any(|note| note.contains("scalar parameter `$duration` evaluated to 15f"))
    );
}

#[test]
fn project_frame_offsets_integrate_with_video_and_audio_markers() {
    let (_directory, video) = project(
        "clipasm 1\nconfig { video { fps = 30 } }\nimage(\"a.ppm\", 30f) as edit\ntrim(value=$edit, range=($edit::start + 3f)..($edit::end - 3f))\n",
    );
    let compiled = compiler::compile(&video).expect("frame-offset Video marker");
    assert_last_slice_range(&compiled, 3, 27);

    let (_directory, audio) = project(
        "clipasm 1\nconfig {\n  video { fps = 30 }\n  audio { sample_rate = 48000 }\n}\naudio(\"a.wav\")\ntrim(0s..2s) as track\ntrim(value=$track, range=($track::start + 3f)..($track::end - 3f))\n",
    );
    let compiled = compiler::compile(&audio).expect("frame-offset Audio marker");
    assert_last_audio_slice_range(&compiled, 4_800, 91_200);
}

#[test]
fn deferred_marker_inspection_preserves_project_frame_offsets() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { fps = 30000/1001 } }\nvideo(\"missing.mkv\") as source\ntrim(value=$source, range=($source::start + 3f)..($source::end - 3f))\n",
    );
    let compiled = compiler::compile(&workflow).expect("deferred frame-offset marker");
    assert_eq!(
        compiled_document(&compiled)
            .last_operation("slice")
            .range_project_frame_offsets(),
        ("3", "-3")
    );
}

#[test]
fn cancelled_project_frame_offsets_preserve_semantic_identity() {
    let source = |offset: &str| {
        format!(
            "clipasm 1\nconfig {{ video {{ fps = 30 }} }}\nimage(\"a.ppm\", 30f) as edit\ntrim(value=$edit, range=($edit::start{offset})..$edit::end)\n"
        )
    };
    let (_plain_directory, plain) = project(&source(""));
    let (_cancelled_directory, cancelled) = project(&source(" + 5f - 5f"));
    assert_eq!(
        compiler::compile(&plain)
            .expect("plain marker")
            .structure_hash(),
        compiler::compile(&cancelled)
            .expect("cancelled frame offset")
            .structure_hash()
    );
}

#[test]
fn frame_and_aligned_wall_clock_durations_have_equal_semantic_identity() {
    let source = |duration: &str| {
        format!(
            "clipasm 1\nconfig {{ video {{ width = 64\nheight = 64\nfps = 30 }} }}\nimage(\"a.ppm\", {duration})\n"
        )
    };
    let (_frame_directory, frame_authored) = project(&source("15f"));
    let (_time_directory, time_authored) = project(&source("500ms"));

    assert_eq!(
        compiler::compile(&frame_authored)
            .expect("frame-authored image")
            .structure_hash(),
        compiler::compile(&time_authored)
            .expect("time-authored image")
            .structure_hash()
    );
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
    assert_eq!(
        compiled_document(&compiled)
            .typed_operation("audio", "slice")
            .range_term_counts(),
        (1, 2)
    );
}

#[test]
fn audio_trim_preserves_a_symbolically_selected_complete_placement() {
    let (_directory, workflow) = project(
        "clipasm 1\naudio(\"first.wav\") as first\naudio(\"second.wav\") as second\njoin as mix\ntrim(value=$mix, range=$mix::second) as selected\ntrim(value=$selected, range=$selected::second)\n",
    );

    let compiled = compiler::compile(&workflow).expect("symbolic Audio placement crop");
    assert_eq!(
        compiled_document(&compiled).typed_operation_count("audio", "slice"),
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
    let document = compiled_document(&compiled);
    let replacement = document.typed_operation("audio", "replace_range");
    assert_eq!(replacement.string_parameter("unit"), "samples");
    assert_eq!(replacement.range(), (100, 300));
}

#[test]
fn during_exposes_the_complete_audio_input_as_timeline() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { audio { sample_rate = 1000 } }\naudio(\"missing.wav\") as song\nduring(timeline=$song, range=100ms..200ms) {\n  drop<Audio>\n  trim(value=$timeline, range=0ms..50ms)\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("Audio body timeline alias");
    assert!(compiled_document(&compiled).has_typed_operation_range("audio", "slice", (0, 50)));
}

#[test]
fn audio_during_markers_remain_deferred_until_source_domains_are_known() {
    let (_directory, workflow) = project(
        "clipasm 1\naudio(\"first.wav\") as first\naudio(\"second.wav\") as second\njoin as mix\nduring(timeline=$mix, range=$mix::second) { repeat(2) }\n",
    );

    let compiled = compiler::compile(&workflow).expect("deferred Audio during");
    assert_eq!(
        compiled_document(&compiled)
            .typed_operation("audio", "replace_range")
            .range_term_counts(),
        (1, 2)
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
