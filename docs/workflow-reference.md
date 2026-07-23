# Workflow reference

## Document shape

```yaml
version: 1

project:
  video:
    width: 1280
    height: 720
    fps: 30

clips: {}
timeline: []
output: final.mp4
```

`version` and `timeline` are required. `project`, `clips`, and `output` are
optional during compilation. Rendering requires `output`, whose extension must
be `.mp4`.

Relative media and output paths are resolved from the workflow file's
directory. Mapping order has no meaning. Sequence order is executable stack
order.

ClipAsm accepts one restricted YAML document. Duplicate keys, anchors, aliases,
custom tags, and nested argument mappings are rejected.

## Project video

`project.video` defines the common dimensions and frame rate. Defaults are
1280x720 at 30 fps.

```yaml
project:
  video: {width: 1920, height: 1080, fps: 30000/1001}
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
the foundation currently does this for a `during` selection.

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

Explicit inputs read references without consuming stack values:

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
frame, so the image approaches the middle equally from all directions.
`percent` defaults to 8 and must be a positive integer representable as `u32`.
Frame count, dimensions, and frame rate are unchanged.

### Wobble

```yaml
- wobble
- wobble: 4
```

`wobble` consumes one Video and applies deterministic phase-shifted horizontal
and vertical motion at a fixed `13/2` Hz. `pixels` defaults to 3 and must be
positive. Twice the requested movement must fit within both project dimensions
without integer overflow; this is the geometric padding required to keep a
project-sized moving crop inside the scaled frame. The effect exposes no
outside border, and the exact Video domain is unchanged.

### Flash

Typical use joins the top two Videos:

```yaml
- join:
    - flash
```

An explicit transition length uses integer shorthand:

```yaml
- join:
    - flash: 4
```

`flash` binds `before` and `after` in ordinary signature order. It returns the
complete `before` followed by the complete `after`, with the first `after`
frame white and a linear fade to normal over `frames`. It never overlaps,
shortens, or extends either input. When `frames` is omitted, the default is a
160-millisecond design choice converted to the smallest project-frame count
that covers that time, with a minimum of one frame. For example, it becomes 1
frame at 5 fps, 5 frames at 30 fps, and 20 frames at 120 fps. An explicit value
must be positive and no longer than `after`.

## Named values

### Clips

`clips` defines reusable named Videos. Each clip body starts with an empty local
stack and must finish with exactly one Video.

```yaml
clips:
  intro:
    - image:
        path: intro.png
        duration: 1s
    - repeat: 2

timeline:
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
    - concat
```

### Timeline

A nested `timeline` has no inputs, consumes nothing from the surrounding local
stack, starts its body with an empty local stack, and concatenates all Videos
left by that body. Its single result is then pushed onto the surrounding stack.

```yaml
- timeline:
    - $first
    - $second
```

The root `timeline` follows the same finalization rule.

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
`$name` inputs read named values and consume nothing.

| Scope | Initial stack | Required result |
|---|---|---|
| named clip | empty | exactly one Video |
| `join` | two preceding Videos | leftovers concatenated in order |
| nested `timeline` | empty | leftovers concatenated in order |
| `during` | selected range | exactly one Video, then splice |
| root `timeline` | empty | leftovers concatenated in order |

There is no hidden replacement, fallback input, or automatic reduction inside
clip and body-program bodies.

## Rendering

`validate` parses, type-checks, and infers every source-independent domain.

`compile` emits canonical JSON without opening assets or invoking tools.
Video-source durations may remain unresolved.

`render` performs preflight, resolves reachable assets and video durations,
checks FFmpeg capabilities, renders verified lossless intermediates, and
exports H.264/yuv420p MP4.

```console
cargo run -- validate workflow.yaml
cargo run -- compile workflow.yaml
cargo run -- compile workflow.yaml --output plan.json
cargo run -- render workflow.yaml
```

The cache is stored at `.clipasm/cache/` beside the workflow. Cache identity
includes the renderer contract, FFmpeg and FFprobe identities, media policy,
graph semantics, and source content hashes. Cached artifacts are verified
before reuse.

The output and manifest are staged and published through one rollback-capable
in-process transaction. Each final rename is atomic. If `render` returns an
error, ClipAsm attempts to preserve both previously published files. The pair
is not crash-atomic across process termination or power loss.
