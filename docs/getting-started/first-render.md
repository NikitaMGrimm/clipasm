# Build and render ClipAsm

This guide builds ClipAsm from its repository and renders the committed scenic
sequence. At the end, you will have a 4.5-second, 320x180 MP4 assembled from
three included images.

ClipAsm is pre-release software. Its language and CLI may change without
compatibility guarantees.

## Requirements

You need:

- Git to clone the repository;
- Rust 1.95 or newer;
- FFmpeg and FFprobe on `PATH`.

Check the tools before continuing:

```console
git --version
rustc --version
cargo --version
ffmpeg -version
ffprobe -version
```

Each command should print version information. The exact versions and output
will vary by system, but `rustc` must report 1.95 or newer. Python is not needed
for this guide.

## Get the repository

Clone ClipAsm and enter the repository:

```console
git clone https://github.com/NikitaMGrimm/clipasm.git
cd clipasm
```

If you already have a checkout, enter its repository root instead. Run every
remaining command on this page from that root.

Build the development binary:

```console
cargo build
```

Cargo downloads the Rust dependencies on the first build. A successful command
ends with a `Finished` line.

ClipAsm does not currently document a packaged installation workflow, so these
pages use `cargo run --` to run the binary from the checkout.

## Validate the source

The first program is already committed at
`examples/scenic-sequence.clipasm`. It uses the three PNG files under
`examples/assets/`; no asset-generation step is required.

Validate it before rendering:

```console
cargo run -- validate examples/scenic-sequence.clipasm
```

The final line should be:

```text
valid: 4 semantic value(s), 108 frame(s)
```

Validation parses and checks the source, builds its pure semantic graph, and
infers the duration available from authored information. It does not open the
PNG files or invoke FFmpeg. This makes validation the normal first check after
editing a program.

## Render the video

Render the same source:

```console
cargo run -- render examples/scenic-sequence.clipasm
```

This time ClipAsm performs preflight, where it resolves the images and media
tools, and then renders the prepared plan. Success is reported with a
`rendered` line containing the output and manifest paths. Cache hit and miss
counts depend on whether you have rendered the program before.

The command creates:

- `examples/generated/scenic-sequence.mp4`, an H.264 video at 320x180 and 24
  frames per second;
- `examples/generated/scenic-sequence.mp4.manifest.json`, the sibling render
  manifest;
- content-addressed intermediates under `examples/.clipasm/cache/`.

The video lasts 4.5 seconds and shows the morning, meadow, and evening images in
that order. Generated outputs, manifests, and caches are ignored by Git.

## Choose another output path

The source declares its normal output path, but the CLI can override it. Try a
second publication under the repository's ignored `local/` area:

```console
cargo run -- render examples/scenic-sequence.clipasm \
  --output local/scenic-sequence.mp4
```

The source and rendered content are unchanged. Only the published MP4 and its
sibling manifest move to `local/`.

## What you learned

You have:

- built ClipAsm from its repository;
- validated a source program without opening its media;
- rendered committed assets through preflight and FFmpeg;
- located the MP4, manifest, and render cache;
- overridden a source-declared output path from the CLI.

Next, work through [the scenic sequence tutorial](../tutorials/scenic-sequence.md)
to understand the source you just rendered. The
[language reference](../language-reference.md) is the normative guide to syntax
and behavior, while the [examples catalog](../examples.md) lists the other
committed programs.
