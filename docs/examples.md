# Runnable examples

The repository's `examples/` directory contains small programs for development
and experimentation. Run the commands below from the repository root.
`validate` checks source only; `render` also requires FFmpeg and FFprobe.

For a standalone project without a repository checkout, use `clipasm init` and
follow [Install and render ClipAsm](getting-started/first-render.md).

## Example catalog

| Example | Demonstrates | Expected render | Extra requirement | Guide |
| --- | --- | --- | --- | --- |
| `examples/scenic-sequence.clipasm` | `clip`, image sources, a name, and a reference | 4.5 seconds | — | [Scenic sequence tutorial](tutorials/scenic-sequence.md) |
| `examples/crossfade.clipasm` | exact crossfade overlap | 3.5 seconds | — | [`crossfade` reference](reference/programs/crossfade.md) |
| `examples/gentle-motion-edit.clipasm` | `during` and `zoom_in` | 2 seconds | — | [`during` reference](reference/programs/during.md) |
| `examples/reusable-composition.clipasm` | named clips, references, and stack-bound `flash_cut` | 3 seconds | — | [Flash-cut tutorial](tutorials/add-a-transition.md) |
| `examples/imported-program.clipasm` | importing a ClipAsm source program | 2 seconds | — | [Import guide](guides/import-a-program.md) |
| `examples/external-brighten.clipasm` | trusted external implementation | 2 seconds | Python 3 and a code review | [External-program guide](guides/external-programs.md) |
| `examples/root-bindings.clipasm` | root Video input and required parameters | 2 seconds | CLI bindings and `--output` | [Root-bindings guide](guides/root-inputs-and-parameters.md) |

## Validate or render an example

Most examples use the same two-command pattern:

```console,ignore
clipasm validate examples/scenic-sequence.clipasm
clipasm render examples/scenic-sequence.clipasm
```

Generated outputs, manifests, and caches are ignored by Git.

## Root bindings

This example requires one Video input and two scalar parameters:

```console,ignore
clipasm validate examples/root-bindings.clipasm \
  --video-input video=examples/assets/gentle-motion.mkv \
  --arg range=500ms..1500ms \
  --arg count=2

clipasm render examples/root-bindings.clipasm \
  --video-input video=examples/assets/gentle-motion.mkv \
  --arg range=500ms..1500ms \
  --arg count=2 \
  --output root-bindings.mp4
```

## External program

`examples/external-brighten.clipasm` may execute Python and FFmpeg during
rendering. Review the declaration and script before running it:

```console,ignore
clipasm validate examples/external-brighten.clipasm
clipasm inspect examples/external-brighten.clipasm
clipasm render examples/external-brighten.clipasm
```

External programs are trusted native code. Read
[Review and run an external program](guides/external-programs.md) first.
