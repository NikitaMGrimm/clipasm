# ClipAsm

ClipAsm is a typed, stack-based language for assembling Video and Audio graphs.
Compilation is media-pure; preflight resolves reachable assets and tools; and
rendering uses FFmpeg and FFprobe to publish an MP4.

> **Pre-release:** ClipAsm's language, file formats, Rust API, and CLI may
> change without compatibility guarantees.

[Try ClipAsm in the browser](https://nikitamgrimm.github.io/clipasm/try-clipasm.html)
to edit, validate, inspect, and render the scenic sequence without installing
anything. The native CLI supports complete source packages, imports, and
the full native feature set.

## Install and make a first video

Install requires Rust 1.95 or newer. Rendering also requires FFmpeg and FFprobe
on `PATH`.

```console
cargo install clipasm --locked
clipasm init hello-video
cd hello-video
clipasm validate main.clipasm
clipasm render main.clipasm
```

`init` creates a self-contained project with the scenic sequence and its three
images. Open `generated/scenic-sequence.mp4` after rendering. Continue with the
[first-render guide](https://nikitamgrimm.github.io/clipasm/getting-started/first-render.html)
to edit it, or browse the [full guide](https://nikitamgrimm.github.io/clipasm/)
for tutorials, task guides, the language reference, and CLI reference. Use
`clipasm programs [NAME]` without a project, or browse the
[built-in program reference](https://nikitamgrimm.github.io/clipasm/reference/programs/index.html).

## Contribute and report security issues

Read [CONTRIBUTING.md](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTRIBUTING.md)
before changing ClipAsm. AI-assisted contributions follow
[AI_POLICY.md](https://github.com/NikitaMGrimm/clipasm/blob/main/AI_POLICY.md).
Report possible vulnerabilities privately through
[SECURITY.md](https://github.com/NikitaMGrimm/clipasm/blob/main/SECURITY.md).
