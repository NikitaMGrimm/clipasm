# 2. From one image to a sequence

In this chapter you will start with one image, then grow it into a 4.5-second
sequence. When three images cannot be published as one video, the diagnostic
will reveal ClipAsm's stack model and motivate `concat`.

## Continue your project

Complete [Get ClipAsm running](01-get-clipasm-running.md) first. Stay
in the `hello-video` directory so the starter images remain available.

Create `learning.clipasm`. This is the file you will develop through the rest
of the learning chapters; leave the generated `main.clipasm` starter unchanged
for comparison.

## 1. Turn one image into a video

Start with:

```clipasm
clipasm 1

config {
    video {
        width = 320
        height = 180
        fps = 24
    }
    output = "generated/learning.mp4"
}

image("assets/morning.png", 1500ms, contain)
```

`clipasm 1` selects the language version. The video configuration establishes
the frame dimensions and frame rate. `contain` keeps the whole image visible
inside that frame. `image` creates one Video value lasting 1.5 seconds.

Validate and render it:

```console,ignore
clipasm validate learning.clipasm
clipasm render learning.clipasm
```

Validation reports 36 frames. Open `generated/learning.mp4` and confirm that the
morning image appears for 1.5 seconds.

## 2. Add two more images

Replace the final image call with these three calls:

```clipasm
image("assets/morning.png", 1500ms, contain)
image("assets/meadow.png", 1500ms, contain)
image("assets/evening.png", 1500ms, contain)
```

Validate again:

```console,ignore
clipasm validate learning.clipasm
```

ClipAsm reports `E_ENTRYPOINT_OUTPUT_COUNT`: three Video values remain, but a
source file with `output` must leave exactly one Video to publish.

When an unfamiliar diagnostic includes a code, ask ClipAsm for its explanation:

```console,ignore
clipasm explain E_ENTRYPOINT_OUTPUT_COUNT
```

## 3. Follow the stack

Each call leaves its result after the values produced earlier:

```text
image morning  -> [morning]
image meadow   -> [morning, meadow]
image evening  -> [morning, meadow, evening]
```

This ordered collection is the stack. Nothing is wrong with any image; the
program needs an operation that consumes three Videos and returns one.

Add `concat`:

```clipasm
image("assets/morning.png", 1500ms, contain)
image("assets/meadow.png", 1500ms, contain)
image("assets/evening.png", 1500ms, contain)
concat
```

`concat` consumes the accessible Videos in stack order and leaves their
combined result:

```text
[morning, meadow, evening] -> concat -> [sequence]
```

## 4. Render the sequence

```console,ignore
clipasm validate learning.clipasm
clipasm render learning.clipasm
```

Validation now reports 108 frames. Reopen `generated/learning.mp4`; the three
1.5-second scenes play in morning, meadow, evening order.

You now know why statement order matters and how a call can consume values from
the stack without receiving them as explicit arguments.

Next, [name and reference a clip](03-name-and-reference-clip.md).
