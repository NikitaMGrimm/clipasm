# Add a flash-cut transition

In this tutorial you will extend the composition pattern from the scenic
sequence with one transition and one inline stack block. The result is a
four-second MP4 with a repeated opening, a flash cut, and a closing scene.

## Before you start

Complete [Build the scenic sequence](scenic-sequence.md) first. Stay in that
initialized project so the three images remain available. To start separately:

```console,ignore
clipasm init reusable-video
cd reusable-video
```

Create `transition.clipasm`. The starter images remain under `assets/`.

## 1. Configure a separate output

Begin with:

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

## 2. Define the reusable scenes

Append:

```clipasm
clip {
    image("assets/morning.png", 1s, contain)
    zoom_in
} as opening

clip {
    image("assets/evening.png", 1s, contain)
} as closing
```

This is the pattern from the previous tutorial: each `clip` combines its body,
and each `as` clause preserves the result under an immutable name.

## 3. Assemble a sequence without a transition

Append:

```clipasm
$opening
$opening
$closing
concat
```

Validate the file:

```console,ignore
clipasm validate transition.clipasm
```

The three references leave three Videos on the outer stack, and `concat`
combines them into a three-second result.

## 4. Replace one scene with a transition

Replace the second `$opening` with this `flash_cut` call:

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

The first `$opening` remains on the output sequence. `before=$opening` reads the
same immutable value again without consuming that outer occurrence.

`after` needs one Video, but creating its value takes two statements. The
`{ ... }` stack block groups those statements into one graph argument: `image`
leaves the meadow Video, and `zoom_in` replaces it with the transformed Video.
This is where a stack block earns its place: it packages a multi-step
computation as one inline input.

`flash_cut` combines its two one-second inputs into one two-second Video.
Finally, `$closing` adds the last scene and `concat` combines the three outer
values.

## 5. Validate and render

```console,ignore
clipasm validate transition.clipasm
clipasm render transition.clipasm
```

Validation reports 96 frames. The result is four seconds at 24 fps: one second
for the standalone opening, two seconds for the flash-cut result, and one second
for the closing. Open `generated/reusable-composition.mp4` and check that the
opening appears twice before the closing scene.

## What you learned

You reused a named Video, supplied explicit transition inputs, and used a stack
block where a multi-step computation had to become one inline graph argument.
Only then did you combine the outer sequence with `concat`.

See [Composition forms](../reference/language/composition-forms.md) for `clip`,
stack blocks, and names; [Stack binding](../reference/language/stack-binding.md)
for argument behavior; and
[`flash_cut`](../reference/programs/flash_cut.md) for the transition contract.
