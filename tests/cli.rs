#![allow(missing_docs)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest as _, Sha256};

fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("image");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\n{\n  image(\"card.ppm\", 1s)\n  concat\n}\n",
    )
    .expect("workflow");
    (directory, workflow)
}

fn run_clipasm(current_directory: &Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .current_dir(current_directory)
        .args(arguments)
        .output()
        .expect("run clipasm")
}

fn project_inventory(root: &Path) -> Vec<PathBuf> {
    fn collect(root: &Path, directory: &Path, inventory: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(directory).expect("read project directory") {
            let entry = entry.expect("project entry");
            let path = entry.path();
            inventory.push(
                path.strip_prefix(root)
                    .expect("project-relative path")
                    .into(),
            );
            if path.is_dir() {
                collect(root, &path, inventory);
            }
        }
    }

    let mut inventory = Vec::new();
    collect(root, root, &mut inventory);
    inventory.sort();
    inventory
}

#[test]
fn init_help_is_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args(["init", "--help"])
        .output()
        .expect("run clipasm");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("UTF-8 help"),
        "\
Create a self-contained ClipAsm starter project.

PATH defaults to the current directory and is created when needed. Existing directories are supported only when every starter path is available. Existing files and incompatible directories are never replaced.

Usage: clipasm init [PATH]

Arguments:
  [PATH]
          Directory to initialize. Defaults to the current directory

Options:
  -h, --help
          Print help (see a summary with '-h')

Examples:
  clipasm init hello-video
  clipasm init
"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn root_help_lists_init_and_video_audio_descriptions() {
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .arg("--help")
        .output()
        .expect("run clipasm");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("UTF-8 help"),
        "\
Compile and render typed Video and Audio graphs.

Usage: clipasm <COMMAND>

Commands:
  init      Create a self-contained ClipAsm starter project
  programs  List built-in programs or show one built-in program reference
  validate  Parse, type-check, and infer source-independent Video and Audio domains
  inspect   Inspect compiled Video and Audio semantics as JSON
  render    Compile and render a Video, including attached Audio, using FFmpeg and FFprobe
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn programs_help_is_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args(["programs", "--help"])
        .output()
        .expect("run clipasm");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("UTF-8 help"),
        "\
List ClipAsm's built-in programs or show the reference for one built-in program.

This command never inspects a project, source file, media asset, FFmpeg, or FFprobe.

Usage: clipasm programs [NAME]

Arguments:
  [NAME]
          Built-in program name. Omit to list every built-in program

Options:
  -h, --help
          Print help (see a summary with '-h')
"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn programs_list_is_exact_and_covers_the_catalog() {
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .arg("programs")
        .env("PATH", "")
        .output()
        .expect("run clipasm");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 output");
    assert_eq!(
        stdout,
        "\
Built-in programs
These are built into ClipAsm; project and imported programs are not inspected.

Sources
  image — Create a Video from an image file.
  video — Load a Video from a video file.
  audio — Load standalone Audio from an audio file.

Timeline
  concat — Concatenate one or more homogeneous timelines.
  repeat — Repeat a Video or Audio timeline.
  trim — Keep a selected range of a Video or Audio timeline.
  drop — Remove one Video or Audio value from the stack.

Audio
  extract_audio — Extract the meaningful Audio from a Video.
  set_audio — Replace a Video's Audio with standalone Audio.

Effects
  zoom_in — Apply a linear zoom-in effect to a Video.

Transitions
  flash_cut — Join two Videos with a brief white-flash transition.
  crossfade — Overlap two Videos with a crossfade transition.

Body programs
  join — Transform and concatenate two Video or Audio timelines in a body.
  during — Replace a selected timeline range with the result of a body.

Details: clipasm programs NAME
"
    );
    for program in clipasm::reference::builtin_programs() {
        assert_eq!(
            stdout
                .lines()
                .filter(|line| line.starts_with(&format!("  {} — ", program.name())))
                .count(),
            1,
            "{} must appear exactly once",
            program.name()
        );
    }
    assert!(output.stderr.is_empty());
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "representative exact CLI snapshots remain inline and reviewable beside the invocation"
)]
fn programs_details_are_exact_for_representative_program_kinds() {
    let cases = [
        (
            "image",
            "\
Built-in program: image
Create a Video from an image file.

Call shape (reference notation; not declaration syntax):
  image(path: File, duration?: Duration, fit?: Keyword(cover | contain | stretch)) -> Video

Inputs:
  none

Parameters:
  path: File (required)
  duration: Duration (optional; when omitted: uses a requested Video extent supplied by the surrounding body; without one, the call reports E_MISSING_IMAGE_DURATION)
  fit: Keyword(cover | contain | stretch) (optional; default: cover)

Outputs:
  Video

Generic type:
  not generic

Stack access:
  owned

Body:
  not accepted

Timeline:
  creates a fresh timeline

Behavior:
  - The image is fitted to the project Video dimensions; cover fills the frame and crops overflow, contain pads, and stretch may distort.
  - A surrounding Video body may supply the requested duration when duration is omitted.

Constraints:
  - The resolved duration must contain at least one project frame.

Important diagnostics:
  E_MISSING_IMAGE_DURATION, E_INVALID_DURATION

Example:
  image(\"assets/title.png\", 2s, contain)

Full guide:
  https://nikitamgrimm.github.io/clipasm/reference/programs/image.html

Related built-in programs:
  video, during
",
        ),
        (
            "concat",
            "\
Built-in program: concat
Concatenate one or more homogeneous timelines.

Call shape (reference notation; not declaration syntax):
  concat<T: Video | Audio>(values: T...) -> T

Inputs:
  values: T (variadic; minimum 1)

Parameters:
  none

Outputs:
  T

Generic type:
  one homogeneous Video or Audio type; use <Video> or <Audio> when ambiguous

Stack access:
  owned

Body:
  not accepted

Timeline:
  concatenates the layouts bound to `values`

Behavior:
  - Every bound value must use the same inferred Video or Audio type and is concatenated in stack order.
  - Use `concat<Video>` or `concat<Audio>` when both homogeneous bindings are possible.

Example:
  image(\"assets/one.png\", 1s)
  image(\"assets/two.png\", 1s)
  concat

Full guide:
  https://nikitamgrimm.github.io/clipasm/reference/programs/concat.html

Related built-in programs:
  join
",
        ),
        (
            "crossfade",
            "\
Built-in program: crossfade
Overlap two Videos with a crossfade transition.

Call shape (reference notation; not declaration syntax):
  crossfade(before: Video, after: Video, duration?: Duration) -> Video

Inputs:
  before: Video
  after: Video

Parameters:
  duration: Duration (optional; default: 500ms)

Outputs:
  Video

Generic type:
  not generic

Stack access:
  owned

Body:
  not accepted

Timeline:
  creates overlapping regions from `before` and `after`

Behavior:
  - duration becomes the smallest whole project-frame count that covers the authored duration.
  - The output exposes before, overlap, and after timeline regions.

Constraints:
  - duration must cover at least one project frame and cannot exceed either input Video.

Important diagnostics:
  E_INVALID_CROSSFADE_DURATION

Example:
  image(\"assets/before.png\", 2s)
  image(\"assets/after.png\", 2s)
  crossfade

Full guide:
  https://nikitamgrimm.github.io/clipasm/reference/programs/crossfade.html

Related built-in programs:
  flash_cut
",
        ),
        (
            "during",
            "\
Built-in program: during
Replace a selected timeline range with the result of a body.

Call shape (reference notation; not declaration syntax):
  during<T: Video | Audio>(timeline: T, range: TimeRange) { ... } -> T

Inputs:
  timeline: T

Parameters:
  range: TimeRange (required)

Outputs:
  T

Generic type:
  one homogeneous Video or Audio type; use <Video> or <Audio> when ambiguous

Stack access:
  visible

Body:
  accepted
  initial stack:
    T selected from `timeline` by `range`
  required body outputs:
    exactly T

Timeline:
  splices the body result into the selected range of `timeline`

Behavior:
  - The body starts with the selected range and exposes the complete bound input as lexical $timeline.
  - The body must return exactly one matching value, which is spliced into the original timeline.
  - Placements before and after the range are preserved or shifted; intersecting or uncertain placements are omitted, and the inserted body is available as replacement.
  - A Video selection supplies its requested extent to an image call whose duration is omitted.

Constraints:
  - The range must be native-grid aligned, within the bound timeline, and owned by that timeline.
  - Use `during<Video>` or `during<Audio>` when a mixed stack makes the generic type ambiguous.

Important diagnostics:
  E_BODY_OUTPUT_COUNT

Example:
  image(\"assets/card.png\", 3s)
  during(1s..2s) {
      zoom_in(4%)
  }

Full guide:
  https://nikitamgrimm.github.io/clipasm/reference/programs/during.html

Related built-in programs:
  trim, join, image
",
        ),
    ];

    for (name, expected) in cases {
        let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
            .args(["programs", name])
            .env("PATH", "")
            .output()
            .expect("run clipasm");
        assert!(output.status.success(), "{name}");
        assert_eq!(
            String::from_utf8(output.stdout).expect("UTF-8 output"),
            expected,
            "{name}"
        );
        assert!(output.stderr.is_empty(), "{name}");
    }
}

#[test]
fn unknown_built_in_program_is_an_exact_nonzero_diagnostic() {
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args(["programs", "imag"])
        .env("PATH", "")
        .output()
        .expect("run clipasm");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("UTF-8 diagnostic"),
        "\
<command-line>:1:1 [E_UNKNOWN_BUILTIN_PROGRAM]

unknown built-in program `imag`

note: run `clipasm programs` to list all built-in programs
"
    );
}

#[test]
fn unknown_built_in_program_diagnostic_escapes_terminal_controls() {
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args(["programs", "bad\u{1b}[31mname"])
        .output()
        .expect("run clipasm");

    assert!(!output.status.success());
    assert!(!output.stderr.contains(&0x1b));
    assert!(
        String::from_utf8(output.stderr)
            .expect("UTF-8 diagnostic")
            .contains("unknown built-in program `bad\\u{001B}[31mname`")
    );
}

#[test]
fn render_help_mentions_ffprobe() {
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args(["render", "--help"])
        .output()
        .expect("run clipasm");

    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .expect("UTF-8 help")
            .contains("FFprobe")
    );
    assert!(output.stderr.is_empty());
}

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
  clipasm validate main.clipasm
  clipasm render main.clipasm
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
        include_bytes!("../examples/starter/README.md")
    );
    for command in [
        "clipasm validate main.clipasm",
        "clipasm render main.clipasm",
    ] {
        assert!(readme.contains(command));
    }
    assert_eq!(
        fs::read(target.join("main.clipasm")).expect("generated source"),
        include_bytes!("../examples/scenic-sequence.clipasm")
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

    let validation = run_clipasm(&target, &["validate", "main.clipasm"]);
    assert!(
        validation.status.success(),
        "generated source failed validation: {}",
        String::from_utf8_lossy(&validation.stderr)
    );
    assert_eq!(
        String::from_utf8(validation.stdout).expect("UTF-8 validation"),
        "valid: 4 semantic value(s), 108 frame(s)\n"
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
  clipasm validate main.clipasm
  clipasm render main.clipasm
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

    let validated = run_clipasm(directory.path(), &["validate", "main.clipasm"]);
    assert!(
        validated.status.success(),
        "starter validation failed: {}",
        String::from_utf8_lossy(&validated.stderr)
    );

    let rendered = run_clipasm(directory.path(), &["render", "main.clipasm"]);

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

#[cfg(unix)]
#[test]
fn init_accepts_a_non_utf8_target_path() {
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
