# Try ClipAsm

The playground below contains a complete three-scene project. Edit the source,
then select **Validate**, **Inspect**, or **Render video**.
<kbd>Ctrl</kbd>+<kbd>Enter</kbd> validates.

```clipasm
{{#include ../examples/scenic-sequence.clipasm}}
```

<div data-clipasm-playground
     data-clipasm-assets-base="playground/example-assets/"
     data-clipasm-assets='["assets/morning.png","assets/meadow.png","assets/evening.png"]'></div>

A useful first experiment is to change one `1500ms` duration to `1s`, validate,
and render again. At `fps = 24`, the complete timeline changes from 108 frames
to 96 frames.

## Project files

The images appear under **Virtual project files**. You can preview, rename,
replace, or delete them. **Reset** restores the original source and files.
Everything stays in your browser; source and project files are not uploaded.

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
