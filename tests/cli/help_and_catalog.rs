use std::process::Command;

use super::support::*;

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
  explain   Explain a ClipAsm diagnostic code
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
fn explain_help_is_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .args(["explain", "--help"])
        .output()
        .expect("run clipasm");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("UTF-8 help"),
        "\
Explain one built-in ClipAsm diagnostic code.

This command uses the catalog built into the executable. It never inspects a project, source file, media asset, FFmpeg, or FFprobe.

Usage: clipasm explain <CODE>

Arguments:
  <CODE>
          Built-in diagnostic code such as `E_UNKNOWN_PROGRAM`

Options:
  -h, --help
          Print help (see a summary with '-h')
"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn explain_resolves_every_catalog_code_without_inspecting_the_environment() {
    let directory = tempfile::tempdir().expect("temporary directory");

    for reference in clipasm::reference::diagnostics() {
        let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
            .current_dir(directory.path())
            .env("PATH", "")
            .args(["explain", reference.code()])
            .output()
            .unwrap_or_else(|error| panic!("run explain for {}: {error}", reference.code()));

        assert!(
            output.status.success(),
            "{} failed: {}",
            reference.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 explanation");
        assert!(
            stdout.starts_with(&format!("{}: {}\n", reference.code(), reference.title())),
            "{}",
            reference.code()
        );
        assert!(stdout.contains(&reference.documentation_url()));
        assert!(output.stderr.is_empty(), "{}", reference.code());
    }

    assert!(project_inventory(directory.path()).is_empty());
}

#[test]
fn explain_rejects_unknown_codes_with_bounded_ascii_safe_output() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let unknown = format!("E_BAD\u{1b}[31m\u{e9}{}", "X".repeat(1_000));
    let output = Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .current_dir(directory.path())
        .env("PATH", "")
        .args(["explain", &unknown])
        .output()
        .expect("run explain");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_ascii());
    assert!(!output.stderr.contains(&0x1b));
    assert!(output.stderr.len() < 2_000);
    let stderr = String::from_utf8(output.stderr).expect("ASCII diagnostic");
    assert!(stderr.contains("[E_UNKNOWN_DIAGNOSTIC_CODE]"));
    assert!(stderr.contains(r"\u{1B}"));
    assert!(stderr.contains(r"\u{E9}"));
    assert!(stderr.contains("...`"));
    assert!(stderr.contains("check the code's spelling"));
    assert!(stderr.contains("diagnostics/"));
    assert!(project_inventory(directory.path()).is_empty());
}

#[test]
fn explain_unknown_program_output_is_exact() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let output = run_clipasm(directory.path(), &["explain", "E_UNKNOWN_PROGRAM"]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("UTF-8 explanation"),
        "\
E_UNKNOWN_PROGRAM: Unknown program

Category: Imports and declarations

ClipAsm could not resolve a called name as a built-in, imported, or locally declared program.

Common causes:
  - The program name is misspelled.
  - The source defining the program was not imported.
  - An import alias or locally declared name differs from the call.

Try:
  - Run `clipasm programs` when the intended target is a built-in program.
  - Check imports, aliases, and the exact program spelling.
  - Use the source location and notes from the original diagnostic.

Retry:
  Retry after correcting the source.

Reference:
  https://nikitamgrimm.github.io/clipasm/diagnostics/index.html#e_unknown_program
"
    );
    assert!(output.stderr.is_empty());
    assert!(project_inventory(directory.path()).is_empty());
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
List ClipAsm's callable built-in programs or show the reference for one built-in program.

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
Callable built-in programs
These are registered calls built into ClipAsm; project and imported programs are not inspected.
Language forms such as `clip` and stack blocks are documented in the book, not listed here.

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
  crossfade — Overlap two Videos or Audio values with a crossfade transition.

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
  duration: Duration (optional; when omitted: uses a requested Video extent from the surrounding body. Without one, the call reports a missing image duration)
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
  - ClipAsm fits the image to the project Video dimensions.
  - The `cover` mode fills the frame and crops overflow. The `contain` mode adds padding. The `stretch` mode can distort the image.
  - A surrounding Video body may supply the requested duration when the author omits `duration`.

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
  - Every bound value must use the same inferred Video or Audio type.
  - The program concatenates the bound values in stack order.
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
Overlap two Videos or Audio values with a crossfade transition.

Call shape (reference notation; not declaration syntax):
  crossfade<T: Video | Audio>(before: T, after: T, duration?: Duration) -> T

Inputs:
  before: T
  after: T

Parameters:
  duration: Duration (optional; default: 500ms)

Outputs:
  T

Generic type:
  one homogeneous Video or Audio type; use <Video> or <Audio> when ambiguous

Stack access:
  owned

Body:
  not accepted

Timeline:
  creates overlapping regions from `before` and `after`

Behavior:
  - For Video, duration becomes the smallest whole project-frame count that covers the authored duration; for Audio, it becomes the smallest whole project-sample count.
  - Video pictures use the existing frame blend, while standalone and attached Audio use the same equal-power fade curves.
  - The output exposes before, overlap, and after timeline regions.

Constraints:
  - before and after must have the same Video or Audio type.
  - duration must cover at least one native frame or sample and cannot exceed either input.

Important diagnostics:
  E_INVALID_CROSSFADE_DURATION, E_CROSSFADE_AUDIO_DURATION

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
  - The body starts with the selected range.
  - The body exposes the complete bound input as the lexical `$timeline` reference.
  - The body must return exactly one matching value. ClipAsm inserts that value into the original timeline.
  - ClipAsm preserves or shifts placements before and after the range.
  - ClipAsm omits intersecting or uncertain placements. The `replacement` name identifies the inserted body.
  - A Video selection supplies its requested extent when the author omits the image call's `duration`.

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
