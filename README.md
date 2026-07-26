# ClipAsm

ClipAsm is a typed, stack-based language for assembling Video and Audio graphs
from `.clipasm` source. Compilation creates a pure semantic graph without
opening media or invoking external tools; preflight resolves reachable assets,
tools, and exact media domains; rendering uses FFmpeg to produce an MP4 and
FFprobe to verify it.

> **Pre-release:** ClipAsm's language, file formats, Rust API, and CLI may
> change without compatibility guarantees.

You can
[edit, validate, inspect, and render the scenic example in your browser](https://nikitamgrimm.github.io/clipasm/try-clipasm.html)
without installing ClipAsm. Browser rendering supports uploaded still images
and video files; the native CLI remains the complete workflow.

## Install

Install ClipAsm from crates.io:

```console
cargo install clipasm --locked
clipasm --version
```

Installation requires Rust 1.95 or newer. Rendering additionally requires
FFmpeg and FFprobe on `PATH`. Native archives are attached to GitHub releases.

## Quick start

The committed scenic-sequence example uses three small images from
`examples/assets/`:

```clipasm
clipasm 1

config {
    video {
        width = 320
        height = 180
        fps = 24
    }
    output = "generated/scenic-sequence.mp4"
}

{
    image("assets/morning.png", 1500ms, contain)
    image("assets/meadow.png", 1500ms, contain)
    image("assets/evening.png", 1500ms, contain)
    concat
}
```

Run these commands from a repository checkout:

```console
clipasm validate examples/scenic-sequence.clipasm
clipasm inspect examples/scenic-sequence.clipasm
clipasm render examples/scenic-sequence.clipasm
```

`validate` parses, type-checks, and infers every source-independent domain.
`inspect` prints the compiled JSON document. `render` performs
preflight and writes `examples/generated/scenic-sequence.mp4` together with its
manifest and cache data.

Continue with the
[first-render guide](https://nikitamgrimm.github.io/clipasm/getting-started/first-render.html)
or the
[scenic-sequence tutorial](https://nikitamgrimm.github.io/clipasm/tutorials/scenic-sequence.html).

## How ClipAsm handles media

ClipAsm keeps authored intent separate from media execution:

1. **Compilation** parses and checks the complete source package, evaluates its
   stack programs, and creates a semantic graph without opening media files.
2. **Preflight** resolves reachable assets and tools, probes media, and lowers
   the graph into an exact prepared plan.
3. **Rendering** executes that plan, caches verified intermediate artifacts,
   and publishes the configured MP4 and manifest.

See
[Compilation, preflight, and rendering](https://nikitamgrimm.github.io/clipasm/concepts/pipeline.html)
for an accessible explanation and
[Architecture](https://nikitamgrimm.github.io/clipasm/architecture.html) for
the maintainer-level phase model.

## External programs are trusted code

An imported source program may delegate its implementation to an executable.
Compilation remains pure, but preflight resolves and hashes that executable and
rendering runs it as trusted code. Review external declarations and their
referenced files before rendering source you do not trust. The
[external-program guide](https://nikitamgrimm.github.io/clipasm/guides/external-programs.html)
describes the workflow and trust boundary. The committed external-program
example additionally requires Python 3 on `PATH`.

## Documentation

The [published ClipAsm guide](https://nikitamgrimm.github.io/clipasm/) is the
main entry point. Start with
[your first render](https://nikitamgrimm.github.io/clipasm/getting-started/first-render.html),
or use the
[CLI reference](https://nikitamgrimm.github.io/clipasm/reference/cli.html),
[language reference](https://nikitamgrimm.github.io/clipasm/language-reference.html),
and [architecture](https://nikitamgrimm.github.io/clipasm/architecture.html).

Contributors should follow
[CONTRIBUTING.md](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTRIBUTING.md).

`CONTEXT.md` owns settled domain language and authoring semantics. The language
reference owns public syntax and behavior, while Architecture and the ADRs
describe maintainer internals and durable design decisions.

## Development

Full contributor verification requires mdBook, FFmpeg, and FFprobe in addition
to Rust:

```console
./scripts/check.sh
```

AI-assisted contributions are welcome under the responsibility and review
requirements in
[AI_POLICY.md](https://github.com/NikitaMGrimm/clipasm/blob/main/AI_POLICY.md).

Report possible vulnerabilities privately as described in
[SECURITY.md](https://github.com/NikitaMGrimm/clipasm/blob/main/SECURITY.md).

The published book also includes the
[repository history chart](https://nikitamgrimm.github.io/clipasm/repository-history.html).
