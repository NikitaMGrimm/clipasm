# Troubleshooting

Start with `validate`, then move to `inspect`, and render only when the source is
structurally correct. This separates language and binding problems from media,
tool, and execution problems.

## The source does not validate

Run:

```console
cargo run -- validate path/to/program.clipasm
```

Use the first reported source location as the starting point. Common causes
include invalid declaration order, an unknown program or argument, a missing
stack input, a type mismatch, an invalid import, or an output-name dependency
cycle.

Validation checks the complete linked package, including imported programs that
the root does not call. An unused import can therefore make validation fail.
Correct or remove the invalid imported source rather than expecting reachability
to hide it.

Consult the [language reference](../language-reference.md) for exact syntax and
binding rules.

## A root input or parameter is missing

Root `input` and `param` declarations without defaults must be supplied to every
command that compiles the root source:

```console
cargo run -- validate path/to/program.clipasm \
  --video-input video=path/to/input.mp4 \
  --arg count=2
```

Binding names are case-sensitive and must match the declarations. Repeat
`--video-input`, `--audio-input`, and `--arg` for multiple bindings. CLI media
and `File` paths resolve from the current working directory.

See [Supply root inputs and parameters](root-inputs-and-parameters.md).

## Validation succeeds but duration is deferred

A message that duration resolves during preflight is not an error. Compilation
does not open authored media, so a file-backed source may not yet have an exact
frame or sample count.

Render the program when you are ready for ClipAsm to resolve and probe reachable
media:

```console
cargo run -- render path/to/program.clipasm
```

## A media file cannot be found

Check which component authored the path:

- paths written in a `.clipasm` file resolve from that source file's directory;
- import paths resolve from the importing source unit;
- CLI media and `File` bindings resolve from the working directory;
- an output override resolves from the working directory.

Imported programs keep their own path base. Moving only the root source or
running the command from another directory does not rebase paths authored in an
imported source unit.

## FFmpeg or FFprobe is unavailable

`validate` and `inspect` do not require media tools. Rendering requires both
`ffmpeg` and `ffprobe` on `PATH`:

```console
ffmpeg -version
ffprobe -version
```

If the commands are installed but ClipAsm cannot find them, run ClipAsm from an
environment whose `PATH` includes the corresponding executables.

## FFmpeg lacks a required capability

ClipAsm checks the encoders, muxers, and filters required by the reachable
prepared graph. Install a build of FFmpeg that provides the named capability,
or change the reachable program so it does not require that operation.

Capabilities needed only by unreachable operations do not reject the render.
External programs are responsible for any additional FFmpeg features they
invoke themselves.

## Rendering has no output path

The root source can declare `config.output`, or the caller can provide an
override:

```console
cargo run -- render path/to/program.clipasm \
  --output local/result.mp4
```

The destination must use the `.mp4` extension. ClipAsm also requires exactly one
publishable `Video` among the root program's ordered outputs.

## The output or manifest destination is rejected

ClipAsm preserves existing files and rejects unsafe destination collisions.
Choose a different output path when the MP4, its sibling manifest, a reachable
input asset, an external executable, or an incompatible filesystem object
occupies the destination.

Do not point output at a source asset. Publication writes both the MP4 and
`<output>.manifest.json`.

## An external program fails or hangs

External programs are trusted native code and are not sandboxed or given a
ClipAsm execution timeout. Review the external declaration, executable, scripts,
and declared file arguments before rendering.

Run `validate` and `inspect` first; they do not execute the external process. If
rendering fails, reproduce the problem with the smallest trusted project and
inspect the process's reported failure. A hung process may need to be terminated
using the operating system's normal process controls.

See [Review and run an external program](external-programs.md) and
[External programs and the trust boundary](../concepts/external-programs-and-trust.md).

## A cached artifact is not reused

Cache reuse requires matching semantic, prepared, tool, and artifact identities.
Changes to source meaning, media bytes, declared external files, relevant
project settings, or FFmpeg and FFprobe builds can produce a cache miss.

A cache miss is not a correctness failure. ClipAsm renders the missing artifact
and stores a verified replacement. Do not edit cache artifacts or sidecars by
hand.

## Inspection output is surprising

`inspect` prints compiled semantic JSON, not canonical source, a prepared plan,
or a rendered preview. Focus on graph relationships such as `nodes`, `outputs`,
and `named_values`; source metadata, hashes, and format details may change as the
internal serialization evolves.

Use the [pipeline explanation](../concepts/pipeline.md) to distinguish compiled
semantics from preflight and rendering.

## Reporting a possible security issue

Do not post exploit details or sensitive inputs in a public issue. Follow the
repository's [security policy](https://github.com/NikitaMGrimm/clipasm/blob/main/SECURITY.md).
