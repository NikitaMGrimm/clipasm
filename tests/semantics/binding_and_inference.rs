use clipasm::compiler;

use super::support::*;

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
    let document = compiled_document(&compiled);
    let operations = document.operation_names();
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
    let document = compiled_document(&compiled);
    assert!(
        document
            .operations("repeat")
            .any(|node| node.integer_parameter("count") == 2)
    );
    assert!(
        document
            .operations("repeat")
            .any(|node| node.value_type() == "audio" && node.integer_parameter("count") == 3)
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
    let document = compiled_document(&compiled);
    assert_eq!(document.operation_count("concat"), 2);
    assert_eq!(document.typed_operation_count("audio", "concat"), 1);
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
    assert_eq!(
        compiled_document(&compiled).typed_operation_count("audio", "repeat"),
        1
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
    assert_eq!(
        compiled_document(&compiled).typed_operation_count("audio", "concat"),
        1
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
    assert_eq!(
        compiled_document(&compiled).typed_operation_count("audio", "concat"),
        1
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
