# ClipAsm

ClipAsm turns a small text program into a video. You describe sources and edits,
check the program without opening media, and render an MP4 with FFmpeg.

> ClipAsm is pre-release software. The language and command line may change as
> the project is simplified.

## Learn ClipAsm in order

After setup, the learning chapters follow one project centered on one evolving
composition. Each chapter starts from the previous checkpoint, introduces an
idea only when the edit needs it, and ends with a valid result you can inspect.
This path teaches ClipAsm's core video-composition workflow.

1. [Get ClipAsm running](learn/01-get-clipasm-running.md) and render the included
   starter.
2. Go [from one image to a sequence](learn/02-first-sequence.md).
3. [Name and reference a clip](learn/03-name-and-reference-clip.md).
4. [Transform one scene](learn/04-transform-scene.md).
5. [Add a flash between scenes](learn/05-transition.md).
6. [Change a named scene after assembly](learn/06-timeline-edit.md).
7. [Reuse a scene style across source files](learn/07-reusable-program.md).

If you only want to evaluate ClipAsm first, [try it in the browser](try-clipasm.md).
The playground contains a complete project and uploads nothing.

## How-to guides

Use these when you already have a concrete task:

- [Check a program before rendering](guides/validate-and-inspect.md)
- [Inspect compiled JSON](guides/inspect-compiled-json.md)
- [Supply root inputs and parameters](guides/root-inputs-and-parameters.md)
- [Import and call a source program](guides/import-a-program.md)
- [Add or replace a soundtrack](guides/add-audio.md)
- [Review and run an external program](guides/external-programs.md)
- [Troubleshoot validation, media, tools, rendering, and cache problems](guides/troubleshooting.md)

## Understand ClipAsm

These pages develop the underlying model without walking through the learning
project:

- [From source to published video](concepts/pipeline.md)
- [Stack ownership and visibility](concepts/stack-values.md)
- [Source programs and imports](concepts/source-programs-and-imports.md)
- [External programs and the trust boundary](concepts/external-programs-and-trust.md)

## Examples and reference

Use the [example catalog](examples.md) for small runnable programs. For exact
lookup, use:

- [Language reference](reference/language/index.md) for `.clipasm` syntax and behavior
- [Built-in programs](reference/programs/index.md) for call signatures and constraints
- [Command-line reference](reference/cli.md) for every CLI command and option
- [Diagnostics](diagnostics/index.html) for error-code guidance
- [Machine-readable contracts](reference/machine-contracts.md) for supported JSON integrations

When ClipAsm reports a diagnostic code, the quickest explanation is usually:

```console,ignore
clipasm explain E_UNKNOWN_PROGRAM
```

## Contributing

Contributor architecture and maintenance documents live in the repository but
are intentionally outside this user guide. Start with the repository's
[contribution workflow](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTRIBUTING.md).
Report possible vulnerabilities through the
[security policy](https://github.com/NikitaMGrimm/clipasm/blob/main/SECURITY.md).
