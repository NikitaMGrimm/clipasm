# Examples

The committed `.clipasm` programs under `examples/` are small executable
language demonstrations. Run their commands from the repository root.
Validation is pure; rendering additionally requires FFmpeg and FFprobe.
Follow the links below for the exact
[built-in program contracts](reference/programs/index.md) used by each example.

## Starter project

`clipasm init [PATH]` creates a standalone project without a repository
checkout. Its bundled starter includes `main.clipasm` and three images in
`assets/`. In an initialized project, render directly:

```console,ignore
clipasm render main.clipasm
```

Use `clipasm validate main.clipasm` when you want a faster source-only check
without opening media or invoking FFmpeg.

The starter is a starting point, not a managed template. The
[CLI reference](reference/cli.md#init) defines its compatibility and lifecycle.
The repository programs below are development examples; they may differ from
the starter shipped by an installed binary.

## Scenic sequence

`examples/scenic-sequence.clipasm` combines three still images with `concat`
inside a stack block.

```console,ignore
clipasm validate examples/scenic-sequence.clipasm
clipasm render examples/scenic-sequence.clipasm
```

## Exact crossfade

`examples/crossfade.clipasm` overlaps two still images for 500 milliseconds
with [`crossfade`](reference/programs/crossfade.md). The Video and attached
Audio timelines use the same exact frame boundaries.

```console,ignore
clipasm validate examples/crossfade.clipasm
clipasm render examples/crossfade.clipasm
```

## Gentle motion edit

`examples/gentle-motion-edit.clipasm` applies
[`zoom_in`](reference/programs/zoom_in.md) only to a selected range through
[`during`](reference/programs/during.md).

```console,ignore
clipasm validate examples/gentle-motion-edit.clipasm
clipasm render examples/gentle-motion-edit.clipasm
```

## Reusable composition

`examples/reusable-composition.clipasm` demonstrates `clip`, immutable
references, named graph inputs, and final concatenation.

```console,ignore
clipasm validate examples/reusable-composition.clipasm
clipasm render examples/reusable-composition.clipasm
```

## Imported program

`examples/imported-program.clipasm` imports
`examples/programs/polish.clipasm` as an ordinary typed program.

```console,ignore
clipasm validate examples/imported-program.clipasm
clipasm render examples/imported-program.clipasm
```

## External program

`examples/external-brighten.clipasm` imports a native `.clipasm` program whose
implementation runs a small Python/FFmpeg script through an explicit interpreter
and content-hashed file argument. It uses the ordinary typed binder and a native
parameter default. External programs are trusted code and execute during
rendering; this example requires `python3` on `PATH`.

```console,ignore
clipasm validate examples/external-brighten.clipasm
clipasm inspect examples/external-brighten.clipasm
clipasm render examples/external-brighten.clipasm
```

## Root bindings

`examples/root-bindings.clipasm` declares one Video input and two required
scalar parameters.

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

Generated outputs, manifests, and caches are ignored by Git in the repository
and by the starter project's `.gitignore`.
