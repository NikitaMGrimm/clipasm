# Add or replace a soundtrack

Use `set_audio` to attach standalone Audio to a Video or replace Audio the Video
already carries.

## Before you start

Create an initialized project and add these media files:

```text
assets/scene.mp4
assets/soundtrack.wav
```

The Video and Audio may have different durations. The resulting Video keeps the
Video timeline: longer Audio is trimmed, and shorter Audio is padded to its
duration.

## 1. Create the source file

Create `soundtrack.clipasm`:

```clipasm
clipasm 1

config {
    video {
        width = 1920
        height = 1080
        fps = 30
    }
    output = "generated/with-soundtrack.mp4"
}

video("assets/scene.mp4", contain)
audio("assets/soundtrack.wav")
set_audio
```

The first two calls leave one Video and one Audio value. `set_audio` binds each
input by its exact type, replaces the Video's Audio, and leaves one Video.

## 2. Validate the structure

```console,ignore
clipasm validate soundtrack.clipasm
```

Validation checks the source without opening either media file. File-backed
durations may remain deferred until rendering.

## 3. Render and listen

```console,ignore
clipasm render soundtrack.clipasm
```

Open `generated/with-soundtrack.mp4`. Confirm that its picture comes from
`scene.mp4` and its sound comes from `soundtrack.wav`.

See [`set_audio`](../reference/programs/set_audio.md) for its exact contract and
[Stack binding](../reference/language/stack-binding.md) for mixed Video and Audio
inputs.
