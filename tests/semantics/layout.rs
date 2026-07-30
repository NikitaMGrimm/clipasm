use std::fmt::Write as _;

use clipasm::compiler;

use super::support::*;

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
    assert_eq!(
        compiled_document(&compiled)
            .operation("replace_range")
            .range(),
        (0, 20)
    );
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
    assert!(compiled_document(&compiled).has_operation("slice"));
}

#[test]
fn media_dependent_during_inherits_its_requested_extent_without_reading_media() {
    let (_directory, workflow) = project(
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 } }\nvideo(\"missing.mkv\") as source\nduring(range=($source::start + 200ms)..($source::end - 300ms)) {\n  drop<Video>\n  image(\"replacement.ppm\")\n}\n",
    );

    let compiled = compiler::compile(&workflow).expect("deferred during compilation");
    assert!(compiled.result_domain().is_none());
    let document = compiled_document(&compiled);
    let operations = document.operation_names();
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
    assert_eq!(
        compiled_document(&compiled).operation("slice").range(),
        (6, 10)
    );
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
    assert_eq!(
        compiled_document(&compiled).last_operation("slice").range(),
        (20, 30)
    );
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
    assert_eq!(
        compiled_document(&compiled).last_operation("slice").range(),
        (20, 30)
    );
}
