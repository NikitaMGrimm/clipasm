# Install and render ClipAsm

This guide installs ClipAsm, creates a project, validates its starter program,
and renders a 4.5-second, 320x180 video from three included images. You do not
need Git or a repository checkout.

ClipAsm is pre-release software. Its language and CLI may change without
compatibility guarantees.

## Requirements

You need Rust 1.95 or newer to install ClipAsm. Rendering also needs FFmpeg and
FFprobe on `PATH`.

```console,ignore
rustc --version
cargo --version
ffmpeg -version
ffprobe -version
```

The exact version output differs by system. `rustc` must report 1.95 or newer.

## Install and initialize a project

Install the CLI, then ask it to create a new directory named `hello-video`:

```console,ignore
cargo install clipasm --locked
clipasm init hello-video
```

`init` creates the directory and writes `main.clipasm`, its three `assets/`
images, a project README, and `.gitignore`. It does not invoke Git, render,
inspect media, or contact the network. Its success message gives the next
commands; enter the new project:

```console,ignore
cd hello-video
```

## Validate the starter program

The generated `main.clipasm` is the canonical scenic sequence. Validate it
before rendering:

```console,ignore
clipasm validate main.clipasm
```

It reports 108 frames: three 1.5-second scenes at 24 frames per second.
Validation parses and checks the source and infers source-independent domains.
It does not open the PNG files or run FFmpeg, so use it as the normal first
check after an edit.

## Render and open the video

```console,ignore
clipasm render main.clipasm
```

Rendering performs preflight, resolves the three images and media tools, then
writes `generated/scenic-sequence.mp4` and its sibling manifest. Open that MP4
with your usual file manager or media player. The scenes appear in order:
morning, meadow, and evening.

## Make one small edit

Open `main.clipasm` in an editor. Change only the meadow duration from `1500ms`
to `1s`, then save the file. Validate and render again:

```console,ignore
clipasm validate main.clipasm
clipasm render main.clipasm
```

Validation now reports 96 frames, or four seconds. Reopen the same MP4 to see
the shorter middle scene. The render output, manifest, and `.clipasm/cache/`
are ordinary generated project files and are ignored by the starter's
`.gitignore`.

## What you learned

You have installed ClipAsm, initialized an unmanaged project, validated a
program without opening media, rendered it, and made a source change. Continue
with [build the scenic sequence](../tutorials/scenic-sequence.md) for a guided
explanation of the starter. The [CLI reference](../reference/cli.md) defines
`init`, validation, inspection, and rendering exactly.
