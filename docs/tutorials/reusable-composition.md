# Build a reusable composition

This tutorial builds on the
[scenic sequence](scenic-sequence.md). You will use `clip` blocks to prepare
named Video values, reuse one value in more than one place, provide an inline
Video input, and concatenate the final stack. The committed result is a
four-second, 320x180 MP4.

Run every command from the repository root.

## Read the complete source

Open `examples/reusable-composition.clipasm`:

```clipasm
clipasm 1

config {
    video {
        width = 320
        height = 180
        fps = 24
    }
    output = "generated/reusable-composition.mp4"
}

clip {
    image("assets/morning.png", 1s, contain)
    zoom
} as opening

clip {
    image("assets/evening.png", 1s, contain)
} as closing

$opening
flash(
    before=$opening,
    after={
        image("assets/meadow.png", 1s, contain)
        wobble(2)
    },
    frames=5,
)
$closing
concat
```

The version and project configuration follow the same pattern as the scenic
sequence. The executable part introduces the new ideas.

## Prepare a named opening

The first `clip` block prepares the opening:

```clipasm
clip {
    image("assets/morning.png", 1s, contain)
    zoom
} as opening
```

Inside the block, `image` produces a one-second Video. The following `zoom`
uses the compatible Video owned by that body and returns the processed Video.

`as opening` gives the block's result an immutable name. A `clip` block is
language sugar for collecting a sequence through `glue`, naming its result, and
removing that result's stack occurrence. The value remains available through
`$opening`, but it is not yet waiting on the outer stack for final
concatenation.

The closing uses the same pattern without an effect:

```clipasm
clip {
    image("assets/evening.png", 1s, contain)
} as closing
```

For the exact lowering and stack rules, see
[`clip` sugar](../language-reference.md#clip-sugar).

## Reuse the opening

The standalone reference places the named opening into the executable
sequence:

```clipasm
$opening
```

References read immutable named values without consuming them. That allows the
same opening to be supplied again as an explicit input to `flash`:

```clipasm
flash(
    before=$opening,
```

The explicit `before` input reads the named value without consuming the
standalone opening already waiting on the caller's stack. Names identify
already-produced results; they do not change statement order or stack effects.
The normative rules are in
[references and output names](../language-reference.md#references-and-output-names).

## Build an inline input

The `after` input is produced by an inline body:

```clipasm
    after={
        image("assets/meadow.png", 1s, contain)
        wobble(2)
    },
```

An inline input body starts with an empty stack and must produce exactly one
value accepted by the input port. Here `image` creates the meadow Video and
`wobble(2)` processes it, so the body supplies one Video to `after`.

The last argument chooses a five-frame flash:

```clipasm
    frames=5,
)
```

`flash` returns its own Video result to the caller's stack.

## Concatenate the final stack

The last reference and call complete the program:

```clipasm
$closing
concat
```

At this point, the accessible Video values are ordered as:

1. the standalone `$opening`;
2. the Video returned by `flash`;
3. the standalone `$closing`.

`concat` consumes that homogeneous Video sequence in physical order and
produces the program's final Video. The
[arguments and stack binding reference](../language-reference.md#arguments-and-stack-binding)
describes the general binding rules.

## Validate, inspect, and render

Validate the committed source:

```console
cargo run -- validate examples/reusable-composition.clipasm
```

Success ends with:

```text
valid: 10 semantic value(s), 96 frame(s)
```

Inspect the compiled JSON document if you want to trace the named values,
references, and final output:

```console
cargo run -- inspect examples/reusable-composition.clipasm
```

The JSON includes diagnostic source metadata and identity hashes. Use the
`nodes`, `named_values`, and `outputs` relationships for this walkthrough
instead of copying incidental fields.

Render the program:

```console
cargo run -- render examples/reusable-composition.clipasm
```

The output is
`examples/generated/reusable-composition.mp4`, a four-second Video at 24 frames
per second. Cache counts may vary between runs.

## Exercise: lengthen the flash

Make an ignored practice copy:

```console
mkdir -p local
cp -R examples local/reusable-practice
```

In `local/reusable-practice/reusable-composition.clipasm`, change `frames=5` to
`frames=8`. Validate the change:

```console
cargo run -- validate local/reusable-practice/reusable-composition.clipasm
```

The program still validates to 96 frames because the change affects the flash
within the same overall composition. Render the practice copy and compare the
transition:

```console
cargo run -- render local/reusable-practice/reusable-composition.clipasm
```

The copied assets, generated output, manifest, and cache remain under the
ignored `local/reusable-practice/` tree.

## What you learned

You have used:

- `clip` blocks to prepare named values without leaving them on the outer
  stack;
- `as` and `$name` to retain and reuse immutable graph results;
- explicit arguments without consuming a caller stack occurrence;
- an isolated inline input body to produce one Video;
- `flash` and `concat` to build the final ordered composition.

Use the [examples catalog](../examples.md) to choose another runnable program,
or consult the [language reference](../language-reference.md) for exact
signatures, name behavior, and stack semantics.
