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
  - [Names, blocks, and `clip`](reference/language/names-blocks-and-clip.md)
  - [Legacy language-reference links](language-reference.md)
<!-- BEGIN GENERATED PROGRAM REFERENCE NAVIGATION -->
- [Built-in programs](reference/programs/index.md)
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
- [Command-line reference](reference/cli.md)
- [Formal grammar](language-grammar.md)
- [Runnable examples](examples.md)

# Concepts and explanation

- [Compilation, preflight, and rendering](concepts/pipeline.md)
- [Stack values, ownership, and visibility](concepts/stack-values.md)
- [Source programs and imports](concepts/source-programs-and-imports.md)
- [External programs and trust](concepts/external-programs-and-trust.md)

# Design and internals

- [Architecture](architecture.md)
- [Architecture decisions](adr/index.md)
  - [ADR template](adr/template.md)
  - [0001: Keep compilation pure](adr/0001-keep-compilation-pure.md)
  - [0002: Use one program model](adr/0002-use-one-program-model.md)
  - [0003: Separate semantic and execution identities](adr/0003-separate-semantic-and-execution-identities.md)
  - [0004: Quantize source duration by coverage](adr/0004-quantize-source-duration-by-coverage.md)
  - [0005: Treat source files as programs](adr/0005-treat-source-files-as-programs.md)
  - [0007: Support ordered program outputs](adr/0007-support-ordered-program-outputs.md)
  - [0008: Separate parsing from canonical source](adr/0008-separate-parsing-from-canonical-source.md)
  - [0009: Call authored source programs](adr/0009-call-authored-source-programs.md)
  - [0010: Add typed Audio and body input scopes](adr/0010-add-typed-audio-and-body-input-scopes.md)
  - [0011: Add type-preserving timeline programs](adr/0011-add-type-preserving-timeline-programs.md)
  - [0012: Run registered external programs](adr/0012-run-external-programs.md)
  - [0013: Adopt the native ClipAsm language](adr/0013-adopt-native-clipasm-language.md)
  - [0014: Map frame and sample boundaries cumulatively](adr/0014-map-frame-and-sample-boundaries.md)
  - [0015: Keep native operations closed and phase-owned](adr/0015-keep-native-operations-phase-owned.md)
  - [0016: Overlap audiovisual transitions on exact boundaries](adr/0016-overlap-audiovisual-transitions-exactly.md)
  - [0017: Run FFmpeg recipes through host adapters](adr/0017-run-ffmpeg-recipes-through-host-adapters.md)
  - [0018: Evaluate scalar expressions exactly](adr/0018-evaluate-scalar-expressions-exactly.md)
  - [0019: Model rooted timeline layouts separately from media values](adr/0019-model-rooted-timeline-layouts.md)

# Development

- [Change guide](development/change-guide.md)
- [Documentation maintenance](development/documentation.md)
