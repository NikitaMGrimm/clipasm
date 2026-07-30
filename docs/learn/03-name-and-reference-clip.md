# 3. Name and reference a clip

Your three-image sequence publishes correctly, but it exists only as values
immediately consumed by `concat`. In this chapter you will package those
statements as one clip, give it an identity, and reference it later.

Continue editing `learning.clipasm` from
[From one image to a sequence](02-first-sequence.md).

## 1. Replace `concat` with `clip`

Replace the four executable statements with:

```clipasm
clip {
    image("assets/morning.png", 1500ms, contain)
    image("assets/meadow.png", 1500ms, contain)
    image("assets/evening.png", 1500ms, contain)
}
```

A `clip` collects the Video values left by its body and concatenates them in
order. The body therefore does not need its own `concat`.

## 2. Validate the clip

Validate this version:

```console,ignore
clipasm validate learning.clipasm
```

Expect `E_ENTRYPOINT_OUTPUT_COUNT` again. This time the diagnostic says that
zero Videos remain, not three. The `clip` form removes its temporary result
from the outer stack, so merely creating a clip does not publish it.

## 3. Give the clip a name

Add `as pictures` after the closing brace:

```clipasm
clip {
    image("assets/morning.png", 1500ms, contain)
    image("assets/meadow.png", 1500ms, contain)
    image("assets/evening.png", 1500ms, contain)
} as pictures
```

`as pictures` preserves the composed Video under an immutable graph name. It
still does not place a Video on the outer stack.

## 4. Reference the named value

Add a reference after the clip:

```clipasm
clip {
    image("assets/morning.png", 1500ms, contain)
    image("assets/meadow.png", 1500ms, contain)
    image("assets/evening.png", 1500ms, contain)
} as pictures

$pictures
```

`$pictures` places an occurrence of the named Video at that point in the
program. It does not move or change the underlying value.

## 5. Validate the reference

Validate:

```console,ignore
clipasm validate learning.clipasm
```

The file again leaves one 108-frame Video ready to publish.

You used `clip` to make one composition and `as` to name it. You used
`$pictures` to place it on the stack where needed.

Next, [transform one scene](04-transform-scene.md).
