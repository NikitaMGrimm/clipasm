# Import and call a source program

Use a source program when an operation needs a reusable callable interface. This
guide creates a `polish` program and imports it under a local name. You will
then render its result without completing the ordered learning chapters.

## Before you start

Create a project so the starter image is available. Then enter the project:

```console,ignore
clipasm init imported-video
cd imported-video
```

Create a `programs` directory inside the project.

## 1. Define the program

Create `programs/polish.clipasm`:

```clipasm
{{#include ../../examples/programs/polish.clipasm}}
```

The file accepts one Video and an optional zoom amount. Its final Video returns
to the caller.

## 2. Create the composition

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

The import path is relative to `composition.clipasm`. `polish` is a local call
name. The call consumes the Video on the stack. It also overrides the program's
default `by` parameter.

## 3. Validate the package

```console,ignore
clipasm validate composition.clipasm
```

Validation checks both source files and reports 48 frames without opening the
PNG.

## 4. Render the result

```console,ignore
clipasm render composition.clipasm
```

## 5. Check the result

Open `generated/imported-program.mp4`. Confirm that the morning image zooms for
two seconds.

See [Imports](../reference/language/imports-and-external-programs.md#imports) for
exact path, alias, isolation, and cycle rules.
