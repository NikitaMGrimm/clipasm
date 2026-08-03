use std::fs;
use std::process::Command;

#[cfg(unix)]
use std::ffi::OsString;
#[cfg(unix)]
use std::path::{Path, PathBuf};

use super::common;

#[cfg(unix)]
fn install_ffprobe_logger(directory: &Path) -> (OsString, String, PathBuf) {
    use std::os::unix::fs::PermissionsExt as _;

    let real_ffprobe = Command::new("sh")
        .args(["-c", "command -v ffprobe"])
        .output()
        .expect("locate ffprobe");
    assert!(real_ffprobe.status.success());
    let real_ffprobe = String::from_utf8(real_ffprobe.stdout)
        .expect("UTF-8 ffprobe path")
        .trim()
        .to_owned();
    let tools = directory.join("tools");
    fs::create_dir(&tools).expect("tools directory");
    let wrapper = tools.join("ffprobe");
    fs::write(
        &wrapper,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$CLIPASM_FFPROBE_LOG\"\nexec \"$CLIPASM_REAL_FFPROBE\" \"$@\"\n",
    )
    .expect("ffprobe wrapper");
    let mut permissions = fs::metadata(&wrapper)
        .expect("wrapper metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&wrapper, permissions).expect("wrapper permissions");
    let log = directory.join("ffprobe.log");
    let mut path = tools.into_os_string();
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    (path, real_ffprobe, log)
}

#[cfg(unix)]
#[test]
fn certified_working_cache_hits_do_not_probe_cached_artifacts() {
    if !common::media_tools_available() {
        eprintln!("skipping cache probe test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("image");
    fs::write(
        directory.path().join("workflow.clipasm"),
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"final.mp4\" }\nimage(\"card.ppm\", 1s)\n",
    )
    .expect("workflow");
    let (path, real_ffprobe, log) = install_ffprobe_logger(directory.path());
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_clipasm"))
            .current_dir(directory.path())
            .args(["render", "workflow.clipasm"])
            .env("PATH", &path)
            .env("CLIPASM_REAL_FFPROBE", &real_ffprobe)
            .env("CLIPASM_FFPROBE_LOG", &log)
            .output()
            .expect("run clipasm")
    };

    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_probes = fs::read_to_string(&log).expect("first probe log");
    assert!(
        first_probes
            .lines()
            .any(|line| { line.contains(".clipasm/cache") && line.contains("-count_frames") }),
        "persistent cache admission did not request decoded frame counts:\n{first_probes}"
    );
    fs::write(&log, b"").expect("clear probe log");
    let second = run();
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let probes = fs::read_to_string(&log).expect("probe log");
    assert!(
        !probes.lines().any(|line| line.contains(".clipasm/cache")),
        "certified cache hit was probed again:\n{probes}"
    );
}

#[cfg(unix)]
#[test]
fn cache_none_native_temporaries_trust_recipe_counts_but_publication_does_not() {
    if !common::media_tools_available() {
        eprintln!("skipping transient probe test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("image");
    fs::write(
        directory.path().join("workflow.clipasm"),
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"final.mp4\" }\nimage(\"card.ppm\", 1s)\nzoom_in(10%)\n",
    )
    .expect("workflow");
    let (path, real_ffprobe, log) = install_ffprobe_logger(directory.path());
    let render = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .current_dir(directory.path())
        .args([
            "render",
            "--cache",
            "none",
            "--materialization",
            "all",
            "workflow.clipasm",
        ])
        .env("PATH", &path)
        .env("CLIPASM_REAL_FFPROBE", &real_ffprobe)
        .env("CLIPASM_FFPROBE_LOG", &log)
        .output()
        .expect("run clipasm");
    assert!(
        render.status.success(),
        "{}",
        String::from_utf8_lossy(&render.stderr)
    );

    let probes = fs::read_to_string(&log).expect("probe log");
    let temporary_probes = probes
        .lines()
        .filter(|line| line.contains(".final.mp4.render-"))
        .collect::<Vec<_>>();
    assert_eq!(temporary_probes.len(), 2, "unexpected probes:\n{probes}");
    assert!(
        temporary_probes
            .iter()
            .all(|line| !line.contains("-count_frames")),
        "native temporary requested decoded frame counts:\n{probes}"
    );
    assert!(
        probes.lines().any(|line| {
            line.contains(".final.mp4.publication-") && line.contains("-count_frames")
        }),
        "publication did not request decoded frame counts:\n{probes}"
    );
}

#[test]
fn inspect_binds_root_audio_inputs() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("audio.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\ninput soundtrack: Audio\n$soundtrack\n",
    )
    .expect("workflow");

    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args([
            "inspect",
            workflow.to_str().expect("UTF-8 path"),
            "--audio-input",
            "soundtrack=sound.wav",
        ])
        .output()
        .expect("clipasm");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let document: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("inspection JSON");
    assert_eq!(document["nodes"][0]["kind"]["operation"], "audio_source");
}

#[test]
fn root_cli_bindings_reject_unknown_and_duplicate_names() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("template.clipasm");
    fs::write(&workflow, "clipasm 1\ninput source: Video\n$source\n").expect("template");

    let unknown = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .current_dir(directory.path())
        .args([
            "validate",
            "template.clipasm",
            "--video-input",
            "other=footage.mp4",
        ])
        .output()
        .expect("run clipasm");
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("[E_UNKNOWN_PROGRAM_ARGUMENT]"));

    let duplicate = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .current_dir(directory.path())
        .args([
            "validate",
            "template.clipasm",
            "--video-input",
            "source=first.mp4",
            "--video-input",
            "source=second.mp4",
        ])
        .output()
        .expect("run clipasm");
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("[E_DUPLICATE_ARGUMENT]"));
}

#[test]
fn render_accepts_caller_relative_input_and_output_bindings() {
    if !common::media_tools_available() {
        eprintln!("skipping CLI render test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("template.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\ninput source: Video\nparam count: Integer\nparam overlay: File\nrepeat($source, $count)\nimage($overlay, 1s)\nconcat\n",
    )
    .expect("template");
    fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/assets/gentle-motion.mkv"),
        directory.path().join("input.mkv"),
    )
    .expect("copy video fixture");
    fs::write(
        directory.path().join("overlay.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("overlay fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .current_dir(directory.path())
        .args([
            "render",
            "template.clipasm",
            "--video-input",
            "source=input.mkv",
            "--arg",
            "count=1",
            "--arg",
            "overlay=overlay.ppm",
            "--output",
            "result.mp4",
        ])
        .output()
        .expect("run clipasm");
    assert!(
        output.status.success(),
        "render failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(directory.path().join("result.mp4").is_file());
    assert!(directory.path().join("result.mp4.manifest.json").is_file());
}
