# Command-line reference

ClipAsm provides four commands: `init`, `validate`, `inspect`, and `render`.
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

The two forms are:

```console,ignore
clipasm init hello-video
clipasm init
```

The starter tree is exactly:

```text
.gitignore
README.md
main.clipasm
assets/
  morning.png
  meadow.png
  evening.png
```

`main.clipasm` has the canonical
[scenic-sequence source](https://github.com/NikitaMGrimm/clipasm/blob/main/examples/scenic-sequence.clipasm)
bytes and declares `clipasm 1`. It validates to 108 frames and publishes
`generated/scenic-sequence.mp4`. Initialization does not invoke Git, render,
or media tools, and it does not contact the network.

For a named path, success is:

```console,ignore
$ clipasm init hello-video
Created ClipAsm project at `hello-video`.

Next:
  cd "hello-video"
  clipasm validate main.clipasm
  clipasm render main.clipasm
```

When the target is the current directory, the `cd` line is omitted. For a path
that cannot be represented as a portable shell command, the output instead
tells you to enter the created directory before running the two exact ClipAsm
commands. The generated files are ordinary, unmanaged project files: ClipAsm
does not update or take ownership of them later. Future releases may generate
different starter bytes, but they do not alter existing projects. The language
declaration in this starter remains `clipasm 1`.

## Common source argument

The remaining commands accept one native `.clipasm` source program:

```text
clipasm <COMMAND> [OPTIONS] <SOURCE>
```

The source file and paths authored inside it resolve according to the source
unit rules in the [language reference](../language-reference.md). Paths supplied
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

## `validate`

```text
clipasm validate [OPTIONS] <SOURCE>
```

`validate` parses and checks the complete linked source package, evaluates its
stack programs, and infers every domain available from authored data. It does
not open media, invoke FFmpeg or FFprobe, or execute external programs.

Use it as the first check while editing:

```console,ignore
clipasm validate main.clipasm
```

Successful output reports the semantic value count and either an exact frame
count or that duration will resolve during preflight.

## `inspect`

```text
clipasm inspect [OPTIONS] <SOURCE>
```

`inspect` performs the same pure compilation work and serializes the compiled
semantic program as JSON. By default it writes JSON to standard output.

```console,ignore
clipasm inspect main.clipasm
```

Use `-o` or `--output` to write a new file. Create the parent directory first
when it does not already exist. The destination must not already exist.
Inspection JSON is a downstream view of compiled semantics, not canonical source
or a stable authoring format.

## `render`

```text
clipasm render [OPTIONS] <SOURCE>
```

`render` compiles the source, performs preflight, executes the prepared plan,
verifies produced artifacts, and publishes an MP4 and sibling manifest.

```console,ignore
clipasm render main.clipasm
```

The root source may declare `config.output`. Override it with `-o` or
`--output`; an override resolves from the caller's working directory. Rendering
requires exactly one publishable `Video` output. It may inspect media, invoke
FFmpeg and FFprobe, and execute reachable external programs as trusted native
code.

## Help and version

Use `-h` or `--help` with the root command or a subcommand, and `-V` or
`--version` on the root command:

```console,ignore
clipasm render --help
clipasm --version
```

## Related documentation

- [Validate and inspect a program](../guides/validate-and-inspect.md)
- [Supply root inputs and parameters](../guides/root-inputs-and-parameters.md)
- [Troubleshooting](../guides/troubleshooting.md)
- [Language reference](../language-reference.md)
