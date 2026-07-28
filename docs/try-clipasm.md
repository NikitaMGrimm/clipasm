# Try ClipAsm

The playground below contains a complete three-scene project. In a few minutes
you can check the source, make one edit, and render the result. Nothing is
uploaded.

```clipasm
{{#include ../examples/scenic-sequence.clipasm}}
```

<div data-clipasm-playground
     data-clipasm-assets-base="playground/example-assets/"
     data-clipasm-assets='["assets/morning.png","assets/meadow.png","assets/evening.png"]'></div>

## 1. Validate the original

Select **Validate**, or press <kbd>Ctrl</kbd>+<kbd>Enter</kbd>. The source
passes with 108 frames: three 1.5-second scenes at 24 frames per second.

## 2. Change one scene

Change the meadow duration from `1500ms` to `1s`, then validate again. The
timeline is now four seconds, or 96 frames.

## 3. Render and play the video

Select **Render video**. When rendering finishes, play the preview and check
that the middle scene is shorter. Use **Reset** whenever you want to restore the
original source and project files.

## Project files

The images appear under **Virtual project files**. You can preview, rename,
replace, or delete them. Everything stays in your browser.

## Browser limits

The playground supports still-image and video-file sources together with the
native operations reachable from them. It does not support imports, standalone
Audio-file sources, or external programs.

It accepts one source file up to 256 KiB, individual assets up to 128 MiB, and
256 MiB of assets in total. Browser rendering uses a single-threaded WebAssembly
FFmpeg runtime and applies a bounded work budget, so larger projects belong in
the installed CLI.

Continue with [Install and render ClipAsm](getting-started/first-render.md) to
create a local project and use the complete native feature set.

The renderer is downloaded only when you select **Render video**. It is separate
GPL-licensed software; see the
[browser runtime notices](https://github.com/NikitaMGrimm/clipasm/blob/main/playground/web/THIRD_PARTY.md).
