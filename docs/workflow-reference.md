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
owned suffix in order. Zero, one, or multiple outputs are valid for `validate`
and `compile`. It is not implicitly wrapped in `glue`. When the header contains
`output`, the source must produce exactly one Video; use `concat` or a nested
`glue` when several Videos should become that render result.

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

An authored interface declares ordered fixed Video inputs and scalar
parameters:

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
    video: $video
    count: $count
```

`inputs` is a sequence because its order controls implicit stack binding.
Initial authored inputs are fixed-cardinality `Video` values. Parameter types
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
are inferred from the program body's complete final owned suffix; no
`outputs:` declaration is required. Authored programs currently have no YAML
primary shorthand, postfix syntax, variadic inputs, or caller-supplied body.

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
| `repeat` | `video: Video` | `count` | none |
| `concat` | `videos: Video...` | none | none |
| `trim` | `video: Video` | `range` | none |
| `zoom` | `video: Video` | optional `percent` | none |
| `wobble` | `video: Video` | optional `pixels` | none |
| `flash` | `before: Video`, `after: Video` | optional `frames` | none |
| `join` | `before: Video`, `after: Video` | none | required |
| `glue` | none | none | required |
| `during` | `base: Video` | `range` | required |

`fit` is `cover`, `contain`, or `stretch`. The default is `cover`.

Every program definition explicitly declares a default stack access. All
current programs default to `owned`. Any invocation may override that default
with generic metadata inside its invocation mapping:

```yaml
- repeat:
    count: 2
    stack_access: visible
```

`stack_access` is not an input or parameter and does not propagate to child
invocations. `owned` limits missing-input binding to the current body's owned
suffix. `visible` may additionally capture values from the visible suffix down
to the nearest visibility boundary. For a body program, the same setting also
controls the suffix visible to its nested body. A no-op setting, such as
`visible` on `image`, is valid.

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
video stream. Source audio is ignored.

`video` does not accept an authored duration.

The prepared duration is the smallest whole number of project frames whose
duration covers the complete source interval. An aligned duration is unchanged;
otherwise rendering may hold the final decoded image for less than one project
frame so the source is never shortened.

### Repeat

```yaml
- repeat: 3
```

`repeat: 3` produces three copies in total. It consumes one implicit Video
unless `video` is supplied explicitly.

### Concat

```yaml
- $first
- $second
- concat
```

Implicit `concat` consumes every Video in its accessible suffix, preserving
order. With the default `stack_access: owned`, that is the current body's owned
suffix. `stack_access: visible` deliberately consumes the complete visible
suffix down to the nearest visibility boundary.

Explicit variadic inputs remain reference-only:

```yaml
- concat:
    videos: [$first, $second]
```

### Trim

```yaml
- trim: 1s..7s
```

`trim` consumes one Video and selects the closed-open range locally within that
Video. The authored endpoints must align exactly to project frames. It uses the
same range validation as `during`, including deferred validation during
preflight for video-file inputs.

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

## Explicit graph inputs

An explicit input may read a named value without consuming the surrounding
stack:

```yaml
- repeat:
    video: $card
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
inline bodies use the same global namespace as named clips and all other item
IDs.

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

Scalar parameters remain authored literals. They cannot read references or
receive values from inline bodies.

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

`join` consumes the two preceding Videos, starts its body with both in order,
and concatenates all Videos left by the body. The single joined result is pushed
onto the surrounding stack.

```yaml
- $first
- $second
- join:
    - flash
```

### Glue

A default `glue` has no inputs, owns no surrounding values, starts its body with
no owned values, and concatenates the body's owned Videos. Its single result is
pushed onto the surrounding stack.

```yaml
- glue:
    - $first
    - $second
```

`glue` is an ordinary nested body program. A source program receives no
implicit glue finalization.

A visible `glue` may expose surrounding visible values to its body. The child
invocation that actually consumes such a value must independently use
`stack_access: visible`:

```yaml
- image: {path: card.png, duration: 1s}
- glue:
    stack_access: visible
    body:
      - repeat:
          count: 2
          stack_access: visible
      - zoom: 12
```

The first visible consumer captures the preceding Video into the glue body's
owned suffix. Later default-owned operations may consume that captured result.

### During

`during` consumes a base Video, selects the range, evaluates its body with the
selection as an owned value, and splices the single owned result between the
unchanged prefix and suffix.

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
An explicit `base: $name` reads that named value without consuming the outer
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

Compilation uses one physical evaluation stack. Each active body frame tracks:

- a visible suffix, bounded by the nearest visibility boundary;
- an owned suffix, consumed by default-owned invocations and the finalizer.

Missing fixed inputs consume the exact required suffix from the invocation's
accessible region while preserving descriptor order. A missing variadic input
consumes the complete accessible region. Binding never searches around a value
of the wrong type. Explicit references read named values without consuming
stack occurrences. Inline fixed inputs execute on isolated stacks.

| Scope | Initial owned values | Finalization |
|---|---|---|
| source program | empty | return the complete ordered owned suffix |
| named clip | empty, isolated | exactly one Video |
| inline fixed input | empty, isolated | exactly one value of the port type |
| `join` | two bound Videos | concatenate owned Videos in order |
| `glue` | none | concatenate owned Videos in order |
| `during` | selected range | exactly one owned Video, then splice |

When a visible invocation consumes below a body's ownership frontier, that
suffix becomes owned. Captured ownership propagates to the enclosing body when
the child body finishes. An owned body creates a new visibility boundary, so a
visible descendant cannot reach through it. Settings are per invocation and do
not inherit.

There is no hidden replacement, parent-stack lookup, or source-level reduction.
Named clips, inline inputs, and `during` still require exactly one result. The
source program returns zero or more outputs literally. Only `join` and `glue`
explicitly concatenate their owned Videos.

## Entrypoint publication and rendering

The source program returns its ordered semantic outputs. A configured `output`
path turns the source into a render entrypoint and therefore requires exactly
one Video output. `render` publishes that Video. Publication is not a semantic
graph operation, and the output path does not change compiled semantic identity.

`validate` parses, type-checks, and infers every source-independent domain.

`compile` emits canonical JSON without opening assets or invoking tools.
Video-source durations may remain unresolved. With `--output`, it creates a new
file and refuses to replace an existing path.

`render` performs preflight, resolves result-reachable assets and video
durations, checks FFmpeg capabilities, verifies that the prepared FFmpeg and
FFprobe builds have not changed, renders verified lossless intermediates, and
exports H.264/yuv420p MP4.

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
is not crash-atomic across process termination or power loss.
