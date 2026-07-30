use std::fs;
use std::path::Path;

use super::support::*;

#[test]
fn learning_chapter_checkpoints_match_the_documented_results() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source_path = directory.path().join("learning.clipasm");
    let header = "\
clipasm 1

config {
    video {
        width = 320
        height = 180
        fps = 24
    }
    output = \"generated/learning.mp4\"
}

";
    let validate = |body: &str| {
        fs::write(&source_path, format!("{header}{body}")).expect("learning checkpoint");
        run_clipasm(directory.path(), &["validate", "learning.clipasm"])
    };
    let first_video = validate("image(\"assets/morning.png\", 1500ms, contain)\n");
    assert!(first_video.status.success());
    assert_eq!(
        String::from_utf8(first_video.stdout).expect("UTF-8 validation"),
        "valid: 1 semantic value(s), 36 frame(s)\n"
    );

    let three_outputs = validate(
        "\
image(\"assets/morning.png\", 1500ms, contain)
image(\"assets/meadow.png\", 1500ms, contain)
image(\"assets/evening.png\", 1500ms, contain)
",
    );
    assert!(!three_outputs.status.success());
    let three_diagnostic = String::from_utf8_lossy(&three_outputs.stderr);
    assert!(three_diagnostic.contains("[E_ENTRYPOINT_OUTPUT_COUNT]"));
    assert!(three_diagnostic.contains("3 Video values remain"));

    let sequence = validate(
        "\
image(\"assets/morning.png\", 1500ms, contain)
image(\"assets/meadow.png\", 1500ms, contain)
image(\"assets/evening.png\", 1500ms, contain)
concat
",
    );
    assert!(sequence.status.success());
    assert_eq!(
        String::from_utf8(sequence.stdout).expect("UTF-8 validation"),
        "valid: 4 semantic value(s), 108 frame(s)\n"
    );

    let unnamed_clip = validate(
        "\
clip {
    image(\"assets/morning.png\", 1500ms, contain)
    image(\"assets/meadow.png\", 1500ms, contain)
    image(\"assets/evening.png\", 1500ms, contain)
}
",
    );
    assert!(!unnamed_clip.status.success());
    let zero_diagnostic = String::from_utf8_lossy(&unnamed_clip.stderr);
    assert!(zero_diagnostic.contains("[E_ENTRYPOINT_OUTPUT_COUNT]"));
    assert!(zero_diagnostic.contains("0 Video values remain"));

    let named_clip = validate(
        "\
clip {
    image(\"assets/morning.png\", 1500ms, contain)
    image(\"assets/meadow.png\", 1500ms, contain)
    image(\"assets/evening.png\", 1500ms, contain)
} as pictures

$pictures
",
    );
    assert!(named_clip.status.success());
    assert_eq!(
        String::from_utf8(named_clip.stdout).expect("UTF-8 validation"),
        "valid: 5 semantic value(s), 108 frame(s)\n"
    );
}

fn inspect_learning_checkpoint(body: &str) -> serde_json::Value {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = format!(
        "\
clipasm 1

config {{
    video {{
        width = 320
        height = 180
        fps = 24
    }}
    output = \"generated/learning.mp4\"
}}

{body}"
    );
    fs::write(directory.path().join("learning.clipasm"), source).expect("learning checkpoint");
    let output = run_clipasm(directory.path(), &["inspect", "learning.clipasm"]);
    assert!(
        output.status.success(),
        "learning checkpoint failed inspection: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("inspection JSON")
}

fn semantic_nodes(document: &serde_json::Value) -> &[serde_json::Value] {
    document["nodes"].as_array().expect("semantic nodes")
}

fn semantic_node_by_id(nodes: &[serde_json::Value], id: u64) -> &serde_json::Value {
    nodes
        .iter()
        .find(|node| node["id"].as_u64() == Some(id))
        .expect("referenced semantic node")
}

fn image_node_id(nodes: &[serde_json::Value], path: &str) -> u64 {
    nodes
        .iter()
        .find(|node| node["kind"]["path"] == path)
        .and_then(|node| node["id"].as_u64())
        .expect("image node")
}
#[test]
fn learning_chapter_four_transforms_only_the_meadow() {
    let document = inspect_learning_checkpoint(
        "\
clip {
    image(\"assets/morning.png\", 1500ms, contain)
    image(\"assets/meadow.png\", 1500ms, contain)
    zoom_in(4%)
    image(\"assets/evening.png\", 1500ms, contain)
} as pictures

$pictures
",
    );
    let nodes = semantic_nodes(&document);
    let meadow_id = image_node_id(nodes, "assets/meadow.png");
    let meadow_zoom = nodes
        .iter()
        .find(|node| node["kind"]["operation"] == "zoom_in")
        .expect("meadow zoom");

    assert_eq!(meadow_zoom["kind"]["by"], "1/25");
    assert_eq!(meadow_zoom["kind"]["input"]["id"].as_u64(), Some(meadow_id));
}

#[test]
fn learning_chapter_five_transitions_morning_into_meadow() {
    let document = inspect_learning_checkpoint(
        "\
clip {
    image(\"assets/morning.png\", 1500ms, contain)
} as morning

clip {
    image(\"assets/meadow.png\", 1500ms, contain)
    zoom_in(4%)
} as meadow

clip {
    image(\"assets/evening.png\", 1500ms, contain)
} as evening

$morning
$meadow
flash_cut(200ms)
$evening
concat
",
    );
    let nodes = semantic_nodes(&document);
    let morning_id = image_node_id(nodes, "assets/morning.png");
    let evening_id = image_node_id(nodes, "assets/evening.png");
    let meadow_zoom_id = nodes
        .iter()
        .find(|node| node["kind"]["operation"] == "zoom_in")
        .and_then(|node| node["id"].as_u64())
        .expect("meadow zoom");
    let flash = nodes
        .iter()
        .find(|node| node["kind"]["operation"] == "flash_cut")
        .expect("flash transition");
    let before = semantic_node_by_id(
        nodes,
        flash["kind"]["before"]["id"]
            .as_u64()
            .expect("before reference"),
    );
    let after = semantic_node_by_id(
        nodes,
        flash["kind"]["after"]["id"]
            .as_u64()
            .expect("after reference"),
    );

    assert_eq!(before["kind"]["target"]["id"].as_u64(), Some(morning_id));
    assert_eq!(after["kind"]["target"]["id"].as_u64(), Some(meadow_zoom_id));

    let concat = nodes
        .iter()
        .find(|node| node["kind"]["operation"] == "concat")
        .expect("final concat");
    assert_eq!(concat["kind"]["inputs"][0]["id"], flash["id"]);
    let ordinary_cut = semantic_node_by_id(
        nodes,
        concat["kind"]["inputs"][1]["id"]
            .as_u64()
            .expect("evening reference"),
    );
    assert_eq!(
        ordinary_cut["kind"]["target"]["id"].as_u64(),
        Some(evening_id)
    );
}

#[test]
fn learning_chapter_six_selects_the_evening_placement() {
    let document = inspect_learning_checkpoint(
        "\
clip { image(\"assets/morning.png\", 1500ms, contain) } as morning
clip {
    image(\"assets/meadow.png\", 1500ms, contain)
    zoom_in(4%)
} as meadow
clip { image(\"assets/evening.png\", 1500ms, contain) } as evening
clip {
    $morning
    $meadow
    flash_cut(200ms)
    $evening
} as edit
$edit
during($edit::evening) {
    zoom_in(2%)
}
",
    );
    let nodes = semantic_nodes(&document);
    let evening_slice = nodes
        .iter()
        .find(|node| node["kind"]["operation"] == "slice")
        .expect("selected evening slice");
    let evening_zoom = nodes
        .iter()
        .find(|node| node["kind"]["operation"] == "zoom_in" && node["kind"]["by"] == "1/50")
        .expect("evening zoom");
    let replacement = nodes
        .iter()
        .find(|node| node["kind"]["operation"] == "replace_range")
        .expect("evening replacement");

    assert_eq!(evening_slice["kind"]["range"]["start"], 72);
    assert_eq!(evening_slice["kind"]["range"]["end"], 108);
    assert_eq!(replacement["kind"]["replacement"]["id"], evening_zoom["id"]);
    assert_eq!(replacement["kind"]["range"], evening_slice["kind"]["range"]);
}

#[test]
fn learning_chapter_seven_preserves_the_default_and_override() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let validation = run_clipasm(
        repository,
        &["validate", "examples/learning-journey.clipasm"],
    );
    assert!(
        validation.status.success(),
        "learning checkpoint failed validation: {}",
        String::from_utf8_lossy(&validation.stderr)
    );
    assert_eq!(
        String::from_utf8(validation.stdout).expect("UTF-8 validation"),
        "valid: 15 semantic value(s), 108 frame(s)\n"
    );

    let inspection = run_clipasm(
        repository,
        &["inspect", "examples/learning-journey.clipasm"],
    );
    assert!(inspection.status.success());
    let document: serde_json::Value =
        serde_json::from_slice(&inspection.stdout).expect("inspection JSON");
    let nodes = semantic_nodes(&document);
    let zoom_amounts = nodes
        .iter()
        .filter(|node| node["kind"]["operation"] == "zoom_in")
        .map(|node| node["kind"]["by"].as_str().expect("zoom amount"))
        .collect::<Vec<_>>();
    let replacement = nodes
        .iter()
        .find(|node| node["kind"]["operation"] == "replace_range")
        .expect("final evening replacement");

    assert_eq!(zoom_amounts, ["3/50", "1/50"]);
    assert_eq!(replacement["kind"]["range"]["start"], 72);
    assert_eq!(replacement["kind"]["range"]["end"], 108);
}

#[test]
fn soundtrack_how_to_validates_without_opening_media() {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("soundtrack.clipasm"),
        "\
clipasm 1

config {
    video {
        width = 1920
        height = 1080
        fps = 30
    }
    output = \"generated/with-soundtrack.mp4\"
}

video(\"assets/scene.mp4\", contain)
audio(\"assets/soundtrack.wav\")
set_audio
",
    )
    .expect("soundtrack source");

    let validation = run_clipasm(directory.path(), &["validate", "soundtrack.clipasm"]);
    assert!(
        validation.status.success(),
        "soundtrack guide failed validation: {}",
        String::from_utf8_lossy(&validation.stderr)
    );
}
