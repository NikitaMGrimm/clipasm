# ClipAsm

ClipAsm compiles a strict YAML workflow into a typed video graph, prepares
reachable media with FFmpeg and FFprobe, and renders an MP4.

Current features:

- still-image and video-file sources
- references and named clips
- concatenation and repetition
- `then`, `during`, `join`, and nested `timeline`
- content-addressed render caching

Audio output, transitions, effects, plugins, and user-defined programs are not
supported.

## Requirements

- Rust toolchain compatible with edition 2024
- FFmpeg and FFprobe on `PATH` for rendering

Compilation and validation do not open media files or invoke external tools.

## Quick start

```yaml
version: 1

project:
  video: {width: 1280, height: 720, fps: 30}

timeline:
  - image:
      path: title.png
      duration: 2s
      fit: contain
  - video: footage.mp4

output: final.mp4
```

```console
cargo run -- validate workflow.yaml
cargo run -- compile workflow.yaml
cargo run -- render workflow.yaml
```

Paths are resolved relative to the workflow file. Rendering writes the MP4, a
sibling `.manifest.json`, and cached intermediates under `.clipasm/cache/`
beside the workflow.

## Documentation

- [Workflow reference](docs/workflow-reference.md)
- [Architecture](docs/architecture.md)
- [Architecture decisions](docs/adr/)
- [Domain language and settled semantics](CONTEXT.md)
- [Contributing](CONTRIBUTING.md)
- [Runnable examples](examples/README.md)

Build or browse the two local documentation surfaces:

```console
cargo doc --no-deps --open
mdbook serve --open
```

## Development

```console
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo test --doc
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
mdbook build
```
