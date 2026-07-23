# RhythmCut

RhythmCut is a strict programmatic-video foundation written in Rust. It
compiles a restricted YAML authoring format into an inspectable plan containing
only three rendering primitives—`image_video`, `slice`, and `concat`—then
executes that plan through FFmpeg.

The current scope intentionally supports only still-image video sources,
first-class references, concatenation, repetition, and the structural compounds
`then`, `during`, `join`, and `timeline`. It does not support video inputs,
audio, transitions, decorative effects, user programs, plugins, a GUI, or
distributed execution.

## Quick start

FFmpeg and FFprobe must be on `PATH` for rendering. Compilation and validation
do not invoke either tool.

```yaml
version: 1

project:
  video:
    width: 1280
    height: 720
    fps: 30

clips:
  card:
    - image:
        path: card.png
        duration: 1s
    - repeat: 3

timeline:
  - $card

output: final.mp4
```

```console
cargo run -- validate workflow.yaml
cargo run -- compile workflow.yaml
cargo run -- compile workflow.yaml --output plan.json
cargo run -- render workflow.yaml
```

`compile` is pure: it emits the typed semantic graph, exact frame domains,
named-value targets, explain data, and a formatting-independent structure hash
without opening media files or invoking external tools. `render` first
preflights reachable assets and FFmpeg capabilities, builds primitive IR with
content-based semantic fingerprints, then uses lossless FFV1 intermediates and
one final H.264/yuv420p MP4 export.

References are syntax rather than programs. Use `$card` directly, or the
expanded form when an annotation is needed:

```yaml
- ref: $card
  id: opening
```

Only `id` and `during` may be sibling fields. Non-primary parameters belong
inside the program mapping; `image: card.png` may use the primary shorthand,
but its duration must use the full mapping shown above.

## Stack rules

Sequence lists are typed postfix programs. Explicit `$name` inputs read
immutable values and consume nothing; missing inputs consume occurrences from
the top of the current local stack. Implicit `concat` consumes every remaining
local occurrence in order.

| Scope | Initial local stack | Finalization |
|---|---|---|
| named clip | empty | exactly one video required |
| `then` | one preceding value | exactly one video required |
| `during` | selected range of one preceding video | exactly one video, then splice |
| `join` | two preceding videos | exactly one video required |
| `timeline` | empty | concatenate all leftovers in order |

There is no hidden replacement or fallback input. For example, placing a
zero-input `image` inside `then` or `during` leaves the existing input and the
new image on the local stack and is therefore an error until an explicit
operation reduces them.

## Time and media contract

Durations use exact project-frame boundaries (`3s`, `500ms`, and ranges such as
`2s..4s`). Non-frame-aligned times are rejected instead of rounded. Every
semantic Video has exact dimensions, frame rate, and frame count. Lossless
working artifacts use square-pixel, non-subsampled yuv444p video; only the final
MP4 export converts to yuv420p. Defaults are 1280×720 at 30 fps.

## Development

```console
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
```

The render integration test skips cleanly when FFmpeg or FFprobe is absent.
