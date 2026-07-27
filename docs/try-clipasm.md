# Try ClipAsm

Edit the program, then select **Validate**, **Inspect**, or **Render video**.
<kbd>Ctrl</kbd>+<kbd>Enter</kbd> validates. Everything runs locally in your
browser; source and project files are not uploaded.

```clipasm
{{#include ../examples/scenic-sequence.clipasm}}
```

<div data-clipasm-playground
     data-clipasm-assets-base="playground/example-assets/"
     data-clipasm-assets='["assets/morning.png","assets/meadow.png","assets/evening.png"]'></div>

The scenic assets begin as ordinary **Virtual project files**, so you can
preview, rename, replace, or delete them. **Reset** restores the original source
and files. The playground accepts one source file up to 256 KiB, individual
assets up to 128 MiB, and 256 MiB of assets in total. Browser rendering uses a
single-threaded WebAssembly FFmpeg runtime, loaded on demand, and applies a
bounded operation/work budget.

Still-image and video-file sources, plus every native operation reachable from
them, are supported. Standalone Audio-file sources, imports, and external
programs remain unavailable in the browser. The installed CLI supports complete
source packages and the full native feature set; continue with
[Install and render ClipAsm](getting-started/first-render.md) to initialize a
project, use those features, or render larger projects.

The renderer is downloaded only when selected. It is separate GPL-licensed
software; see the
[browser runtime notices](https://github.com/NikitaMGrimm/clipasm/blob/main/playground/web/THIRD_PARTY.md).
