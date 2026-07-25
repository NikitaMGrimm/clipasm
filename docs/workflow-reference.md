# YAML frontend reference

## Document shape

The current YAML frontend accepts a YAML sequence. Its first item must be
exactly the `program` header; every remaining item lowers to executable
canonical source:

```yaml
- program:
    version: 1

    project:
      video:
        width: 1280
        height: 720
        fps: 30

    imports: {}
    inputs: []
    parameters: {}
    clips: {}
    output: final.mp4

- image: {path: title.png, duration: 2s}
- video: footage.mp4
- concat
```

`version` is required. `project`, `imports`, `inputs`, `parameters`, `clips`,
`output`, and `stack_access` are optional. Only the root file may declare
`project` or `output`. Rendering requires `output`, whose extension must be
`.mp4`. Source programs explicitly default to `stack_access: owned`.

The source-program body owns no initial values and returns its complete final
owned values in order. Zero, one, or multiple outputs are valid for `validate`
and `compile`. It is not implicitly wrapped in `glue`. When the header contains
`output`, the source outputs must contain exactly one Video. Any number of
standalone Audio outputs may remain auxiliary; use `concat` or a nested `glue`
when several Videos should become the render result.

Relative import, media, and output paths resolve from the YAML source unit's
directory. A caller-supplied file parameter remains relative to the caller;
an imported program's literal default remains relative to that imported file.
Mapping order has no meaning. Sequence order is executable stack order.

Each source unit is one restricted YAML document. Duplicate keys, anchors,
aliases, custom tags, and multiple documents are rejected. Unknown
program-header fields are rejected.

## Authored programs and imports

One YAML file defines one callable source program. A root file imports another
file under an explicit local alias:

```yaml
- program:
    version: 1
    imports:
      repeat_twice: ./programs/repeat-twice.yaml

- image: {path: title.png, duration: 1s}
- repeat_twice
```

Aliases are local to the importing file, are not re-exported, and may not
collide with built-in program names. Two aliases may point to the same file;
the source definition is deduplicated while each invocation receives an
independent local scope. Import cycles, including self-import and a triangle
such as `yaml1 -> yaml2 -> yaml3 -> yaml1`, are rejected. Recursive authored
programs are not supported.

An authored interface declares ordered fixed `Video` or `Audio` inputs and
scalar parameters:

```yaml
- program:
    version: 1
    inputs:
      - video: Video
    parameters:
      count: Integer
      overlay:
        type: File
        default: overlay.png
      fit:
        type: Keyword
        values: [cover, contain, stretch]
        default: cover

- repeat:
    value: $video
    count: $count
```

`inputs` is a sequence because its order controls implicit stack binding.
Authored inputs are fixed-cardinality `Video` or `Audio` values. Parameter types
are the same shared types used by built-ins: `Integer`, `File`, `Duration`,
`TimeRange`, and `Keyword`. A parameter without a default is required.

An authored invocation uses the same explicit/implicit input binding,
`stack_access`, parameter conversion, `id`, and `ids` rules as a built-in. The
callee starts with an empty local stack. Its inputs are local graph-value
bindings and its parameters are local scalar bindings. Therefore `$video` may
be pushed as a body item, while standalone `$count` is an error; `$count` is
valid in a compatible scalar parameter position such as `repeat.count`.

Inputs, parameters, clips, and local output names share one local namespace.
They cannot collide. Local names do not escape an invocation. Ordered outputs
are inferred from the program body's complete ordered final owned values; no
`outputs:` declaration is required. Authored programs currently have no YAML
primary shorthand, postfix syntax, variadic inputs, or caller-supplied body.
Every linked authored program is checked, including unused imports. Invalid
parameter defaults, bodies, names, references, or output contracts therefore
fail compilation even when the root never invokes that program.

## External programs

A source unit may register trusted executable programs under local aliases:

```yaml
- program:
    version: 1
    externals:
      brighten: ./programs/brighten/program.json

- video: footage.mp4
- brighten:
    amount: 15
```

Manifest paths resolve relative to the YAML file declaring them. Aliases share
the imported-program namespace, are local to that source unit, and may not
collide with built-ins or authored imports. `parse_str` cannot load manifests;
file-backed loading is required.

The initial JSON manifest format is:

```json
{
  "format_version": 2,
  "protocol_version": 1,
  "semantic_version": 1,
  "command": "./brighten.py",
  "inputs": [
    {"name": "video", "type": "Video"}
  ],
  "parameters": [
    {"name": "amount", "type": "Integer", "required": true}
  ],
  "output": {"type": "Video", "preserve": "video"}
}
```

`command` is either a directly executable path relative to the manifest or one
executable name resolved from `PATH`. It is not a shell command and accepts no
inline argument string. The initial manifest supports fixed `Video` or `Audio`
inputs and `Integer` or `Keyword` parameters. Keyword parameters require a
nonempty `values` list. The one output must be `Video`; `preserve` names the
Video input whose exact frame domain and meaningful-audio state the output must
retain.

External calls use ordinary explicit and implicit input binding, parameter
validation, stack access, IDs, references, and output checks. YAML uses the full mapping form for external programs. Invocation shorthand is
frontend-owned metadata and is not part of the external manifest or shared
program descriptor.

Validation and compilation read the manifest but do not resolve or execute its
command. Preflight resolves the executable and records a content hash. Rendering
starts it directly and writes one JSON request to standard input:

```json
{
  "protocol_version": 1,
  "inputs": {
    "video": {
      "path": "/absolute/cache/input.mkv",
      "value_type": "Video",
      "domain": {
        "frames": 60,
        "width": 320,
        "height": 180,
        "frame_rate": {"numerator": 30, "denominator": 1}
      },
      "audio_domain": null,
      "has_audio": true
    }
  },
  "parameters": {"amount": 15},
  "output": "/absolute/cache/temporary.mkv",
  "project": {
    "video": {
      "width": 320,
      "height": 180,
      "fps": {"numerator": 30, "denominator": 1}
    },
    "audio": {"sample_rate": 48000, "channels": 2}
  },
  "tools": {
    "ffmpeg": "/absolute/path/to/ffmpeg",
    "ffprobe": "/absolute/path/to/ffprobe"
  }
}
```

The executable must write the requested output and exit successfully. The
working Video must contain exactly one project-sized Video stream and one
canonical Audio stream, retain the preserved input's exact frame count, and use
the renderer's working media contract. ClipAsm probes the result before cache
commit. Standard error is included in failure diagnostics.

External executables are trusted native code. Render only projects and manifests
you trust. ClipAsm does not sandbox them, impose a timeout, guarantee
termination, or make nondeterministic programs reproducible. Cache identity
cannot discover undeclared runtime dependencies such as interpreter versions,
imported modules, environment variables, clocks, random input, or network
responses; update the executable or manifest `semantic_version` whenever those
dependencies change output semantics. Compilation itself remains media- and
execution-pure.

### Root CLI bindings

The root source program may receive its declared interface directly from every
CLI pipeline command:

```console
clipasm validate template.yaml \
  --input video=footage.mp4 \
  --arg range=3s..8s \
  --arg count=2

clipasm compile template.yaml \
  --input video=footage.mp4 \
  --arg range=3s..8s \
  --arg count=2

clipasm render template.yaml \
  --input video=footage.mp4 \
  --arg range=3s..8s \
  --arg count=2 \
  --output final.mp4
```

`--input NAME=PATH` binds one declared root `Video` input as a normal full-file
`video` source using the default `cover` fit. `--arg NAME=VALUE` binds one
declared scalar parameter and applies its declared `Integer`, `File`,
`Duration`, `TimeRange`, or `Keyword` conversion. Both options may be repeated.
Missing, unknown, duplicate, and ill-typed bindings are errors.

CLI-supplied video paths, `File` parameter values, and `render --output` paths
resolve from the caller's working directory. Values authored in YAML continue
to resolve from the source unit containing them. `render --output` overrides
`program.output` for that invocation. The `compile --output` option remains the
compiled-JSON destination rather than a render destination.

## Project video

`project.video` defines the common dimensions and frame rate. Defaults are
1280x720 at 30 fps.

```yaml
- program:
    version: 1
    project:
      video: {width: 1920, height: 1080, fps: 30000/1001}

- video: footage.mp4
```

Authored times must align exactly to project frames. Ranges are closed-open:
`2s..4s` includes frames from 2 seconds up to, but not including, 4 seconds.

## Programs

| Program | Inputs | Parameters | Body |
|---|---|---|---|
| `image` | none | `path`, optional `duration`, optional `fit` | none |
| `video` | none | `path`, optional `fit` | none |
| `audio` | none | `path` | none |
| `extract_audio` | `video: Video` | none | none |
| `set_audio` | `video: Video`, `audio: Audio` | none | none |
| `repeat` | `value: Video|Audio` | `count`, optional `type` | none |
| `concat` | `values: Video...|Audio...` | optional `type` | none |
| `trim` | `value: Video|Audio` | `range`, optional `type` | none |
| `drop` | `value: Video|Audio` | optional `type` | none |
| `zoom` | `video: Video` | optional `percent` | none |
| `wobble` | `video: Video` | optional `pixels` | none |
| `flash` | `before: Video`, `after: Video` | optional `frames` | none |
| `join` | homogeneous `before`, `after`: Video or Audio | optional `type` | required |
| `glue` | none | optional `type` | required |
| `during` | `video: Video` | `range` | required |

`fit` is `cover`, `contain`, or `stretch`. The default is `cover`.

Every program definition explicitly declares a default stack access. Direct
built-ins and source programs default to `owned`; `join`, `glue`, and `during`
default to `visible`. Any invocation may override that default with generic
metadata inside its invocation mapping:

```yaml
- repeat:
    count: 2
    stack_access: visible
```

`stack_access` is not an input or parameter and does not propagate to child
invocations. `owned` limits missing-input binding to values owned by the current
body. `visible` may additionally consume enclosing values down to the nearest
visibility boundary. For a body program, the same setting also controls which
enclosing values are visible to its nested body. Its children still use their
own defaults or overrides. A no-op setting, such as `visible` on `image`, is
valid.

### Image

```yaml
- image:
    path: card.png
    duration: 2s
    fit: contain
```

An image must decode as exactly one video frame with no audio. `duration` is
required unless the surrounding body context supplies a requested duration;
`during` supplies the selected range's duration.

Primary shorthand is valid when no other parameter is needed:

```yaml
- image: card.png
```

This form still needs a duration-providing context.

### Video

```yaml
- video:
    path: footage.mp4
    fit: contain
```

A video uses its full intrinsic duration. Duration is resolved during
preflight, not pure compilation. The source must contain exactly one decodable
video stream. The first audio stream is imported when present; a source without
audio remains a valid silent Video.

`video` does not accept an authored duration.

The prepared duration is the smallest whole number of project frames whose
duration covers the complete source interval. An aligned duration is unchanged;
otherwise rendering may hold the final decoded image for less than one project
frame so the source is never shortened.


### Audio

```yaml
- audio: music.wav
```

`audio` loads the first audio stream from a supported media file. Preflight
normalizes it to the canonical 48 kHz stereo Audio format and records its exact
sample count.

### Extract audio

```yaml
- extract_audio:
    video: $clip
```

`extract_audio` consumes a Video and returns its synchronized Audio timeline. A
silent Video produces matching-duration silence, so authored audio workflows do
not need media-presence branches.

### Set audio

```yaml
- video: picture.mp4
- audio: music.wav
- set_audio
```

`set_audio` replaces a Video's attached audio beginning at time zero. The Video
determines the output duration: longer Audio is trimmed and shorter Audio is
padded with silence. The existing Video audio is replaced.

### Repeat

```yaml
- repeat: 3
```

`repeat` consumes the nearest accessible Video or Audio and produces the same
type repeated `count` times. `repeat: 3` means three copies in total. The full
form uses `value` for an explicit graph input:

```yaml
- repeat:
    value: $music
    count: 3
```

Naming the result does not change inference. This infers `Video` from the same
nearest stack value as an unnamed `repeat`:

```yaml
- image: {path: card.png, duration: 1s}
- repeat: 2
  id: doubled
```

Forward references to `doubled` use that inferred type. If an unresolved
forward type could change an earlier stack selection, the checker retries that
selection after later constraints resolve the type.

### Concat

```yaml
- $first
- $second
- concat
```

`concat` is homogeneous and returns the same type it consumes. Bare `concat`
works when exactly one accessible timeline type is present. When both Video and
Audio are accessible, select the intended typed stack view:

```yaml
- concat: Video
- concat: Audio
```

The selected invocation consumes every accessible value of that exact type in
physical order and leaves values of the other type untouched. With default
`stack_access: owned`, only values owned by the current body are eligible;
`stack_access: visible` may consume matching enclosing values down to the
nearest visibility boundary.

Explicit variadic inputs are reference-only and must be homogeneous:

```yaml
- concat:
    values: [$first, $second]
```

### Trim

```yaml
- trim: 1s..7s
```

`trim` consumes the nearest accessible Video or Audio and returns the same type
for the selected closed-open range. Video endpoints must align exactly to
project frames. Audio endpoints must align exactly to samples in the canonical
48 kHz format. The full form uses `value`:

```yaml
- trim:
    value: $music
    range: 1s..7s
```

Range bounds that depend on media duration are validated during preflight.

### Drop

```yaml
- drop
- drop: Audio
```

Bare `drop` removes the nearest accessible graph value and returns nothing.
`drop: Video` or `drop: Audio` removes the nearest accessible value of that
specific type. An explicit `value` is accepted but, like every explicit input,
does not remove a separate caller-stack occurrence of the referenced value.

### Zoom

```yaml
- zoom
- zoom: 12
```

`zoom` consumes one Video and applies a centered linear zoom-in over the full
clip, from 100% to `100 + percent` percent. The crop remains centered on every
frame. `percent` defaults to 8 and must be a positive integer representable as
`u32`. Frame count, dimensions, and frame rate are unchanged.

### Wobble

```yaml
- wobble
- wobble: 4
```

`wobble` consumes one Video and applies deterministic phase-shifted horizontal
and vertical motion at a fixed `13/2` Hz. `pixels` defaults to 3 and must be
positive. Twice the requested movement must fit within both project dimensions
without integer overflow. The effect exposes no outside border, and the exact
Video domain is unchanged.

### Flash

Typical stack use joins the top two Videos:

```yaml
- image: {path: before.png, duration: 1s}
- image: {path: after.png, duration: 1s}
- flash: 4
```

`flash` binds `before` and `after` in signature order. It returns the complete
`before` followed by the complete `after`, with the first `after` frame white
and a linear fade to normal over `frames`. It never overlaps, shortens, or
extends either input.

When `frames` is omitted, the default is a 160-millisecond design choice
converted to the smallest project-frame count that covers that time, with a
minimum of one frame. An explicit value must be positive and no longer than
`after`.

### Crossfade

Typical stack use consumes the nearest two Videos in `before`, then `after`
order:

```yaml
- image: {path: before.png, duration: 2s}
- image: {path: after.png, duration: 2s}
- crossfade: 500ms
```

Full form may supply either input explicitly:

```yaml
- crossfade:
    before: $first
    after: $second
    duration: 500ms
```

`crossfade` overlaps the end of `before` with the start of `after`. `duration`
defaults to 500 milliseconds and becomes the smallest project-frame count that
covers the authored time. It must cover at least one frame and may not exceed
either input. Known domains are checked during compilation; media-derived
domains are checked during preflight.

If the input lengths are `before`, `after`, and `overlap` frames, the output is
`before + after - overlap` frames. The first overlap frame is the complete
`before` picture, the final overlap frame is the complete `after` picture, and
intermediate frames blend linearly. A one-frame overlap is an equal blend.
Attached Audio fades over the exact sample interval corresponding to the same
cumulative frame boundaries. If only one input has meaningful Audio, it fades
to or from normalized silence.

## Explicit graph inputs

An explicit input may read a named value without consuming the surrounding
stack:

```yaml
- repeat:
    value: $card
    count: 3
```

A fixed, single-value input may instead evaluate an inline input body:

```yaml
- flash:
    before:
      - image: {path: before.png, duration: 2s}
      - zoom
    after:
      image: {path: after.png, duration: 2s}
    frames: 4
```

An inline input body:

- starts with a fresh empty isolated evaluation stack;
- inherits the enclosing requested-frame context;
- evaluates ordinary items;
- must leave exactly one value of the input port's declared type;
- neither consumes from nor pushes onto the surrounding stack.

A sequence supplies a multi-item body. One invocation mapping or an
unambiguous scalar invocation or reference supplies a one-item body. IDs inside
inline bodies use the same source-program invocation-local namespace as named
clips and all other item IDs.

```yaml
- flash:
    before:
      - image: {path: before.png, duration: 2s}
      - zoom:
        id: reusable_before
    after:
      image: {path: after.png, duration: 2s}

- $reusable_before
- wobble
```

Only fixed inputs support inline bodies. Variadic inputs accept one `$reference`
or a list of `$references`.

At an explicitly supplied graph-input boundary, one direct contextual adaptation
is allowed when the produced type differs from the port type:

- `Video` to `Audio` extracts the synchronized audio timeline;
- `Audio` to `Video` creates a project-sized black Video carrying that Audio.

The adaptation is a real semantic operation. Implicit stack binding, program
outputs, body outputs, and generic `value` or `values` inputs never adapt types.
Nested explicit concrete inputs may compose direct adaptations; for example,
Audio may be adapted to black Video for `zoom.video`, transformed, and then
adapted back for an outer Audio port.

Scalar parameters accept authored literals or compatible local scalar
parameter references. They cannot read graph-value references or receive
values from inline bodies.

## Named values

### Clips

`clips` in the program header defines reusable named Videos. Each clip body is
isolated, starts empty, and must finish with exactly one Video.

```yaml
- program:
    version: 1
    clips:
      intro:
        - image: {path: intro.png, duration: 1s}
        - repeat: 2

- $intro
```

Clips are definitions, not media storage. Unreferenced clips are still compiled
and validated but are not prepared or rendered.

### Item output names

`id` binds the single output of a one-output item:

```yaml
- video: footage.mp4
  id: source
- $source
```

Clip names and item output names share one namespace. Forward references are
allowed. Missing references, duplicate names, and cycles are errors.

`ids` completely names a multi-output item in output order:

```yaml
- split: 2s
  ids: [before, after]
```

If the stack was `[A, B, C]`, and the invocation produces `[before, after]`,
the resulting stack is `[A, B, C, before, after]`. `after` is the top value.
`ids` must contain exactly as many names as the item produces. `id` and `ids`
cannot be combined. Zero-output items cannot be named. Omitting both annotations
leaves every output on the stack unnamed.

### References

Plain reference:

```yaml
- $source
```

References read immutable values and consume nothing from the evaluation stack.
References produce one value and may use `id`. Named clips remain the explicit
declaration mechanism for reusable clip bodies.

## Body programs

Body programs use the same registered descriptors and input binding rules as
direct programs. They evaluate one nested body exactly once.

### Join

`join` resolves one homogeneous timeline type, consumes the nearest two
accessible values of that type, starts its body with both in order as locally
owned values, exposes them as `$before` and `$after`, and concatenates all owned
body values. Bare `join` is ambiguous when both Video and Audio can satisfy both
inputs; use `type: Video` or `type: Audio` to select the intended stack view.
Its default visible access lets it bind those inputs through an enclosing body
boundary. Default-owned children still consume only the two seeded values;
explicitly visible children may reach farther down the inherited visible
suffix. The single joined result is pushed onto the surrounding stack.

```yaml
- $first
- $second
- join:
    - flash
```


The port references remain usable after body operations consume the seeded
stack values. Explicit adaptation makes audiovisual overrides concise:

```yaml
- join:
    - flash
    - set_audio:
        audio: $before
```

Here `join` resolves to Video, so `flash` consumes the two seeded Videos.
`$before` still names the immutable original input; the explicit
`set_audio.audio` port adapts that Video to Audio, and the missing
`set_audio.video` port binds the flashed Video from the stack.

### Glue

A default `glue` has no inputs and starts its body with no owned values while
inheriting the enclosing visible suffix. It infers Video or Audio from the
homogeneous owned values left by its body, concatenates them, and pushes the
single result onto the surrounding stack. Mixed body outputs are an error.
`type: Video` or `type: Audio` may constrain the body explicitly.

```yaml
- glue:
    - $first
    - $second
```

`glue` is an ordinary nested body program. A source program receives no
implicit glue finalization.

A named `glue` normally infers its type from its homogeneous body, including
through forward references to other named values:

```yaml
- glue:
    - audio: first.wav
    - audio: second.wav
  id: soundtrack
```

`type: Video` or `type: Audio` remains an optional constraint and readability
aid. Naming never changes generic inference: selectors, explicit inputs, body
contracts, and ordinary stack binding provide the same evidence for named and
unnamed invocations. A selector is required only for genuine ambiguity,
deliberate selection, or an irreducible inference dependency. Dependency
cycles are reported as cycles; a type annotation does not make a cyclic graph
valid.

A child invocation that consumes an enclosing value must independently use
`stack_access: visible`:

```yaml
- image: {path: card.png, duration: 1s}
- glue:
    - repeat:
        count: 2
        stack_access: visible
    - zoom: 12
```

The first visible consumer may consume an enclosing Video and pushes its result
as a value owned by the glue body. Later default-owned operations may consume
that result.
Set `stack_access: owned` on `glue` when its body must establish a visibility
boundary.

### During

`during` consumes the nearest accessible Video, exposes the complete bound input
as `$video`, selects the range, evaluates its body with only the selection as an
owned value, and splices the single
owned result between the unchanged prefix and suffix. Its default visible
access lets it bind that Video through an enclosing body boundary and exposes the
same visible suffix to explicitly visible descendants.

Canonical form:

```yaml
- during:
    range: 4s..6s
    body:
      - repeat: 2
```

Postfix shorthand:

```yaml
- repeat: 2
  during: 4s..6s
```

Both forms repeat only the selected middle range and normalize identically.
An explicit `video: $name` reads that named value without consuming the outer
stack.

`id` and `ids` are item output annotations. A postfix-capable program such as
`during`
may appear beside one head invocation. Its scalar value remains shorthand for
the wrapper parameter, or its mapping may contain ordinary wrapper parameters
and generic metadata:

```yaml
- repeat: 2
  during:
    range: 4s..6s
    stack_access: visible
```

Program parameters otherwise belong inside the program mapping.

## Stack rules

Compilation uses one physical ordered stack containing both Video and Audio
values. Ownership is tracked per occurrence rather than as contiguous suffixes.

Missing fixed inputs bind from the last missing port to the first. Each port
consumes the nearest accessible value of its exact type. Values of other types
remain in place and preserve their relative order. A missing variadic input
consumes every accessible value of its declared type in physical order.
Implicit binding never adapts types.

Each body program exposes its resolved fixed graph inputs as local immutable
references named after the ports. Input expressions are evaluated in the caller
scope before those aliases are installed, so nested same-name ports use normal
lexical shadowing without self-reference. `body` is structural syntax, not a
port. A body program with no fixed inputs exposes no aliases.

| Scope | Initial owned values | Local port references | Finalization |
|---|---|---|---|
| source program | empty | authored inputs | return all ordered owned values |
| named clip | empty, isolated | none | exactly one Video |
| inline fixed input | empty, isolated | inherited caller scope | exactly one accepted value |
| `join` | two bound homogeneous timeline values | `$before`, `$after` | concatenate owned values of the resolved type |
| `glue` | none | none | infer or select one timeline type, then concatenate owned values |
| `during` | selected range | `$video` for the complete bound input | exactly one owned Video, then splice |

A body invocation with `stack_access: owned` creates a visibility boundary, so a
visible descendant cannot reach through it. Settings remain per invocation.
There is no hidden replacement, parent-stack fallback, or source-level
reduction. Named clips, inline inputs, and body contracts remain strict.

## Entrypoint publication and rendering

The source program returns its ordered semantic outputs. A configured `output`
path turns the source into a render entrypoint and therefore requires exactly
one Video among the ordered outputs. Any additional Audio outputs are auxiliary;
`render` publishes only the Video and preflights only its reachable graph.
Publication is not a semantic graph operation, and the output path does not change compiled semantic identity.

`validate` parses, type-checks, and infers every source-independent domain.

`compile` emits canonical JSON without opening assets or invoking tools.
Video-source durations may remain unresolved. With `--output`, it creates a new
file and refuses to replace an existing path.

`render` performs preflight, resolves result-reachable assets and video
durations, checks FFmpeg capabilities, verifies that the prepared FFmpeg and
FFprobe builds have not changed, renders verified FFV1+FLAC Video and FLAC Audio intermediates, and
exports H.264/yuv420p MP4 with AAC when the result Video has meaningful audio. Silent
Videos publish without an audio stream.

```console
cargo run -- validate program.yaml
cargo run -- compile program.yaml
cargo run -- compile program.yaml --output plan.json
cargo run -- render program.yaml
```

The cache is stored at `.clipasm/cache/` beside the source program. Cache
identity includes the renderer contract, FFmpeg and FFprobe identities, media
policy, graph semantics, and source content hashes. Cached artifacts are
verified before reuse.

The output and manifest are staged and published through one rollback-capable
in-process transaction. Each final rename is atomic. If `render` returns an
error, ClipAsm attempts to preserve both previously published files. The pair
is not crash-atomic across process termination or power loss. Existing output
and manifest destinations must be regular files; symlinks are rejected rather
than replaced or followed during publication.
