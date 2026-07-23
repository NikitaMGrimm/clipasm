# Examples

The committed workflows under `examples/` are small executable demonstrations
of the language. Run commands from the repository root. Validation is pure;
rendering additionally requires FFmpeg, FFprobe, and any generated assets.

## Image sequence

`examples/image-sequence.yaml` demonstrates still-image sources and ordered
root-timeline concatenation.

```console
cargo run -- validate examples/image-sequence.yaml
cargo run -- render examples/image-sequence.yaml
```

## Video source

`examples/video-source.yaml` demonstrates a full-duration video-file source and
project-frame fitting. Generate its local video asset first as described in
`examples/README.md`.

```console
cargo run -- validate examples/video-source.yaml
cargo run -- render examples/video-source.yaml
```

## Repeat during a range

`examples/repeat-during.yaml` demonstrates selecting a closed-open time range,
repeating only that selection, and splicing it back into the base Video.

```console
cargo run -- validate examples/repeat-during.yaml
cargo run -- render examples/repeat-during.yaml
```

## Clips and references

`examples/clips-and-references.yaml` demonstrates named clip declarations,
immutable references, and value reuse.

```console
cargo run -- validate examples/clips-and-references.yaml
cargo run -- render examples/clips-and-references.yaml
```

Generated media, outputs, manifests, and cache files remain untracked.
