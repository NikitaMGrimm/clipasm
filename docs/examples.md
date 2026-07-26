# Examples

The committed `.clipasm` programs under `examples/` are small executable
language demonstrations. Run commands from the repository root. Validation is
pure; rendering additionally requires FFmpeg and FFprobe.

## Scenic sequence

`examples/scenic-sequence.clipasm` combines three still images in a `glue`
body.

```console
clipasm validate examples/scenic-sequence.clipasm
clipasm render examples/scenic-sequence.clipasm
```

## Exact crossfade

`examples/crossfade.clipasm` overlaps two still images for 500 milliseconds.
The Video and attached Audio timelines use the same exact frame boundaries.

```console
clipasm validate examples/crossfade.clipasm
clipasm render examples/crossfade.clipasm
```

## Gentle motion edit

`examples/gentle-motion-edit.clipasm` applies `wobble` only to a selected range
through `during`.

```console
clipasm validate examples/gentle-motion-edit.clipasm
clipasm render examples/gentle-motion-edit.clipasm
```

## Reusable composition

`examples/reusable-composition.clipasm` demonstrates `clip`, immutable
references, named graph inputs, and final concatenation.

```console
clipasm validate examples/reusable-composition.clipasm
clipasm render examples/reusable-composition.clipasm
```

## Imported program

`examples/imported-program.clipasm` imports
`examples/programs/polish.clipasm` as an ordinary typed program.

```console
clipasm validate examples/imported-program.clipasm
clipasm render examples/imported-program.clipasm
```

## External program

`examples/external-brighten.clipasm` imports a native `.clipasm` program whose
implementation runs a small Python/FFmpeg script through an explicit interpreter
and content-hashed file argument. It uses the ordinary typed binder and a native
parameter default. External programs are trusted code and execute during
rendering; this example requires `python3` on `PATH`.

```console
clipasm validate examples/external-brighten.clipasm
clipasm inspect examples/external-brighten.clipasm
clipasm render examples/external-brighten.clipasm
```

## Root bindings

`examples/root-bindings.clipasm` declares one Video input and two required
scalar parameters.

```console
clipasm validate examples/root-bindings.clipasm \
  --video-input video=examples/assets/gentle-motion.mkv \
  --arg range=500ms..1500ms \
  --arg count=2

clipasm render examples/root-bindings.clipasm \
  --video-input video=examples/assets/gentle-motion.mkv \
  --arg range=500ms..1500ms \
  --arg count=2 \
  --output root-bindings.mp4
```

Generated outputs, manifests, and caches are ignored by Git.
