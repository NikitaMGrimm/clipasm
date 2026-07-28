# Check a program before rendering

Use `validate` for a fast source check while editing. If you need compiled graph
data for debugging or tooling, export it with `inspect`. Neither command opens
media, runs FFmpeg or FFprobe, or executes an external program.

## Before you start

Run the steps from an initialized project containing `main.clipasm`. In a
repository checkout, substitute `examples/scenic-sequence.clipasm`.

## 1. Validate the source

```console,ignore
clipasm validate main.clipasm
```

A successful validation confirms that ClipAsm can parse the complete source
package, resolve imports and program calls, bind stack inputs, check types, and
calculate every duration available from authored data.

It does **not** confirm that media files exist or that rendering tools are
installed. A video-file source may therefore validate with a message that its
duration will resolve later during rendering.

When validation fails, fix the first reported source location and run the
command again. Use `clipasm explain <CODE>` when the diagnostic needs more
context. Continue when validation reports a successful frame count or a
duration that will resolve during preflight.

## 2. Inspect compiled JSON when needed

Skip this step unless you need to examine graph structure or feed compiled data
to a tool.

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
Open `local/compiled.json` and confirm that its `format_version` is supported.
Choose a new destination or remove the old debugging file before repeating the
command; `inspect` does not overwrite it.

## 3. Render the checked program

```console,ignore
clipasm render main.clipasm
```

Rendering repeats the source checks, then opens reachable media, checks the
required tools, and creates the output. You do not need to run `validate` or
`inspect` first. Open the configured MP4 after the command succeeds.

See [From source to published video](../concepts/pipeline.md) for the phase
model and [Troubleshooting](troubleshooting.md) for common failures.
