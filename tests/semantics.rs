#![allow(missing_docs)]

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
fn default_visible_during_binds_its_video_from_the_visible_suffix() {
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
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nimage(\"a.ppm\", 5s)\nduring(1s..3s) {\n  during(video=$video, range=2s..4s) { repeat(1) }\n  concat\n}\n",
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
    assert!(operations.contains(&"audio_slice"));
    assert!(!operations.contains(&"audio_on_black"));
    assert!(!operations.contains(&"extract_audio"));
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
    assert!(
        nodes.iter().any(|node| {
            node["kind"]["operation"] == "audio_repeat" && node["kind"]["count"] == 3
        })
    );
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
    assert!(operations.contains(&"concat"));
    assert!(operations.contains(&"audio_concat"));
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
            .any(|node| node["kind"]["operation"] == "audio_repeat")
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
            .any(|node| node["kind"]["operation"] == "audio_concat")
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
            .any(|node| node["kind"]["operation"] == "audio_concat")
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
