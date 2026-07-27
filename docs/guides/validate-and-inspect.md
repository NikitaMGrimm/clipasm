# Validate and inspect a program

Use `validate` for a fast source check while editing. Use `inspect` only when
you need the compiled JSON structure for debugging or tooling. Neither command
opens media, runs FFmpeg or FFprobe, or executes an external program.

The examples below use `main.clipasm` in an initialized project. In a repository
checkout, use `examples/scenic-sequence.clipasm` instead.

## Validate first

```console,ignore
clipasm validate main.clipasm
```

A successful validation confirms that ClipAsm can parse the complete source
package, resolve imports and program calls, bind stack inputs, check types, and
calculate every duration available from authored data.

It does **not** confirm that media files exist or that rendering tools are
installed. A video-file source may therefore validate with a message that its
duration will resolve later during rendering.

When validation fails, fix the first reported source location before moving on.
Use `clipasm explain <CODE>` when the diagnostic needs more context.

## Inspect compiled JSON

```console,ignore
clipasm inspect main.clipasm
```

The command writes JSON to standard output. To create a new file instead:

```console,ignore
mkdir -p local
clipasm inspect main.clipasm --output local/compiled.json
```

The destination must not already exist.

The document is useful for checking:

- project Video and Audio settings;
- compiled operations and their inputs;
- known frame or sample counts;
- ordered outputs and named values;
- authored source origins;
- the configured publication path.

Inspection JSON is a compiled view, not `.clipasm` source, a render plan, or a
preview. Read `format_version` before consuming it in software; see
[Compiled inspection JSON](../reference/machine-contracts.md#compiled-inspection-json).

## Render when source is ready

```console,ignore
clipasm render main.clipasm
```

Rendering repeats the source checks, then opens reachable media, checks the
required tools, and creates the output. You do not need to run `validate` or
`inspect` first.

See [From source to published video](../concepts/pipeline.md) for the phase
model and [Troubleshooting](troubleshooting.md) for common failures.
