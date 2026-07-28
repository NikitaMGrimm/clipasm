# Build the scenic sequence

In this tutorial you will build a 4.5-second, three-scene video from an empty
source file. Along the way you will use the language version, project settings,
a stack block, image values, an effect, and `concat`.

## Before you start

Complete [Install and render ClipAsm](../getting-started/first-render.md) first.
Stay in that initialized project so you can reuse its three images. If you are
starting separately, create and enter a project:

```console,ignore
clipasm init scenic-video
cd scenic-video
```

Create a new file named `scenic-tutorial.clipasm`. The steps below build that
file incrementally without changing `main.clipasm`.

## 1. Create one scene

Add the language version, project settings, and one image:

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

{
    image("assets/morning.png", 1500ms, contain)
}
```

`clipasm 1` selects the language version. The configuration sets the output
frame and publication path. The outer `{ ... }` is a stack block: it groups the
executable statements and returns the values left inside it.

Validate the file:

```console,ignore
clipasm validate scenic-tutorial.clipasm
```

Validation reports 36 frames: 1.5 seconds at 24 frames per second. It derives
that duration from source without opening the image.

## 2. Add two more scenes

Inside the stack block, add the meadow and evening images after the morning
image:

```clipasm
{
    image("assets/morning.png", 1500ms, contain)
    image("assets/meadow.png", 1500ms, contain)
    image("assets/evening.png", 1500ms, contain)
}
```

Each call leaves one Video value in the block. Validate again:

```console,ignore
clipasm validate scenic-tutorial.clipasm
```

The program is valid, but it has three root outputs. Rendering needs exactly one
Video to publish, so the next step combines them.

## 3. Join the sequence

Add `concat` after the three image calls:

```clipasm
{
    image("assets/morning.png", 1500ms, contain)
    image("assets/meadow.png", 1500ms, contain)
    image("assets/evening.png", 1500ms, contain)
    concat
}
```

`concat` consumes the accessible Videos in statement order and returns one
combined Video. Predict the duration, then validate:

```console,ignore
clipasm validate scenic-tutorial.clipasm
```

Three scenes × 1.5 seconds × 24 fps produces 108 frames.

## 4. Render and check the result

Render the one combined Video:

```console,ignore
clipasm render scenic-tutorial.clipasm
```

Open `generated/scenic-tutorial.mp4`. The scenes appear in this order: morning,
meadow, evening, for a total of 4.5 seconds.

## 5. Add movement without changing the duration

Place `zoom_in(4%)` immediately after the meadow image:

```clipasm
{
    image("assets/morning.png", 1500ms, contain)
    image("assets/meadow.png", 1500ms, contain)
    zoom_in(4%)
    image("assets/evening.png", 1500ms, contain)
    concat
}
```

`zoom_in` takes the nearest Video, transforms it, and returns a Video with the
same duration. Validate and render once more:

```console,ignore
clipasm validate scenic-tutorial.clipasm
clipasm render scenic-tutorial.clipasm
```

Validation still reports 108 frames. Reopen the MP4 to see movement only in the
middle scene.

## What you learned

You built a source file from its version declaration to one publishable Video.
You used a stack block to group work, followed values in statement order, joined
three scenes, and transformed the nearest Video without changing its duration.

Next, [Build a reusable composition](reusable-composition.md). For exact lookup,
see [Composition forms](../reference/language/composition-forms.md),
[Stack binding](../reference/language/stack-binding.md), [`image`](../reference/programs/image.md),
and [`concat`](../reference/programs/concat.md).
