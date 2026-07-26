# Import and call a source program

Use an import when a reusable operation belongs in its own `.clipasm` source
unit. This guide runs the committed wrapper
`examples/imported-program.clipasm`, which calls
`examples/programs/polish.clipasm`.

Run all commands from the repository root.

## Define the reusable program

The imported program declares its callable interface and body:

```clipasm
clipasm 1

input video: Video
param percent: Integer = 6

zoom($video, $percent)
wobble(2)
```

The `video` input and `percent` parameter are local to each invocation.
The source program returns its ordered final owned values to its caller.

## Import it under a local alias

The wrapper imports the file and calls the alias like an ordinary program:

```clipasm
clipasm 1

config {
    video {
        width = 320
        height = 180
        fps = 24
    }
    output = "generated/imported-program.mp4"
}

import "programs/polish.clipasm" as polish

video("assets/gentle-motion.mkv", contain)
polish(10)
```

Import paths resolve relative to the source unit containing the declaration, so
the wrapper finds `programs/polish.clipasm` below the same `examples/`
directory that contains the wrapper. The required alias is local to the wrapper
and is not re-exported. The imported invocation also has its own local stack and
namespace; local inputs, parameters, and output names do not escape it.

Here `video(...)` produces the Video that binds to `polish`'s declared input,
and `10` overrides the `percent` default.

## Validate and render the wrapper

```console
$ clipasm validate examples/imported-program.clipasm
valid: 4 semantic value(s), duration resolves during preflight

```

Validation links and checks the source package without rendering. Then render
it:

```console,ignore
clipasm render examples/imported-program.clipasm
```

The render command publishes the two-second source as
`examples/generated/imported-program.mp4` because the authored output path
resolves from the wrapper source unit.

For exact import, namespace, and path rules, see
[Imports](../language-reference.md#imports). Read
[Source programs and imports](../concepts/source-programs-and-imports.md) for
the broader model and the [examples catalog](../examples.md#imported-program)
for the canonical command listing.
