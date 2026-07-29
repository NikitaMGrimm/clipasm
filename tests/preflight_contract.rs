#![allow(missing_docs)]

mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use clipasm::preflight::{PreparedAudioKind, PreparedVideoKind};

fn compile_file(path: &Path) -> clipasm::diagnostic::Result<clipasm::compiler::CompiledProgram> {
    let source = clipasm::language::parse_file(path)?;
    clipasm::compiler::compile(&source)
}

fn write_image(directory: &Path, name: &str, color: &str) {
    fs::write(directory.join(name), format!("P3\n1 1\n255\n{color}\n"))
        .expect("write image fixture");
}

fn write_pcm_wav(directory: &Path, name: &str, sample_rate: u32, samples: u32) {
    let channels = 2_u16;
    let bits_per_sample = 16_u16;
    let block_align = channels * (bits_per_sample / 8);
    let byte_rate = sample_rate * u32::from(block_align);
    let data_size = samples * u32::from(block_align);
    let mut bytes = Vec::with_capacity(44 + data_size as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&byte_rate.to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&bits_per_sample.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());
    bytes.resize(44 + data_size as usize, 0);
    fs::write(directory.join(name), bytes).expect("write PCM WAV fixture");
}

#[test]
fn prepared_json_serializes_one_distinguished_result() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let source = directory.path().join("program.clipasm");
    fs::write(
        &source,
        "clipasm 1\nconfig { output = \"final.mp4\" }\nimage(\"card.ppm\", 1s)\n",
    )
    .expect("source program");

    let compiled = compile_file(&source).expect("compile");
    let plan = clipasm::preflight::preflight(&compiled).expect("preflight");
    let document: serde_json::Value =
        serde_json::from_str(&plan.prepared_json().expect("prepared JSON"))
            .expect("prepared document");

    assert!(document.get("result").is_some());
    assert_eq!(document["format_version"], 13);
    assert_eq!(document["semantic_hash"], plan.semantic_hash());
    assert!(document["output"].is_string());
    assert!(document["manifest"].is_string());
    assert!(document["ffmpeg"]["executable"].is_string());
    assert!(document["ffmpeg"]["build_fingerprint"].is_string());
    assert!(document["ffprobe"]["executable"].is_string());
    assert!(document["execution_namespace"].is_string());
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
    let source = directory.path().join("program.clipasm");
    fs::write(
        &source,
        "clipasm 1\nconfig { output = \"final.mp4\" }\nimage(\"card.ppm\", 1s) as picture\ndrop<Video>\nextract_audio($picture) as sound\ndrop<Audio>\nset_audio(video=$picture, audio=$sound)\n",
    )
    .expect("source program");

    let compiled = compile_file(&source).expect("compile");
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

    let document: serde_json::Value =
        serde_json::from_str(&plan.prepared_json().expect("prepared JSON"))
            .expect("prepared document");
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
fn video_working_identity_includes_its_physical_audio_domain() {
    if !common::media_tools_available() {
        eprintln!("skipping working identity test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let default_source = directory.path().join("default.clipasm");
    let changed_source = directory.path().join("changed.clipasm");
    fs::write(
        &default_source,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"default.mp4\" }\nimage(\"card.ppm\", 1s)\n",
    )
    .expect("default source");
    fs::write(
        &changed_source,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\naudio { sample_rate = 44100 }\noutput = \"changed.mp4\" }\nimage(\"card.ppm\", 1s)\n",
    )
    .expect("changed source");

    let default_plan = clipasm::preflight::preflight(
        &compile_file(&default_source).expect("compile default source"),
    )
    .expect("default preflight");
    let changed_plan = clipasm::preflight::preflight(
        &compile_file(&changed_source).expect("compile changed source"),
    )
    .expect("changed preflight");

    assert_ne!(
        default_plan.nodes()[default_plan.result().get() as usize].fingerprint(),
        changed_plan.nodes()[changed_plan.result().get() as usize].fingerprint(),
        "working Video bytes depend on the project audio format"
    );
    let default_json: serde_json::Value =
        serde_json::from_str(&default_plan.prepared_json().expect("default JSON")).expect("JSON");
    let changed_json: serde_json::Value =
        serde_json::from_str(&changed_plan.prepared_json().expect("changed JSON")).expect("JSON");
    assert_eq!(
        default_json["execution_namespace"], changed_json["execution_namespace"],
        "project media belongs to per-node artifact identity, not tool/policy namespace"
    );
}

#[test]
fn unreachable_auxiliary_audio_is_not_preflighted() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let source = directory.path().join("program.clipasm");
    fs::write(
        &source,
        "clipasm 1\nconfig { output = \"final.mp4\" }\naudio(\"missing.wav\")\nimage(\"card.ppm\", 1s)\n",
    )
    .expect("source program");

    let compiled = compile_file(&source).expect("compile");
    let plan = clipasm::preflight::preflight(&compiled).expect("unique Video reachability");
    assert_eq!(plan.nodes().len(), 1);
    assert!(matches!(
        plan.nodes()[0].video_kind(),
        Some(PreparedVideoKind::ImageVideo { .. })
    ));
}

#[test]
fn audio_preflight_maps_source_timelines_to_project_samples() {
    if !common::media_tools_available() {
        eprintln!("skipping audio timeline test because FFmpeg/FFprobe are unavailable");
        return;
    }
    for (name, sample_rate, codec, extension) in [
        ("pcm-44100", 44_100_u32, "pcm_s16le", "wav"),
        ("pcm-48000", 48_000_u32, "pcm_s16le", "wav"),
        ("pcm-96000", 96_000_u32, "pcm_s16le", "wav"),
        ("aac-48000", 48_000_u32, "aac", "m4a"),
    ] {
        let directory = tempfile::tempdir().expect("temporary directory");
        write_image(directory.path(), "card.ppm", "255 0 0");
        let audio = directory.path().join(format!("{name}.{extension}"));
        let source = format!("anullsrc=r={sample_rate}:cl=stereo:d=1");
        let status = Command::new("ffmpeg")
            .args([
                "-y", "-v", "error", "-f", "lavfi", "-i", &source, "-c:a", codec,
            ])
            .arg(&audio)
            .status()
            .expect("create audio fixture");
        assert!(status.success(), "failed fixture {name}");

        let workflow = directory.path().join("program.clipasm");
        fs::write(
            &workflow,
            format!(
                "clipasm 1\nconfig {{ output = \"final.mp4\" }}\nimage(\"card.ppm\", 1s)\naudio(\"{}\")\nset_audio\n",
                audio.file_name().expect("audio name").to_string_lossy()
            ),
        )
        .expect("source program");

        let compiled = compile_file(&workflow).expect("compile");
        let plan = clipasm::preflight::preflight(&compiled).expect("preflight");
        let prepared_audio = plan
            .nodes()
            .iter()
            .find(|node| {
                matches!(
                    node.audio_kind(),
                    Some(PreparedAudioKind::AudioSource { .. })
                )
            })
            .expect("prepared audio source");
        assert_eq!(
            prepared_audio.audio_domain().expect("Audio node").samples(),
            48_000,
            "wrong project duration for {name}"
        );
        clipasm::render::render(&plan).expect("render normalized audio");
    }
}

#[test]
fn preflight_resolves_audio_markers_to_exact_project_sample_ranges() {
    if !common::media_tools_available() {
        eprintln!("skipping Audio marker test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    write_pcm_wav(directory.path(), "first.wav", 48_000, 480);
    write_pcm_wav(directory.path(), "second.wav", 48_000, 960);
    let workflow = directory.path().join("program.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { output = \"final.mp4\" }\nimage(\"card.ppm\", 1s) as picture\ndrop<Video>\naudio(\"first.wav\") as first\naudio(\"second.wav\") as second\njoin as mix\ntrim(value=$mix, range=$mix::second) as selected\nset_audio(video=$picture, audio=$selected)\n",
    )
    .expect("source program");

    let compiled = compile_file(&workflow).expect("compile");
    let plan = clipasm::preflight::preflight(&compiled).expect("preflight Audio marker");
    let range = plan
        .nodes()
        .iter()
        .find_map(|node| match node.audio_kind() {
            Some(PreparedAudioKind::AudioSlice { range, .. }) => Some(*range),
            _ => None,
        })
        .expect("resolved Audio marker slice");
    assert_eq!(range.start(), 480);
    assert_eq!(range.end(), 1_440);
    assert_eq!(range.samples(), 960);
}

#[test]
fn preflight_resolves_audio_during_and_shifted_placements_to_exact_samples() {
    if !common::media_tools_available() {
        eprintln!("skipping Audio during test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    write_pcm_wav(directory.path(), "first.wav", 48_000, 480);
    write_pcm_wav(directory.path(), "second.wav", 48_000, 960);
    write_pcm_wav(directory.path(), "third.wav", 48_000, 480);
    let workflow = directory.path().join("program.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { output = \"final.mp4\" }\nimage(\"card.ppm\", 1s) as picture\ndrop<Video>\naudio(\"first.wav\") as intro\naudio(\"second.wav\") as section\naudio(\"third.wav\") as outro\nconcat as song\nduring(timeline=$song, range=$song::section) { repeat(2) } as revised\ntrim(value=$revised, range=$revised::outro) as selected\nset_audio(video=$picture, audio=$selected)\n",
    )
    .expect("source program");

    let compiled = compile_file(&workflow).expect("compile deferred Audio during");
    let plan = clipasm::preflight::preflight(&compiled).expect("preflight Audio during");
    let shifted = plan
        .nodes()
        .iter()
        .find_map(|node| match node.audio_kind() {
            Some(PreparedAudioKind::AudioSlice { range, .. })
                if range.start() == 2_400 && range.end() == 2_880 =>
            {
                Some(*range)
            }
            _ => None,
        })
        .expect("shifted outro slice");
    assert_eq!(shifted.samples(), 480);
}

#[test]
fn preflight_rejects_audio_during_outside_the_base_domain() {
    if !common::media_tools_available() {
        eprintln!("skipping Audio during bounds test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    write_pcm_wav(directory.path(), "short.wav", 48_000, 480);
    let workflow = directory.path().join("program.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { output = \"final.mp4\" }\nimage(\"card.ppm\", 1s) as picture\ndrop<Video>\naudio(\"short.wav\")\nduring(0ms..20ms) { repeat(1) } as revised\nset_audio(video=$picture, audio=$revised)\n",
    )
    .expect("source program");

    let compiled = compile_file(&workflow).expect("compile Audio during bounds");
    let error = clipasm::preflight::preflight(&compiled).expect_err("Audio range must fit base");
    assert_eq!(error.code, "E_INVALID_TIME_RANGE");
    assert!(error.message.contains("960"));
    assert!(error.message.contains("480"));
}

#[test]
fn preflight_rejects_an_audio_marker_between_sample_boundaries() {
    if !common::media_tools_available() {
        eprintln!("skipping Audio marker alignment test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    write_pcm_wav(directory.path(), "odd.wav", 48_000, 1);
    let workflow = directory.path().join("program.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { output = \"final.mp4\" }\nimage(\"card.ppm\", 1s) as picture\ndrop<Video>\naudio(\"odd.wav\") as odd\ntrim(value=$odd, range=$odd::start..$odd::middle) as half\nset_audio(video=$picture, audio=$half)\n",
    )
    .expect("source program");

    let compiled = compile_file(&workflow).expect("compile deferred midpoint");
    let error = clipasm::preflight::preflight(&compiled).expect_err("half sample must fail");
    assert_eq!(error.code, "E_TIME_NOT_SAMPLE_ALIGNED");
}

#[test]
fn relocated_identical_projects_have_equal_semantic_hashes() {
    let first = tempfile::tempdir().expect("first directory");
    let second = tempfile::tempdir().expect("second directory");
    for directory in [first.path(), second.path()] {
        write_image(directory, "card.ppm", "255 0 0");
        fs::write(
            directory.join("workflow.clipasm"),
            "clipasm 1\nconfig { output = \"final.mp4\" }\nimage(\"card.ppm\", 1s)\n",
        )
        .expect("workflow");
    }

    let first_compiled = compile_file(&first.path().join("workflow.clipasm")).expect("compile");
    let second_compiled = compile_file(&second.path().join("workflow.clipasm")).expect("compile");
    let first_prepared = clipasm::preflight::preflight(&first_compiled).expect("preflight");
    let second_prepared = clipasm::preflight::preflight(&second_compiled).expect("preflight");
    assert_eq!(
        first_prepared.semantic_hash(),
        second_prepared.semantic_hash()
    );
}

#[test]
fn relocated_external_file_parameters_have_equal_semantic_hashes() {
    if !common::media_tools_available() {
        eprintln!("skipping external relocation test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let first = tempfile::tempdir().expect("first directory");
    let second = tempfile::tempdir().expect("second directory");
    for directory in [first.path(), second.path()] {
        write_image(directory, "card.ppm", "255 0 0");
        fs::write(directory.join("lut.bin"), b"identical lookup table").expect("file parameter");
        fs::write(
            directory.join("effect.clipasm"),
            "clipasm 1\ninput video: Video\nparam lut: File = \"lut.bin\"\nexternal {\n  executable = \"ffmpeg\"\n  semantic_version = 1\n  preserve = video\n}\n",
        )
        .expect("external program");
        fs::write(
            directory.join("workflow.clipasm"),
            "clipasm 1\nconfig { output = \"final.mp4\" }\nimport \"effect.clipasm\" as effect\nimage(\"card.ppm\", 1s)\neffect\n",
        )
        .expect("workflow");
    }

    let first_compiled = compile_file(&first.path().join("workflow.clipasm")).expect("compile");
    let second_compiled = compile_file(&second.path().join("workflow.clipasm")).expect("compile");
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
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { output = \"final.mp4\" }\nclip { image(\"unused.ppm\", 1s) } as unused\nimage(\"used.ppm\", 1s)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
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
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"final.mp4\" }\nimage(\"card.ppm\", 1s)\n",
    )
    .expect("workflow");
    let compiled = compile_file(&workflow).expect("compile");
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
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 63\nheight = 65\nfps = 10 }\noutput = \"final.mp4\" }\nimage(\"card.ppm\", 1s)\n",
    )
    .expect("workflow");
    let compiled = compile_file(&workflow).expect("pure compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("export dimensions");
    assert_eq!(error.code, "E_EXPORT_DIMENSIONS");
}

#[test]
fn output_extension_is_strictly_mp4() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { output = \"final.mov\" }\nimage(\"missing.png\", 1s)\n",
    )
    .expect("workflow");
    let compiled = compile_file(&workflow).expect("compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("extension");
    assert_eq!(error.code, "E_INVALID_OUTPUT_EXTENSION");
}

#[test]
fn output_cannot_replace_a_reachable_image_asset() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.mp4", "255 0 0");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { output = \"card.mp4\" }\nimage(\"card.mp4\", 1s)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("output collision");
    assert_eq!(error.code, "E_OUTPUT_COLLISION");
    assert!(error.message.contains("output"));
    assert!(error.message.contains("image asset"));
}

#[test]
fn output_cannot_replace_a_reachable_video_asset() {
    if !common::media_tools_available() {
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
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { output = \"source.mp4\" }\nvideo(\"source.mp4\")\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("output collision");
    assert_eq!(error.code, "E_OUTPUT_COLLISION");
    assert!(error.message.contains("video asset"));
}

#[test]
fn manifest_cannot_replace_a_reachable_asset() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "final.mp4.manifest.json", "255 0 0");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { output = \"final.mp4\" }\nimage(\"final.mp4.manifest.json\", 1s)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
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
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { output = \"final.mp4\" }\nimage(\"card.ppm\", 1s)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
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
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { output = \"final.mp4\" }\nimage(\"card.ppm\", 1s)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
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
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { output = \"final.mp4\" }\nimage(\"card.ppm\", 1s)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
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
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { output = \"final.mp4\" }\nvideo(\"missing.mp4\")\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("missing video");
    assert_eq!(error.code, "E_MISSING_VIDEO_FILE");
}

#[test]
fn video_preflight_derives_the_full_source_duration() {
    if !common::media_tools_available() {
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
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"final.mp4\" }\nvideo(\"source.mkv\")\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
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
fn preflight_resolves_media_dependent_marker_trim() {
    if !common::media_tools_available() {
        eprintln!("skipping deferred marker test because FFmpeg/FFprobe are unavailable");
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
            "color=c=red:s=64x64:r=10:d=2",
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
        r#"clipasm 1
config { video { width = 64
height = 64
fps = 10 }
output = "final.mp4" }
video("source.mkv") as source
trim(range=($source::start + 200ms)..($source::end - 300ms))
"#,
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("pure compile");
    assert!(compiled.result_domain().is_none());
    let plan = clipasm::preflight::preflight(&compiled).expect("deferred marker preflight");
    let result = &plan.nodes()[plan.result().get() as usize];
    let Some(PreparedVideoKind::Slice { range, .. }) = result.video_kind() else {
        panic!("prepared marker slice");
    };
    assert_eq!(range.start(), 2);
    assert_eq!(range.end(), 17);
    assert_eq!(result.video_domain().expect("Video node").frames().0, 15);
}

#[test]
fn preflight_resolves_nested_media_marker_offsets() {
    if !common::media_tools_available() {
        eprintln!("skipping nested deferred marker test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "intro.ppm", "0 0 0");
    let source = directory.path().join("source.mkv");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x64:r=10:d=2",
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
        r#"clipasm 1
config { video { width = 64
height = 64
fps = 10 }
output = "final.mp4" }
clip {
    image("intro.ppm", 1s) as intro
    video("source.mkv") as main
} as edit
$edit
trim(range=($edit::main::start + 200ms)..($edit::main::end - 300ms))
"#,
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("pure compile");
    let plan = clipasm::preflight::preflight(&compiled).expect("nested marker preflight");
    let result = &plan.nodes()[plan.result().get() as usize];
    let Some(PreparedVideoKind::Slice { range, .. }) = result.video_kind() else {
        panic!("prepared nested marker slice");
    };
    assert_eq!(range.start(), 12);
    assert_eq!(range.end(), 27);
    assert_eq!(result.video_domain().expect("Video node").frames().0, 15);
}

#[test]
fn preflight_resolves_deferred_during_and_inherited_image_extent() {
    if !common::media_tools_available() {
        eprintln!("skipping deferred during test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "replacement.ppm", "0 255 0");
    let source = directory.path().join("source.mkv");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x64:r=10:d=2",
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
        r#"clipasm 1
config { video { width = 64
height = 64
fps = 10 }
output = "final.mp4" }
video("source.mkv") as source
during(range=($source::start + 200ms)..($source::end - 300ms)) {
    drop<Video>
    image("replacement.ppm")
}
"#,
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("pure compile");
    assert!(compiled.result_domain().is_none());
    let plan = clipasm::preflight::preflight(&compiled).expect("deferred during preflight");
    let inherited = plan
        .nodes()
        .iter()
        .find(|node| {
            matches!(
                node.video_kind(),
                Some(PreparedVideoKind::ImageVideo { frames, .. }) if frames.0 == 15
            )
        })
        .expect("15-frame inherited replacement image");
    assert_eq!(inherited.video_domain().expect("Video node").frames().0, 15);
    assert_eq!(
        plan.nodes()[plan.result().get() as usize]
            .video_domain()
            .expect("Video result")
            .frames()
            .0,
        20
    );
}

#[test]
fn preflight_resolves_crossfade_overlap_markers_from_video_sources() {
    if !common::media_tools_available() {
        eprintln!(
            "skipping deferred transition-marker test because FFmpeg/FFprobe are unavailable"
        );
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    for (name, color) in [("before.mkv", "red"), ("after.mkv", "blue")] {
        let path = directory.path().join(name);
        let source = format!("color=c={color}:s=64x64:r=10:d=1");
        let status = Command::new("ffmpeg")
            .args([
                "-y", "-v", "error", "-f", "lavfi", "-i", &source, "-c:v", "ffv1",
            ])
            .arg(&path)
            .status()
            .expect("create source video");
        assert!(status.success());
    }
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        r#"clipasm 1
config { video { width = 64
height = 64
fps = 10 }
output = "final.mp4" }
video("before.mkv")
video("after.mkv")
crossfade(400ms) as transition
trim(range=$transition::overlap)
"#,
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("pure compile");
    assert!(compiled.result_domain().is_none());
    let plan = clipasm::preflight::preflight(&compiled).expect("transition marker preflight");
    assert_eq!(
        plan.nodes()[plan.result().get() as usize]
            .video_domain()
            .expect("Video result")
            .frames()
            .0,
        4
    );
}

#[test]
fn preflight_rejects_unaligned_media_dependent_marker() {
    if !common::media_tools_available() {
        eprintln!("skipping deferred marker alignment test because FFmpeg/FFprobe are unavailable");
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
            "color=c=red:s=64x64:r=10:d=1.9",
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
        r#"clipasm 1
config { video { width = 64
height = 64
fps = 10 }
output = "final.mp4" }
video("source.mkv") as source
trim(range=$source::middle..$source::end)
"#,
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("pure compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("half-frame midpoint");
    assert_eq!(error.code, "E_TIME_NOT_FRAME_ALIGNED");
}

#[test]
fn preflight_rejects_out_of_bounds_media_dependent_marker() {
    if !common::media_tools_available() {
        eprintln!("skipping deferred marker bounds test because FFmpeg/FFprobe are unavailable");
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
            "color=c=red:s=64x64:r=10:d=2",
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
        r#"clipasm 1
config { video { width = 64
height = 64
fps = 10 }
output = "final.mp4" }
video("source.mkv") as source
trim(range=$source::start..($source::end + 100ms))
"#,
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("pure compile");
    let error = clipasm::preflight::preflight(&compiled).expect_err("out-of-bounds marker");
    assert_eq!(error.code, "E_INVALID_TIME_RANGE");
    assert!(error.message.contains("21"));
    assert!(error.message.contains("20"));
}

#[test]
fn prepared_repeat_keeps_one_upstream_edge() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"final.mp4\" }\nimage(\"card.ppm\", 1s)\nrepeat(2)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
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
fn prepared_zoom_in_preserves_the_exact_input_domain() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "card.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"final.mp4\" }\nimage(\"card.ppm\", 1s)\nzoom_in(12%)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
    let input_domain = *compiled.result_domain().expect("known zoom_in domain");
    let plan = clipasm::preflight::preflight(&compiled).expect("preflight");
    let result = &plan.nodes()[plan.result().get() as usize];
    let Some(PreparedVideoKind::ZoomIn { input, by }) = result.video_kind() else {
        panic!("prepared zoom_in");
    };
    assert_eq!(input.get(), 0);
    assert_eq!(by.canonical(), "3/25");
    assert_eq!(result.video_domain(), Some(&input_domain));
}

#[test]
fn prepared_flash_cut_preserves_order_frames_and_exact_summed_domain() {
    let directory = tempfile::tempdir().expect("temporary directory");
    write_image(directory.path(), "before.ppm", "0 0 0");
    write_image(directory.path(), "after.ppm", "255 0 0");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 }\noutput = \"final.mp4\" }\nimage(\"before.ppm\", 1s)\nimage(\"after.ppm\", 1s)\nflash_cut(400ms)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("compile");
    let plan = clipasm::preflight::preflight(&compiled).expect("preflight");
    let result = &plan.nodes()[plan.result().get() as usize];
    let Some(PreparedVideoKind::FlashCut {
        before,
        after,
        frames,
    }) = result.video_kind()
    else {
        panic!("prepared flash_cut");
    };
    assert_eq!(before.get(), 0);
    assert_eq!(after.get(), 1);
    assert_eq!(frames.0, 4);
    assert_eq!(result.video_domain().expect("Video node").frames().0, 20);
}

#[test]
fn preflight_rejects_flash_cut_longer_than_a_deferred_after_video() {
    if !common::media_tools_available() {
        eprintln!("skipping deferred flash_cut test because FFmpeg/FFprobe are unavailable");
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
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nconfig { video { width = 64\nheight = 48\nfps = 10 }\noutput = \"final.mp4\" }\nimage(\"before.ppm\", 1s)\nvideo(\"after.mkv\")\nflash_cut(1100ms)\n",
    )
    .expect("workflow");

    let compiled = compile_file(&workflow).expect("deferred compile");
    assert!(compiled.result_domain().is_none());
    let error = clipasm::preflight::preflight(&compiled).expect_err("excessive flash_cut duration");
    assert_eq!(error.code, "E_INVALID_FLASH_CUT_DURATION");
    assert!(error.message.contains("11"));
    assert!(error.message.contains("10"));
}
