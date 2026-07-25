#![allow(missing_docs)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clipasm::{compiler, preflight, render};

fn compile_source(source: &str) -> clipasm::diagnostic::Result<compiler::CompiledProgram> {
    let source = clipasm::language::parse_str(Path::new("transitions.clipasm"), source)?;
    compiler::compile(&source)
}

fn compile_file(path: &Path) -> clipasm::diagnostic::Result<compiler::CompiledProgram> {
    let source = clipasm::language::parse_file(path)?;
    compiler::compile(&source)
}

fn write_image(directory: &Path, name: &str, color: &str) {
    fs::write(directory.join(name), format!("P3\n1 1\n255\n{color}\n"))
        .expect("write image fixture");
}

fn write_constant_audio(path: &Path, value: &str) {
    let source = format!("aevalsrc={value}|{value}:s=48000:d=1");
    let status = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-f", "lavfi", "-i", &source])
        .args(["-c:a", "pcm_s16le"])
        .arg(path)
        .status()
        .expect("create Audio fixture");
    assert!(status.success());
}

fn decode_video(path: &Path) -> Vec<u8> {
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .expect("decode Video");
    assert!(
        output.status.success(),
        "Video decode failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn decode_audio(path: &Path) -> Vec<u8> {
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-c:a", "pcm_s16le", "-f", "s16le", "-"])
        .output()
        .expect("decode Audio");
    assert!(
        output.status.success(),
        "Audio decode failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn cache_artifact(directory: &Path, fingerprint: &str, extension: &str) -> PathBuf {
    let cache = directory.join(".clipasm").join("cache");
    let namespaces = fs::read_dir(&cache)
        .expect("cache directory")
        .filter_map(|entry| {
            let entry = entry.expect("cache entry");
            entry
                .file_type()
                .expect("cache entry type")
                .is_dir()
                .then(|| entry.path())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        namespaces.len(),
        1,
        "unexpected cache namespaces: {namespaces:?}"
    );
    namespaces[0].join(format!("{fingerprint}.{extension}"))
}

#[test]
fn crossfade_normalizes_duration_and_shortens_the_domain() {
    let source = |crossfade: &str| {
        format!(
            "clipasm 1\nconfig {{ video {{ width = 64\nheight = 48\nfps = 10 }} }}\nimage(\"before.ppm\", 1s)\nimage(\"after.ppm\", 1s)\njoin {{ {crossfade} }}\n"
        )
    };
    let default = compile_source(&source("crossfade")).expect("default crossfade");
    let explicit = compile_source(&source("crossfade(500ms)")).expect("explicit default crossfade");

    assert_eq!(default.structure_hash(), explicit.structure_hash());
    assert_eq!(
        default.result_domain().expect("known domain").frames().0,
        15
    );
    let document: serde_json::Value =
        serde_json::from_str(&default.compiled_json().expect("compiled JSON"))
            .expect("compiled document");
    let transition = document["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["kind"]["operation"] == "crossfade")
        .expect("crossfade node");
    assert_eq!(transition["kind"]["frames"], 5);
    assert_eq!(transition["domain"]["frames"], 15);
}

#[test]
fn crossfade_rejects_empty_or_excessive_overlap() {
    for (duration, expected) in [("0ms", "at least one"), ("2s", "before")] {
        let source = format!(
            "clipasm 1\nconfig {{ video {{ fps = 10 }} }}\nimage(\"before.ppm\", 1s)\nimage(\"after.ppm\", 1s)\njoin {{ crossfade({duration}) }}\n"
        );
        let error = compile_source(&source).expect_err("invalid crossfade overlap");
        assert_eq!(error.code, "E_INVALID_CROSSFADE_DURATION");
        assert!(error.message.contains(expected), "{}", error.message);
    }
}

#[test]
fn preflight_checks_crossfade_against_deferred_video_duration() {
    if !common::media_tools_available() {
        eprintln!("skipping deferred crossfade test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "before.ppm", "255 0 0");
    let after = directory.path().join("after.mkv");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=64x48:r=10:d=0.2",
            "-c:v",
            "ffv1",
        ])
        .arg(&after)
        .status()
        .expect("create deferred Video");
    assert!(status.success());
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig {\n  video { width = 64\nheight = 48\nfps = 10 }\n  output = \"result.mp4\"\n}\nimage(\"before.ppm\", 1s)\nvideo(\"after.mkv\")\njoin { crossfade(500ms) }\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("pure compile");
    assert!(compiled.result_domain().is_none());
    let error = preflight::preflight(&compiled).expect_err("deferred overlap validation");
    assert_eq!(error.code, "E_INVALID_CROSSFADE_DURATION");
    assert!(error.message.contains("`after` contains only 2 frames"));
}

#[test]
fn crossfade_renders_a_one_frame_full_overlap() {
    const WIDTH: usize = 64;
    const HEIGHT: usize = 48;

    if !common::media_tools_available() {
        eprintln!("skipping one-frame crossfade test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "before.ppm", "255 0 0");
    write_image(directory.path(), "after.ppm", "0 0 255");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig {\n  video { width = 64\nheight = 48\nfps = 10 }\n  output = \"result.mp4\"\n}\nimage(\"before.ppm\", 100ms, stretch)\nimage(\"after.ppm\", 100ms, stretch)\njoin { crossfade(100ms) }\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
    assert_eq!(compiled.result_domain().expect("domain").frames().0, 1);
    let plan = preflight::preflight(&compiled).expect("preflight");
    let report = render::render(&plan).expect("render crossfade");
    let video = decode_video(&report.output);
    assert_eq!(video.len(), WIDTH * HEIGHT * 3);
    let red = video
        .chunks_exact(3)
        .map(|pixel| u64::from(pixel[0]))
        .sum::<u64>()
        / u64::try_from(WIDTH * HEIGHT).expect("pixel count");
    let blue = video
        .chunks_exact(3)
        .map(|pixel| u64::from(pixel[2]))
        .sum::<u64>()
        / u64::try_from(WIDTH * HEIGHT).expect("pixel count");
    assert!(red > 80 && blue > 80);
    assert!(red.abs_diff(blue) < 40);
}

#[test]
fn crossfade_renders_exact_picture_and_phase_aligned_audio() {
    const WIDTH: usize = 64;
    const HEIGHT: usize = 48;
    const FRAME_BYTES: usize = WIDTH * HEIGHT * 3;
    const FPS: u64 = 29;
    const SOURCE_FRAMES: u64 = 29;
    const OVERLAP_FRAMES: u64 = 15;
    const OUTPUT_FRAMES: u64 = 43;
    const SAMPLE_RATE: u64 = 48_000;

    if !common::media_tools_available() {
        eprintln!("skipping crossfade render test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "before.ppm", "255 0 0");
    write_image(directory.path(), "after.ppm", "0 0 255");
    write_constant_audio(&directory.path().join("before.wav"), "0.2");
    write_constant_audio(&directory.path().join("after.wav"), "-0.2");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig {\n  video { width = 64\nheight = 48\nfps = 29 }\n  output = \"result.mp4\"\n}\nimage(\"before.ppm\", 1s, stretch)\naudio(\"before.wav\")\nset_audio\nimage(\"after.ppm\", 1s, stretch)\naudio(\"after.wav\")\nset_audio\njoin { crossfade(500ms) }\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        OUTPUT_FRAMES
    );
    let plan = preflight::preflight(&compiled).expect("preflight");
    let transition = plan
        .nodes()
        .iter()
        .find(|node| {
            matches!(
                node.video_kind(),
                Some(preflight::PreparedVideoKind::Crossfade { .. })
            )
        })
        .expect("prepared crossfade");
    assert_eq!(
        transition.video_domain().expect("Video domain").frames().0,
        OUTPUT_FRAMES
    );
    assert!(transition.has_audio());

    let report = render::render(&plan).expect("render crossfade");
    let video = decode_video(&report.output);
    assert_eq!(
        video.len(),
        FRAME_BYTES * usize::try_from(OUTPUT_FRAMES).expect("frame count")
    );
    let average = |frame: usize, channel: usize| {
        let pixels = &video[frame * FRAME_BYTES..(frame + 1) * FRAME_BYTES];
        let sum = pixels
            .chunks_exact(3)
            .map(|pixel| u64::from(pixel[channel]))
            .sum::<u64>();
        sum / u64::try_from(WIDTH * HEIGHT).expect("pixel count")
    };
    let prefix_last = usize::try_from(SOURCE_FRAMES - OVERLAP_FRAMES - 1).expect("frame");
    let overlap_first = prefix_last + 1;
    let overlap_middle = overlap_first + usize::try_from(OVERLAP_FRAMES / 2).expect("frame");
    let overlap_last = overlap_first + usize::try_from(OVERLAP_FRAMES - 1).expect("frame");
    let suffix_first = overlap_last + 1;
    for frame in [prefix_last, overlap_first] {
        assert!(average(frame, 0) > 220 && average(frame, 2) < 35);
    }
    assert!(average(overlap_middle, 0) > 80);
    assert!(average(overlap_middle, 2) > 80);
    assert!(average(overlap_middle, 0).abs_diff(average(overlap_middle, 2)) < 50);
    for frame in [overlap_last, suffix_first] {
        assert!(average(frame, 2) > 220 && average(frame, 0) < 35);
    }

    let artifact = cache_artifact(directory.path(), transition.fingerprint(), "mkv");
    let audio = decode_audio(&artifact);
    let boundary = |frame: u64| frame.saturating_mul(SAMPLE_RATE).div_ceil(FPS);
    let prefix_samples = boundary(SOURCE_FRAMES - OVERLAP_FRAMES);
    let before_end = boundary(SOURCE_FRAMES);
    let output_samples = boundary(OUTPUT_FRAMES);
    let overlap_samples = before_end - prefix_samples;
    assert_eq!(
        audio.len(),
        usize::try_from(output_samples * 2 * 2).expect("Audio byte count")
    );
    let sample = |index: u64| {
        let offset = usize::try_from(index * 4).expect("sample byte offset");
        i16::from_le_bytes([audio[offset], audio[offset + 1]])
    };
    assert!(sample(0) > 5_000);
    assert!(sample(prefix_samples - 1) > 5_000);
    assert!(sample(prefix_samples) > 5_000);
    assert!(sample(prefix_samples + overlap_samples / 2).unsigned_abs() < 300);
    assert!(sample(before_end - 1) < -5_000);
    assert!(sample(before_end) < -5_000);
    assert!(sample(output_samples - 2) < -5_000);
    assert!(sample(output_samples - 1).unsigned_abs() < 300);
}
