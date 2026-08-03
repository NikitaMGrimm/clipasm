#![allow(missing_docs)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};
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

fn render_to_cache(
    plan: &preflight::PreparedPlan,
    cache_root: &Path,
    materialization: render::MaterializationMode,
) -> render::RenderReport {
    render::render_with_options(
        plan,
        &render::RenderOptions::new(render::CacheMode::Persistent, materialization)
            .with_cache_root(cache_root),
    )
    .expect("render to isolated cache")
}

fn endpoint_cache_artifact(cache_root: &Path, plan: &preflight::PreparedPlan) -> PathBuf {
    let namespace = fs::read_dir(cache_root)
        .expect("cache root")
        .find_map(|entry| {
            let entry = entry.expect("cache entry");
            entry
                .file_type()
                .expect("cache entry type")
                .is_dir()
                .then(|| entry.path())
        })
        .expect("execution namespace");
    namespace.join(format!(
        "{}.mkv",
        plan.nodes()[plan.result().get() as usize].fingerprint()
    ))
}

fn decode_working_stream(path: &Path, arguments: [&str; 5]) -> Vec<u8> {
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(arguments)
        .output()
        .expect("decode working artifact");
    assert!(
        output.status.success(),
        "decode failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn assert_working_artifacts_equal(all: &Path, fused: &Path) {
    for arguments in [
        ["-map", "0:v:0", "-f", "framemd5", "-"],
        ["-map", "0:a:0", "-f", "s16le", "-"],
    ] {
        assert_eq!(
            decode_working_stream(fused, arguments),
            decode_working_stream(all, arguments)
        );
    }
}

#[test]
fn fused_materialization_preserves_exact_video_and_audio_with_one_region() {
    if !common::media_tools_available() {
        eprintln!("skipping fusion test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n2 2\n255\n255 0 0  0 255 0\n0 0 255  255 255 0\n",
    )
    .expect("image");
    let tone = directory.path().join("tone.wav");
    let tone_status = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
        .arg("sine=frequency=440:sample_rate=48000:duration=1")
        .args(["-c:a", "pcm_s16le"])
        .arg(&tone)
        .status()
        .expect("create tone fixture");
    assert!(tone_status.success());
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 }\noutput = \"final.mp4\" }\nimage(\"card.ppm\", 1s, stretch)\nzoom_in(20%)\naudio(\"tone.wav\")\ntrim<Audio>(100ms..900ms)\nset_audio\n",
    )
    .expect("workflow");
    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert!(plan.nodes().len() > 1);

    let all_cache = directory.path().join("all-cache");
    let all = render_to_cache(&plan, &all_cache, render::MaterializationMode::All);
    assert_eq!(all.rendered_jobs(), plan.nodes().len());

    let fused_cache = directory.path().join("fused-cache");
    let fused = render_to_cache(&plan, &fused_cache, render::MaterializationMode::Fused);
    assert_eq!(fused.rendered_jobs(), 1);
    assert_eq!(
        fused.materialization_mode(),
        render::MaterializationMode::Fused
    );

    assert_working_artifacts_equal(
        &endpoint_cache_artifact(&all_cache, &plan),
        &endpoint_cache_artifact(&fused_cache, &plan),
    );

    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(fused.manifest()).expect("manifest"))
            .expect("manifest JSON");
    assert_eq!(manifest["execution"]["materialization"], "fused");
}

#[test]
fn fused_materialization_preserves_adaptations_concat_and_video_slice() {
    if !common::media_tools_available() {
        eprintln!("skipping fusion adaptation test because media tools are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("image");
    for (name, frequency) in [("first.wav", 440), ("second.wav", 660)] {
        let status = Command::new("ffmpeg")
            .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
            .arg(format!(
                "sine=frequency={frequency}:sample_rate=48000:duration=1"
            ))
            .args(["-c:a", "pcm_s16le"])
            .arg(directory.path().join(name))
            .status()
            .expect("create tone fixture");
        assert!(status.success());
    }
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 }\noutput = \"final.mp4\" }\nset_audio(\n  video={\n    image(\"card.ppm\", 1s, stretch)\n    trim(100ms..900ms)\n  },\n  audio=zoom_in(\n    video={\n      audio(\"first.wav\")\n      trim<Audio>(0ms..400ms)\n      audio(\"second.wav\")\n      trim<Audio>(0ms..400ms)\n      concat<Audio>\n    },\n    by=10%,\n  ),\n)\n",
    )
    .expect("workflow");
    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert!(plan.nodes().iter().any(|node| matches!(
        node.video_kind(),
        Some(preflight::PreparedVideoKind::AudioOnBlack { .. })
    )));
    assert!(plan.nodes().iter().any(|node| matches!(
        node.audio_kind(),
        Some(preflight::PreparedAudioKind::ExtractAudio { .. })
    )));
    assert!(plan.nodes().iter().any(|node| matches!(
        node.audio_kind(),
        Some(preflight::PreparedAudioKind::AudioConcat { .. })
    )));

    let all_cache = directory.path().join("all-cache");
    let all = render_to_cache(&plan, &all_cache, render::MaterializationMode::All);
    assert_eq!(all.rendered_jobs(), plan.nodes().len());
    let fused_cache = directory.path().join("fused-cache");
    let fused = render_to_cache(&plan, &fused_cache, render::MaterializationMode::Fused);
    assert_eq!(fused.rendered_jobs(), 3);
    assert_working_artifacts_equal(
        &endpoint_cache_artifact(&all_cache, &plan),
        &endpoint_cache_artifact(&fused_cache, &plan),
    );
}

#[test]
fn fused_materialization_keeps_sequential_fan_out_bounded() {
    if !common::media_tools_available() {
        eprintln!("skipping fused fan-out test because media tools are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n2 2\n255\n255 0 0  0 255 0\n0 0 255  255 255 0\n",
    )
    .expect("image");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 }\noutput = \"final.mp4\" }\nclip { image(\"card.ppm\", 500ms, stretch) } as card\nzoom_in(video=$card, by=10%) as first\ndrop<Video>\nzoom_in(video=$card, by=20%) as second\ndrop<Video>\n{\n  $first\n  $second\n  concat\n}\n",
    )
    .expect("workflow");
    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert_eq!(plan.nodes().len(), 4);

    let all_cache = directory.path().join("all-cache");
    let all = render_to_cache(&plan, &all_cache, render::MaterializationMode::All);
    assert_eq!(all.rendered_jobs(), 4);
    let fused_cache = directory.path().join("fused-cache");
    let fused = render_to_cache(&plan, &fused_cache, render::MaterializationMode::Fused);
    assert_eq!(fused.rendered_jobs(), 4);
    assert_working_artifacts_equal(
        &endpoint_cache_artifact(&all_cache, &plan),
        &endpoint_cache_artifact(&fused_cache, &plan),
    );
}

#[test]
fn fused_materialization_combines_stream_disjoint_video_fan_out() {
    if !common::media_tools_available() {
        eprintln!("skipping mixed-stream fan-out test because media tools are unavailable");
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
            "testsrc2=s=64x48:r=10:d=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=1",
            "-c:v",
            "ffv1",
            "-pix_fmt",
            "yuv444p",
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
            "-colorspace",
            "bt709",
            "-color_range",
            "tv",
            "-c:a",
            "pcm_s16le",
            "-shortest",
        ])
        .arg(&source)
        .status()
        .expect("create audiovisual fixture");
    assert!(status.success());
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 }\noutput = \"final.mp4\" }\nvideo(\"source.mkv\", stretch) as source\ndrop<Video>\nextract_audio(video=$source) as sound\ndrop<Audio>\nset_audio(audio=$sound, video=$source)\n",
    )
    .expect("workflow");
    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert_eq!(plan.nodes().len(), 3);

    let all_cache = directory.path().join("all-cache");
    let all = render_to_cache(&plan, &all_cache, render::MaterializationMode::All);
    assert_eq!(all.rendered_jobs(), 3);
    let fused_cache = directory.path().join("fused-cache");
    let fused = render_to_cache(&plan, &fused_cache, render::MaterializationMode::Fused);
    assert_eq!(fused.rendered_jobs(), 1);
    assert_working_artifacts_equal(
        &endpoint_cache_artifact(&all_cache, &plan),
        &endpoint_cache_artifact(&fused_cache, &plan),
    );
}

#[test]
fn fused_materialization_preserves_chained_video_crossfades_and_attached_audio() {
    if !common::media_tools_available() {
        eprintln!("skipping fused crossfade test because media tools are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("before.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("before image");
    fs::write(
        directory.path().join("after.ppm"),
        b"P3\n1 1\n255\n0 0 255\n",
    )
    .expect("after image");
    fs::write(
        directory.path().join("third.ppm"),
        b"P3\n1 1\n255\n0 255 0\n",
    )
    .expect("third image");
    for (name, frequency) in [("before.wav", 440), ("after.wav", 660)] {
        let status = Command::new("ffmpeg")
            .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
            .arg(format!(
                "sine=frequency={frequency}:sample_rate=48000:duration=1"
            ))
            .args(["-c:a", "pcm_s16le"])
            .arg(directory.path().join(name))
            .status()
            .expect("create tone fixture");
        assert!(status.success());
    }
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 }\noutput = \"final.mp4\" }\nimage(\"before.ppm\", 1s, stretch)\naudio(\"before.wav\")\nset_audio\nimage(\"after.ppm\", 1s, stretch)\naudio(\"after.wav\")\nset_audio\ncrossfade(400ms)\nimage(\"third.ppm\", 1s, stretch)\ncrossfade(400ms)\nzoom_in(10%)\n",
    )
    .expect("workflow");
    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert!(plan.nodes().iter().any(|node| matches!(
        node.video_kind(),
        Some(preflight::PreparedVideoKind::Crossfade { .. })
    )));

    let all_cache = directory.path().join("all-cache");
    let all = render_to_cache(&plan, &all_cache, render::MaterializationMode::All);
    assert_eq!(all.rendered_jobs(), plan.nodes().len());
    let fused_cache = directory.path().join("fused-cache");
    let fused = render_to_cache(&plan, &fused_cache, render::MaterializationMode::Fused);
    assert_eq!(fused.rendered_jobs(), 5);
    assert_working_artifacts_equal(
        &endpoint_cache_artifact(&all_cache, &plan),
        &endpoint_cache_artifact(&fused_cache, &plan),
    );
}

#[test]
fn fused_materialization_preserves_input_scoped_video_repeat() {
    if !common::media_tools_available() {
        eprintln!("skipping fused Video repeat test because media tools are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n2 2\n255\n255 0 0  0 255 0\n0 0 255  255 255 0\n",
    )
    .expect("image");
    let tone = directory.path().join("tone.wav");
    let status = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
        .arg("sine=frequency=440:sample_rate=48000:duration=1")
        .args(["-c:a", "pcm_s16le"])
        .arg(&tone)
        .status()
        .expect("create tone fixture");
    assert!(status.success());
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 7 }\noutput = \"final.mp4\" }\nimage(\"card.ppm\", 3f, stretch)\naudio(\"tone.wav\")\nset_audio\nrepeat(3)\nzoom_in(10%)\n",
    )
    .expect("workflow");
    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert_eq!(plan.nodes().len(), 5);

    let all_cache = directory.path().join("all-cache");
    let all = render_to_cache(&plan, &all_cache, render::MaterializationMode::All);
    assert_eq!(all.rendered_jobs(), plan.nodes().len());
    let fused_cache = directory.path().join("fused-cache");
    let fused = render_to_cache(&plan, &fused_cache, render::MaterializationMode::Fused);
    assert_eq!(fused.rendered_jobs(), 2);
    assert_working_artifacts_equal(
        &endpoint_cache_artifact(&all_cache, &plan),
        &endpoint_cache_artifact(&fused_cache, &plan),
    );
}

#[test]
fn fused_materialization_stops_at_a_verified_cache_frontier() {
    if !common::media_tools_available() {
        eprintln!("skipping fusion cache-frontier test because media tools are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    let image = directory.path().join("card.ppm");
    fs::write(&image, b"P3\n1 1\n255\n255 0 0\n").expect("image");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"final.mp4\" }\nimage(\"card.ppm\", 1s, stretch)\nzoom_in(10%)\nzoom_in(10%)\n",
    )
    .expect("workflow");
    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert_eq!(plan.nodes().len(), 3);
    render::render(&plan).expect("populate per-node cache");

    let result = common::cache_artifact(
        directory.path(),
        plan.nodes()[plan.result().get() as usize].fingerprint(),
        "mkv",
    );
    fs::remove_file(&result).expect("remove result artifact");
    fs::remove_file(common::cache_metadata(&result)).expect("remove result metadata");
    fs::write(&image, b"P3\n1 1\n255\n0 0 255\n").expect("change pruned source");

    let report = render::render_with_options(
        &plan,
        &render::RenderOptions::new(
            render::CacheMode::Persistent,
            render::MaterializationMode::Fused,
        ),
    )
    .expect("render from the verified intermediate frontier");
    assert_eq!(report.reused_artifacts(), 1);
    assert_eq!(report.rendered_jobs(), 1);
}

#[test]
fn fused_materialization_rechecks_resources_inside_the_region() {
    if !common::media_tools_available() {
        eprintln!("skipping fusion resource test because media tools are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("image");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"final.mp4\" }\nimage(\"card.ppm\", 1s, stretch)\nzoom_in(10%)\n",
    )
    .expect("workflow");
    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert_eq!(plan.nodes().len(), 2);
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n1 1\n255\n0 0 255\n",
    )
    .expect("change fused source after preflight");

    let error = render::render_with_options(
        &plan,
        &render::RenderOptions::new(render::CacheMode::None, render::MaterializationMode::Fused),
    )
    .expect_err("changed source inside a fused region");
    assert_eq!(error.code, "E_ASSET_CHANGED");
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
    assert!(first.output().is_file());
    assert_ne!(
        fs::read(first.output()).expect("new output"),
        b"previous valid destination"
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(first.manifest()).expect("new manifest"))
            .expect("manifest JSON");
    assert_eq!(manifest["format_version"], 4);
    assert_eq!(manifest["project"]["video"]["fps"]["numerator"], 10);
    assert_eq!(manifest["project"]["video"]["color"]["transfer"], "bt709");
    assert_eq!(manifest["output_encoding"]["pixel_format"], "yuv420p");
    assert_eq!(manifest["output_encoding"]["chroma_location"], "left");
    assert_eq!(manifest["semantic_hash"], plan.semantic_hash());
    assert_eq!(manifest["cache"]["reused_artifacts"], 0);
    assert_eq!(manifest["execution"]["rendered_jobs"], plan.nodes().len());
    assert_eq!(manifest["cache"]["mode"], "persistent");
    assert_eq!(manifest["execution"]["materialization"], "all");
    assert!(manifest.get("plan").is_none());
    assert!(manifest.get("execution_namespace").is_none());
    assert_eq!(first.reused_artifacts(), 0);
    assert_eq!(first.rendered_jobs(), plan.nodes().len());
    let upstream = common::cache_artifact(directory.path(), plan.nodes()[0].fingerprint(), "mkv");
    fs::remove_file(&upstream).expect("remove upstream artifact");
    fs::remove_file(common::cache_metadata(&upstream)).expect("remove upstream metadata");
    fs::write(directory.path().join("card.ppm"), b"P3\n1 1\n255\n0 0 0\n")
        .expect("change authored image after preflight");
    let second = render::render(&plan).expect("cached render");
    assert_eq!(second.reused_artifacts(), 1);
    assert_eq!(second.rendered_jobs(), 0);
    assert!(!upstream.exists(), "pruned upstream cache was recreated");
    assert!(second.manifest().is_file());
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(second.manifest()).expect("cached manifest"))
            .expect("cached manifest JSON");
    assert_eq!(manifest["cache"]["reused_artifacts"], 1);
    assert_eq!(manifest["execution"]["rendered_jobs"], 0);
}

#[test]
fn cache_none_uses_temporary_artifacts_and_releases_shared_inputs_after_last_use() {
    if !common::media_tools_available() {
        eprintln!("skipping cache-none test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("image");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"final.mp4\" }\nclip {\n  image(\"card.ppm\", 100ms)\n} as card\n{\n  $card\n  $card\n  concat\n}\n",
    )
    .expect("workflow");
    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");

    let report = render::render_with_options(
        &plan,
        &render::RenderOptions::new(render::CacheMode::None, render::MaterializationMode::Fused),
    )
    .expect("uncached fused render");

    assert_eq!(report.cache_mode(), render::CacheMode::None);
    assert_eq!(
        report.materialization_mode(),
        render::MaterializationMode::Fused
    );
    assert_eq!(report.reused_artifacts(), 0);
    assert_eq!(report.rendered_jobs(), 2);
    assert!(report.output().is_file());
    assert!(!directory.path().join(".clipasm").exists());
    assert!(
        fs::read_dir(directory.path())
            .expect("render directory")
            .all(|entry| !entry
                .expect("render directory entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".final.mp4.render-")),
        "private render staging directory was not removed"
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(report.manifest()).expect("manifest"))
            .expect("manifest JSON");
    assert_eq!(manifest["format_version"], 4);
    assert_eq!(manifest["cache"]["mode"], "none");
    assert_eq!(manifest["cache"]["reused_artifacts"], 0);
    assert_eq!(manifest["execution"]["rendered_jobs"], plan.nodes().len());
    assert_eq!(manifest["execution"]["materialization"], "fused");
}

#[cfg(unix)]
#[test]
fn cache_paths_cannot_alias_prepared_resources() {
    use std::os::unix::fs::symlink;

    if !common::media_tools_available() {
        eprintln!("skipping cache collision test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let (directory, compiled) = color_project("255 0 0");
    let plan = preflight::preflight(&compiled).expect("preflight");
    let document: serde_json::Value =
        serde_json::from_str(&plan.prepared_json().expect("prepared JSON"))
            .expect("prepared document");
    let namespace = document["execution_namespace"]
        .as_str()
        .expect("execution namespace");
    let cache_root = directory.path().join("custom-cache");
    let cache_directory = cache_root.join(namespace);
    fs::create_dir_all(&cache_directory).expect("cache directory");
    let artifact = cache_directory.join(format!("{}.mkv", plan.nodes()[0].fingerprint()));
    let asset = directory.path().join("card.ppm");
    let original = fs::read(&asset).expect("asset bytes");
    symlink(&asset, &artifact).expect("cache path alias");

    let error = render::render_with_cache_root(&plan, &cache_root)
        .expect_err("cache path must not alias an asset");

    assert_eq!(error.code, "E_CACHE_IO");
    assert!(error.message.contains("collides with image asset"));
    assert_eq!(fs::read(asset).expect("preserved asset"), original);
}

#[cfg(unix)]
#[test]
fn cache_and_publication_lock_paths_cannot_alias_imported_sources() {
    use std::os::unix::fs::symlink;

    if !common::media_tools_available() {
        eprintln!("skipping source collision test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    let imported = directory.path().join("effect.clipasm");
    let imported_source = "clipasm 1\ninput video: Video\nzoom_in($video, 10%)\n";
    fs::write(&imported, imported_source).expect("imported source");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("image");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"final.mp4\" }\nimport \"effect.clipasm\" as effect\nimage(\"card.ppm\", 1s, stretch)\neffect\n",
    )
    .expect("workflow");
    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    let document: serde_json::Value =
        serde_json::from_str(&plan.prepared_json().expect("prepared JSON"))
            .expect("prepared document");
    let namespace = document["execution_namespace"]
        .as_str()
        .expect("execution namespace");
    let cache_root = directory.path().join("custom-cache");
    let cache_directory = cache_root.join(namespace);
    fs::create_dir_all(&cache_directory).expect("cache directory");
    let artifact = cache_directory.join(format!("{}.mkv", plan.nodes()[0].fingerprint()));
    symlink(&imported, &artifact).expect("cache artifact alias");

    let cache_error = render::render_with_cache_root(&plan, &cache_root)
        .expect_err("cache path must not alias an imported source");

    assert_eq!(cache_error.code, "E_CACHE_IO");
    assert!(cache_error.message.contains("source program"));
    assert_eq!(
        fs::read_to_string(&imported).expect("preserved imported source"),
        imported_source
    );

    fs::remove_file(&artifact).expect("remove cache alias");
    render::render_with_cache_root(&plan, &cache_root).expect("initial render");
    let publication_lock = directory.path().join(".final.mp4.publication.lock");
    fs::remove_file(&publication_lock).expect("remove regular publication lock");
    symlink(&imported, &publication_lock).expect("publication lock alias");

    let lock_error = render::render_with_cache_root(&plan, &cache_root)
        .expect_err("publication lock must not alias an imported source");

    assert_eq!(lock_error.code, "E_PUBLICATION_LOCK");
    assert!(lock_error.message.contains("source program"));
    assert_eq!(
        fs::read_to_string(imported).expect("preserved imported source"),
        imported_source
    );
}

#[test]
fn invalid_downstream_cache_expands_to_its_valid_input() {
    if !common::media_tools_available() {
        eprintln!("skipping cache frontier test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("image");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"final.mp4\" }\nimage(\"card.ppm\", 1s, stretch)\nrepeat(2)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert_eq!(plan.nodes().len(), 2);
    render::render(&plan).expect("initial render");
    let input = common::cache_artifact(directory.path(), plan.nodes()[0].fingerprint(), "mkv");
    let result = common::cache_artifact(directory.path(), plan.nodes()[1].fingerprint(), "mkv");
    let substituted = fs::read(&input).expect("input artifact");
    fs::write(&result, &substituted).expect("replace result with shorter input");

    let report = render::render(&plan).expect("repair invalid result cache");
    assert_eq!(report.reused_artifacts(), 1);
    assert_eq!(report.rendered_jobs(), 1);
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
    assert_eq!(report.reused_artifacts(), 0);
    assert_eq!(report.rendered_jobs(), 1);
    let decoded = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(report.output())
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .expect("decode output");
    assert!(decoded.status.success());
    assert!(decoded.stdout[0] > 200, "expected red output");
    assert!(decoded.stdout[2] < 50, "unexpected blue substitution");
}

#[test]
fn image_paths_are_literal_for_rendering_and_cache_identity() {
    if !common::media_tools_available() {
        eprintln!("skipping literal image path test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("frame-%d.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("literal image");
    fs::write(
        directory.path().join("frame-0.ppm"),
        b"P3\n1 1\n255\n0 0 255\n",
    )
    .expect("pattern neighbor");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"final.mp4\" }\nimage(\"frame-%d.ppm\", 1s, stretch)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight literal image");
    let first = render::render(&plan).expect("render literal image");
    assert_eq!(first.reused_artifacts(), 0);
    assert_eq!(first.rendered_jobs(), 1);
    let decode = |path: &Path| {
        let output = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(path)
            .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .output()
            .expect("decode output");
        assert!(
            output.status.success(),
            "decode failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    };
    let first_frame = decode(first.output());
    assert!(first_frame[0] > 200, "literal red image was not decoded");
    assert!(
        first_frame[2] < 50,
        "pattern-expanded blue image was decoded"
    );

    fs::write(
        directory.path().join("frame-0.ppm"),
        b"P3\n1 1\n255\n0 255 0\n",
    )
    .expect("change pattern neighbor");
    let second = render::render(&plan).expect("reuse literal image cache");
    assert_eq!(second.reused_artifacts(), 1);
    assert_eq!(second.rendered_jobs(), 0);
    let second_frame = decode(second.output());
    assert!(
        second_frame[0] > 200,
        "cached output stopped using literal image"
    );
    assert!(
        second_frame[1] < 50,
        "pattern neighbor changed cached output"
    );
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
    assert!(report.output().is_file());
    assert_eq!(report.rendered_jobs(), plan.nodes().len());
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the end-to-end source test keeps normalization, audio, and cache assertions together"
)]
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
            "testsrc2=size=96x48:rate=12:duration=2",
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
            "-pix_fmt",
            "yuv444p",
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
            "-colorspace",
            "bt709",
            "-color_range",
            "tv",
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
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"video-source.mp4\" }\n{\n  image(\"card.ppm\", 1s)\n  video(\"source.mkv\", contain)\n  concat\n}\n",
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
    let all_cache = directory.path().join("all-cache");
    let all = render_to_cache(&plan, &all_cache, render::MaterializationMode::All);
    assert_eq!(all.rendered_jobs(), plan.nodes().len());
    let fused_cache = directory.path().join("fused-cache");
    let first = render_to_cache(&plan, &fused_cache, render::MaterializationMode::Fused);
    assert_eq!(first.reused_artifacts(), 0);
    assert_eq!(first.rendered_jobs(), 3);
    assert_working_artifacts_equal(
        &endpoint_cache_artifact(&all_cache, &plan),
        &endpoint_cache_artifact(&fused_cache, &plan),
    );
    assert!(first.output().is_file());
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "json",
        ])
        .arg(first.output())
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
    let second = render_to_cache(&plan, &fused_cache, render::MaterializationMode::Fused);
    assert_eq!(second.reused_artifacts(), 1);
    assert_eq!(second.rendered_jobs(), 0);
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
            "-pix_fmt",
            "yuv444p",
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
            "-colorspace",
            "bt709",
            "-color_range",
            "tv",
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
    assert!(report.output().is_file());
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
            "-pix_fmt",
            "yuv444p",
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
            "-colorspace",
            "bt709",
            "-color_range",
            "tv",
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
        .arg(report.output())
        .output()
        .expect("probe output");
    assert!(probe.status.success());
    assert_eq!(String::from_utf8_lossy(&probe.stdout).trim(), "1");
}

#[test]
fn zoom_renders_exact_frames_and_dimensions_including_one_frame() {
    if !common::media_tools_available() {
        eprintln!("skipping zoom_in render test because FFmpeg/FFprobe are unavailable");
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
                "clipasm 1\nconfig {{ video {{ width = 64\nheight = 48\nfps = 10 }}\noutput = \"zoom_in.mp4\" }}\nimage(\"card.ppm\", {duration}, stretch)\nzoom_in(20%)\n"
            ),
        )
        .expect("workflow");

        let compiled = compile_file(&workflow).expect("compile");
        let plan = preflight::preflight(&compiled).expect("preflight");
        let report = render::render(&plan).expect("render zoom_in");
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
            .arg(report.output())
            .output()
            .expect("probe zoom_in");
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
        eprintln!("skipping zoom_in centering test because FFmpeg/FFprobe are unavailable");
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
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 }\noutput = \"zoom_in.mp4\" }\nimage(\"center.ppm\", 1s, stretch)\nzoom_in(100%)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    let report = render::render(&plan).expect("render zoom_in");
    let decoded = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(report.output())
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .expect("decode zoom_in");
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
fn flash_cut_renders_an_exact_join_with_a_white_to_normal_after_cut() {
    const FRAME_BYTES: usize = 64 * 48 * 3;

    if !common::media_tools_available() {
        eprintln!("skipping flash_cut render test because FFmpeg/FFprobe are unavailable");
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
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 }\noutput = \"flash_cut.mp4\" }\nimage(\"before.ppm\", 1s, stretch)\nimage(\"after.ppm\", 1s, stretch)\njoin { flash_cut(400ms) }\n",
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
    let all_cache = directory.path().join("all-cache");
    render_to_cache(&plan, &all_cache, render::MaterializationMode::All);
    let fused_cache = directory.path().join("fused-cache");
    let report = render_to_cache(&plan, &fused_cache, render::MaterializationMode::Fused);
    assert_eq!(report.rendered_jobs(), 3);
    assert_working_artifacts_equal(
        &endpoint_cache_artifact(&all_cache, &plan),
        &endpoint_cache_artifact(&fused_cache, &plan),
    );
    let decoded = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(report.output())
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .expect("decode flash_cut");
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
    let transition_last = brightness(13);
    let post_transition = brightness(14);
    let normal_after = brightness(19);
    assert!(before < 15, "before-cut frame was not black: {before}");
    assert!(
        first_after > 225,
        "first post-cut frame was not white: {first_after}"
    );
    assert!(
        transition_last + 30 < first_after,
        "flash_cut did not visibly clear: first={first_after}, end={transition_last}"
    );
    assert!(
        transition_last > normal_after,
        "the last linear-light fade frame should remain brighter than normal: end={transition_last}, normal={normal_after}"
    );
    assert!(
        post_transition.abs_diff(normal_after) < 10,
        "the frame after the fade did not return to normal: after={post_transition}, normal={normal_after}"
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
            .arg(report.output())
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
    let all_cache = directory.path().join("all-cache");
    let all = render_to_cache(&plan, &all_cache, render::MaterializationMode::All);
    assert!(all.rendered_jobs() > 2);
    let fused_cache = directory.path().join("fused-cache");
    let fused = render_to_cache(&plan, &fused_cache, render::MaterializationMode::Fused);
    assert_eq!(fused.rendered_jobs(), 4);
    assert_working_artifacts_equal(
        &endpoint_cache_artifact(&all_cache, &plan),
        &endpoint_cache_artifact(&fused_cache, &plan),
    );
}

#[test]
fn renders_audio_during_through_existing_audio_primitives() {
    if !common::media_tools_available() {
        eprintln!("skipping Audio during render test because FFmpeg/FFprobe are unavailable");
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
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"audio-during.mp4\" }\nimage(\"card.ppm\", 2s)\naudio(\"tone.wav\")\nduring<Audio>(200ms..400ms) { repeat(2) }\nset_audio\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile Audio during");
    let plan = preflight::preflight(&compiled).expect("preflight Audio during");
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
    let report = render::render(&plan).expect("render Audio during");
    assert!(report.output().is_file());
}

#[cfg(target_os = "linux")]
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

    assert_eq!(report.output(), output);
    assert!(report.output().is_file());
    assert!(report.manifest().is_file());
    let document: serde_json::Value =
        serde_json::from_slice(&fs::read(report.manifest()).expect("manifest"))
            .expect("manifest JSON");
    assert_eq!(document["format_version"], 4);
    assert!(document.get("plan").is_none());
    output_name.push(".manifest.json");
    assert_eq!(report.manifest(), directory.path().join(output_name));
}

#[cfg(target_os = "linux")]
#[test]
fn renders_a_native_video_input_with_a_non_utf8_path() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;

    if !common::media_tools_available() {
        eprintln!("skipping non-UTF input test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory
        .path()
        .join(OsString::from_vec(b"source-\xFF.mkv".to_vec()));
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x64:r=10:d=0.2",
            "-c:v",
            "ffv1",
            "-pix_fmt",
            "yuv444p",
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
            "-colorspace",
            "bt709",
            "-color_range",
            "tv",
        ])
        .arg(&source)
        .status()
        .expect("create non-UTF source video");
    assert!(status.success());
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"final.mp4\" }\ninput source: Video\n$source\n",
    )
    .expect("workflow");
    let package = clipasm::language::parse_file(&workflow).expect("parse");
    let mut bindings = compiler::EntrypointBindings::new();
    bindings
        .bind_video_input(
            "source",
            source,
            clipasm::source::SourceSpan::file_start("<test>"),
        )
        .expect("video binding");

    let compiled = compiler::compile_with_bindings(&package, &bindings).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert_eq!(
        plan.prepared_json()
            .expect_err("prepared inspection requires Unicode paths")
            .code,
        "E_PREPARED_JSON"
    );
    let report = render::render(&plan).expect("render non-UTF input");

    assert!(report.output().is_file());
    assert_eq!(report.rendered_jobs(), plan.nodes().len());
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
assert r["protocol_version"] == 3
assert r["parameters"]["amount"] in (7, 8)
lut = pathlib.Path(r["parameters"]["lut"])
assert lut.read_bytes() == b"original lookup"
with lut.with_name("amounts.log").open("a") as log:
    print(r["parameters"]["amount"], file=log)
subprocess.run([r["tools"]["ffmpeg"], "-y", "-v", "error", "-i", r["inputs"]["video"]["path"], "-map", "0:v:0", "-map", "0:a:0", "-c", "copy", r["output"]["path"]], check=True)
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
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"result.mp4\" }\nimport \"effect.clipasm\" as effect\nclip { image(\"card.ppm\", 500ms) } as card\n{\n  $card\n  effect(7)\n  $card\n  effect(8)\n  concat\n}\nzoom_in(10%)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile external program");
    let plan = preflight::preflight(&compiled).expect("preflight external program");
    let report = render::render_with_options(
        &plan,
        &render::RenderOptions::new(
            render::CacheMode::Persistent,
            render::MaterializationMode::Fused,
        ),
    )
    .expect("render external program with fusion boundaries");
    assert!(report.output().is_file());
    assert_eq!(plan.nodes().len(), 5);
    assert_eq!(report.rendered_jobs(), 4);
    let mut amounts = fs::read_to_string(directory.path().join("amounts.log"))
        .expect("external invocation log")
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    amounts.sort();
    assert_eq!(amounts, ["7", "8"]);
}

#[cfg(unix)]
#[test]
fn cached_downstream_node_prunes_a_changed_external_executable() {
    use std::os::unix::fs::PermissionsExt as _;

    if !common::media_tools_available() || !common::executable_available("python3", "--version") {
        eprintln!("skipping external cache frontier test because a required tool is unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("image");
    let executable = directory.path().join("effect.py");
    fs::write(
        &executable,
        r#"#!/usr/bin/env python3
import json, subprocess, sys
r = json.load(sys.stdin)
subprocess.run([r["tools"]["ffmpeg"], "-y", "-v", "error", "-i", r["inputs"]["video"]["path"], "-map", "0:v:0", "-map", "0:a:0", "-c", "copy", r["output"]["path"]], check=True)
"#,
    )
    .expect("external executable");
    let mut permissions = fs::metadata(&executable)
        .expect("external metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).expect("executable permissions");
    fs::write(
        directory.path().join("effect.clipasm"),
        "clipasm 1\ninput video: Video\nexternal {\n  executable = \"./effect.py\"\n  arguments = []\n  semantic_version = 1\n  preserve = video\n}\n",
    )
    .expect("external program");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"result.mp4\" }\nimport \"effect.clipasm\" as effect\nimage(\"card.ppm\", 1s, stretch)\neffect\nzoom_in(10%)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
    let plan = preflight::preflight(&compiled).expect("preflight");
    assert_eq!(plan.nodes().len(), 3);
    let first = render::render(&plan).expect("initial render");
    assert_eq!(first.rendered_jobs(), 3);

    fs::write(&executable, "#!/bin/sh\nexit 1\n").expect("change external executable");
    let cached = render::render(&plan).expect("cached downstream prunes external executable");
    assert_eq!(cached.reused_artifacts(), 1);
    assert_eq!(cached.rendered_jobs(), 0);

    let result = common::cache_artifact(
        directory.path(),
        plan.nodes()[plan.result().get() as usize].fingerprint(),
        "mkv",
    );
    fs::remove_file(&result).expect("remove downstream cache artifact");
    fs::remove_file(common::cache_metadata(&result)).expect("remove downstream cache metadata");
    let error = render::render(&plan).expect_err("reached changed external executable");
    assert_eq!(error.code, "E_EXTERNAL_CHANGED");
}
