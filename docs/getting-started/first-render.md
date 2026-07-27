# Install and render ClipAsm

By the end of this guide you will have a standalone project and a 4.5-second,
320x180 MP4 made from three included images. You do not need Git or a repository
checkout.

## Requirements

Install Rust 1.95 or newer. Rendering also requires `ffmpeg` and `ffprobe` on
`PATH`.

```console,ignore
rustc --version
cargo --version
ffmpeg -version
ffprobe -version
```

The exact output differs by system. `rustc` must report version 1.95 or newer.

## Create a project

Install the CLI and create a directory named `hello-video`:

```console,ignore
cargo install clipasm --locked
clipasm init hello-video
cd hello-video
```

The new project contains:

```text
.gitignore
README.md
main.clipasm
assets/
  morning.png
  meadow.png
  evening.png
```

These are ordinary files you control. `init` does not run Git, inspect media,
render, or contact the network, and ClipAsm does not rewrite the project later.

## Render the included video

```console,ignore
clipasm render main.clipasm
```

ClipAsm checks the source, opens the three images, verifies the required media
tools, and writes:

```text
generated/scenic-sequence.mp4
generated/scenic-sequence.mp4.manifest.json
```

Open the MP4 with your usual file manager or media player. The scenes appear in
this order: morning, meadow, evening.

## Validate while editing

`render` already validates, so this step is optional. Use `validate` when you
want a faster source-only check:

```console,ignore
clipasm validate main.clipasm
```

The starter reports 108 frames: three 1.5-second scenes at 24 frames per second.
Validation does not open the PNG files or run FFmpeg.

## Make one edit

Open `main.clipasm` and change the meadow duration from `1500ms` to `1s`. Then
validate and render again:

```console,ignore
clipasm validate main.clipasm
clipasm render main.clipasm
```

The timeline is now four seconds, or 96 frames at 24 fps. Reopen the same MP4 to
see the shorter middle scene.

The generated MP4, manifest, and `.clipasm/cache/` directory are ignored by the
project's `.gitignore`.

## Next step

Continue with [Build the scenic sequence](../tutorials/scenic-sequence.md) to
understand each line of the starter, or use the
[command-line reference](../reference/cli.md) for exact command behavior.
