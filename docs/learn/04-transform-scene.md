# 4. Transform one scene

The sequence is structurally complete. Now you will add movement to the meadow
without changing the other scenes or the total duration.

Continue editing `learning.clipasm` from
[Name and reference a clip](03-name-and-reference-clip.md).

## 1. Add the effect where it belongs

Place `zoom_in(4%)` immediately after the meadow image:

```clipasm
clip {
    image("assets/morning.png", 1500ms, contain)
    image("assets/meadow.png", 1500ms, contain)
    zoom_in(4%)
    image("assets/evening.png", 1500ms, contain)
} as pictures

$pictures
```

`zoom_in` needs one Video. Because no Video was supplied explicitly, it consumes
the nearest Video on the stack—the meadow—and leaves the transformed Video in
its place:

```text
[morning, meadow] -> zoom_in -> [morning, zoomed meadow]
```

The evening image is created afterward, so the clip still combines three scenes
in the original order.

## 2. Validate and render

```console,ignore
clipasm validate learning.clipasm
clipasm render learning.clipasm
```

Validation still reports 108 frames because `zoom_in` preserves duration. Open
`generated/learning.mp4` and check that only the meadow moves.

Calls that omit Video or Audio inputs bind matching Video or Audio values from
the stack. Their position is therefore part of the program's meaning.

Next, [add a flash between scenes](05-transition.md).
