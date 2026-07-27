# Supply root inputs and parameters

A root source program can declare graph inputs and scalar parameters for the
caller to provide. This guide validates and renders the committed
`examples/root-bindings.clipasm` program with one Video input, one `TimeRange`,
and one `Integer`.

Run all commands from the repository root.

## Identify the required bindings

The example declares:

```clipasm
input video: Video
param range: TimeRange
param count: Integer

trim($video, $range)
repeat($count)
```

None of these declarations has a default, so the CLI caller must bind all three.
The body trims the supplied Video to the requested range and repeats the result.

## Validate with the bindings

Use `--video-input` for the graph input and one `--arg` for each scalar
parameter:

```console
$ clipasm validate examples/root-bindings.clipasm
> --video-input video=examples/assets/gentle-motion.mkv
> --arg range=500ms..1500ms
> --arg count=2
valid: 4 semantic value(s), 48 frame(s)

```

Binding names must match their declarations. The input value must have the
declared graph type, and each scalar value must parse as its declared parameter
type.

## Render and choose the output

The example does not declare an output path, so provide one to `render`:

```console,ignore
clipasm render examples/root-bindings.clipasm \
  --video-input video=examples/assets/gentle-motion.mkv \
  --arg range=500ms..1500ms \
  --arg count=2 \
  --output root-bindings.mp4
```

Because the command runs from the repository root, both the CLI-supplied media
path and `root-bindings.mp4` resolve from that directory. Paths authored inside
a `.clipasm` source unit instead resolve from the directory containing that
source unit. The published Video is two seconds long: the selected one-second
range repeated twice.

Use `--audio-input name=path` for a declared root Audio input. Repeat
`--video-input`, `--audio-input`, or `--arg` when a source program declares more
than one corresponding binding.

The [root-bindings reference](../reference/cli.md#root-bindings) defines the
accepted flags and path bases. See the [examples catalog](../examples.md#root-bindings)
for the canonical command listing and
[Source programs and imports](../concepts/source-programs-and-imports.md) for
the source-program mental model.
