# 5. Add a flash between scenes

You now want a flash between the morning and meadow while keeping the cut to
evening unchanged. The transition needs those first two scenes as separate
Video values. Individual named clips now become useful.

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

Each clip stays off the outer stack until a reference places it there. The
earlier `pictures` grouping worked when the whole sequence moved together.
Individual scene clips are useful when an operation needs two scenes separately.

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

## 3. Validate the assembly

Validate before adding the transition:

```console,ignore
clipasm validate learning.clipasm
```

The result remains 108 frames.

## 4. Add the transition

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

The code references evening only after `flash_cut`, so evening is not a
transition input. One ordinary cut follows the flash transition.

## 5. Validate the transition

```console,ignore
clipasm validate learning.clipasm
```

Validation still reports 108 frames because `flash_cut` places its inputs
sequentially.

## 6. Render the result

```console,ignore
clipasm render learning.clipasm
```

## 7. Check the transition

Open `generated/learning.mp4`. Confirm that one white flash appears between
morning and meadow. Evening follows the transition.

You used named clips to address scenes independently. A fixed-input program then
consumed the correct stack values in order.

Next, [change a named scene after assembly](06-timeline-edit.md).
