# Build a reusable composition

This tutorial names a composed clip, reuses it, and supplies an inline Video to
a transition. The result is a four-second MP4 with a repeated opening, a flash
cut, and a closing scene.

## Before you start

Complete [Build the scenic sequence](scenic-sequence.md), then stay in that
initialized project so the three images remain available. To start separately,
create and enter a project:

```console,ignore
clipasm init reusable-video
cd reusable-video
```

Create `composition.clipasm`. The starter's three images will remain available
under `assets/`.

## 1. Configure a separate output

Begin with:

```clipasm
clipasm 1

config {
    video {
        width = 320
        height = 180
        fps = 24
    }
    output = "generated/reusable-composition.mp4"
}
```

## 2. Build and name two clips

Append:

```clipasm
clip {
    image("assets/morning.png", 1s, contain)
    zoom_in
} as opening

clip {
    image("assets/evening.png", 1s, contain)
} as closing
```

Each `clip` body becomes one Video. `as opening` and `as closing` give those
Videos immutable names. A `clip` does not leave its temporary result on the
outer stack, so it is safe to reference later exactly where it is needed.

## 3. Reuse a name and provide an inline input

Append:

```clipasm
$opening
flash_cut(
    before=$opening,
    after={
        image("assets/meadow.png", 1s, contain)
        zoom_in(2%)
    },
    duration=200ms,
)
$closing
concat
```

The first `$opening` places the named Video on the output sequence. The
`before=$opening` argument reads the same immutable value again without
consuming the first occurrence.

The `after={ ... }` block builds one isolated Video for the transition. Finally,
`$closing` adds the last scene and `concat` combines the three outer values.

## 4. Validate and render

```console,ignore
clipasm validate composition.clipasm
clipasm render composition.clipasm
```

Validation reports 96 frames. The result is four seconds at 24 fps: one second
for the standalone opening, two seconds for the flash-cut result, and one second
for the closing. Open `generated/reusable-composition.mp4` and check that the
opening appears twice before the closing scene.

## 5. Change the transition

Change `duration=200ms` to `duration=320ms`, then validate and render again:

```console,ignore
clipasm validate composition.clipasm
clipasm render composition.clipasm
```

The total remains 96 frames because `flash_cut` changes the visual transition
inside a two-second joined result; it does not shorten the two inputs.

## What you learned

You used `clip` to create reusable named Videos, referenced one value more than
once, supplied a graph input with an inline block, and assembled the final
sequence with `concat`.

See [Composition forms](../reference/language/composition-forms.md) for `clip`,
blocks, and names; [Stack binding](../reference/language/stack-binding.md) for
argument behavior; and [`flash_cut`](../reference/programs/flash_cut.md) for the
transition contract.
