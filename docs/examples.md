# Examples

The committed source programs under `examples/` are small executable
demonstrations of the language. Run commands from the repository root.
Validation is pure; rendering additionally requires FFmpeg and FFprobe. All
source assets are committed, so no setup step is required.

## Scenic sequence

`examples/scenic-sequence.yaml` combines three illustrated PPM still images in
an isolated `glue` body. It demonstrates image fitting, authored durations, and
automatic concatenation inside a body program.

```console
cargo run -- validate examples/scenic-sequence.yaml
cargo run -- render examples/scenic-sequence.yaml
```

## Gentle motion edit

`examples/gentle-motion-edit.yaml` uses the committed two-second YUV4MPEG video.
The asset contains a slowly moving boat and cloud rather than a flashing test
pattern. The example applies `wobble` only to a selected middle range through
postfix `during`.

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

The ASCII PPM images are small illustrated landscapes rather than flat color
swatches. The YUV4MPEG file is raw but remains small because it is only 64×36,
12 fps, and two seconds long. Generated outputs, manifests, and caches remain
untracked.
