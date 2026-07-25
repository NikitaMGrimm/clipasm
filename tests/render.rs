#![allow(missing_docs)]

mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use clipasm::{compiler, preflight, render};

fn compile_file(path: &Path) -> clipasm::diagnostic::Result<compiler::CompiledProgram> {
    let source = clipasm::language::parse_file(path)?;
    compiler::compile(&source)
}

fn color_project(color: &str) -> (tempfile::TempDir, compiler::CompiledProgram) {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        format!("P3\n1 1\n255\n{color}\n"),
    )
    .expect("image");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"final.mp4\" }\nimage(\"card.ppm\", 1s, stretch)\n",
    )
    .expect("workflow");
    let compiled = compile_file(&workflow).expect("compile");
    (directory, compiled)
}

#[test]
fn renders_and_reuses_verified_cache() {
    if !common::media_tools_available() {
        eprintln!("skipping render test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n2 2\n255\n255 0 0  0 255 0\n0 0 255  255 255 0\n",
    )
    .expect("image");
    let workflow_path = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow_path,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 20/2 }\noutput = \"final.mp4\" }\nclip { image(\"card.ppm\", 1s)\nrepeat(2) } as card\n$card\n",
    )
    .expect("workflow");
    let compiled = compile_file(&workflow_path).expect("compile");
    assert_eq!(compiled.video().fps().numerator(), 10);
    assert_eq!(compiled.video().fps().denominator(), 1);
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
    assert_eq!(manifest["format_version"], 1);
    assert_eq!(manifest["project"]["video"]["fps"]["numerator"], 10);
    assert_eq!(manifest["semantic_hash"], plan.semantic_hash());
    assert_eq!(manifest["cache"]["hits"], 0);
    assert_eq!(manifest["cache"]["misses"], plan.nodes().len());
    assert!(manifest.get("plan").is_none());
    assert!(manifest.get("execution_namespace").is_none());
    assert_eq!(first.cache_hits, 0);
    assert_eq!(first.cache_misses, plan.nodes().len());
    let second = render::render(&plan).expect("cached render");
    assert_eq!(second.cache_hits, plan.nodes().len());
    assert_eq!(second.cache_misses, 0);
    assert!(second.manifest.is_file());
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&second.manifest).expect("cached manifest"))
            .expect("cached manifest JSON");
    assert_eq!(manifest["cache"]["hits"], plan.nodes().len());
    assert_eq!(manifest["cache"]["misses"], 0);
}

#[test]
fn shape_compatible_cache_substitution_is_rejected() {
    if !common::media_tools_available() {
        eprintln!("skipping cache substitution test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let (red_directory, red_compiled) = color_project("255 0 0");
    let red_plan = preflight::preflight(&red_compiled).expect("red preflight");
    render::render(&red_plan).expect("red render");
    let red_node = &red_plan.nodes()[red_plan.result().get() as usize];
    let red_artifact = common::cache_artifact(red_directory.path(), red_node.fingerprint(), "mkv");

    let (blue_directory, blue_compiled) = color_project("0 0 255");
    let blue_plan = preflight::preflight(&blue_compiled).expect("blue preflight");
    render::render(&blue_plan).expect("blue render");
    let blue_node = &blue_plan.nodes()[blue_plan.result().get() as usize];
    let blue_artifact =
        common::cache_artifact(blue_directory.path(), blue_node.fingerprint(), "mkv");
    fs::copy(&blue_artifact, &red_artifact).expect("substitute shape-compatible artifact");

    let report = render::render(&red_plan).expect("rerender substituted cache");
    assert_eq!(report.cache_hits, 0);
    assert_eq!(report.cache_misses, 1);
    let decoded = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&report.output)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .expect("decode output");
    assert!(decoded.status.success());
    assert!(decoded.stdout[0] > 200, "expected red output");
    assert!(decoded.stdout[2] < 50, "unexpected blue substitution");
}

#[test]
fn renders_during_with_an_exact_duration_change() {
    if !common::media_tools_available() {
        eprintln!("skipping render test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n2 2\n255\n255 0 0  0 255 0\n0 0 255  255 255 0\n",
    )
    .expect("image");
    let workflow_path = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow_path,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"during.mp4\" }\nimage(\"card.ppm\", 1s)\nduring(200ms..400ms) { repeat(2) }\n",
    )
    .expect("workflow");
    let compiled = compile_file(&workflow_path).expect("compile");
    assert_eq!(
        compiled.result_domain().expect("known domain").frames().0,
        12
    );
    let plan = preflight::preflight(&compiled).expect("preflight");
    let report = render::render(&plan).expect("render during");
    assert!(report.output.is_file());
    assert_eq!(report.cache_misses, plan.nodes().len());
}

#[test]
fn renders_and_normalizes_a_video_source() {
    if !common::media_tools_available() {
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
    let workflow_path = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow_path,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"video-source.mp4\" }\nglue {\n  image(\"card.ppm\", 1s)\n  video(\"source.mkv\", contain)\n}\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow_path).expect("compile");
    assert!(compiled.result_domain().is_none());
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert!(matches!(
        plan.nodes()[1].video_kind(),
        Some(preflight::PreparedVideoKind::VideoSource { .. })
    ));
    assert_eq!(
        plan.nodes()[1]
            .video_domain()
            .expect("Video node")
            .frames()
            .0,
        20
    );
    assert_eq!(
        plan.nodes()[plan.result().get() as usize]
            .video_domain()
            .expect("Video node")
            .frames()
            .0,
        30
    );
    let first = render::render(&plan).expect("first render");
    assert_eq!(first.cache_hits, 0);
    assert_eq!(first.cache_misses, plan.nodes().len());
    assert!(first.output.is_file());
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "json",
        ])
        .arg(&first.output)
        .output()
        .expect("probe rendered source audio");
    let document: serde_json::Value = serde_json::from_slice(&probe.stdout).expect("probe JSON");
    assert_eq!(
        document["streams"]
            .as_array()
            .expect("streams")
            .iter()
            .filter(|stream| stream["codec_type"] == "audio")
            .count(),
        1
    );
    let second = render::render(&plan).expect("cached render");
    assert_eq!(second.cache_hits, plan.nodes().len());
    assert_eq!(second.cache_misses, 0);
}

#[test]
fn video_source_duration_is_quantized_by_coverage() {
    if !common::media_tools_available() {
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
            "color=c=red:s=64x64:r=25:d=1",
            "-c:v",
            "ffv1",
        ])
        .arg(&source)
        .status()
        .expect("create source video");
    assert!(status.success());
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 30000/1001 }\noutput = \"final.mp4\" }\nvideo(\"source.mkv\")\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert_eq!(
        plan.nodes()[plan.result().get() as usize]
            .video_domain()
            .expect("Video node")
            .frames()
            .0,
        30
    );
    let report = render::render(&plan).expect("render");
    assert!(report.output.is_file());
}

#[test]
fn nonempty_video_shorter_than_one_project_frame_renders_one_frame() {
    if !common::media_tools_available() {
        eprintln!("skipping short-source render test because FFmpeg/FFprobe are unavailable");
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
            "color=c=red:s=64x64:r=120:d=0.1",
            "-c:v",
            "ffv1",
        ])
        .arg(&source)
        .status()
        .expect("create one-frame source");
    assert!(status.success());
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 5 }\noutput = \"final.mp4\" }\nvideo(\"source.mkv\")\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert_eq!(
        plan.nodes()[plan.result().get() as usize]
            .video_domain()
            .expect("Video node")
            .frames()
            .0,
        1
    );
    let report = render::render(&plan).expect("render");
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-count_frames",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=nb_read_frames",
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(&report.output)
        .output()
        .expect("probe output");
    assert!(probe.status.success());
    assert_eq!(String::from_utf8_lossy(&probe.stdout).trim(), "1");
}

#[test]
fn zoom_renders_exact_frames_and_dimensions_including_one_frame() {
    if !common::media_tools_available() {
        eprintln!("skipping zoom render test because FFmpeg/FFprobe are unavailable");
        return;
    }

    for (frames, duration) in [(1_u64, "100ms"), (4, "400ms")] {
        let directory = tempfile::tempdir().expect("temporary directory");
        fs::write(
            directory.path().join("card.ppm"),
            b"P3\n2 2\n255\n255 0 0  0 255 0\n0 0 255  255 255 0\n",
        )
        .expect("image");
        let workflow = directory.path().join("workflow.clipasm");
        fs::write(
            &workflow,
            format!(
                "clipasm 1\nconfig {{ video {{ width = 64\nheight = 48\nfps = 10 }}\noutput = \"zoom.mp4\" }}\nimage(\"card.ppm\", {duration}, stretch)\nzoom(20)\n"
            ),
        )
        .expect("workflow");

        let compiled = compile_file(&workflow).expect("compile");
        let plan = preflight::preflight(&compiled).expect("preflight");
        let report = render::render(&plan).expect("render zoom");
        let output = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-count_frames",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=width,height,r_frame_rate,nb_read_frames",
                "-of",
                "json",
            ])
            .arg(&report.output)
            .output()
            .expect("probe zoom");
        assert!(
            output.status.success(),
            "FFprobe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let probe: serde_json::Value = serde_json::from_slice(&output.stdout).expect("probe JSON");
        let stream = &probe["streams"][0];
        assert_eq!(stream["width"], 64);
        assert_eq!(stream["height"], 48);
        assert_eq!(stream["r_frame_rate"], "10/1");
        assert_eq!(stream["nb_read_frames"], frames.to_string());
    }
}

#[test]
fn zoom_remains_centered_instead_of_anchoring_to_the_top_left() {
    const WIDTH: usize = 64;
    const HEIGHT: usize = 48;
    const FRAME_BYTES: usize = WIDTH * HEIGHT * 3;

    if !common::media_tools_available() {
        eprintln!("skipping zoom centering test because FFmpeg/FFprobe are unavailable");
        return;
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    let mut image = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
    let mut pixels = vec![0_u8; FRAME_BYTES];
    for y in HEIGHT / 2 - 2..=HEIGHT / 2 + 2 {
        for x in WIDTH / 2 - 2..=WIDTH / 2 + 2 {
            let offset = (y * WIDTH + x) * 3;
            pixels[offset..offset + 3].fill(255);
        }
    }
    image.extend_from_slice(&pixels);
    fs::write(directory.path().join("center.ppm"), image).expect("center marker image");

    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 }\noutput = \"zoom.mp4\" }\nimage(\"center.ppm\", 1s, stretch)\nzoom(100)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    let report = render::render(&plan).expect("render zoom");
    let decoded = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&report.output)
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .expect("decode zoom");
    assert!(
        decoded.status.success(),
        "FFmpeg decode failed: {}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    assert_eq!(decoded.stdout.len(), FRAME_BYTES * 10);

    let final_frame = &decoded.stdout[FRAME_BYTES * 9..FRAME_BYTES * 10];
    let center = ((HEIGHT / 2) * WIDTH + WIDTH / 2) * 3;
    assert!(
        final_frame[center..center + 3]
            .iter()
            .all(|channel| *channel > 200),
        "the centered marker moved away from the frame center"
    );
}

#[test]
fn wobble_renders_exact_domain_without_exposing_borders() {
    if !common::media_tools_available() {
        eprintln!("skipping wobble render test because FFmpeg/FFprobe are unavailable");
        return;
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("white.ppm"),
        b"P3\n2 2\n255\n255 255 255  255 255 255\n255 255 255  255 255 255\n",
    )
    .expect("image");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 }\noutput = \"wobble.mp4\" }\nimage(\"white.ppm\", 1s, stretch)\nwobble(4)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    let report = render::render(&plan).expect("render wobble");
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-count_frames",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate,nb_read_frames",
            "-of",
            "json",
        ])
        .arg(&report.output)
        .output()
        .expect("probe wobble");
    assert!(
        probe.status.success(),
        "FFprobe failed: {}",
        String::from_utf8_lossy(&probe.stderr)
    );
    let probe: serde_json::Value = serde_json::from_slice(&probe.stdout).expect("probe JSON");
    let stream = &probe["streams"][0];
    assert_eq!(stream["width"], 64);
    assert_eq!(stream["height"], 48);
    assert_eq!(stream["r_frame_rate"], "10/1");
    assert_eq!(stream["nb_read_frames"], "10");

    let decoded = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&report.output)
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .expect("decode wobble");
    assert!(
        decoded.status.success(),
        "FFmpeg decode failed: {}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    assert_eq!(decoded.stdout.len(), 64 * 48 * 3 * 10);
    assert!(
        decoded.stdout.iter().all(|sample| *sample >= 240),
        "wobble exposed a dark border; darkest decoded sample was {}",
        decoded.stdout.iter().min().copied().unwrap_or_default()
    );
}

#[test]
fn flash_renders_an_exact_join_with_a_white_to_normal_after_cut() {
    const FRAME_BYTES: usize = 64 * 48 * 3;

    if !common::media_tools_available() {
        eprintln!("skipping flash render test because FFmpeg/FFprobe are unavailable");
        return;
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("before.ppm"),
        b"P3\n2 2\n255\n0 0 0  0 0 0\n0 0 0  0 0 0\n",
    )
    .expect("before image");
    fs::write(
        directory.path().join("after.ppm"),
        b"P3\n2 2\n255\n255 0 0  255 0 0\n255 0 0  255 0 0\n",
    )
    .expect("after image");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 }\noutput = \"flash.mp4\" }\nimage(\"before.ppm\", 1s, stretch)\nimage(\"after.ppm\", 1s, stretch)\njoin { flash(4) }\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert_eq!(
        plan.nodes()[plan.result().get() as usize]
            .video_domain()
            .expect("Video node")
            .frames()
            .0,
        20
    );
    let report = render::render(&plan).expect("render flash");
    let decoded = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&report.output)
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .expect("decode flash");
    assert!(
        decoded.status.success(),
        "FFmpeg decode failed: {}",
        String::from_utf8_lossy(&decoded.stderr)
    );

    assert_eq!(decoded.stdout.len(), FRAME_BYTES * 20);
    let brightness = |frame: usize| {
        let pixels = &decoded.stdout[frame * FRAME_BYTES..(frame + 1) * FRAME_BYTES];
        pixels.iter().map(|sample| u64::from(*sample)).sum::<u64>()
            / u64::try_from(pixels.len()).expect("frame byte count fits u64")
    };
    let before = brightness(9);
    let first_after = brightness(10);
    let transition_end = brightness(13);
    let normal_after = brightness(19);
    assert!(before < 15, "before-cut frame was not black: {before}");
    assert!(
        first_after > 225,
        "first post-cut frame was not white: {first_after}"
    );
    assert!(
        transition_end + 50 < first_after,
        "flash did not visibly clear: first={first_after}, end={transition_end}"
    );
    assert!(
        transition_end.abs_diff(normal_after) < 80,
        "transition end did not approach normal: end={transition_end}, normal={normal_after}"
    );
}

#[test]
fn set_audio_trims_or_pads_to_the_video_duration() {
    if !common::media_tools_available() {
        eprintln!("skipping audio render test because FFmpeg/FFprobe are unavailable");
        return;
    }

    for (name, audio_duration) in [("short", "1"), ("long", "5")] {
        let directory = tempfile::tempdir().expect("temporary directory");
        fs::write(
            directory.path().join("card.ppm"),
            b"P3\n2 2\n255\n255 0 0  255 0 0\n255 0 0  255 0 0\n",
        )
        .expect("image");
        let audio = directory.path().join("tone.wav");
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                &format!("sine=frequency=440:sample_rate=48000:duration={audio_duration}"),
                "-ac",
                "2",
            ])
            .arg(&audio)
            .status()
            .expect("create audio fixture");
        assert!(status.success());

        let workflow = directory.path().join("workflow.clipasm");
        fs::write(
            &workflow,
            format!(
                "clipasm 1\nconfig {{ video {{ width = 64\nheight = 64\nfps = 10 }}\noutput = \"{name}.mp4\" }}\nimage(\"card.ppm\", 3s)\naudio(\"tone.wav\")\nset_audio\n"
            ),
        )
        .expect("workflow");

        let compiled = compile_file(&workflow).expect("compile");
        let plan = preflight::preflight(&compiled).expect("preflight");
        let report = render::render(&plan).expect("render");
        let probe = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type",
                "-show_entries",
                "format=duration",
                "-of",
                "json",
            ])
            .arg(&report.output)
            .output()
            .expect("probe output");
        assert!(probe.status.success());
        let document: serde_json::Value =
            serde_json::from_slice(&probe.stdout).expect("probe JSON");
        let streams = document["streams"].as_array().expect("streams");
        assert_eq!(
            streams
                .iter()
                .filter(|stream| stream["codec_type"] == "audio")
                .count(),
            1
        );
        let duration = document["format"]["duration"]
            .as_str()
            .expect("duration")
            .parse::<f64>()
            .expect("numeric duration");
        assert!((duration - 3.0).abs() < 0.15, "duration was {duration}");
    }
}

#[test]
fn renders_native_audio_trim_repeat_and_concat() {
    if !common::media_tools_available() {
        eprintln!("skipping audio render test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n2 2\n255\n255 0 0  255 0 0\n255 0 0  255 0 0\n",
    )
    .expect("image");
    let tone = directory.path().join("tone.wav");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=1",
            "-ac",
            "2",
        ])
        .arg(&tone)
        .status()
        .expect("create audio fixture");
    assert!(status.success());

    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"native-audio.mp4\" }\nimage(\"card.ppm\", 2s)\naudio(\"tone.wav\")\ntrim(100ms..300ms)\nrepeat(2)\naudio(\"tone.wav\")\ntrim(300ms..500ms)\nconcat<Audio>\nset_audio\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert!(plan.nodes().iter().any(|node| matches!(
        node.audio_kind(),
        Some(preflight::PreparedAudioKind::AudioSlice { .. })
    )));
    assert!(plan.nodes().iter().any(|node| matches!(
        node.audio_kind(),
        Some(preflight::PreparedAudioKind::AudioRepeat { .. })
    )));
    assert!(plan.nodes().iter().any(|node| matches!(
        node.audio_kind(),
        Some(preflight::PreparedAudioKind::AudioConcat { .. })
    )));
    let report = render::render(&plan).expect("render native audio operations");
    assert!(report.output.is_file());
}

#[cfg(unix)]
#[test]
fn renders_non_utf8_output_without_serializing_local_paths() {
    use std::os::unix::ffi::OsStringExt as _;

    if !common::media_tools_available() {
        eprintln!("skipping non-UTF output test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("image");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(&workflow, "clipasm 1\nimage(\"card.ppm\", 100ms)\n").expect("workflow");
    let package = clipasm::language::parse_file(&workflow).expect("parse");
    let mut bindings = compiler::EntrypointBindings::new();
    let mut output_name = std::ffi::OsString::from_vec(b"video-\xFF.mp4".to_vec());
    let output = directory.path().join(&output_name);
    bindings.set_output(
        output.clone(),
        clipasm::source::SourceSpan::file_start(&workflow),
    );
    let compiled = compiler::compile_with_bindings(&package, &bindings).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    let inspection_error = plan
        .prepared_json()
        .expect_err("non-UTF local path cannot be represented in prepared JSON");
    assert_eq!(inspection_error.code, "E_PREPARED_JSON");
    let report = render::render(&plan).expect("render non-UTF output");

    assert_eq!(report.output, output);
    assert!(report.output.is_file());
    assert!(report.manifest.is_file());
    let document: serde_json::Value =
        serde_json::from_slice(&fs::read(&report.manifest).expect("manifest"))
            .expect("manifest JSON");
    assert_eq!(document["format_version"], 1);
    assert!(document.get("plan").is_none());
    output_name.push(".manifest.json");
    assert_eq!(report.manifest, directory.path().join(output_name));
}

#[cfg(unix)]
#[test]
fn renders_an_external_video_program() {
    if !common::media_tools_available() || !common::executable_available("python3", "--version") {
        eprintln!("skipping external render test because a required tool is unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n2 2\n255\n255 0 0  0 255 0\n0 0 255  255 255 0\n",
    )
    .expect("image");
    let script = directory.path().join("effect.py");
    fs::write(
        &script,
        r#"import json, pathlib, subprocess, sys
r = json.load(sys.stdin)
assert r["protocol_version"] == 2
assert r["parameters"]["amount"] == 7
assert pathlib.Path(r["parameters"]["lut"]).read_bytes() == b"original lookup"
subprocess.run([r["tools"]["ffmpeg"], "-y", "-v", "error", "-i", r["inputs"]["video"]["path"], "-map", "0:v:0", "-map", "0:a:0", "-c", "copy", r["output"]], check=True)
"#,
    )
    .expect("script");
    fs::write(directory.path().join("lut.bin"), b"original lookup").expect("lookup file");
    fs::write(
        directory.path().join("effect.clipasm"),
        "clipasm 1\ninput video: Video\nparam amount: Integer\nparam lut: File = \"lut.bin\"\nexternal {\n  executable = \"python3\"\n  arguments = [file(\"effect.py\")]\n  semantic_version = 1\n  preserve = video\n}\n",
    )
    .expect("external program");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"result.mp4\" }\nimport \"effect.clipasm\" as effect\nimage(\"card.ppm\", 1s)\neffect(7)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile external program");
    let plan = preflight::preflight(&compiled).expect("preflight external program");
    fs::write(&script, "raise RuntimeError('authored script changed')\n")
        .expect("change authored script");
    fs::write(directory.path().join("lut.bin"), b"changed lookup").expect("change lookup file");
    let report = render::render(&plan).expect("render external program");
    assert!(report.output.is_file());
    assert_eq!(report.cache_misses, 2);
}
