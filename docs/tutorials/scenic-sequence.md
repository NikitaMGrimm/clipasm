# Build the scenic sequence

In this tutorial you will grow one image into a 4.5-second, three-scene video.
Each step introduces a language feature only when the program needs it: first
the stack, then `concat`, then `clip`, names, references, and an effect.

## Before you start

Complete [Install and render ClipAsm](../getting-started/first-render.md) first.
Stay in that initialized project so you can reuse its three images. If you are
starting separately, create and enter a project:

```console,ignore
clipasm init scenic-video
cd scenic-video
```

Create `scenic-tutorial.clipasm`. The steps below build that file from scratch
without changing `main.clipasm`.

## 1. Turn one image into a video

Start with the language version, project settings, and one image:

```clipasm
clipasm 1

config {
    video {
        width = 320
        height = 180
        fps = 24
    }
    output = "generated/scenic-tutorial.mp4"
}

image("assets/morning.png", 1500ms, contain)
```

`image` creates one Video value. Because it is the only value left by the
source program, ClipAsm can publish it.

Validate the file:

```console,ignore
clipasm validate scenic-tutorial.clipasm
clipasm render scenic-tutorial.clipasm
```

Validation succeeds with 36 frames: 1.5 seconds at 24 frames per second.
Open `generated/scenic-tutorial.mp4` to see the still image as a video.

## 2. Add more images

Append two more image calls:

```clipasm
image("assets/morning.png", 1500ms, contain)
image("assets/meadow.png", 1500ms, contain)
image("assets/evening.png", 1500ms, contain)
```

Validate again:

```console,ignore
clipasm validate scenic-tutorial.clipasm
```

This time validation reports `E_ENTRYPOINT_OUTPUT_COUNT`: three Video values
remain, but a source program with `output` must leave exactly one.

Ask ClipAsm for more detail when you encounter an unfamiliar code:

```console,ignore
clipasm explain E_ENTRYPOINT_OUTPUT_COUNT
```

The calls run in statement order and leave their results on the stack:

```text
image morning  -> [morning]
image meadow   -> [morning, meadow]
image evening  -> [morning, meadow, evening]
```

Nothing is wrong with any individual image. The program needs an operation that
turns those three values into one.

## 3. Combine the stack

Add `concat` after the images:

```clipasm
image("assets/morning.png", 1500ms, contain)
image("assets/meadow.png", 1500ms, contain)
image("assets/evening.png", 1500ms, contain)
concat
```

`concat` consumes the accessible Videos in their existing order and leaves one
combined Video:

```text
[morning, meadow, evening] -> concat -> [sequence]
```

Validate again:

```console,ignore
clipasm validate scenic-tutorial.clipasm
clipasm render scenic-tutorial.clipasm
```

The program now succeeds with 108 frames. Reopen the MP4 to see all three
scenes in order.

## 4. Turn the sequence into a clip

Suppose you want to treat the three scenes as one reusable composition. Replace
the four executable statements with:

```clipasm
clip {
    image("assets/morning.png", 1500ms, contain)
    image("assets/meadow.png", 1500ms, contain)
    image("assets/evening.png", 1500ms, contain)
}
```

A `clip` combines the Video values left by its body, so its body does not need
an explicit `concat`. Validate this version:

```console,ignore
clipasm validate scenic-tutorial.clipasm
```

Validation again reports `E_ENTRYPOINT_OUTPUT_COUNT`, but now zero Videos
remain. A `clip` removes its temporary result from the outer stack so that a
reusable composition does not publish itself merely by being declared.

## 5. Name and reference the clip

Give the clip a name, then reference that name after the declaration:

```clipasm
clip {
    image("assets/morning.png", 1500ms, contain)
    image("assets/meadow.png", 1500ms, contain)
    image("assets/evening.png", 1500ms, contain)
} as pictures

$pictures
```

`as pictures` gives the composed Video an immutable name. It still does not
place the Video on the outer stack. `$pictures` does, exactly where the
reference appears.

Validate once more:

```console,ignore
clipasm validate scenic-tutorial.clipasm
```

The source again leaves one 108-frame Video ready for publication.

## 6. Transform the nearest value

Place `zoom_in(4%)` immediately after the meadow image:

```clipasm
clip {
    image("assets/morning.png", 1500ms, contain)
    image("assets/meadow.png", 1500ms, contain)
    zoom_in(4%)
    image("assets/evening.png", 1500ms, contain)
} as pictures

$pictures
```

`zoom_in` takes the nearest Video—the meadow—then leaves its transformed result
in the same position. The `clip` combines morning, the transformed meadow, and
evening. The effect does not change their durations.

## 7. Render the result

Validate and render the finished source:

```console,ignore
clipasm validate scenic-tutorial.clipasm
clipasm render scenic-tutorial.clipasm
```

Open `generated/scenic-tutorial.mp4`. The scenes appear in order for a total of
4.5 seconds, with movement only in the middle scene.

## What you learned

You started with one Video, saw how statements build a stack, and used `concat`
when three root values could not be published. You then replaced that explicit
combination with a `clip`, named its off-stack result, returned it with a
reference, and transformed the nearest value.

Next, [Add a flash-cut transition](add-a-transition.md). For exact behavior, see
[Stack binding](../reference/language/stack-binding.md),
[Composition forms](../reference/language/composition-forms.md),
[`concat`](../reference/programs/concat.md), and
[`zoom_in`](../reference/programs/zoom_in.md).
