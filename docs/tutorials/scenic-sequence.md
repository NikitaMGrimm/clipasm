# Build the scenic sequence

This tutorial explains the starter one idea at a time: project settings, image
values, statement order, and `concat`. You will predict each result, validate
it, deliberately create one harmless error, and repair it.

Create a fresh project and enter it:

```console,ignore
clipasm init scenic-video
cd scenic-video
```

Open `main.clipasm`. ClipAsm created the file, but it will not manage or rewrite
it after initialization.

## 1. Read the project settings

The configuration begins with:

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

**Predict:** the output will be 320x180 at exactly 24 frames per second, and
`render` will publish it to `generated/scenic-sequence.mp4`.

Now validate:

```console,ignore
clipasm validate main.clipasm
```

**Observe:** validation succeeds with 108 frames. It can calculate that duration
from the source without opening an image.

## 2. Follow the values

The next three statements are:

```clipasm
image("assets/morning.png", 1500ms, contain)
image("assets/meadow.png", 1500ms, contain)
image("assets/evening.png", 1500ms, contain)
```

Each call creates one 1.5-second Video. `contain` fits the complete image inside
the 320x180 frame, adding empty space when the aspect ratios differ.

**Predict:** three scenes × 1.5 seconds × 24 fps = 108 frames.

The validation result confirms that calculation. The files themselves are not
opened until rendering.

## 3. Join the scenes

The final statement is:

```clipasm
concat
```

`concat` takes the accessible Video values in their existing order and returns
one combined Video. The rendered order is therefore morning, meadow, evening.

Create a safe error by changing `concat` to `concatt`, then validate:

```console,ignore
clipasm validate main.clipasm
```

The command reports an unknown program at `concatt`. Restore the spelling and
validate once more. No media was opened and no output was written while testing
this error.

## 4. Change the timeline

Change only the meadow duration from `1500ms` to `1s`.

**Predict:** the complete sequence becomes four seconds, so validation should
report 96 frames at 24 fps.

```console,ignore
clipasm validate main.clipasm
clipasm render main.clipasm
```

**Observe:** validation reports 96 frames, and the rendered MP4 has a shorter
middle scene.

## What you learned

You configured a project, created three Video values, relied on statement order,
and reduced them to one output with `concat`. You also used `validate` to repair
a source error before rendering.

Next, [Build a reusable composition](reusable-composition.md). For exact lookup,
see [`image`](../reference/programs/image.md),
[`concat`](../reference/programs/concat.md), and
[Files and configuration](../reference/language/files-and-configuration.md).
