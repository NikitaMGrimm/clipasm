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

The PNG images are lossless 320×180 illustrations. The video uses H.264 in a
Matroska container because it is compact while relying only on codec and
container capabilities ClipAsm already requires. Generated outputs, manifests,
and caches remain untracked.
