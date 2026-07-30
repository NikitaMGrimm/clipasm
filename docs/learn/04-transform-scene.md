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

`zoom_in` needs one Video. This call has no explicit Video input. It consumes
the nearest Video on the stack, which is the meadow. The call leaves the
transformed Video in its place:

```text
[morning, meadow] -> zoom_in -> [morning, zoomed meadow]
```

The following `image` call creates the evening scene. The clip therefore still
combines three scenes in the original order.

## 2. Validate the source

```console,ignore
clipasm validate learning.clipasm
```

## 3. Render the video

```console,ignore
clipasm render learning.clipasm
```

Validation still reports 108 frames because `zoom_in` preserves duration.

## 4. Check the result

Open `generated/learning.mp4`. Confirm that only the meadow moves.

Calls that omit Video or Audio inputs bind matching Video or Audio values from
the stack. Their position is therefore part of the program's meaning.

Next, [add a flash between scenes](05-transition.md).
