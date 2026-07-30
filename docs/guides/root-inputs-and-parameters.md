# Supply root inputs and parameters

A root source program can ask its caller for Video or Audio inputs and scalar
parameters. This guide uses `examples/root-bindings.clipasm`, which declares one
Video, one time range, and one repeat count. You will bind all three and render
a two-second MP4.

## Before you start

Use a ClipAsm source checkout. Run the commands from its repository root. The
committed `examples/assets/gentle-motion.mkv` supplies the Video. Rendering also
requires FFmpeg and FFprobe.

## 1. Read the declarations

```clipasm
input video: Video
param range: TimeRange
param count: Integer

trim($video, $range)
repeat($count)
```

None of the declarations has a default. Every compiling command must supply all
three values.

## 2. Bind every required value

```console
$ clipasm validate examples/root-bindings.clipasm
> --video-input video=examples/assets/gentle-motion.mkv
> --arg range=500ms..1500ms
> --arg count=2
valid: 4 semantic value(s), 48 frame(s)

```

Use:

- `--video-input NAME=PATH` for a declared Video
- `--audio-input NAME=PATH` for a declared Audio
- `--arg NAME=VALUE` for a scalar parameter

Names are case-sensitive and must match the source declarations. Repeat an
option when a program declares several values of that kind.

If a binding is missing or misspelled, note the reported diagnostic code. Run
`clipasm explain <CODE>`. Then correct the command and validate again.

## 3. Render to an explicit output

The example has no configured output, so provide one:

```console,ignore
clipasm render examples/root-bindings.clipasm \
  --video-input video=examples/assets/gentle-motion.mkv \
  --arg range=500ms..1500ms \
  --arg count=2 \
  --output root-bindings.mp4
```

`repeat` repeats the selected one-second range twice. The result is a two-second
Video.
CLI-supplied paths resolve from the current working directory. Paths written in
a `.clipasm` file resolve from the directory containing that file.

## 4. Check the output

Open `root-bindings.mp4` in the repository root. Confirm that it lasts two
seconds. Use explicit `--output` because the example does not set `config.output`.

See [Root bindings](../reference/cli.md#root-bindings) for all accepted options
and [Source programs and imports](../concepts/source-programs-and-imports.md) for
the broader program model.
