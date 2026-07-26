# Try ClipAsm

Edit the program, then select **Validate** or press <kbd>Ctrl</kbd>+<kbd>Enter</kbd>.
**Inspect** shows the compiled semantic graph. Everything runs locally in your
browser: the source is not uploaded, media files are not opened, and FFmpeg is
not invoked.

```clipasm
{{#include ../examples/scenic-sequence.clipasm}}
```

<div data-clipasm-playground></div>

This playground accepts one source file up to 256 KiB and stops compilation
after five seconds. File-backed imports are unavailable, and external programs
are never run. The installed CLI supports complete source packages, preflight,
and rendering; continue with
[Install and render ClipAsm](getting-started/first-render.md) when you want to
produce a video.
