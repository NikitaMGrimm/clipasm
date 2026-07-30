# Troubleshooting

Every ordinary ClipAsm error includes a diagnostic code. Explain the code with
`clipasm explain <CODE>` for a concise explanation. You can also search the
[diagnostic reference](../diagnostics/index.html) for the complete catalog.
The sections below follow common symptoms. The diagnostic reference contains
the full advice and retry guidance for each code.

## Diagnostic workflow

1. Run `clipasm validate SOURCE` to separate source and binding problems from
   media, tool, and execution problems.
2. If validation fails, inspect the first reported source location.
3. If the diagnostic has a code, run `clipasm explain <CODE>`.
4. Correct the source or binding problem.
5. Run `clipasm validate SOURCE` again.
6. When validation succeeds, run `render`. Rendering repeats source checks and
   then reports reachable media, tool, external-process, cache, or publication
   problems.
7. Use `inspect` only when the compiled graph or JSON integration is itself the
   question. Rendering does not require `inspect`.

## The source does not validate

Run:

```console
clipasm validate path/to/program.clipasm
```

Start at the first reported source location. Common causes include invalid
declaration order, an unknown program or argument, and a missing stack input.
Other causes include a type mismatch, an invalid import, or an output-name
dependency cycle.

Validation checks the complete linked package, including imported programs that
the root does not call. An unused import can therefore make validation fail.
Correct or remove the invalid imported source rather than expecting reachability
to hide it.

Consult the [language reference](../reference/language/index.md) for exact
syntax and the [stack-binding reference](../reference/language/stack-binding.md)
for binding rules. The [parsing and source](../diagnostics/index.html#parsing-and-source),
[imports and declarations](../diagnostics/index.html#imports-and-declarations),
and [types and stack](../diagnostics/index.html#types-and-stack) diagnostic
sections group the corresponding failures.

## A root input or parameter is missing

Every command that compiles the root source requires root declarations without
defaults. Supply the required `input` and `param` values:

```console
clipasm validate path/to/program.clipasm \
  --video-input video=path/to/input.mp4 \
  --arg count=2
```

Binding names are case-sensitive and must match the declarations. Repeat
`--video-input`, `--audio-input`, and `--arg` for multiple bindings. CLI media
and `File` paths resolve from the current working directory.

See [Supply root inputs and parameters](root-inputs-and-parameters.md).

## Validation defers a duration

A message that duration resolves during preflight is not an error. Compilation
does not open authored media, so a file-backed source may not yet have an exact
frame or sample count.

Render the program when you are ready for ClipAsm to resolve and probe reachable
media:

```console
clipasm render path/to/program.clipasm
```

## A media file cannot be found

Check which component authored the path:

- Paths in a `.clipasm` file resolve from that source file's directory.
- Import paths resolve from the importing source file.
- CLI media and `File` bindings resolve from the working directory.
- An output override resolves from the working directory.

Imported programs keep their own path base. Moving only the root source or
changing the working directory does not rebase paths in an imported source
file.

See [preflight and media diagnostics](../diagnostics/index.html#preflight-and-media)
when the reported code concerns an unreadable or unsuitable asset.

## FFmpeg or FFprobe is unavailable

`validate` and `inspect` do not require media tools. Rendering requires both
`ffmpeg` and `ffprobe` on `PATH`:

```console
ffmpeg -version
ffprobe -version
```

If ClipAsm cannot find installed commands, check your environment. Make sure
that `PATH` includes the corresponding executables.

See [preflight and media diagnostics](../diagnostics/index.html#preflight-and-media)
for tool discovery and capability failures.

## FFmpeg lacks a required capability

ClipAsm checks the encoders, muxers, and filters required by the reachable
work needed for the output. Install an FFmpeg build that provides the named
capability. Alternatively, remove the operation that requires that capability.

Capabilities needed only by unreachable operations do not reject the render.
External programs are responsible for any additional FFmpeg features they
invoke themselves.

## Rendering has no output path

The root source can declare `config.output`, or the caller can provide an
override:

```console
clipasm render path/to/program.clipasm \
  --output local/result.mp4
```

The destination must use the `.mp4` extension. ClipAsm also requires exactly one
publishable `Video` among the root program's ordered outputs.

## ClipAsm rejects the output or manifest destination

ClipAsm transactionally replaces existing regular MP4 and manifest files while
preserving them if publication fails. It rejects unsafe destination collisions.
Choose a different output path if a reachable input asset occupies either
destination. Do the same for an external executable or an incompatible
filesystem object.

Do not point output at a source asset. Publication writes both the MP4 and
`<output>.manifest.json`.

See [rendering and publication diagnostics](../diagnostics/index.html#rendering-and-publication)
for the reported destination or publication code.

## An external program fails or hangs

External programs are trusted native code. ClipAsm does not sandbox them or set
an execution timeout. Review the external declaration, executable, scripts, and
declared file arguments before rendering.

Run `validate` and `inspect` first. These commands do not execute the external
process. If rendering fails, reproduce the problem with the smallest trusted
project. Then inspect the process's reported failure. Use the operating system's
normal controls to stop a process that hangs.

See [Review and run an external program](external-programs.md) and
[External programs and the trust boundary](../concepts/external-programs-and-trust.md).
The [external-program diagnostics](../diagnostics/index.html#external-programs)
section explains protocol and process failures.

## ClipAsm does not reuse a cached artifact

Cache reuse requires matching semantic, prepared, tool, and artifact identities.
Changes to source meaning or media bytes can produce a cache miss. Changes to
declared external files or project settings can also produce a miss. FFmpeg and
FFprobe build changes can have the same result.

A cache miss is not a correctness failure. ClipAsm renders the missing artifact
and stores a verified replacement. Do not edit cache artifacts or sidecars by
hand.

For a cache lock or filesystem error, use the
[cache and filesystem diagnostics](../diagnostics/index.html#cache-and-filesystem)
section to determine whether retrying is appropriate.

## Inspection output differs from your expectations

`inspect` prints compiled JSON. It does not print source code, a render plan, or
a rendered preview. Focus on graph relationships such as `nodes`, `outputs`,
and `named_values`. Source metadata, hashes, and format details can change with
the internal serialization.

Use the [pipeline explanation](../concepts/pipeline.md) to distinguish compiled
semantics from preflight and rendering.

## ClipAsm reports an internal diagnostic

An [internal-contract diagnostic](../diagnostics/index.html#internal) usually
means user input exposed a ClipAsm defect rather than a source mistake. Preserve
the diagnostic code, ClipAsm version, safe reproduction steps, and the original
output. Do not delete caches or generated state unless that code's explanation
specifically recommends it.

Report a minimal reproduction through the repository's
[issue tracker](https://github.com/NikitaMGrimm/clipasm/issues), but do not post
private source, media, credentials, or sensitive paths. Use the private security
reporting route below when the failure may have security impact.

## Reporting a possible security issue

Do not post exploit details or sensitive inputs in a public issue. Follow the
repository's [security policy](https://github.com/NikitaMGrimm/clipasm/blob/main/SECURITY.md).
