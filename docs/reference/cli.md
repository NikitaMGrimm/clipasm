# Command-line reference

ClipAsm provides six commands: `init`, `programs`, `explain`, `validate`,
`inspect`, and `render`.
Run commands from the directory whose relative CLI paths you intend to use.
From a source checkout, `cargo run -- <COMMAND>` is the equivalent development
form.

## `init`

```text
clipasm init [PATH]
```

The exact built-in help is:

```console
$ clipasm init --help
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

```

Unrelated paths in an existing compatible directory are left alone.
Initialization never prompts for permission.
It follows ordinary local filesystem directory links. ClipAsm assumes that the
caller controls the target tree during initialization. It does not guarantee
behavior when another process changes target paths concurrently.

The two forms are:

```console,ignore
clipasm init hello-video
clipasm init
```

The starter tree is exactly:

```text
.gitignore
README.md
clipasm.toml
main.clipasm
assets/
  morning.png
  meadow.png
  evening.png
```

The installed binary ships this starter tree. The starter program validates to
108 frames and
publishes `generated/scenic-sequence.mp4`. Initialization does not invoke Git,
render, or media tools, and it does not contact the network.

For a named path, success is:

```console,ignore
$ clipasm init hello-video
Created ClipAsm project at `hello-video`.

Next:
  cd "hello-video"
  clipasm render

Optional source check:
  clipasm validate
```

When the target is the current directory, ClipAsm omits the `cd` line. For a path
that a portable shell command cannot represent, the output tells you to
enter the created directory. You can then run the render command. The
source-only validation command remains optional.

The generated files are ordinary, unmanaged project files. ClipAsm does not
update, rewrite, or take ownership of them later. Future releases may
ship different starter files, but they do not alter existing projects. The
development examples in a source checkout are not the installed binary's
starter contract and may differ from it.

## `programs`

```text
clipasm programs [NAME]
```

With no `NAME`, this command lists every built-in program in deterministic
categories. With `NAME`, it prints the terminal reference for that exact
built-in. The reference includes its call shape, inputs, parameters, defaults,
outputs, and binding behavior. It also includes the body contract, example, and
full guide URL. An unknown name fails with `E_UNKNOWN_BUILTIN_PROGRAM`.

`programs` always describes programs built into the installed ClipAsm binary.
It never inspects a project, source file, imported program, media asset, FFmpeg,
or FFprobe, and it does not require a repository checkout. See the generated
[built-in program index](programs/index.md) for the browsable reference.

## `explain`

```text
clipasm explain <CODE>
```

`explain` looks up one built-in ClipAsm diagnostic code, such as
`E_UNKNOWN_PROGRAM`. It prints the title, category, explanation, common causes,
recommended actions, and retry guidance. It also prints a link to the relevant
reference page. The code identifies the diagnostic class. Its original error
message and source location provide the instance-specific context.

This command reads only the diagnostic catalog compiled into the installed
binary. It never parses source, discovers a project, opens media, or inspects
FFmpeg, FFprobe, or external programs, and it does not require a repository
checkout. Unknown codes fail with a dedicated diagnostic and direct readers to
the [diagnostic index](../diagnostics/index.html).

For a complete, searchable list of built-in diagnostics, see the
[diagnostics reference](../diagnostics/index.html).

## Projects and source selection

The `validate`, `inspect`, and `render` commands accept an optional native
`.clipasm` source program:

```text
clipasm <COMMAND> [OPTIONS] [SOURCE]
```

When you omit `SOURCE`, ClipAsm searches the current directory and then each
parent directory for the nearest `clipasm.toml`. The manifest is strict:

```toml
[project]
entrypoint = "main.clipasm"

[render]
cache = "persistent"
materialization = "all"
```

`project.entrypoint` is a forward-slash relative `.clipasm` path
resolved from the manifest directory. Unknown fields, absolute paths,
backslashes, drive-style prefixes, and paths containing `.` or `..` are
rejected. A discovered manifest symlink must resolve to a regular file. A
broken nearer manifest path causes an error. ClipAsm does not continue the
search in a parent project. An explicit `SOURCE` remains a
standalone invocation and does not read an ambient project manifest.

Project renders keep persistent state under `.clipasm/` at the manifest root,
even when the entrypoint is in a nested directory. Explicit standalone sources
keep the existing source-adjacent cache location.

`render.cache` accepts `"persistent"` or `"none"` and defaults to
`"persistent"` when `[render]` is absent. Persistent mode reads verified cache
entries and retains newly rendered working artifacts. None mode does not read,
create, change, or delete persistent cache entries. It still materializes
working artifacts in a private temporary directory, deletes intermediates after
their final consumer, and removes the directory when the render ends. Override
the project setting for one invocation with `clipasm render --cache MODE`.

`render.materialization` is independent of cache retention. It accepts `"all"`
or `"fused"` and defaults to `"all"`. All mode materializes every reached
prepared node, matching the original execution model. Fused mode combines
compatible FFmpeg primitives that lead to one materialized endpoint into one
filter graph. Stream-disjoint picture and Audio consumers may share a region,
but duplicated physical streams remain materialized so fusion does not add
duration-scaled buffering. Temporal joins materialize their inputs for the same
reason. Cache hits, external programs, operations that require their own FFmpeg
input behavior, and branches with different materialized endpoints remain
artifact boundaries. Override the project setting for one invocation with
`clipasm render --materialization MODE`.

The selected source file and paths authored inside it resolve according to the
source-unit rules in the [language reference](language/index.md). Paths supplied
through CLI options resolve from the caller's working directory.

## Root bindings

`validate`, `inspect`, and `render` accept repeatable bindings for declarations
on the root source program:

| Option | Meaning |
| --- | --- |
| `--video-input NAME=VIDEO_PATH` | Bind one declared root `Video` input. |
| `--audio-input NAME=AUDIO_PATH` | Bind one declared root `Audio` input. |
| `--arg NAME=VALUE` | Bind one declared root scalar parameter. |

Names must match declarations exactly. Duplicate, unknown, missing, or
type-incompatible bindings are errors. Media and `File` paths supplied through
these options resolve from the working directory.

Binding options work the same way for validation, inspection, and rendering:

```console,ignore
clipasm validate template.clipasm \
  --video-input source=footage.mp4 \
  --arg range=1s..3s \
  --arg count=2

clipasm render template.clipasm \
  --video-input source=footage.mp4 \
  --arg range=1s..3s \
  --arg count=2 \
  --output final.mp4
```

CLI paths resolve from the caller's working directory. Authored paths resolve
from the source file that contains them.

## `validate`

```text
clipasm validate [OPTIONS] [SOURCE]
```

`validate` parses and checks the complete linked source package, evaluates its
stack programs, and infers every domain available from authored data. It does
not open media, invoke FFmpeg or FFprobe, or execute external programs.

Use it as the first check while editing:

```console,ignore
clipasm validate
```

Successful output reports the semantic value count. It also reports one of
these root result summaries:

| Root result | Success summary |
| --- | --- |
| One Video with an authored domain | exact frame count |
| One Video whose domain depends on media | duration resolves during preflight |
| One Audio output | output type |
| Zero or multiple outputs | output count |

## `inspect`

```text
clipasm inspect [OPTIONS] [SOURCE]
```

`inspect` performs the same pure compilation work and serializes the compiled
semantic program as JSON. By default it writes JSON to standard output.

```console,ignore
clipasm inspect
```

Use `-o` or `--output` to write a new file. Create the parent directory first
when it does not already exist. The destination must not already exist.

Inspection JSON is a versioned downstream view of compiled semantics. It is not
canonical source or an authoring format. Consumers must check `format_version`.
See
[Machine-readable contracts](machine-contracts.md#compiled-inspection-json).

## `render`

```text
clipasm render [OPTIONS] [SOURCE]
```

`render` compiles the source, performs preflight, executes the prepared plan,
verifies produced artifacts, and publishes an MP4 and sibling versioned manifest.
See [Machine-readable contracts](machine-contracts.md#render-manifest) before
consuming that JSON.

```console,ignore
clipasm render
```

The root source may declare `config.output`. Override it with `-o` or
`--output`. An override resolves from the caller's working directory. Rendering
requires an output path from one of those sources and exactly one publishable
`Video` output. It may inspect media, invoke FFmpeg and FFprobe, and execute
reachable external programs as trusted native code.

Use `--cache persistent` or `--cache none` to override cache retention for one
render. This option also applies to explicit standalone sources. In `none`
mode, the reused-artifact count is zero. Use `--materialization all` or
`--materialization fused` independently to select intermediate execution. The
render report and manifest record reused artifacts separately from rendered
jobs, so one fused region counts as one rendered job.

## Help and version

Use `-h` or `--help` with the root command or a subcommand, and `-V` or
`--version` on the root command:

```console,ignore
clipasm render --help
clipasm --version
```

## Related documentation

- [Check a program before rendering](../guides/validate-and-inspect.md)
- [Supply root inputs and parameters](../guides/root-inputs-and-parameters.md)
- [Troubleshooting](../guides/troubleshooting.md)
- [Language reference](language/index.md)
