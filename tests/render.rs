#![allow(missing_docs)]

use std::fs;
use std::path::Path;
use std::process::Command;

use clipasm::{compiler, preflight, render};

fn compile_yaml(path: &Path) -> clipasm::diagnostic::Result<compiler::CompiledProgram> {
    let source = clipasm::frontend::yaml::parse_file(path)?;
    compiler::compile(&source)
}

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
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 20/2}\n    clips:\n      card:\n        - image:\n            path: card.ppm\n            duration: 1s\n        - repeat: 2\n    output: final.mp4\n\n\n- glue:\n    - $card",
    )
    .expect("workflow");
    let compiled = compile_yaml(&workflow_path).expect("compile");
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
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n    output: during.mp4\n\n\n- glue:\n    - image:\n        path: card.ppm\n        duration: 1s\n    - repeat: 2\n      during: 200ms..400ms",
    )
    .expect("workflow");
    let compiled = compile_yaml(&workflow_path).expect("compile");
    assert_eq!(compiled.result_domain().expect("known domain").frames.0, 12);
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
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n    output: video-source.mp4\n\n\n- glue:\n    - image:\n        path: card.ppm\n        duration: 1s\n    - video:\n        path: source.mkv\n        fit: contain",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow_path).expect("compile");
    assert!(compiled.result_domain().is_none());
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert!(matches!(
        plan.nodes()[1].kind(),
        preflight::PreparedNodeKind::VideoSource { .. }
    ));
    assert_eq!(plan.nodes()[1].domain().frames.0, 20);
    assert_eq!(
        plan.nodes()[plan.result().get() as usize].domain().frames.0,
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
            "color=c=red:s=64x64:r=25:d=1",
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
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 30000/1001}\n    output: final.mp4\n\n\n- glue:\n    - video: source.mkv",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert_eq!(
        plan.nodes()[plan.result().get() as usize].domain().frames.0,
        30
    );
    let report = render::render(&plan).expect("render");
    assert!(report.output.is_file());
}

#[test]
fn nonempty_video_shorter_than_one_project_frame_renders_one_frame() {
    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
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
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 5}\n    output: final.mp4\n\n\n- glue:\n    - video: source.mkv",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert_eq!(
        plan.nodes()[plan.result().get() as usize].domain().frames.0,
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
    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
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
        let workflow = directory.path().join("workflow.yaml");
        fs::write(
            &workflow,
            format!(
                "- program:\n    version: 1\n    project:\n      video: {{width: 64, height: 48, fps: 10}}\n    output: zoom.mp4\n\n\n- glue:\n    - image: {{path: card.ppm, duration: {duration}, fit: stretch}}\n    - zoom: 20"
            ),
        )
        .expect("workflow");

        let compiled = compile_yaml(&workflow).expect("compile");
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

    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
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

    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 48, fps: 10}\n    output: zoom.mp4\n\n\n- glue:\n    - image: {path: center.ppm, duration: 1s, fit: stretch}\n    - zoom: 100",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
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
    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
        eprintln!("skipping wobble render test because FFmpeg/FFprobe are unavailable");
        return;
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("white.ppm"),
        b"P3\n2 2\n255\n255 255 255  255 255 255\n255 255 255  255 255 255\n",
    )
    .expect("image");
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 48, fps: 10}\n    output: wobble.mp4\n\n\n- glue:\n    - image: {path: white.ppm, duration: 1s, fit: stretch}\n    - wobble: 4",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
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

    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
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
    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 48, fps: 10}\n    output: flash.mp4\n\n\n- glue:\n    - image: {path: before.ppm, duration: 1s, fit: stretch}\n    - image: {path: after.ppm, duration: 1s, fit: stretch}\n    - join:\n        - flash: 4",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert_eq!(
        plan.nodes()[plan.result().get() as usize].domain().frames.0,
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
    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
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

        let workflow = directory.path().join("workflow.yaml");
        fs::write(
            &workflow,
            format!(
                "- program:\n    version: 1\n    project:\n      video: {{width: 64, height: 64, fps: 10}}\n    output: {name}.mp4\n\n- image: {{path: card.ppm, duration: 3s}}\n- audio: tone.wav\n- set_audio\n"
            ),
        )
        .expect("workflow");

        let compiled = compile_yaml(&workflow).expect("compile");
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
    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
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

    let workflow = directory.path().join("workflow.yaml");
    fs::write(
        &workflow,
        "- program:\n    version: 1\n    project:\n      video: {width: 64, height: 64, fps: 10}\n    output: native-audio.mp4\n\n- image: {path: card.ppm, duration: 2s}\n- audio: tone.wav\n- trim: 100ms..300ms\n- repeat: 2\n- audio: tone.wav\n- trim: 300ms..500ms\n- concat: Audio\n- set_audio\n",
    )
    .expect("workflow");

    let compiled = compile_yaml(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert!(
        plan.nodes()
            .iter()
            .any(|node| matches!(node.kind(), preflight::PreparedNodeKind::AudioSlice { .. }))
    );
    assert!(
        plan.nodes()
            .iter()
            .any(|node| matches!(node.kind(), preflight::PreparedNodeKind::AudioRepeat { .. }))
    );
    assert!(
        plan.nodes()
            .iter()
            .any(|node| matches!(node.kind(), preflight::PreparedNodeKind::AudioConcat { .. }))
    );
    let report = render::render(&plan).expect("render native audio operations");
    assert!(report.output.is_file());
}
