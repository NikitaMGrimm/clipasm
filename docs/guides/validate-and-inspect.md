# Check a program before rendering

Use `validate` for a fast source check while editing. It checks the complete
source package without opening media, running FFmpeg or FFprobe, or executing an
external program.

## Before you start

Run the steps from an initialized project containing `main.clipasm`. In a
repository checkout, substitute `examples/scenic-sequence.clipasm`.

## 1. Validate the source

```console,ignore
clipasm validate main.clipasm
```

A successful validation confirms that ClipAsm can parse the package, resolve
imports and calls, bind stack inputs, check types, and calculate every duration
available from authored data.

It does not confirm that media files exist or that rendering tools are
installed. A video-file source may therefore validate with a duration that will
resolve later during rendering.

## 2. Resolve a diagnostic

When validation fails, start at the first reported source location. If the
diagnostic includes a code, ask for its longer explanation:

```console,ignore
clipasm explain E_ENTRYPOINT_OUTPUT_COUNT
```

Edit the source and validate again. Continue when validation reports a
successful frame count or a duration that will resolve during preflight.

## 3. Render the checked program

```console,ignore
clipasm render main.clipasm
```

Rendering repeats the source checks, then opens reachable media, verifies the
required tools, and creates the output. Running `validate` first is useful while
editing but never required before `render`.

See [From source to published video](../concepts/pipeline.md) for the phase
model, [Inspect compiled JSON](inspect-compiled-json.md) for tooling data, and
[Troubleshooting](troubleshooting.md) for common failures.
