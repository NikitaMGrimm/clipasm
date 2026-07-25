# ClipAsm

ClipAsm is a typed, stack-based language for assembling Video and Audio graphs
from `.clipasm` source. Compilation creates a pure semantic graph without
opening media or invoking external tools; preflight resolves reachable assets,
tools, and exact media domains; rendering uses FFmpeg to produce an MP4 and
FFprobe to verify it.

> **Pre-release:** ClipAsm's language, file formats, Rust API, and CLI may
> change without compatibility guarantees.

## Quick start

To build ClipAsm from this repository, install:

- Rust 1.95 or newer;
- FFmpeg and FFprobe on `PATH` for rendering.

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

glue {
    image("assets/morning.png", 1500ms, contain)
    image("assets/meadow.png", 1500ms, contain)
    image("assets/evening.png", 1500ms, contain)
}
```

Run these commands from the repository root:

```console
cargo run -- validate examples/scenic-sequence.clipasm
cargo run -- inspect examples/scenic-sequence.clipasm
cargo run -- render examples/scenic-sequence.clipasm
```

`validate` parses, type-checks, and infers every source-independent domain.
`inspect` prints the compiled JSON document. `render` performs
preflight and writes `examples/generated/scenic-sequence.mp4` together with its
manifest and cache data.

Continue with the
[first-render guide](docs/getting-started/first-render.md) or the
[scenic-sequence tutorial](docs/tutorials/scenic-sequence.md).

## How ClipAsm handles media

ClipAsm keeps authored intent separate from media execution:

1. **Compilation** parses and checks the complete source package, evaluates its
   stack programs, and creates a semantic graph without opening media files.
2. **Preflight** resolves reachable assets and tools, probes media, and lowers
   the graph into an exact prepared plan.
3. **Rendering** executes that plan, caches verified intermediate artifacts,
   and publishes the configured MP4 and manifest.

See [Compilation, preflight, and rendering](docs/concepts/pipeline.md) for an
accessible explanation and [Architecture](docs/architecture.md) for the
maintainer-level phase model.

## External programs are trusted code

An imported source program may delegate its implementation to an executable.
Compilation remains pure, but preflight resolves and hashes that executable and
rendering runs it as trusted code. Review external declarations and their
referenced files before rendering source you do not trust. The
[external-program guide](docs/guides/external-programs.md) describes the
workflow and trust boundary. The committed external-program example additionally
requires Python 3 on `PATH`.

## Documentation

The [published ClipAsm guide](https://nikitamgrimm.github.io/clipasm/) is the
main documentation entry point.

- **Learn ClipAsm:** start with
  [your first render](docs/getting-started/first-render.md), then work through
  the [scenic sequence](docs/tutorials/scenic-sequence.md) and
  [reusable composition](docs/tutorials/reusable-composition.md).
- **Complete a task:** learn how to
  [validate and inspect](docs/guides/validate-and-inspect.md),
  [supply root inputs and parameters](docs/guides/root-inputs-and-parameters.md),
  [import a source program](docs/guides/import-a-program.md), or
  [review and run an external program](docs/guides/external-programs.md). Start
  with [troubleshooting](docs/guides/troubleshooting.md) when a command fails.
- **Look up exact behavior:** use the
  [command-line reference](docs/reference/cli.md), normative
  [language reference](docs/language-reference.md), and the
  [runnable examples catalog](docs/examples.md).
- **Understand the design:** read the
  [concept explanations](docs/concepts/pipeline.md),
  [architecture](docs/architecture.md), and
  [architecture decision index](docs/adr/index.md).
- **Contribute:** follow [Contributing](CONTRIBUTING.md), use the
  [change guide](docs/development/change-guide.md), and keep documentation
  consistent with the [documentation maintenance guide](docs/development/documentation.md).

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
requirements in [AI_POLICY.md](AI_POLICY.md).

Report possible vulnerabilities privately as described in
[SECURITY.md](SECURITY.md).

The published book also includes the
[repository history chart](https://nikitamgrimm.github.io/clipasm/repository-history.html).
