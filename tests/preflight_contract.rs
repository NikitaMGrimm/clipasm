#![allow(missing_docs)]

use std::fs;
use std::path::Path;
use std::process::Command;

use clipasm::preflight::PreparedNodeKind;

fn write_image(directory: &Path, name: &str, color: &str) {
    fs::write(directory.join(name), format!("P3\n1 1\n255\n{color}\n"))
        .expect("write image fixture");
}

#[test]
fn relocated_identical_projects_have_equal_semantic_hashes() {
    let first = tempfile::tempdir().expect("first directory");
    let second = tempfile::tempdir().expect("second directory");
    for directory in [first.path(), second.path()] {
        write_image(directory, "card.ppm", "255 0 0");
        fs::write(
            directory.join("workflow.yaml"),
            "version: 1\ntimeline:\n  - image:\n      path: card.ppm\n      duration: 1s\noutput: final.mp4\n",
        )
        .expect("workflow");
    }

    let first_compiled =
        clipasm::compiler::compile_file(&first.path().join("workflow.yaml")).expect("compile");
    let second_compiled =
        clipasm::compiler::compile_file(&second.path().join("workflow.yaml")).expect("compile");
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
        "version: 1\nclips:\n  unused:\n    image:\n      path: unused.ppm\n      duration: 1s\ntimeline:\n  - image:\n      path: used.ppm\n      duration: 1s\noutput: final.mp4\n",
    )
    .expect("workflow");

    let compiled = clipasm::compiler::compile_file(&workflow).expect("compile");
    let plan = clipasm::preflight::preflight(&compiled).expect("preflight");
    let image_nodes = plan
        .nodes()
        .iter()
        .filter(|node| matches!(node.kind(), PreparedNodeKind::ImageVideo { .. }))
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
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\ntimeline:\n  - image:\n      path: card.ppm\n      duration: 1s\noutput: final.mp4\n",
    )
    .expect("workflow");
    let compiled = clipasm::compiler::compile_file(&workflow).expect("compile");
    let prepared = clipasm::preflight::preflight(&compiled).expect("preflight");
    let PreparedNodeKind::ImageVideo { asset, .. } = prepared.nodes()[0].kind() else {
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
        "version: 1\nproject:\n  video: {width: 63, height: 65, fps: 10}\ntimeline:\n  - image:\n      path: card.ppm\n      duration: 1s\noutput: final.mp4\n",
    )
    .expect("workflow");
    let compiled = clipasm::compiler::compile_file(&workflow).expect("pure compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("export dimensions");
    assert_eq!(error.code, "E_EXPORT_DIMENSIONS");
}

#[test]
fn output_extension_is_strictly_mp4() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "version: 1\ntimeline:\n  - image:\n      path: missing.png\n      duration: 1s\noutput: final.mov\n",
    )
    .expect("workflow");
    let compiled = clipasm::compiler::compile_file(&workflow).expect("compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("extension");
    assert_eq!(error.code, "E_INVALID_OUTPUT_EXTENSION");
}

#[test]
fn video_preflight_reports_missing_files_by_source_kind() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "version: 1\ntimeline:\n  - video: missing.mp4\noutput: final.mp4\n",
    )
    .expect("workflow");

    let compiled = clipasm::compiler::compile_file(&workflow).expect("compile");
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
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\ntimeline:\n  - video: source.mkv\noutput: final.mp4\n",
    )
    .expect("workflow");

    let compiled = clipasm::compiler::compile_file(&workflow).expect("compile");
    let plan = clipasm::preflight::preflight(&compiled).expect("preflight");
    assert_eq!(plan.nodes()[0].domain().frames.0, 10);
}
