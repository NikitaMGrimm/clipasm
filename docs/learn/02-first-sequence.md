# 2. From one image to a sequence

In this chapter you will start with one image, then grow it into a 4.5-second
sequence. ClipAsm cannot publish three images as one Video. The diagnostic will
reveal ClipAsm's stack model and motivate `concat`.

## Continue your project

Complete [Get ClipAsm running](01-get-clipasm-running.md) first. Stay
in the `hello-video` directory so the starter images remain available.

Create `learning.clipasm`. This is the file you will develop through the rest
of the learning chapters. Leave the generated `main.clipasm` starter unchanged
for comparison.

## 1. Create a one-image video source

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

## 2. Validate the one-image source

```console,ignore
clipasm validate learning.clipasm
```

Validation reports 36 frames.

## 3. Render the one-image Video

```console,ignore
clipasm render learning.clipasm
```

## 4. Check the one-image Video

Open `generated/learning.mp4`. Confirm that the morning image appears for 1.5
seconds.

## 5. Add two more images

Replace the final image call with these three calls:

```clipasm
image("assets/morning.png", 1500ms, contain)
image("assets/meadow.png", 1500ms, contain)
image("assets/evening.png", 1500ms, contain)
```

## 6. Validate the three-image source

```console,ignore
clipasm validate learning.clipasm
```

ClipAsm reports `E_ENTRYPOINT_OUTPUT_COUNT`: three Video values remain, but a
source file with `output` must leave exactly one Video to publish.

## 7. Explain the diagnostic

When an unfamiliar diagnostic includes a code, ask ClipAsm for its explanation:

```console,ignore
clipasm explain E_ENTRYPOINT_OUTPUT_COUNT
```

## 8. Examine the stack

Each call leaves its result after the values produced earlier:

```text
image morning  -> [morning]
image meadow   -> [morning, meadow]
image evening  -> [morning, meadow, evening]
```

This ordered collection is the stack. Each image is valid. The program needs an
operation that consumes three Videos and returns one.

## 9. Add `concat`

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

## 10. Validate the sequence

```console,ignore
clipasm validate learning.clipasm
```

Validation now reports 108 frames.

## 11. Render the sequence

```console,ignore
clipasm render learning.clipasm
```

## 12. Check the sequence

Reopen `generated/learning.mp4`. The three 1.5-second scenes play in morning,
meadow, evening order.

You now know why statement order matters. A call can consume stack values
without receiving them as explicit arguments.

Next, [name and reference a clip](03-name-and-reference-clip.md).
