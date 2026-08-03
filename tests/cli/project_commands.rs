use std::fs;
use std::process::Command;

use super::common;
use super::support::*;

#[test]
fn inspect_prints_machine_readable_semantics() {
    let (_directory, workflow) = fixture();
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args(["inspect", workflow.to_str().expect("UTF-8 path")])
        .output()
        .expect("run clipasm");
    assert!(output.status.success());
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("plan JSON");
    assert!(plan["structure_hash"].as_str().is_some());
    assert_eq!(plan["nodes"][0]["kind"]["operation"], "image_video");
}

#[test]
fn project_commands_discover_the_nearest_manifest_from_subdirectories() {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::create_dir_all(directory.path().join("src/nested")).expect("nested directory");
    fs::write(
        directory.path().join("clipasm.toml"),
        "[project]\nentrypoint = \"src/main.clipasm\"\n",
    )
    .expect("manifest");
    fs::write(
        directory.path().join("src/main.clipasm"),
        "clipasm 1\nimage(\"missing.png\", 1s)\n",
    )
    .expect("source");

    let validate = run_clipasm(&directory.path().join("src/nested"), &["validate"]);
    assert!(
        validate.status.success(),
        "{}",
        String::from_utf8_lossy(&validate.stderr)
    );

    let inspect = run_clipasm(&directory.path().join("src/nested"), &["inspect"]);
    assert!(
        inspect.status.success(),
        "{}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    let document: serde_json::Value =
        serde_json::from_slice(&inspect.stdout).expect("compiled JSON");
    assert_eq!(document["nodes"][0]["kind"]["operation"], "image_video");
}

#[test]
fn project_render_keeps_cache_at_the_manifest_root() {
    if !common::media_tools_available() {
        eprintln!("skipping project-root cache test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::create_dir_all(directory.path().join("src/nested")).expect("nested directory");
    fs::write(
        directory.path().join("clipasm.toml"),
        "[project]\nentrypoint = \"src/main.clipasm\"\n",
    )
    .expect("manifest");
    fs::write(
        directory.path().join("src/card.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("image");
    fs::write(
        directory.path().join("src/main.clipasm"),
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"final.mp4\" }\nimage(\"card.ppm\", 1s)\n",
    )
    .expect("source");

    let rendered = run_clipasm(&directory.path().join("src/nested"), &["render"]);
    assert!(
        rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    assert!(directory.path().join(".clipasm/cache").is_dir());
    assert!(!directory.path().join("src/.clipasm").exists());

    let namespace = fs::read_dir(directory.path().join(".clipasm/cache"))
        .expect("cache root")
        .next()
        .expect("execution namespace")
        .expect("execution namespace entry")
        .path();
    let artifact = fs::read_dir(namespace)
        .expect("execution namespace")
        .map(|entry| entry.expect("cache entry").path())
        .find(|path| {
            matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("mkv" | "flac")
            )
        })
        .expect("cache artifact");
    let sentinel = b"persistent cache remains untouched";
    fs::write(&artifact, sentinel).expect("replace cache artifact with sentinel");

    let uncached_again = run_clipasm(
        &directory.path().join("src/nested"),
        &["render", "--cache", "none"],
    );
    assert!(
        uncached_again.status.success(),
        "{}",
        String::from_utf8_lossy(&uncached_again.stderr)
    );
    assert_eq!(
        fs::read(artifact).expect("preserved cache artifact"),
        sentinel
    );
}

#[test]
fn project_cache_none_is_temporary_and_the_cli_can_override_it() {
    if !common::media_tools_available() {
        eprintln!("skipping project cache-mode test because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::create_dir_all(directory.path().join("src/nested")).expect("nested directory");
    fs::write(
        directory.path().join("clipasm.toml"),
        "[project]\nentrypoint = \"src/main.clipasm\"\n\n[render]\ncache = \"none\"\nmaterialization = \"fused\"\n",
    )
    .expect("manifest");
    fs::write(
        directory.path().join("src/card.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("image");
    fs::write(
        directory.path().join("src/main.clipasm"),
        "clipasm 1\nconfig { video { width = 64\nheight = 64\nfps = 10 }\noutput = \"final.mp4\" }\nimage(\"card.ppm\", 100ms)\n",
    )
    .expect("source");

    let uncached = run_clipasm(&directory.path().join("src/nested"), &["render"]);
    assert!(
        uncached.status.success(),
        "{}",
        String::from_utf8_lossy(&uncached.stderr)
    );
    assert!(String::from_utf8_lossy(&uncached.stdout).contains("cache: none"));
    assert!(String::from_utf8_lossy(&uncached.stdout).contains("materialization: fused"));
    assert!(!directory.path().join(".clipasm/cache").exists());
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(directory.path().join("src/final.mp4.manifest.json")).expect("manifest"),
    )
    .expect("manifest JSON");
    assert_eq!(manifest["cache"]["mode"], "none");
    assert_eq!(manifest["execution"]["materialization"], "fused");

    let materialized_all = run_clipasm(
        &directory.path().join("src/nested"),
        &["render", "--materialization", "all"],
    );
    assert!(
        materialized_all.status.success(),
        "{}",
        String::from_utf8_lossy(&materialized_all.stderr)
    );
    assert!(String::from_utf8_lossy(&materialized_all.stdout).contains("cache: none"));
    assert!(String::from_utf8_lossy(&materialized_all.stdout).contains("materialization: all"));
    assert!(!directory.path().join(".clipasm/cache").exists());

    let persistent = run_clipasm(
        &directory.path().join("src/nested"),
        &["render", "--cache", "persistent"],
    );
    assert!(
        persistent.status.success(),
        "{}",
        String::from_utf8_lossy(&persistent.stderr)
    );
    assert!(String::from_utf8_lossy(&persistent.stdout).contains("cache: persistent"));
    assert!(String::from_utf8_lossy(&persistent.stdout).contains("materialization: fused"));
    assert!(directory.path().join(".clipasm/cache").is_dir());
    assert!(!directory.path().join("src/.clipasm").exists());
}

#[test]
fn explicit_sources_do_not_depend_on_ambient_project_manifests() {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(directory.path().join("clipasm.toml"), "not valid toml").expect("manifest");
    fs::write(
        directory.path().join("standalone.clipasm"),
        "clipasm 1\nimage(\"missing.png\", 1s)\n",
    )
    .expect("source");

    let output = run_clipasm(directory.path(), &["validate", "standalone.clipasm"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn omitted_sources_report_missing_and_invalid_project_manifests() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let missing = run_clipasm(directory.path(), &["validate"]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("[E_PROJECT_NOT_FOUND]"));

    fs::write(
        directory.path().join("clipasm.toml"),
        "[project]\nentrypoint = \"../outside.clipasm\"\n",
    )
    .expect("invalid manifest");
    let invalid = run_clipasm(directory.path(), &["validate"]);
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("[E_PROJECT_MANIFEST]"));
}

#[test]
fn inspect_writes_an_explicit_output_path() {
    let (directory, workflow) = fixture();
    let plan_path = directory.path().join("plan.json");
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args([
            "inspect",
            workflow.to_str().expect("UTF-8 path"),
            "--output",
            plan_path.to_str().expect("UTF-8 path"),
        ])
        .output()
        .expect("run clipasm");
    assert!(output.status.success());
    assert!(plan_path.is_file());
}

#[test]
fn inspect_refuses_to_replace_an_existing_file() {
    let (_directory, workflow) = fixture();
    let original = fs::read(&workflow).expect("original workflow");
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args([
            "inspect",
            workflow.to_str().expect("UTF-8 path"),
            "--output",
            workflow.to_str().expect("UTF-8 path"),
        ])
        .output()
        .expect("run clipasm");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[E_INSPECTION_EXISTS]"));
    assert_eq!(fs::read(&workflow).expect("preserved workflow"), original);
}

#[test]
fn inspect_preserves_an_existing_destination() {
    let (directory, workflow) = fixture();
    let plan = directory.path().join("plan.json");
    fs::write(&plan, b"existing plan").expect("existing plan");

    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args([
            "inspect",
            workflow.to_str().expect("UTF-8 path"),
            "--output",
            plan.to_str().expect("UTF-8 path"),
        ])
        .output()
        .expect("run clipasm");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[E_INSPECTION_EXISTS]"));
    assert_eq!(fs::read(&plan).expect("preserved plan"), b"existing plan");
}

#[test]
fn diagnostics_produce_a_failure_exit_code() {
    let (directory, workflow) = fixture();
    fs::write(
        &workflow,
        "clipasm 1\n{\n  repeat<Video>(2)\n  concat<Video>\n}\n",
    )
    .expect("invalid workflow");
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args(["validate", workflow.to_str().expect("UTF-8 path")])
        .output()
        .expect("run clipasm");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[E_STACK_UNDERFLOW]"));
    drop(directory);
}

#[test]
fn cli_rejects_non_clipasm_source_paths() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.txt");
    fs::write(&workflow, "clipasm 1\n").expect("source");

    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args(["validate", workflow.to_str().expect("UTF-8 path")])
        .output()
        .expect("run clipasm");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[E_SOURCE_EXTENSION]"));
}

#[test]
fn validate_reports_a_deferred_video_duration_without_opening_the_asset() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\n{\n  video(\"missing.mp4\")\n  concat\n}\n",
    )
    .expect("workflow");

    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args(["validate", workflow.to_str().expect("UTF-8 path")])
        .output()
        .expect("run clipasm");
    assert!(
        output.status.success(),
        "validation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("duration resolves during preflight"));
}

#[test]
fn inspect_binds_root_video_inputs_and_typed_parameters() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("template.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\ninput source: Video\nparam range: TimeRange\nparam count: Integer\ntrim($source, $range)\nrepeat($count)\n",
    )
    .expect("template");

    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .current_dir(directory.path())
        .args([
            "inspect",
            "template.clipasm",
            "--video-input",
            "source=footage.mp4",
            "--arg",
            "range=1s..2s",
            "--arg",
            "count=2",
        ])
        .output()
        .expect("run clipasm");
    assert!(
        output.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("plan JSON");
    let operations = plan["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|node| node["kind"]["operation"].as_str().expect("operation"))
        .collect::<Vec<_>>();
    assert_eq!(
        operations,
        vec!["video_source", "reference", "slice", "repeat"]
    );
    assert_eq!(plan["nodes"][0]["kind"]["path"], "footage.mp4");
}

#[test]
fn inspect_binds_project_frame_duration_parameters() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let workflow = directory.path().join("frames.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\nparam duration: Duration\nimage(\"card.ppm\", $duration)\n",
    )
    .expect("workflow");

    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .current_dir(directory.path())
        .args(["inspect", "frames.clipasm", "--arg", "duration=15f"])
        .output()
        .expect("run clipasm");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let document: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("inspection JSON");
    assert_eq!(document["nodes"][0]["kind"]["frames"], 15);

    let invalid = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .current_dir(directory.path())
        .args(["validate", "frames.clipasm", "--arg", "duration=-1f"])
        .output()
        .expect("run clipasm");
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("cannot be negative"));
}
