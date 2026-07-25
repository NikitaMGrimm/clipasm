# Examples

The committed source programs under `examples/` are small executable
demonstrations of the language. Run commands from the repository root.
Validation is pure; rendering additionally requires FFmpeg and FFprobe. All
source assets are committed, so no setup step is required.

## Scenic sequence

`examples/scenic-sequence.yaml` combines three illustrated PNG still images in
a nested `glue` body. It demonstrates image fitting, authored durations, and
automatic concatenation inside a body program.

```console
cargo run -- validate examples/scenic-sequence.yaml
cargo run -- render examples/scenic-sequence.yaml
```

## Exact crossfade

`examples/crossfade.yaml` overlaps two committed illustrated still images for
500 milliseconds. It demonstrates that `crossfade` is a direct two-input Video
program: the output is shorter than concatenation by the overlap duration, and
its normalized Audio timeline follows the same exact frame boundaries.

```console
cargo run -- validate examples/crossfade.yaml
cargo run -- render examples/crossfade.yaml
```

## Gentle motion edit

`examples/gentle-motion-edit.yaml` uses the committed two-second
H.264/Matroska video. The native-resolution asset contains a slowly moving boat
and cloud rather than a flashing test pattern. The example applies `wobble`
only to a selected middle range through postfix `during`.

```console
cargo run -- validate examples/gentle-motion-edit.yaml
cargo run -- render examples/gentle-motion-edit.yaml
```

## Reusable composition

`examples/reusable-composition.yaml` demonstrates named clips, immutable
references, an inline body supplying a fixed `flash` input, and explicit final
concatenation. It intentionally reuses the opening clip to show that references
do not consume or rebuild named values.

```console
cargo run -- validate examples/reusable-composition.yaml
cargo run -- render examples/reusable-composition.yaml
```

## Imported authored program

`examples/imported-program.yaml` imports
`examples/programs/polish.yaml` as an ordinary typed program. The imported
program receives one Video input, forwards a scalar parameter into `zoom`, and
returns its final Video to the caller. Its local names and stack do not escape
the invocation.

```console
cargo run -- validate examples/imported-program.yaml
cargo run -- render examples/imported-program.yaml
```


## External brighten program

`examples/external-brighten.yaml` registers the JSON manifest under
`examples/programs/brighten/` as the local `brighten` program. The executable
Python script reads ClipAsm's one-shot JSON request and invokes the supplied
FFmpeg executable with a minimal brightness filter. It demonstrates that custom
program logic, inputs, and parameters can live outside the Rust binary while
still using ordinary typed binding, preflight identity, cache behavior, and
artifact verification.

```console
cargo run -- validate examples/external-brighten.yaml
cargo run -- compile examples/external-brighten.yaml
cargo run -- render examples/external-brighten.yaml
```

The script requires Python 3 in addition to the normal FFmpeg and FFprobe render
requirements. External programs execute trusted native code during rendering.

## Root bindings

`examples/root-bindings.yaml` is a reusable entrypoint with one declared Video
input and two required scalar parameters. External Video inputs are adapted to
the ordinary `video` program and all values use the same compiler binder as
authored calls.

```console
cargo run -- validate examples/root-bindings.yaml \
  --input video=examples/assets/gentle-motion.mkv \
  --arg range=500ms..1500ms \
  --arg count=2

cargo run -- render examples/root-bindings.yaml \
  --input video=examples/assets/gentle-motion.mkv \
  --arg range=500ms..1500ms \
  --arg count=2 \
  --output root-bindings.mp4
```

The PNG images are lossless 320×180 illustrations. The video uses H.264 in a
Matroska container because it is compact while relying only on codec and
container capabilities ClipAsm already requires. Generated outputs, manifests,
and caches remain untracked.
