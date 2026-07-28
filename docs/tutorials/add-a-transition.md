# Add a flash-cut transition

In this tutorial you will extend the composition pattern from the scenic
sequence with one transition. The result is a three-second MP4 with a flash
between the opening and meadow scenes, followed by the closing scene.

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
    image("assets/meadow.png", 1s, contain)
    zoom_in(2%)
} as meadow

clip {
    image("assets/evening.png", 1s, contain)
} as closing
```

This is the pattern from the previous tutorial: each `clip` combines its body,
and each `as` clause preserves the result under an immutable name. None of the
three clips enters the outer stack until it is referenced.

## 3. Assemble a sequence without a transition

Append:

```clipasm
$opening
$meadow
$closing
concat
```

Validate the file:

```console,ignore
clipasm validate transition.clipasm
```

The three references leave three Videos on the outer stack, and `concat`
combines them into a three-second result.

## 4. Add the transition

Insert `flash_cut(200ms)` immediately after `$meadow`:

```clipasm
$opening
$meadow
flash_cut(200ms)
$closing
concat
```

`flash_cut` needs a `before` Video and an `after` Video. With those inputs
omitted, it consumes the two nearest Videos from the stack: `$opening` first,
then `$meadow`. It leaves their two-second transition in the same place:

```text
$opening  -> [opening]
$meadow   -> [opening, meadow]
flash_cut -> [opening-to-meadow]
$closing  -> [opening-to-meadow, closing]
concat    -> [finished video]
```

The closing clip is referenced only after the transition, so it is not one of
the transition inputs. `concat` joins the transition result and closing clip.

## 5. Validate and render

```console,ignore
clipasm validate transition.clipasm
clipasm render transition.clipasm
```

Validation reports 72 frames. The result is three seconds at 24 fps. Open
`generated/reusable-composition.mp4` and check for one white flash between the
opening and meadow, followed by a normal cut to the closing scene.

## What you learned

You built three named clips, referenced them in playback order, and let
`flash_cut` consume its two inputs from the stack. You then added the closing
clip and combined the remaining sequence with `concat`.

See [Composition forms](../reference/language/composition-forms.md) for `clip`,
names, and references; [Stack binding](../reference/language/stack-binding.md)
for implicit input behavior; and
[`flash_cut`](../reference/programs/flash_cut.md) for the transition contract.
