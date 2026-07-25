# Note about the documentation

Most of the README and documentation were written with AI assistance. I plan to rewrite and improve them later.

# ClipAsm

ClipAsm is pre-release software. Its language, file formats, Rust API, and
command-line interface may change without compatibility guarantees.

ClipAsm compiles a representation-neutral typed source program into a video
graph, prepares result-reachable media with FFmpeg and FFprobe, and renders an
MP4. The current authoring frontend is strict YAML.

Current features:

- still-image and video-file sources
- references and named clips
- imported callable source programs with Video or Audio inputs and typed parameters
- registered external executables using the same typed program binder
- ordered source-program results and isolated inline fixed inputs
- type-preserving trimming, concatenation, repetition, and stack dropping
- trimming, centered zoom, deterministic wobble, white-flash cuts, and audiovisual crossfades
- `during`, `join`, and nested `glue`
- `audio`, `extract_audio`, and `set_audio` with synchronized Video audio
- content-addressed render caching

## Requirements

- Rust 1.89 or newer
- FFmpeg and FFprobe on `PATH` for rendering

Compilation and validation do not open media files or invoke external tools.
Rendering a workflow with registered external programs executes trusted native
code declared by that workflow.

## Quick start

```yaml
- program:
    version: 1

    project:
      video: {width: 1280, height: 720, fps: 30}

    output: final.mp4

- image:
    path: title.png
    duration: 2s
    fit: contain
- video: footage.mp4
- concat
```

```console
cargo run -- validate program.yaml
cargo run -- compile program.yaml
cargo run -- render program.yaml
```

A root source program may expose reusable inputs and parameters:

```yaml
- program:
    version: 1
    inputs:
      - video: Video
    parameters:
      range: TimeRange
      count: Integer

- trim:
    value: $video
    range: $range
- repeat: $count
```

```console
cargo run -- render template.yaml \
  --input video=footage.mp4 \
  --arg range=3s..8s \
  --arg count=2 \
  --output final.mp4
```

CLI-supplied media, `File` parameters, and render output paths resolve from the
caller's working directory. Authored paths continue to resolve from their YAML
source unit.

Authored paths are resolved relative to the source unit containing them.
Rendering writes the MP4, a
sibling `.manifest.json`, and cached intermediates under `.clipasm/cache/`
beside the entrypoint source.

## Documentation

- [Read the ClipAsm book](https://nikitamgrimm.github.io/clipasm/)
- [YAML frontend reference](docs/workflow-reference.md)
- [Architecture](docs/architecture.md)
- [Architecture decisions](docs/adr/)
- [Domain language and settled semantics](CONTEXT.md)
- [Development change guide](docs/development/change-guide.md)
- [Contributing](CONTRIBUTING.md)
- [Runnable examples](examples/README.md)

## Development

```console
./scripts/check.sh
```

## Repository history

![Rust source lines over main history](https://nikitamgrimm.github.io/clipasm/loc-history.svg)

[Open the repository history chapter](https://nikitamgrimm.github.io/clipasm/repository-history.html).
