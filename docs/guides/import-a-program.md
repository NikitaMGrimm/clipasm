# Import and call a source program

Move a reusable operation into its own `.clipasm` file when it has a useful
callable interface. This guide uses the committed wrapper
`examples/imported-program.clipasm` and program
`examples/programs/polish.clipasm`.

Run the commands from the repository root.

## Define the reusable program

```clipasm
clipasm 1

input video: Video
param by: Number = 6%

zoom_in($video, $by)
```

This program accepts one Video and an optional zoom amount. Its final values are
returned to the caller in order.

## Import it under a local name

The wrapper contains:

```clipasm
import "programs/polish.clipasm" as polish

video("assets/gentle-motion.mkv", contain)
polish(10%)
```

The import path is relative to the wrapper file. `polish` is a local alias; it
is not re-exported to another source file. The call takes the Video already on
the stack and overrides the default `by` parameter with `10%`.

Each invocation has its own local stack and names. Values leave the imported
program only through its ordered outputs.

## Validate and render

```console
$ clipasm validate examples/imported-program.clipasm
valid: 3 semantic value(s), duration resolves during preflight

```

The deferred duration is expected because validation does not open the video
file. Render when ready:

```console,ignore
clipasm render examples/imported-program.clipasm
```

The configured output is written to
`examples/generated/imported-program.mp4`, relative to the wrapper file.

See [Imports](../reference/language/imports-and-external-programs.md#imports) for
exact rules and [Source programs and imports](../concepts/source-programs-and-imports.md)
for the mental model.
