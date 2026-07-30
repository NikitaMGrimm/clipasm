use std::path::Path;

use clipasm::compiler;

use super::support::*;

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
    assert!(compiled_document(&compiled).has_operation("replace_range"));
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

    let document = compiled_document(&compiled);
    let trim = document.operation_for_construct("trim");
    assert_eq!(trim.name(), "slice");
    assert_eq!(trim.range(), (10, 20));
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
    assert_eq!(
        compiled_document(&compiled).last_operation("slice").range(),
        (0, 10)
    );
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

    let document = compiled_document(&compiled);
    let concat_inputs = document
        .operations("concat")
        .map(CompiledOperation::input_count)
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
    let compiled = compiler::compile(&workflow).expect("compile");
    assert_eq!(
        compiled_document(&compiled).named_value("edited").name(),
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
    assert_eq!(
        compiled_document(&compiled).operation("slice").range(),
        (0, 10)
    );
}

#[test]
fn join_exposes_named_values_created_by_its_body() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 1s)\nimage(\"b.ppm\", 1s)\njoin { image(\"bridge.ppm\", 500ms) as bridge } as joined\ntrim(value=$joined, range=$joined::bridge)\n",
    );

    let compiled = compiler::compile(&workflow).expect("joined body marker");
    assert_eq!(
        compiled_document(&compiled).operation("slice").range(),
        (20, 25)
    );
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

    assert_eq!(
        compiled_document(&compiled)
            .operation_for_construct_named("join", "concat")
            .input_count(),
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
    assert_eq!(compiled_document(&compiled).operation_count("concat"), 2);
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
