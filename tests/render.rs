use std::fs;
use std::process::Command;

use rhythmcut::{compiler, preflight, render};

#[test]
fn renders_and_reuses_verified_cache() {
    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
        eprintln!("skipping render test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n2 2\n255\n255 0 0  0 255 0\n0 0 255  255 255 0\n",
    )
    .expect("image");
    let workflow_path = directory.path().join("workflow.yaml");
    fs::write(
        &workflow_path,
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 20/2}\nclips:\n  card:\n    - image:\n        path: card.ppm\n        duration: 1s\n    - repeat: 2\ntimeline:\n  - $card\noutput: final.mp4\n",
    )
    .expect("workflow");
    let compiled = compiler::compile_file(&workflow_path).expect("compile");
    assert_eq!(compiled.video().fps.numerator(), 10);
    assert_eq!(compiled.video().fps.denominator(), 1);
    let plan = preflight::preflight(&compiled).expect("preflight");
    fs::write(plan.output(), b"previous valid destination").expect("old output");
    fs::write(plan.manifest(), b"previous manifest").expect("old manifest");
    let first = render::render(&plan).expect("first render");
    assert!(first.output.is_file());
    assert_ne!(
        fs::read(&first.output).expect("new output"),
        b"previous valid destination"
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&first.manifest).expect("new manifest"))
            .expect("manifest JSON");
    assert_eq!(manifest["plan"]["video"]["fps"]["numerator"], 10);
    assert_eq!(first.cache_hits, 0);
    assert_eq!(first.cache_misses, plan.nodes().len());
    let second = render::render(&plan).expect("cached render");
    assert_eq!(second.cache_hits, plan.nodes().len());
    assert_eq!(second.cache_misses, 0);
    assert!(second.manifest.is_file());
}

#[test]
fn renders_during_with_an_exact_duration_change() {
    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
        eprintln!("skipping render test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n2 2\n255\n255 0 0  0 255 0\n0 0 255  255 255 0\n",
    )
    .expect("image");
    let workflow_path = directory.path().join("workflow.yaml");
    fs::write(
        &workflow_path,
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\ntimeline:\n  - image:\n      path: card.ppm\n      duration: 1s\n  - repeat: 2\n    during: 200ms..400ms\noutput: during.mp4\n",
    )
    .expect("workflow");
    let compiled = compiler::compile_file(&workflow_path).expect("compile");
    assert_eq!(compiled.root_domain().expect("known domain").frames.0, 12);
    let plan = preflight::preflight(&compiled).expect("preflight");
    let report = render::render(&plan).expect("render during");
    assert!(report.output.is_file());
    assert_eq!(report.cache_misses, plan.nodes().len());
}

#[test]
fn renders_and_normalizes_a_video_source() {
    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
        eprintln!("skipping render test because FFmpeg/FFprobe are unavailable");
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
            "testsrc=size=96x48:rate=12:duration=2",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=1000:sample_rate=48000:duration=2",
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-c:v",
            "ffv1",
            "-c:a",
            "pcm_s16le",
            "-shortest",
        ])
        .arg(&source)
        .status()
        .expect("create source video");
    assert!(status.success());
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n2 2\n255\n255 0 0  0 255 0\n0 0 255  255 255 0\n",
    )
    .expect("image");
    let workflow_path = directory.path().join("workflow.yaml");
    fs::write(
        &workflow_path,
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\ntimeline:\n  - image:\n      path: card.ppm\n      duration: 1s\n  - video:\n      path: source.mkv\n      fit: contain\noutput: video-source.mp4\n",
    )
    .expect("workflow");

    let compiled = compiler::compile_file(&workflow_path).expect("compile");
    assert!(compiled.root_domain().is_none());
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert!(matches!(
        plan.nodes()[1].kind(),
        preflight::PreparedNodeKind::VideoSource { .. }
    ));
    assert_eq!(plan.nodes()[1].domain().frames.0, 20);
    assert_eq!(
        plan.nodes()[plan.root().get() as usize].domain().frames.0,
        30
    );
    let first = render::render(&plan).expect("first render");
    assert_eq!(first.cache_hits, 0);
    assert_eq!(first.cache_misses, plan.nodes().len());
    assert!(first.output.is_file());
    let second = render::render(&plan).expect("cached render");
    assert_eq!(second.cache_hits, plan.nodes().len());
    assert_eq!(second.cache_misses, 0);
}
