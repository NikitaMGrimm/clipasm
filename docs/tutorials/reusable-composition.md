# Build a reusable composition

This tutorial adds named values, references, and an inline input body to a
safe project you control. Start in the initialized project from the
[scenic-sequence tutorial](scenic-sequence.md), or create a new one now:

```console,ignore
clipasm init reusable-video
cd reusable-video
```

There is no repository checkout or directory copying in this workflow. The
starter supplies the three image assets used below. Create a new file named
`composition.clipasm` in your editor, so your existing `main.clipasm` remains
available for comparison.

## Start with the project and two named clips

Set these project properties, using a separate output:

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
```

Then add two `clip` blocks:

```clipasm
clip {
    image("assets/morning.png", 1s, contain)
    zoom_in
} as opening

clip {
    image("assets/evening.png", 1s, contain)
} as closing
```

Each block produces one Video and gives it an immutable name without leaving an
occurrence on the outer stack. `opening` is zoomed; `closing` is unchanged.
The exact lowering rules are in
[`clip` sugar](../reference/language/names-blocks-and-clip.md#clip-sugar).

## Reuse a named value and build an inline input

Append the composition:

```clipasm
$opening
flash_cut(
    before=$opening,
    after={
        image("assets/meadow.png", 1s, contain)
        zoom_in(2%)
    },
    duration=200ms,
)
$closing
concat
```

The standalone `$opening` starts the output sequence. The `before=$opening`
argument reads the same immutable named value again; it does not consume the
standalone occurrence. The `after` body starts with an empty stack, produces one
meadow Video, and supplies that Video to `flash_cut`. Finally, `$closing` and
`concat` make one ordered result.

The snippets above make the complete file, so no repository checkout is
needed. The
[references and output names](../reference/language/names-blocks-and-clip.md#references-and-output-names)
and
[arguments and stack binding](../reference/language/stack-binding.md#arguments-and-stack-binding)
sections own the exact rules.

## Validate and render your composition

```console,ignore
clipasm validate composition.clipasm
clipasm render composition.clipasm
```

Validation reports 96 frames. Rendering writes
`generated/reusable-composition.mp4`, a four-second Video at 24 fps. The middle
transition overlaps its inputs; the composition still has the authored
four-second result.

## Experiment safely

Change only `duration=200ms` to `duration=320ms`, save, then validate and
render again:

```console,ignore
clipasm validate composition.clipasm
clipasm render composition.clipasm
```

The program remains 96 frames because the transition changes the overlap within
the same composition. Compare the rendered transition, then keep or revert the
edit in your own file.

## What you learned

You prepared named values with `clip`, read an immutable value more than once,
provided an isolated inline input body, and assembled an ordered Video with
`flash_cut` and `concat`. Choose another committed program in the
[examples catalog](../examples.md), or use the
[built-in program reference](../reference/programs/index.md) for exact call
shapes and the [stack-binding reference](../reference/language/stack-binding.md)
for stack semantics.
