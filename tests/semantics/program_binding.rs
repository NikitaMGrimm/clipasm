use clipasm::compiler;

use super::support::*;

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
    let document = compiled_document(&compiled);
    let repeat = document.operation_for_construct("repeat");
    assert_eq!(repeat.name(), "repeat");
    assert_eq!(repeat.integer_parameter("count"), 3);
    assert!(repeat.has_input("input"));
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

    let document = compiled_document(&compiled);
    let zoom_in = document.operation_for_construct("zoom_in");
    assert_eq!(zoom_in.name(), "zoom_in");
    assert_eq!(zoom_in.string_parameter("by"), "2/25");
    assert_eq!(zoom_in.domain_frames(), 10);
    assert_eq!(zoom_in.domain_width(), 64);
    assert_eq!(zoom_in.domain_height(), 64);
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
    assert!(error.message.contains("got Number and wall-clock Duration"));
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
    assert_eq!(
        compiled_document(&compiled)
            .operation("zoom_in")
            .string_parameter("by"),
        "2/25"
    );
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

    let document = compiled_document(&compiled);
    let zoom_in = document.operation("zoom_in");
    assert_eq!(zoom_in.input_id("input"), 1);
    assert_eq!(zoom_in.string_parameter("by"), "3/25");
    assert_eq!(document.last_node().name(), "concat");
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

    let document = compiled_document(&compiled);
    let flash_cut = document.operation("flash_cut");
    assert_eq!(flash_cut.input_id("before"), 0);
    assert_eq!(flash_cut.input_id("after"), 1);
    assert_eq!(flash_cut.integer_parameter("frames"), 2);
    assert_eq!(flash_cut.domain_frames(), 20);
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

    let document = compiled_document(&compiled);
    let result = document.last_node();
    assert_eq!(result.name(), "concat");
    assert_eq!(result.input_count(), 3);
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
    let document = compiled_document(&compiled);
    let flash_cut = document.operation("flash_cut");
    assert_eq!(flash_cut.input_id("before"), 1);
    assert_eq!(flash_cut.input_id("after"), 2);
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
    assert!(compiled_document(&compiled).has_named_value("reusable"));
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
    assert!(compiled_document(&compiled).has_named_value("reusable"));
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
        assert_eq!(
            compiled_document(&compiled)
                .operation("flash_cut")
                .integer_parameter("frames"),
            expected
        );
    }
}
