# 5. Add a flash between scenes

You now want a flash between the morning and meadow while keeping the cut to
evening unchanged. The transition needs those first two scenes as separate
Video values, so this is the point where individual named clips become useful.

Continue editing `learning.clipasm` from
[Transform one scene](04-transform-scene.md).

## 1. Name each scene

Replace the `pictures` clip and its reference with three scene clips:

```clipasm
clip {
    image("assets/morning.png", 1500ms, contain)
} as morning

clip {
    image("assets/meadow.png", 1500ms, contain)
    zoom_in(4%)
} as meadow

clip {
    image("assets/evening.png", 1500ms, contain)
} as evening
```

Each clip stays off the outer stack until it is referenced. The earlier
`pictures` grouping worked when the whole sequence moved together; individual
scene clips are useful now that an operation needs two scenes separately.

## 2. Assemble the scenes

Append:

```clipasm
$morning
$meadow
$evening
concat
```

This is the familiar stack sequence: three references leave three Videos, and
`concat` returns one.

Validate before adding the transition:

```console,ignore
clipasm validate learning.clipasm
```

The result remains 108 frames.

## 3. Let the transition consume its inputs

Insert `flash_cut(200ms)` immediately after `$meadow`:

```clipasm
$morning
$meadow
flash_cut(200ms)
$evening
concat
```

`flash_cut` needs a `before` Video and an `after` Video. With those inputs
omitted, it consumes the two nearest Videos: morning first, then meadow.

```text
$morning  -> [morning]
$meadow   -> [morning, meadow]
flash_cut -> [morning-to-meadow]
$evening  -> [morning-to-meadow, evening]
concat    -> [finished video]
```

The evening is referenced only after `flash_cut`, so it is not a transition
input. There is one flash transition followed by one ordinary cut.

## 4. Render the result

```console,ignore
clipasm validate learning.clipasm
clipasm render learning.clipasm
```

Validation still reports 108 frames: `flash_cut` places its two inputs
sequentially. Open `generated/learning.mp4` and check for one white flash between
morning and meadow, followed by evening.

You used named clips to make scenes independently addressable, then let a
fixed-input program consume the correct stack values in order.

Next, [change a named scene after assembly](06-timeline-edit.md).
