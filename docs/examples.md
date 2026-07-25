# Examples

The committed `.clipasm` programs under `examples/` are small executable
language demonstrations. Run commands from the repository root. Validation is
pure; rendering additionally requires FFmpeg and FFprobe.

## Scenic sequence

`examples/scenic-sequence.clipasm` combines three still images in a `glue`
body.

```console
cargo run -- validate examples/scenic-sequence.clipasm
cargo run -- render examples/scenic-sequence.clipasm
```

## Exact crossfade

`examples/crossfade.clipasm` overlaps two still images for 500 milliseconds.
The Video and attached Audio timelines use the same exact frame boundaries.

```console
cargo run -- validate examples/crossfade.clipasm
cargo run -- render examples/crossfade.clipasm
```

## Gentle motion edit

`examples/gentle-motion-edit.clipasm` applies `wobble` only to a selected range
through `during`.

```console
cargo run -- validate examples/gentle-motion-edit.clipasm
cargo run -- render examples/gentle-motion-edit.clipasm
```

## Reusable composition

`examples/reusable-composition.clipasm` demonstrates `clip`, immutable
references, named graph inputs, and final concatenation.

```console
cargo run -- validate examples/reusable-composition.clipasm
cargo run -- render examples/reusable-composition.clipasm
```

## Imported program

`examples/imported-program.clipasm` imports
`examples/programs/polish.clipasm` as an ordinary typed program.

```console
cargo run -- validate examples/imported-program.clipasm
cargo run -- render examples/imported-program.clipasm
```

## External program

`examples/external-brighten.clipasm` imports a native `.clipasm` program whose
implementation is a small Python/FFmpeg executable. It uses the ordinary typed
binder and a native parameter default. External programs are trusted native code
and execute during rendering.

```console
cargo run -- validate examples/external-brighten.clipasm
cargo run -- compile examples/external-brighten.clipasm
cargo run -- render examples/external-brighten.clipasm
```

## Root bindings

`examples/root-bindings.clipasm` declares one Video input and two required
scalar parameters.

```console
cargo run -- validate examples/root-bindings.clipasm \
  --input video=examples/assets/gentle-motion.mkv \
  --arg range=500ms..1500ms \
  --arg count=2

cargo run -- render examples/root-bindings.clipasm \
  --input video=examples/assets/gentle-motion.mkv \
  --arg range=500ms..1500ms \
  --arg count=2 \
  --output root-bindings.mp4
```

Generated outputs, manifests, and caches are ignored by Git.
