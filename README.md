# ClipAsm

ClipAsm is a typed, stack-based language for Video and Audio graphs. Compilation
is media-pure. Preflight resolves reachable assets and tools. Rendering uses
FFmpeg and FFprobe to publish an MP4.

> **Pre-release:** ClipAsm's language, Rust API, and CLI may change. Supported
> machine-readable contracts use explicit version fields.

[Try ClipAsm in the browser](https://nikitamgrimm.github.io/clipasm/try-clipasm.html)
to edit, validate, inspect, and render the scenic sequence without installing
anything. The native CLI supports complete source packages, imports, and
the full native feature set.

## Install and make a first video

ClipAsm requires Rust 1.95 or newer. Rendering also requires FFmpeg and FFprobe
on `PATH`.

1. Install ClipAsm:

   ```console,ignore
   cargo install clipasm --locked
   ```

2. Create a project:

   ```console,ignore
   clipasm init hello-video
   ```

3. Enter the project directory:

   ```console
   cd hello-video
   ```

4. Render the project:

   ```console,ignore
   clipasm render
   ```

`init` creates a self-contained project with the scenic sequence and its three
images. `render` discovers `clipasm.toml` and performs the required source
checks.

Use `clipasm validate` for a faster check that does not open media. After
rendering, open `generated/scenic-sequence.mp4`.

Next, [build the sequence from one image](https://nikitamgrimm.github.io/clipasm/learn/02-first-sequence.html).
The [documentation](https://nikitamgrimm.github.io/clipasm/) also contains the
learning path, task guides, language reference, and CLI reference.

Use `clipasm programs [NAME]` without a project. You can also browse the
[built-in program reference](https://nikitamgrimm.github.io/clipasm/reference/programs/index.html).
For a diagnostic code, run `clipasm explain <CODE>`. You can also browse the
[diagnostic reference](https://nikitamgrimm.github.io/clipasm/diagnostics/).

## Contribute and report security issues

Read [CONTRIBUTING.md](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTRIBUTING.md)
before changing ClipAsm. AI-assisted contributions follow
[AI_POLICY.md](https://github.com/NikitaMGrimm/clipasm/blob/main/AI_POLICY.md).
Report possible vulnerabilities privately through
[SECURITY.md](https://github.com/NikitaMGrimm/clipasm/blob/main/SECURITY.md).
