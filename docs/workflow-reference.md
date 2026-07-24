# Source-program reference

## Document shape

A ClipAsm source file is a YAML sequence. Its first item must be exactly the
`program` header; every remaining item is executable:

```yaml
- program:
    version: 1

    project:
      video:
        width: 1280
        height: 720
        fps: 30

    clips: {}
    output: final.mp4

- image: {path: title.png, duration: 2s}
- video: footage.mp4
- concat
```

`version` is required. `project`, `clips`, and `output` are optional during
compilation. Rendering the source file as the CLI entrypoint requires `output`,
whose extension must be `.mp4`.

The source-program body starts with an empty local stack and must finish with
exactly one Video. It is not implicitly wrapped in `timeline`. If multiple
Videos should become the result, use `concat` or a nested `timeline`
explicitly.

Relative media and output paths resolve from the source file's directory.
Mapping order has no meaning. Sequence order is executable stack order.

ClipAsm accepts one restricted YAML document. Duplicate keys, anchors, aliases,
custom tags, and multiple documents are rejected. Unknown program-header fields
are rejected; source-program inputs, parameters, imports, and names are not yet
supported.

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
| `timeline` | none | none | required |
| `during` | `base: Video` | `range` | required |

`fit` is `cover`, `contain`, or `stretch`. The default is `cover`.

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

Implicit `concat` consumes every remaining Video in the current local stack,
preserving order.

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

- starts with a fresh empty local stack;
- inherits the enclosing requested-frame context;
- evaluates ordinary items;
- must leave exactly one value of the input port's declared type;
- neither consumes from nor pushes onto the surrounding stack.

A sequence supplies a multi-item body. One invocation mapping or an
unambiguous scalar invocation or reference supplies a one-item body. IDs inside
inline bodies use the same global namespace as named clips and all other item
IDs.

Only fixed inputs support inline bodies. Variadic inputs accept one `$reference`
or a list of `$references`.

Scalar parameters remain authored literals. They cannot read references or
receive values from inline bodies.

## Named values

### Clips

`clips` in the program header defines reusable named Videos. Each clip body
starts with an empty local stack and must finish with exactly one Video.

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

### Item IDs

`id` binds an item's result:

```yaml
- video: footage.mp4
  id: source
- $source
```

Clip names and item IDs share one namespace. Forward references are allowed.
Missing references, duplicate names, and cycles are errors.

### References

Plain reference:

```yaml
- $source
```

References read immutable values and consume nothing from the local stack.
References cannot carry `id`; named clips are the explicit alias mechanism.

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

### Timeline

A `timeline` has no inputs, consumes nothing from the surrounding local stack,
starts its body empty, and concatenates all Videos left by that body. Its single
result is pushed onto the surrounding stack.

```yaml
- timeline:
    - $first
    - $second
```

`timeline` is an ordinary nested body program. A source program receives no
implicit timeline finalization.

### During

`during` consumes a base Video, selects the range, evaluates its body with the
selection on the local stack, and splices the single result between the
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

`id` is the only item annotation. A postfix-capable program such as `during`
may appear beside one head invocation; its scalar value becomes the wrapper
parameter. Program parameters otherwise belong inside the program mapping.

## Stack rules

Missing inputs consume values from the top of the current local stack. Explicit
references read named values without consuming stack occurrences. Inline fixed
inputs execute on isolated stacks.

| Scope | Initial stack | Required result |
|---|---|---|
| source program | empty | exactly one Video |
| named clip | empty | exactly one Video |
| inline fixed input | empty | exactly one value of the port type |
| `join` | two preceding Videos | leftovers concatenated in order |
| `timeline` | empty | leftovers concatenated in order |
| `during` | selected range | exactly one Video, then splice |

There is no hidden replacement, fallback input, or source-level reduction.
Named clips, inline inputs, the source program, and `during` require exactly one
result. Only `join` and `timeline` explicitly concatenate their leftover local
Videos.

## Entrypoint publication and rendering

The source program always returns its semantic Video result. When invoked as
the CLI entrypoint, `render` additionally publishes that result to the
configured `output`. Publication is not a semantic graph operation, and the
output path does not change compiled semantic identity.

`validate` parses, type-checks, and infers every source-independent domain.

`compile` emits canonical JSON without opening assets or invoking tools.
Video-source durations may remain unresolved.

`render` performs preflight, resolves result-reachable assets and video
durations, checks FFmpeg capabilities, renders verified lossless intermediates,
and exports H.264/yuv420p MP4.

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
