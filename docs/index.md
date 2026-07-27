# The ClipAsm guide

ClipAsm is a typed, stack-based language for assembling Video and Audio graphs.
Compilation creates a pure semantic graph without opening media files;
preflight resolves reachable media and tools; rendering uses FFmpeg to produce
the configured MP4 and FFprobe to verify it.

ClipAsm is pre-release software, so its language, Rust API, and CLI may change.
Supported machine-readable contracts carry explicit version fields; consumers
must reject versions they do not support.

You can [edit, validate, inspect, and render a video](try-clipasm.md) in the
browser before installing anything.

## Recommended learning path

1. [Try ClipAsm in the browser](try-clipasm.md), or install the CLI.
2. [Initialize a project and make your first render](getting-started/first-render.md).
3. [Build the scenic sequence](tutorials/scenic-sequence.md) by predicting and
   checking one source concept at a time.
4. [Build a reusable composition](tutorials/reusable-composition.md) to work
   with names, references, and composition.
5. Choose a task guide or concept below as your next project requires it.

`clipasm init` creates a standalone project, so the getting-started path does
not require a repository checkout. The [examples catalog](examples.md) is for
development examples in a source checkout; those examples may differ from the
starter bundled with an installed CLI. The [command-line reference](reference/cli.md)
defines the starter contents and lifecycle.

## Find what you need

To accomplish a specific task:

- [Validate and inspect a source file](guides/validate-and-inspect.md).
- [Supply root inputs and parameters](guides/root-inputs-and-parameters.md).
- [Import and call a source program](guides/import-a-program.md).
- [Review and run an external program](guides/external-programs.md).
- [Diagnose common failures](guides/troubleshooting.md).

To look up exact behavior, use the [command-line reference](reference/cli.md),
[programs and composition reference](reference/programs/index.md),
[diagnostics reference](reference/diagnostics/index.md),
[machine-readable contracts](reference/machine-contracts.md), and normative
[language reference](reference/language/index.md). Run `clipasm explain <CODE>`
after an error for an installed-binary lookup. To build a mental model
first, read about:

- [compilation, preflight, and rendering](concepts/pipeline.md);
- [stack values, ownership, and visibility](concepts/stack-values.md);
- [source programs and imports](concepts/source-programs-and-imports.md);
- [pure compilation and external-program trust](concepts/external-programs-and-trust.md).

## Public language and contributor internals

The language reference specifies public `.clipasm` syntax and behavior.
Tutorials, task guides, and concept pages teach that behavior without exposing
implementation history. Contributor architecture and maintenance documents
remain in the repository but are intentionally outside the user guide.

## Contributing

Start with the repository's
[contribution workflow](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTRIBUTING.md).
The
[AI contribution policy](https://github.com/NikitaMGrimm/clipasm/blob/main/AI_POLICY.md)
allows assisted work while keeping a human accountable for every submitted
change. Report possible vulnerabilities through the repository's
[security policy](https://github.com/NikitaMGrimm/clipasm/blob/main/SECURITY.md).
