#![allow(missing_docs)]

use std::fs;
use std::path::Path;
use std::process::Command;

use clipasm::preflight::{PreparedAudioKind, PreparedVideoKind};

fn compile_yaml(path: &Path) -> clipasm::diagnostic::Result<clipasm::compiler::CompiledProgram> {
    let source = clipasm::frontend::yaml::parse_file(path)?;
    clipasm::compiler::compile(&source)
}

fn write_image(directory: &Path, name: &str, color: &str) {
    fs::write(directory.join(name), format!("P3\n1 1\n255\n{color}\n"))
        .expect("write image fixture");
}

#[test]
fn prepared_plan_serializes_one_distinguished_result() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let source = directory.path().join("program.yaml");
    fs::write(
        &source,
        "- program:\n    version: 1\n    output: final.mp4\n\n- image: {path: card.ppm, duration: 1s}\n",
    )
    .expect("source program");

    let compiled = compile_yaml(&source).expect("compile");
    let plan = clipasm::preflight::preflight(&compiled).expect("preflight");
    let document = serde_json::to_value(&plan).expect("prepared JSON");

    assert!(document.get("result").is_some());
    assert_eq!(document["format_version"], 7);
    assert_eq!(
        plan.nodes()[plan.result().get() as usize]
            .video_domain()
            .expect("Video node")
            .frames()
            .0,
        30
    );
}

#[test]
fn prepared_media_is_structurally_typed_without_changing_json_shape() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let source = directory.path().join("program.yaml");
    fs::write(
        &source,
        "- program:\n    version: 1\n    output: final.mp4\n\n- image: {path: card.ppm, duration: 1s}\n  id: picture\n- drop: {type: Video}\n- extract_audio: {video: $picture}\n  id: sound\n- drop: {type: Audio}\n- set_audio: {audio: $sound, video: $picture}\n",
    )
    .expect("source program");

    let compiled = compile_yaml(&source).expect("compile");
    let plan = clipasm::preflight::preflight(&compiled).expect("preflight");
    let video = plan
        .nodes()
        .iter()
        .find(|node| {
            matches!(
                node.video_kind(),
                Some(PreparedVideoKind::ImageVideo { .. })
            )
        })
        .expect("prepared Video node");
    let audio = plan
        .nodes()
        .iter()
        .find(|node| {
            matches!(
                node.audio_kind(),
                Some(PreparedAudioKind::ExtractAudio { .. })
            )
        })
        .expect("prepared Audio node");

    assert!(video.video_domain().is_some());
    assert!(video.audio_domain().is_none());
    assert!(video.video_kind().is_some());
    assert!(video.audio_kind().is_none());
    assert!(audio.video_domain().is_none());
    assert!(audio.audio_domain().is_some());
    assert!(audio.video_kind().is_none());
    assert!(audio.audio_kind().is_some());

    let document = serde_json::to_value(&plan).expect("prepared JSON");
    let nodes = document["nodes"].as_array().expect("prepared nodes");
    let video_json = nodes
        .iter()
        .find(|node| node["kind"]["operation"] == "image_video")
        .expect("serialized Video node");
    assert_eq!(video_json["value_type"], "video");
    assert!(video_json["domain"].is_object());
    assert!(video_json["audio_domain"].is_null());
    let audio_json = nodes
        .iter()
        .find(|node| node["kind"]["operation"] == "extract_audio")
        .expect("serialized Audio node");
    assert_eq!(audio_json["value_type"], "audio");
    assert!(audio_json["domain"].is_null());
    assert!(audio_json["audio_domain"].is_object());
    assert_eq!(audio_json["has_audio"], false);
}

#[test]
fn unreachable_auxiliary_audio_is_not_preflighted() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let source = directory.path().join("program.yaml");
    fs::write(
        &source,
        "- program:\n    version: 1\n    output: final.mp4\n\n- audio: missing.wav\n- image: {path: card.ppm, duration: 1s}\n",
    )
    .expect("source program");

    let compiled = compile_yaml(&source).expect("compile");
    let plan = clipasm::preflight::preflight(&compiled).expect("unique Video reachability");
    assert_eq!(plan.nodes().len(), 1);
    assert!(matches!(
        plan.nodes()[0].video_kind(),
        Some(PreparedVideoKind::ImageVideo { .. })
    ));
}

#[test]
fn audio_preflight_counts_exact_decoded_samples() {
    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
        eprintln!("skipping exact audio test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let audio = directory.path().join("exact.mka");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=48000:cl=stereo",
            "-af",
            "atrim=end_sample=12345,asetpts=PTS-STARTPTS",
            "-c:a",
            "flac",
        ])
        .arg(&audio)
        .status()
        .expect("create exact audio fixture");
    assert!(status.success());

    let source = directory.path().join("program.yaml");
    fs::write(
        &source,
        "- program:\n    version: 1\n    output: final.mp4\n\n- image: {path: card.ppm, duration: 1s}\n- audio: exact.mka\n- set_audio\n",
    )
    .expect("source program");

    let compiled = compile_yaml(&source).expect("compile");
    let plan = clipasm::preflight::preflight(&compiled).expect("preflight");
    let audio = plan
        .nodes()
        .iter()
        .find(|node| {
            matches!(
                node.audio_kind(),
                Some(PreparedAudioKind::AudioSource { .. })
            )
        })
        .expect("prepared audio source");
    assert_eq!(audio.audio_domain().expect("Audio node").samples(), 12_345);
}

#[test]
fn relocated_identical_projects_have_equal_semantic_hashes() {
    let first = tempfile::tempdir().expect("first directory");
    let second = tempfile::tempdir().expect("second directory");
    for directory in [first.path(), second.path()] {
        write_image(directory, "card.ppm", "255 0 0");
        fs::write(
            directory.join("workflow.yaml"),
            "- program:\n    version: 1\n    output: final.mp4\n\n\n- glue:\n    - image:\n        path: card.ppm\n        duration: 1s",
        )
        .expect("workflow");
    }

    let first_compiled = compile_yaml(&first.path().join("workflow.yaml")).expect("compile");
    let second_compiled = compile_yaml(&second.path().join("workflow.yaml")).expect("compile");
    let first_prepared = clipasm::preflight::preflight(&first_compiled).expect("preflight");
    let second_prepared = clipasm::preflight::preflight(&second_compiled).expect("preflight");
    assert_eq!(
        first_prepared.semantic_hash(),
        second_prepared.semantic_hash()
    );
}

#[test]
fn unused_named_values_are_absent_from_executable_nodes() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "used.ppm", "255 0 0");
    write_image(directory.path(), "unused.ppm", "0 255 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    clips:\n      unused:\n        image:\n          path: unused.ppm\n          duration: 1s\n    output: final.mp4\n\n\n- glue:\n    - image:\n        path: used.ppm\n        duration: 1s",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
    let plan = clipasm::preflight::preflight(&compiled).expect("preflight");
    let image_nodes = plan
        .nodes()
        .iter()
        .filter(|node| {
            matches!(
                node.video_kind(),
                Some(PreparedVideoKind::ImageVideo { .. })
            )
        })
        .count();
    assert_eq!(image_nodes, 1);
}

#[test]
fn preflight_hashes_assets_and_render_rejects_later_changes() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n    output: final.mp4\n\n\n- glue:\n    - image:\n        path: card.ppm\n        duration: 1s",
    )
    .expect("workflow");
    let compiled = compile_yaml(&workflow).expect("compile");
    let prepared = clipasm::preflight::preflight(&compiled).expect("preflight");
    let Some(PreparedVideoKind::ImageVideo { asset, .. }) = prepared.nodes()[0].video_kind() else {
        panic!("prepared image");
    };
    assert_eq!(asset.content_hash().len(), 64);

    fs::write(directory.path().join("card.ppm"), b"changed").expect("change asset");
    fs::write(directory.path().join("final.mp4"), b"existing output").expect("existing output");
    let error = clipasm::render::render(&prepared).expect_err("changed asset");
    assert_eq!(error.code, "E_ASSET_CHANGED");
    assert_eq!(
        fs::read(directory.path().join("final.mp4")).expect("preserved output"),
        b"existing output"
    );
}

#[test]
fn backend_export_constraints_do_not_leak_into_pure_compilation() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    project:\n      video: {width: 63, height: 65, fps: 10}\n    output: final.mp4\n\n\n- glue:\n    - image:\n        path: card.ppm\n        duration: 1s",
    )
    .expect("workflow");
    let compiled = compile_yaml(&workflow).expect("pure compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("export dimensions");
    assert_eq!(error.code, "E_EXPORT_DIMENSIONS");
}

#[test]
fn output_extension_is_strictly_mp4() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    output: final.mov\n\n\n- glue:\n    - image:\n        path: missing.png\n        duration: 1s",
    )
    .expect("workflow");
    let compiled = compile_yaml(&workflow).expect("compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("extension");
    assert_eq!(error.code, "E_INVALID_OUTPUT_EXTENSION");
}

#[test]
fn output_cannot_replace_the_source_program() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let source = directory.path().join("program.mp4");
    fs::write(
        &source,
        "- program:\n    version: 1\n    output: program.mp4\n\n\n- glue:\n    - image:\n        path: card.ppm\n        duration: 1s",
    )
    .expect("source program");

    let compiled = compile_yaml(&source).expect("compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("output collision");
    assert_eq!(error.code, "E_OUTPUT_COLLISION");
    assert!(error.message.contains("output"));
    assert!(error.message.contains("source program"));
}

#[test]
fn manifest_cannot_replace_the_source_program() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let source = directory.path().join("final.mp4.manifest.json");
    fs::write(
        &source,
        "- program:\n    version: 1\n    output: final.mp4\n\n\n- glue:\n    - image: {path: card.ppm, duration: 1s}",
    )
    .expect("source program");

    let compiled = compile_yaml(&source).expect("compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("manifest collision");
    assert_eq!(error.code, "E_MANIFEST_COLLISION");
    assert!(error.message.contains("manifest"));
    assert!(error.message.contains("source program"));
}

#[test]
fn output_cannot_replace_a_reachable_image_asset() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.mp4", "255 0 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    output: card.mp4\n\n\n- glue:\n    - image:\n        path: card.mp4\n        duration: 1s",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("output collision");
    assert_eq!(error.code, "E_OUTPUT_COLLISION");
    assert!(error.message.contains("output"));
    assert!(error.message.contains("image asset"));
}

#[test]
fn output_cannot_replace_a_reachable_video_asset() {
    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
        eprintln!("skipping video collision test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    let video = directory.path().join("source.mp4");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x64:r=10:d=1",
            "-c:v",
            "libx264",
        ])
        .arg(&video)
        .status()
        .expect("create video");
    assert!(status.success());
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    output: source.mp4\n\n\n- glue:\n    - video: source.mp4",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("output collision");
    assert_eq!(error.code, "E_OUTPUT_COLLISION");
    assert!(error.message.contains("video asset"));
}

#[test]
fn manifest_cannot_replace_a_reachable_asset() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "final.mp4.manifest.json", "255 0 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    output: final.mp4\n\n\n- glue:\n    - image:\n        path: final.mp4.manifest.json\n        duration: 1s",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("manifest collision");
    assert_eq!(error.code, "E_MANIFEST_COLLISION");
    assert!(error.message.contains("manifest"));
    assert!(error.message.contains("image asset"));
}

#[test]
fn existing_directory_output_is_rejected() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    fs::create_dir(directory.path().join("final.mp4")).expect("output directory");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    output: final.mp4\n\n\n- glue:\n    - image:\n        path: card.ppm\n        duration: 1s",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("invalid output");
    assert_eq!(error.code, "E_INVALID_OUTPUT_DESTINATION");
    assert!(error.message.contains("not a regular file"));
}

#[cfg(unix)]
#[test]
fn output_symlink_to_a_regular_file_is_rejected() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    fs::write(directory.path().join("existing.mp4"), b"old output").expect("output target");
    symlink(
        directory.path().join("existing.mp4"),
        directory.path().join("final.mp4"),
    )
    .expect("output symlink");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    output: final.mp4\n\n\n- glue:\n    - image:\n        path: card.ppm\n        duration: 1s",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("output symlink");
    assert_eq!(error.code, "E_INVALID_OUTPUT_DESTINATION");
    assert!(error.message.contains("is a symlink"));
    assert!(
        fs::symlink_metadata(directory.path().join("final.mp4"))
            .expect("output link")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read(directory.path().join("existing.mp4")).expect("output target"),
        b"old output"
    );
}

#[cfg(unix)]
#[test]
fn manifest_symlink_to_a_regular_file_is_rejected() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    fs::write(
        directory.path().join("existing-manifest.json"),
        b"old manifest",
    )
    .expect("manifest target");
    symlink(
        directory.path().join("existing-manifest.json"),
        directory.path().join("final.mp4.manifest.json"),
    )
    .expect("manifest symlink");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    output: final.mp4\n\n\n- glue:\n    - image:\n        path: card.ppm\n        duration: 1s",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("manifest symlink");
    assert_eq!(error.code, "E_INVALID_MANIFEST_DESTINATION");
    assert!(error.message.contains("is a symlink"));
    assert!(
        fs::symlink_metadata(directory.path().join("final.mp4.manifest.json"))
            .expect("manifest link")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read(directory.path().join("existing-manifest.json")).expect("manifest target"),
        b"old manifest"
    );
}

#[test]
fn video_preflight_reports_missing_files_by_source_kind() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    output: final.mp4\n\n\n- glue:\n    - video: missing.mp4",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("missing video");
    assert_eq!(error.code, "E_MISSING_VIDEO_FILE");
}

#[test]
fn video_preflight_derives_the_full_source_duration() {
    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
        eprintln!("skipping video preflight test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("source.mkv");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x64:r=10:d=1",
            "-c:v",
            "ffv1",
        ])
        .arg(&source)
        .status()
        .expect("create source video");
    assert!(status.success());
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n    output: final.mp4\n\n\n- glue:\n    - video: source.mkv",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
    let plan = clipasm::preflight::preflight(&compiled).expect("preflight");
    assert_eq!(
        plan.nodes()[0]
            .video_domain()
            .expect("Video node")
            .frames()
            .0,
        10
    );
}

#[test]
fn prepared_repeat_keeps_one_upstream_edge() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n    output: final.mp4\n\n\n- glue:\n    - image:\n        path: card.ppm\n        duration: 1s\n    - repeat: 2",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
    let plan = clipasm::preflight::preflight(&compiled).expect("preflight");
    let Some(PreparedVideoKind::Repeat {
        input,
        count,
        frames,
    }) = plan.nodes()[plan.result().get() as usize].video_kind()
    else {
        panic!("prepared repeat");
    };
    assert_eq!(input.get(), 0);
    assert_eq!(count.get(), 2);
    assert_eq!(frames.0, 20);
}

#[test]
fn prepared_zoom_preserves_the_exact_input_domain() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n    output: final.mp4\n\n\n- glue:\n    - image: {path: card.ppm, duration: 1s}\n    - zoom: 12",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
    let input_domain = *compiled.result_domain().expect("known zoom domain");
    let plan = clipasm::preflight::preflight(&compiled).expect("preflight");
    let result = &plan.nodes()[plan.result().get() as usize];
    let Some(PreparedVideoKind::Zoom { input, percent }) = result.video_kind() else {
        panic!("prepared zoom");
    };
    assert_eq!(input.get(), 0);
    assert_eq!(*percent, 12);
    assert_eq!(result.video_domain(), Some(&input_domain));
}

#[test]
fn prepared_wobble_preserves_the_exact_input_domain_and_amplitude() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 48, fps: 10}\n    output: final.mp4\n\n\n- glue:\n    - image: {path: card.ppm, duration: 1s}\n    - wobble: 4",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
    let input_domain = *compiled.result_domain().expect("known wobble domain");
    let plan = clipasm::preflight::preflight(&compiled).expect("preflight");
    let result = &plan.nodes()[plan.result().get() as usize];
    let Some(PreparedVideoKind::Wobble { input, pixels }) = result.video_kind() else {
        panic!("prepared wobble");
    };
    assert_eq!(input.get(), 0);
    assert_eq!(*pixels, 4);
    assert_eq!(result.video_domain(), Some(&input_domain));
    assert_ne!(result.fingerprint(), plan.nodes()[0].fingerprint());
}

#[test]
fn prepared_flash_preserves_order_frames_and_exact_summed_domain() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "before.ppm", "0 0 0");
    write_image(directory.path(), "after.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 48, fps: 10}\n    output: final.mp4\n\n\n- glue:\n    - image: {path: before.ppm, duration: 1s}\n    - image: {path: after.ppm, duration: 1s}\n    - flash: 4",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
    let plan = clipasm::preflight::preflight(&compiled).expect("preflight");
    let result = &plan.nodes()[plan.result().get() as usize];
    let Some(PreparedVideoKind::FlashJoin {
        before,
        after,
        frames,
    }) = result.video_kind()
    else {
        panic!("prepared flash");
    };
    assert_eq!(before.get(), 0);
    assert_eq!(after.get(), 1);
    assert_eq!(frames.0, 4);
    assert_eq!(result.video_domain().expect("Video node").frames().0, 20);
}

#[test]
fn preflight_rejects_flash_longer_than_a_deferred_after_video() {
    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
        eprintln!("skipping deferred flash test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "before.ppm", "0 0 0");
    let source = directory.path().join("after.mkv");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x48:r=10:d=1",
            "-c:v",
            "ffv1",
        ])
        .arg(&source)
        .status()
        .expect("create source video");
    assert!(status.success());
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 48, fps: 10}\n    output: final.mp4\n\n\n- glue:\n    - image: {path: before.ppm, duration: 1s}\n    - video: after.mkv\n    - flash: 11",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("deferred compile");
    assert!(compiled.result_domain().is_none());
    let error = clipasm::preflight::preflight(&compiled).expect_err("excessive flash frames");
    assert_eq!(error.code, "E_INVALID_FLASH_FRAMES");
    assert!(error.message.contains("11"));
    assert!(error.message.contains("10"));
}
