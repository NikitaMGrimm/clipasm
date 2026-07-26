# Understand the scenic sequence

In this tutorial, you will read the scenic sequence from declarations to
executable statements, inspect the graph it describes, and make a duration
change in an ignored practice copy. The result is the same 4.5-second sequence
rendered in [the first-render guide](../getting-started/first-render.md).

Run every command from the repository root.

## Read the program

Open `examples/scenic-sequence.clipasm` in your editor. The complete source is:

```clipasm
clipasm 1

config {
    video {
        width = 320
        height = 180
        fps = 24
    }
    output = "generated/scenic-sequence.mp4"
}

glue {
    image("assets/morning.png", 1500ms, contain)
    image("assets/meadow.png", 1500ms, contain)
    image("assets/evening.png", 1500ms, contain)
}
```

The first line selects version 1 of the native language:

```clipasm
clipasm 1
```

Every `.clipasm` source file begins with this line. File declarations follow it,
before any executable statements.

## Set the project video

The `config` declaration sets the project's Video properties and publication
path:

```clipasm
config {
    video {
        width = 320
        height = 180
        fps = 24
    }
    output = "generated/scenic-sequence.mp4"
}
```

The project is 320x180 at 24 frames per second. The authored output path
resolves from the source file, so this program publishes under
`examples/generated/`.

Only the root source unit may declare project configuration and publication
settings. See
[configuration and declarations](../language-reference.md#configuration-and-declarations)
for the complete rules.

## Produce three Video values

The first executable statement is the `glue` body:

```clipasm
glue {
    image("assets/morning.png", 1500ms, contain)
    image("assets/meadow.png", 1500ms, contain)
    image("assets/evening.png", 1500ms, contain)
}
```

Each `image` call produces one Video. Its arguments are:

1. a file path, resolved from `examples/scenic-sequence.clipasm`;
2. the image duration, here 1,500 milliseconds;
3. the `contain` fit mode.

The committed images already match the 320x180 project, but the explicit fit
mode makes the intended behavior visible in the example.

Statements run in order. The `glue` body starts without owned values, collects
the three Video values in that order, and concatenates its homogeneous
remainder. The result is one Video lasting 4.5 seconds.

The [built-in program table](../language-reference.md#built-in-programs)
defines `image` and `glue`. Use it as the reference rather than treating this
tutorial as a complete signature listing.

## Validate and inspect

Validate the program:

```console
clipasm validate examples/scenic-sequence.clipasm
```

Success ends with:

```text
valid: 4 semantic value(s), 108 frame(s)
```

The 108 frames are the three 1.5-second sections on a 24-frame-per-second
project timeline.

Now inspect the compiled JSON document:

```console
clipasm inspect examples/scenic-sequence.clipasm
```

The command writes JSON to standard output. At a high level, its `nodes` show
three image operations followed by their concatenation, and `outputs` selects
the combined Video. The document also carries diagnostic source metadata and
identity hashes; focus here on the graph relationships rather than copying
incidental fields.

## Render the sequence

Render once you are satisfied with validation:

```console
clipasm render examples/scenic-sequence.clipasm
```

The expected result is `examples/generated/scenic-sequence.mp4`: morning,
meadow, and evening, each shown for 1.5 seconds. A successful render also writes
its sibling manifest and reusable cache entries.

## Exercise: shorten the middle scene

Keep the committed example unchanged by copying the examples tree into the
ignored `local/` directory:

```console
mkdir -p local
cp -R examples local/scenic-practice
```

In `local/scenic-practice/scenic-sequence.clipasm`, change the meadow duration
from `1500ms` to `1s`. Then validate the practice copy:

```console
clipasm validate local/scenic-practice/scenic-sequence.clipasm
```

The result is now 96 frames: four seconds at 24 frames per second. Render it
with:

```console
clipasm render local/scenic-practice/scenic-sequence.clipasm
```

Because authored asset and output paths resolve from the copied source, its
inputs, output, manifest, and cache all remain within the ignored practice
tree.

## What you learned

You have seen how a ClipAsm file:

- selects its language version and declares project configuration;
- resolves authored paths from the source that contains them;
- produces Video values with `image`;
- uses statement order and `glue` to form one sequence;
- moves from pure validation and inspection to rendering.

Continue with [build a reusable composition](reusable-composition.md) to name
and reuse graph values, supply an inline input body, and assemble a richer
stack. For exact syntax and stack behavior, use the
[language reference](../language-reference.md).
