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

Change the meadow duration from `1500ms` to `1s`.

## 3. Validate the change

Select **Validate**, or press <kbd>Ctrl</kbd>+<kbd>Enter</kbd>. The timeline is
now four seconds, or 96 frames.

## 4. Render the video

Select **Render video**.

## 5. Check the video

When rendering finishes, play the preview. Confirm that the middle scene is
shorter. **Reset** restores the original source and project files.

## Project files

The images appear under **Virtual project files**. You can preview, rename,
replace, or delete them. Everything stays in your browser.

## Browser limits

The playground supports still-image and video-file sources together with the
native operations reachable from them. It does not support imports, standalone
Audio-file sources, or external programs.

It accepts one source file up to 256 KiB. Each asset can be up to 128 MiB, with
a 256 MiB total limit. Browser rendering uses a single-threaded WebAssembly
FFmpeg runtime with a bounded work budget. Use the installed CLI for larger
projects.

Continue with [Get ClipAsm running](learn/01-get-clipasm-running.md) to
create a local project and use the complete native feature set.

The browser downloads the renderer only when you select **Render video**. The
renderer is separate GPL-licensed software. See the
[browser runtime notices](https://github.com/NikitaMGrimm/clipasm/blob/main/playground/web/THIRD_PARTY.md).
