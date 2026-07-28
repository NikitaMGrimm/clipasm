# Import and call a source program

Move a reusable operation into its own `.clipasm` file when it has a useful
callable interface. In this guide you will create a `polish` program, import it
under a local name, and render its result.

## Before you start

Create and enter a project so the starter image is available:

```console,ignore
clipasm init imported-video
cd imported-video
```

Create a `programs/` directory inside the project.

## 1. Define the reusable program

Create `programs/polish.clipasm`:

```clipasm
clipasm 1

input video: Video
param by: Number = 6%

zoom_in($video, $by)
```

This file defines one callable source program. It accepts a Video and an
optional zoom amount. Its final values return to the caller in order.

## 2. Import it under a local name

Create `composition.clipasm` in the project root:

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

image("assets/morning.png", 2s, contain)
polish(10%)
```

The import path is relative to `composition.clipasm`. `polish` is a local alias;
it is not re-exported to another source file. The call takes the Video already
on the stack and overrides the default `by` parameter with `10%`.

Each invocation has its own stack, inputs, parameters, and names. Values leave
the imported program only through its ordered outputs.

## 3. Validate the package

```console,ignore
clipasm validate composition.clipasm
```

Validation checks both source files and reports 48 frames. It does not open the
PNG. If ClipAsm reports an import error, check that the path is relative to the
file containing the import and that the alias matches the call.

## 4. Render and verify the result

```console,ignore
clipasm render composition.clipasm
```

Open `generated/imported-program.mp4`. It should show the morning image zooming
in for two seconds.

See [Imports](../reference/language/imports-and-external-programs.md#imports) for
exact path, alias, and cycle rules. The repository's
[`examples/imported-program.clipasm`](https://github.com/NikitaMGrimm/clipasm/blob/main/examples/imported-program.clipasm)
shows the same pattern with a file-backed Video.
