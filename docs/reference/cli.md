# Command-line reference

ClipAsm provides three commands: `validate`, `inspect`, and `render`. Run
commands from the directory whose relative CLI paths you intend to use.

```console
cargo run -- --help
```

When using an installed binary, replace `cargo run --` with `clipasm`.

## Common source argument

Every command accepts one native `.clipasm` source program:

```text
clipasm <COMMAND> [OPTIONS] <SOURCE>
```

The source file and paths authored inside it resolve according to the source
unit rules in the [language reference](../language-reference.md). Paths supplied
through CLI options resolve from the caller's working directory.

## Root bindings

All three commands accept repeatable bindings for declarations on the root
source program:

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

```console
cargo run -- validate examples/scenic-sequence.clipasm
```

Successful output reports the semantic value count and either an exact frame
count or that duration will resolve during preflight.

## `inspect`

```text
clipasm inspect [OPTIONS] <SOURCE>
```

`inspect` performs the same pure compilation work and serializes the compiled
semantic program as JSON. By default it writes JSON to standard output.

```console
cargo run -- inspect examples/scenic-sequence.clipasm
```

Use `-o` or `--output` to write a new file. Create the parent directory first
when it does not already exist:

```console
cargo run -- inspect examples/scenic-sequence.clipasm \
  --output local/scenic-sequence.json
```

The destination must not already exist. Inspection JSON is a downstream view of
compiled semantics, not canonical source or a stable authoring format.

## `render`

```text
clipasm render [OPTIONS] <SOURCE>
```

`render` compiles the source, performs preflight, executes the prepared plan,
verifies produced artifacts, and publishes an MP4 and sibling manifest.

```console
cargo run -- render examples/scenic-sequence.clipasm
```

The root source may declare `config.output`. Override it with `-o` or
`--output`:

```console
cargo run -- render examples/scenic-sequence.clipasm \
  --output local/scenic-sequence.mp4
```

An output override resolves from the caller's working directory. Rendering
requires exactly one publishable `Video` output. It may inspect media, invoke
FFmpeg and FFprobe, and execute reachable external programs as trusted native
code.

## Help and version

Use `-h` or `--help` with the root command or a subcommand:

```console
cargo run -- render --help
```

Use `-V` or `--version` on the root command:

```console
cargo run -- --version
```

## Related documentation

- [Validate and inspect a program](../guides/validate-and-inspect.md)
- [Supply root inputs and parameters](../guides/root-inputs-and-parameters.md)
- [Troubleshooting](../guides/troubleshooting.md)
- [Language reference](../language-reference.md)
