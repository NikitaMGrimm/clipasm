use std::fs;
use std::process::Command;

use rhythmcut::{compiler, render};

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
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\nclips:\n  card:\n    - image: card.ppm\n      duration: 1s\n    - repeat: 2\ntimeline:\n  - $card\noutput: final.mp4\n",
    )
    .expect("workflow");
    let plan = compiler::compile_file(&workflow_path).expect("compile");
    let first = render::render(&plan, &workflow_path).expect("first render");
    assert!(first.output.is_file());
    assert_eq!(first.cache_hits, 0);
    assert_eq!(first.cache_misses, plan.nodes.len());
    let second = render::render(&plan, &workflow_path).expect("cached render");
    assert_eq!(second.cache_hits, plan.nodes.len());
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
        "version: 1\nproject:\n  video: {width: 64, height: 64, fps: 10}\ntimeline:\n  - image: card.ppm\n    duration: 1s\n  - repeat: 2\n    during: 200ms..400ms\noutput: during.mp4\n",
    )
    .expect("workflow");
    let plan = compiler::compile_file(&workflow_path).expect("compile");
    assert_eq!(plan.nodes[plan.root.0 as usize].frames.0, 12);
    let report = render::render(&plan, &workflow_path).expect("render during");
    assert!(report.output.is_file());
    assert_eq!(report.cache_misses, plan.nodes.len());
}
