# Note about the documentation

Most of the README and documentation were written with AI assistance. I plan to rewrite and improve them later.

# ClipAsm

ClipAsm is pre-release software. Its language, file formats, Rust API, and CLI
may change without compatibility guarantees.

ClipAsm is a typed, stack-based language for assembling Video and Audio graphs.
It compiles `.clipasm` source without opening media files, resolves assets and
tools during preflight, and renders an MP4 through FFmpeg and FFprobe.

Current features include still images, video and audio files, references,
reusable imported programs, external executable programs, timeline operations,
effects, transitions, exact audiovisual crossfades, scoped body programs,
structural stack blocks, root inputs and parameters, and content-addressed
render caching.

## Requirements

- Rust 1.95 or newer
- FFmpeg and FFprobe on `PATH` for rendering
- Python 3 only for the external-program example

Compilation and validation do not open media files or invoke external tools.
Rendering a source file with registered external programs executes trusted code
declared by that source file.

## Quick start

```clipasm
clipasm 1

config {
    video {
        width = 1280
        height = 720
        fps = 30
    }
    output = "final.mp4"
}

image("title.png", 2s, contain)
video("footage.mp4")
concat
```

```console
cargo run -- validate program.clipasm
cargo run -- inspect program.clipasm
cargo run -- render program.clipasm
```

A root program may declare inputs and scalar parameters:

```clipasm
clipasm 1

input video: Video
param range: TimeRange
param count: Integer

trim($video, $range)
repeat($count)
```

```console
cargo run -- render template.clipasm \
  --video-input video=footage.mp4 \
  --arg range=3s..8s \
  --arg count=2 \
  --output final.mp4
```

CLI-supplied media, `File` parameters, and output overrides resolve from the
caller's working directory. Paths written in source resolve from the `.clipasm`
file containing them. Rendering writes the MP4, a sibling manifest, and cached
intermediates under `.clipasm/cache/` beside the entrypoint source.

## Documentation

- [Language reference](docs/language-reference.md)
- [Runnable examples](docs/examples.md)
- [Architecture](docs/architecture.md)
- [Architecture decisions](docs/adr/)
- [Settled terminology and semantics](CONTEXT.md)
- [Development change guide](docs/development/change-guide.md)
- [Contributing](CONTRIBUTING.md)
- [Published book](https://nikitamgrimm.github.io/clipasm/)

## Development

```console
./scripts/check.sh
```

## Repository history

![Rust source lines over main history](https://nikitamgrimm.github.io/clipasm/loc-history.svg)

[Open the repository history chapter](https://nikitamgrimm.github.io/clipasm/repository-history.html).
