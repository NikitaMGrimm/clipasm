# 1. Get ClipAsm running

This chapter installs the CLI, creates a standalone project, and renders the
included video. The following chapters will build a new source file one concept
at a time.

## Before you start

Install Rust 1.95 or newer. Rendering also requires `ffmpeg` and `ffprobe` on
`PATH`.

```console,ignore
rustc --version
cargo --version
ffmpeg -version
ffprobe -version
```

The exact output differs by system. `rustc` must report version 1.95 or newer.

## 1. Create a project

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

## 2. Render the starter

```console,ignore
clipasm render main.clipasm
```

ClipAsm checks the source, opens the three images, verifies the required media
tools, and writes:

```text
generated/scenic-sequence.mp4
generated/scenic-sequence.mp4.manifest.json
```

Open the MP4 with your usual file manager or media player. You should see the
morning, meadow, and evening scenes in that order.

You have confirmed that the CLI and media tools work. Leave `main.clipasm`
unchanged; it remains a useful finished example while the learning path builds
the same idea from an empty file.

Next, [go from one image to a sequence](02-first-sequence.md).
