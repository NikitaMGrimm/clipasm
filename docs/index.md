# The ClipAsm guide

ClipAsm turns a small text program into a video. You describe media sources and
operations, ClipAsm checks the program before opening media, and `render`
produces an MP4 with FFmpeg.

Try the [browser playground](try-clipasm.md) to edit and render the included
scenic sequence without installing anything. To work locally, follow
[Install and render ClipAsm](getting-started/first-render.md).

> ClipAsm is pre-release software. The language and command line may change as
> the project is simplified.

## Choose a path

| Goal | Start here |
| --- | --- |
| Try ClipAsm in the browser | [Try ClipAsm](try-clipasm.md) |
| Install the CLI and render a project | [Install and render ClipAsm](getting-started/first-render.md) |
| Learn the language one idea at a time | [Build the scenic sequence](tutorials/scenic-sequence.md) |
| Add a transition between named clips | [Add a flash-cut transition](tutorials/add-a-transition.md) |
| Solve a specific problem | [How-to guides](#how-to-guides) |
| Understand how the phases and stack fit together | [Concepts](#understand-the-model) |
| Look up exact syntax or behavior | [Reference](#reference) |

`clipasm init` creates a normal standalone project. You do not need a Git
checkout, and ClipAsm does not continue managing the files after creating them.
See the [`init` reference](reference/cli.md#init) for its exact lifecycle.

## How-to guides

- [Check a program before rendering](guides/validate-and-inspect.md)
- [Supply root inputs and parameters](guides/root-inputs-and-parameters.md)
- [Import and call a source program](guides/import-a-program.md)
- [Review and run an external program](guides/external-programs.md)
- [Troubleshoot validation, media, tools, rendering, and cache problems](guides/troubleshooting.md)

## Understand the model

These pages explain why the language behaves as it does without requiring
compiler internals:

- [From source to published video](concepts/pipeline.md)
- [Stack values, ownership, and visibility](concepts/stack-values.md)
- [Source programs and imports](concepts/source-programs-and-imports.md)
- [External programs and the trust boundary](concepts/external-programs-and-trust.md)

## Explore runnable examples

Use the [example catalog](examples.md) when you have a source checkout and want
small programs to validate, render, or adapt.

## Reference

Use reference pages when you already know what you need to look up:

- [Language reference](reference/language/index.md) for `.clipasm` syntax and behavior
- [Built-in programs](reference/programs/index.md) for call signatures and constraints
- [Command-line reference](reference/cli.md) for every CLI command and option
- [Diagnostics](diagnostics/index.html) for error-code guidance
- [Machine-readable contracts](reference/machine-contracts.md) for supported JSON integrations

After an error, the quickest lookup is usually:

```console,ignore
clipasm explain E_UNKNOWN_PROGRAM
```

## Contributing

Contributor architecture and maintenance documents live in the repository but
are intentionally outside this user guide. Start with the repository's
[contribution workflow](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTRIBUTING.md).
Report possible vulnerabilities through the
[security policy](https://github.com/NikitaMGrimm/clipasm/blob/main/SECURITY.md).
