# Summary

[Introduction](index.md)

[Try ClipAsm](try-clipasm.md)

# Getting started

- [Install and render ClipAsm](getting-started/first-render.md)

# Tutorials

- [Build the scenic sequence](tutorials/scenic-sequence.md)
- [Build a reusable composition](tutorials/reusable-composition.md)

# How-to guides

- [Validate and inspect a program](guides/validate-and-inspect.md)
- [Supply root inputs and parameters](guides/root-inputs-and-parameters.md)
- [Import and call a source program](guides/import-a-program.md)
- [Review and run an external program](guides/external-programs.md)
- [Troubleshooting](guides/troubleshooting.md)

# Reference

- [Language](reference/language/index.md)
  - [Files and configuration](reference/language/files-and-configuration.md)
  - [Scalar values and expressions](reference/language/scalar-values-and-expressions.md)
  - [Timeline selectors and ranges](reference/language/timeline-selectors.md)
  - [Imports and external programs](reference/language/imports-and-external-programs.md)
  - [Statements and calls](reference/language/statements-and-calls.md)
  - [Stack binding](reference/language/stack-binding.md)
<!-- BEGIN GENERATED PROGRAM REFERENCE NAVIGATION -->
- [Programs and composition](reference/programs/index.md)
  - [`clip` and stack blocks](reference/language/composition-forms.md)
  - [`image`](reference/programs/image.md)
  - [`video`](reference/programs/video.md)
  - [`audio`](reference/programs/audio.md)
  - [`extract_audio`](reference/programs/extract_audio.md)
  - [`set_audio`](reference/programs/set_audio.md)
  - [`concat`](reference/programs/concat.md)
  - [`repeat`](reference/programs/repeat.md)
  - [`trim`](reference/programs/trim.md)
  - [`drop`](reference/programs/drop.md)
  - [`zoom_in`](reference/programs/zoom_in.md)
  - [`flash_cut`](reference/programs/flash_cut.md)
  - [`crossfade`](reference/programs/crossfade.md)
  - [`join`](reference/programs/join.md)
  - [`during`](reference/programs/during.md)
<!-- END GENERATED PROGRAM REFERENCE NAVIGATION -->

- [Machine-readable contracts](reference/machine-contracts.md)
- [Command-line reference](reference/cli.md)
- [Formal grammar](language-grammar.md)
- [Runnable examples](examples.md)

# Concepts and explanation

- [Compilation, preflight, and rendering](concepts/pipeline.md)
- [Stack values, ownership, and visibility](concepts/stack-values.md)
- [Source programs and imports](concepts/source-programs-and-imports.md)
- [External programs and trust](concepts/external-programs-and-trust.md)
