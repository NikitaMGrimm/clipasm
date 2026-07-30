use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest as _, Sha256};

use super::common;
use super::support::*;

#[test]
fn init_creates_the_canonical_starter_in_a_new_nested_path() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let target = directory.path().join("projects/hello-video");
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .current_dir(directory.path())
        .args(["init", "projects/hello-video"])
        .env("PATH", "")
        .output()
        .expect("run clipasm");

    assert!(
        output.status.success(),
        "initialization failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("UTF-8 output"),
        "\
Created ClipAsm project at `projects/hello-video`.

Next:
  cd \"projects/hello-video\"
  clipasm render

Optional source check:
  clipasm validate
"
    );
    assert!(output.stderr.is_empty());
    assert_eq!(
        project_inventory(&target),
        [
            ".gitignore",
            "README.md",
            "assets",
            "assets/evening.png",
            "assets/meadow.png",
            "assets/morning.png",
            "clipasm.toml",
            "main.clipasm",
        ]
        .map(PathBuf::from)
    );
    let ignore = fs::read_to_string(target.join(".gitignore")).expect("generated ignore");
    assert_eq!(ignore, "/.clipasm/\n/generated/\n");
    assert!(!ignore.contains("*.mp4"));
    let readme = fs::read_to_string(target.join("README.md")).expect("generated readme");
    assert_eq!(
        readme.as_bytes(),
        include_bytes!("../../examples/starter/README.md")
    );
    assert_eq!(
        fs::read(target.join("clipasm.toml")).expect("generated manifest"),
        include_bytes!("../../examples/starter/clipasm.toml")
    );
    for command in ["clipasm validate", "clipasm render"] {
        assert!(readme.contains(command));
    }
    assert_eq!(
        fs::read(target.join("main.clipasm")).expect("generated source"),
        include_bytes!("../../examples/scenic-sequence.clipasm")
    );
    for (asset, expected_hash) in [
        (
            "morning.png",
            "27276968af71e810cea6e3d85372555af2cfb6b04a772478aba71b5bf4d72083",
        ),
        (
            "meadow.png",
            "3c907bd3bf187adc816b583c4bc32c83f43dcc06f5202d5231b6b5de9c6c142d",
        ),
        (
            "evening.png",
            "7981d3472637275cf410d7f5bca952d1ce87bb01ebf656f401962cdab8a79a40",
        ),
    ] {
        let generated = fs::read(target.join("assets").join(asset)).expect("generated asset");
        assert_eq!(
            generated,
            fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("examples/assets")
                    .join(asset)
            )
            .expect("canonical asset")
        );
        assert_eq!(hex::encode(Sha256::digest(&generated)), expected_hash);
    }
    assert!(!target.join(".git").exists());

    let validation = run_clipasm(&target.join("assets"), &["validate"]);
    assert!(
        validation.status.success(),
        "generated source failed validation: {}",
        String::from_utf8_lossy(&validation.stderr)
    );
    assert_eq!(
        String::from_utf8(validation.stdout).expect("UTF-8 validation"),
        "valid: 5 semantic value(s), 108 frame(s)\n"
    );
}
#[test]
fn init_uses_the_current_directory_and_preserves_unrelated_content() {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::create_dir(directory.path().join("assets")).expect("compatible asset directory");
    fs::write(directory.path().join("notes.txt"), b"keep me").expect("unrelated content");

    let output = run_clipasm(directory.path(), &["init"]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("UTF-8 output"),
        "\
Created ClipAsm project at `.`.

Next:
  clipasm render

Optional source check:
  clipasm validate
"
    );
    assert_eq!(
        fs::read(directory.path().join("notes.txt")).expect("unrelated content"),
        b"keep me"
    );
    assert!(directory.path().join("main.clipasm").is_file());
}

#[test]
fn init_with_an_absolute_current_directory_omits_cd() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let target = directory.path().to_str().expect("UTF-8 temporary path");

    let output = run_clipasm(directory.path(), &["init", target]);

    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("cd \""));
}

#[test]
fn init_detects_all_predictable_conflicts_before_writing() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let target = directory.path().join("existing");
    fs::create_dir(&target).expect("target");
    fs::write(target.join("main.clipasm"), b"owned source").expect("conflicting source");
    fs::write(target.join("assets"), b"owned asset path").expect("conflicting directory");

    let output = run_clipasm(directory.path(), &["init", "existing"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("[E_INIT_CONFLICT]"), "{stderr}");
    assert!(stderr.contains("main.clipasm"), "{stderr}");
    assert!(stderr.contains("assets"), "{stderr}");
    assert_eq!(
        fs::read(target.join("main.clipasm")).expect("preserved source"),
        b"owned source"
    );
    assert_eq!(
        fs::read(target.join("assets")).expect("preserved asset path"),
        b"owned asset path"
    );
    assert!(!target.join("README.md").exists());
    assert!(!target.join(".gitignore").exists());
}

#[test]
fn init_rejects_repeated_initialization_without_replacing_files() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let first = run_clipasm(directory.path(), &["init"]);
    assert!(first.status.success());
    fs::write(directory.path().join("README.md"), b"my project").expect("edit readme");

    let repeated = run_clipasm(directory.path(), &["init"]);

    assert!(!repeated.status.success());
    assert!(String::from_utf8_lossy(&repeated.stderr).contains("[E_INIT_CONFLICT]"));
    assert_eq!(
        fs::read(directory.path().join("README.md")).expect("preserved readme"),
        b"my project"
    );
}

#[test]
fn init_rejects_a_target_that_is_a_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(directory.path().join("project"), b"owned").expect("target file");

    let output = run_clipasm(directory.path(), &["init", "project"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[E_INIT_CONFLICT]"));
    assert_eq!(
        fs::read(directory.path().join("project")).expect("preserved target"),
        b"owned"
    );
}

#[test]
fn initialized_project_renders_when_media_tools_are_available() {
    if !common::media_tools_available() {
        eprintln!("skipping initialized-project render because FFmpeg/FFprobe are unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    let initialized = run_clipasm(directory.path(), &["init"]);
    assert!(initialized.status.success());

    let validated = run_clipasm(directory.path(), &["validate"]);
    assert!(
        validated.status.success(),
        "starter validation failed: {}",
        String::from_utf8_lossy(&validated.stderr)
    );

    let rendered = run_clipasm(directory.path(), &["render"]);

    assert!(
        rendered.status.success(),
        "starter render failed: {}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    assert!(
        directory
            .path()
            .join("generated/scenic-sequence.mp4")
            .is_file()
    );
    assert!(
        directory
            .path()
            .join("generated/scenic-sequence.mp4.manifest.json")
            .is_file()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn init_accepts_a_non_utf8_target_path_on_linux() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;

    let directory = tempfile::tempdir().expect("temporary directory");
    let target_name = OsString::from_vec(b"project-\xFF".to_vec());
    let target = directory.path().join(&target_name);
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .current_dir(directory.path())
        .arg("init")
        .arg(&target_name)
        .output()
        .expect("run clipasm");

    assert!(
        output.status.success(),
        "initialization failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(target.join("main.clipasm").is_file());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("  cd \""), "{stdout}");
    assert!(
        stdout.contains("In the created project directory, run:"),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn init_accepts_an_existing_target_directory_symlink() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("temporary directory");
    let owned = directory.path().join("owned");
    fs::create_dir(&owned).expect("owned directory");
    fs::write(owned.join("notes.txt"), b"keep me").expect("owned content");
    symlink(&owned, directory.path().join("project")).expect("target symlink");

    let output = run_clipasm(directory.path(), &["init", "project"]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(owned.join("notes.txt")).expect("preserved content"),
        b"keep me"
    );
    assert!(owned.join("main.clipasm").is_file());
}

#[cfg(unix)]
#[test]
fn init_accepts_a_target_below_a_symlinked_directory_ancestor() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("temporary directory");
    let owned = directory.path().join("owned");
    fs::create_dir(&owned).expect("owned directory");
    symlink(&owned, directory.path().join("link")).expect("ancestor symlink");

    let output = run_clipasm(directory.path(), &["init", "link/project"]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(owned.join("project/main.clipasm").is_file());
}

#[cfg(unix)]
#[test]
fn init_accepts_an_existing_assets_directory_symlink() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("temporary directory");
    let target = directory.path().join("project");
    let asset_store = directory.path().join("asset-store");
    fs::create_dir(&target).expect("target directory");
    fs::create_dir(&asset_store).expect("asset store");
    fs::write(asset_store.join("notes.txt"), b"keep me").expect("unrelated asset");
    symlink(&asset_store, target.join("assets")).expect("assets symlink");

    let output = run_clipasm(directory.path(), &["init", "project"]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(asset_store.join("notes.txt")).expect("preserved asset"),
        b"keep me"
    );
    assert!(asset_store.join("morning.png").is_file());
}

#[cfg(unix)]
#[test]
fn init_rejects_a_broken_target_symlink() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("temporary directory");
    let missing = directory.path().join("missing");
    let project = directory.path().join("project");
    symlink(&missing, &project).expect("broken target symlink");

    let output = run_clipasm(directory.path(), &["init", "project"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[E_INIT_CONFLICT]"));
    assert!(
        fs::symlink_metadata(&project)
            .expect("preserved symlink")
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn init_rejects_a_target_symlink_resolving_to_a_file() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("temporary directory");
    let owned = directory.path().join("owned");
    fs::write(&owned, b"keep me").expect("owned file");
    symlink(&owned, directory.path().join("project")).expect("target symlink");

    let output = run_clipasm(directory.path(), &["init", "project"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[E_INIT_CONFLICT]"));
    assert_eq!(fs::read(&owned).expect("preserved file"), b"keep me");
}

#[cfg(unix)]
#[test]
fn init_rejects_an_existing_planned_file_symlink() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("temporary directory");
    let target = directory.path().join("project");
    let owned = directory.path().join("owned-morning.png");
    fs::create_dir_all(target.join("assets")).expect("asset directory");
    fs::write(&owned, b"keep me").expect("owned file");
    symlink(&owned, target.join("assets/morning.png")).expect("planned file symlink");

    let output = run_clipasm(directory.path(), &["init", "project"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[E_INIT_CONFLICT]"));
    assert_eq!(fs::read(&owned).expect("preserved file"), b"keep me");
    assert!(!target.join("README.md").exists());
}

#[cfg(unix)]
#[test]
fn init_rejects_an_assets_symlink_resolving_to_a_file() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("temporary directory");
    let target = directory.path().join("project");
    let owned = directory.path().join("owned-assets");
    fs::create_dir(&target).expect("target directory");
    fs::write(&owned, b"keep me").expect("owned file");
    symlink(&owned, target.join("assets")).expect("assets symlink");

    let output = run_clipasm(directory.path(), &["init", "project"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[E_INIT_CONFLICT]"));
    assert_eq!(fs::read(&owned).expect("preserved file"), b"keep me");
}

#[cfg(unix)]
#[test]
fn init_omits_an_unsafe_cd_command_for_a_quoted_path() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let target = "project-\"quoted";

    let output = run_clipasm(directory.path(), &["init", target]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(directory.path().join(target).join("main.clipasm").is_file());
    assert!(!stdout.contains("  cd \""), "{stdout}");
    assert!(
        stdout.contains("In the created project directory, run:"),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn init_success_does_not_emit_terminal_controls_from_the_target_path() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let target = "project-\u{1b}[31m";

    let output = run_clipasm(directory.path(), &["init", target]);

    assert!(output.status.success());
    assert!(directory.path().join(target).join("main.clipasm").is_file());
    assert!(!output.stdout.contains(&0x1b), "{:?}", output.stdout);
    assert!(String::from_utf8_lossy(&output.stdout).contains("In the created project directory"));
}

#[cfg(unix)]
#[test]
fn init_failure_does_not_emit_terminal_controls_from_the_target_path() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let target = "project-\u{1b}[31m";
    fs::create_dir(directory.path().join(target)).expect("target directory");
    fs::write(
        directory.path().join(target).join("main.clipasm"),
        b"keep me",
    )
    .expect("conflicting source");

    let output = run_clipasm(directory.path(), &["init", target]);

    assert!(!output.status.success());
    assert!(!output.stderr.contains(&0x1b), "{:?}", output.stderr);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(r"\u{001B}"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[test]
fn init_preserves_dotdot_resolution_through_a_symlink() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("temporary directory");
    let resolved = directory.path().join("resolved");
    let linked_child = resolved.join("child");
    fs::create_dir_all(&linked_child).expect("linked target");
    symlink(&linked_child, directory.path().join("link")).expect("directory symlink");

    let output = run_clipasm(directory.path(), &["init", "link/../project"]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(resolved.join("project/main.clipasm").is_file());
    assert!(!directory.path().join("project").exists());
}

#[test]
fn init_supports_a_target_above_the_current_directory() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let nested = directory.path().join("nested");
    fs::create_dir(&nested).expect("nested directory");

    let output = run_clipasm(&nested, &["init", "../project"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(directory.path().join("project/main.clipasm").is_file());
    assert!(
        stdout.contains("Created ClipAsm project at `../project`."),
        "{stdout}"
    );
    assert!(stdout.contains("  cd \"../project\""), "{stdout}");
}
