# 6. Change a named scene after assembly

The edit is now assembled, but its structure is still useful. In this chapter
you will name the finished edit and select its evening placement. You will then
apply an effect without writing a fixed time range.

Continue editing `learning.clipasm` from
[Add a flash between scenes](05-transition.md).

## 1. Save the assembled edit

Replace the final five statements with a named clip:

```clipasm
clip {
    $morning
    $meadow
    flash_cut(200ms)
    $evening
} as edit

$edit
```

The body produces the morning-to-meadow transition and the evening clip.
`clip` concatenates those values into one Video. The bare `$evening` reference
also gives that occurrence the placement name `evening` inside `edit`.

## 2. Validate the assembled edit

Validate:

```console,ignore
clipasm validate learning.clipasm
```

The result remains one 108-frame Video.

## 3. Select the evening placement

Add `during` after `$edit`:

```clipasm
$edit
during($edit::evening) {
    zoom_in(2%)
}
```

`$edit::evening` is the exact range occupied by the named evening occurrence.
Unlike `3s..4500ms`, the selector continues to identify that scene if an earlier
scene changes duration.

`during` consumes the complete edit. Its body starts with the selected evening
slice on the body stack, so `zoom_in` can consume that slice normally. The
`during` call then splices the body's result into the original timeline.

## 4. Validate the revised edit

```console,ignore
clipasm validate learning.clipasm
```

Validation still reports 108 frames.

## 5. Render the revised edit

```console,ignore
clipasm render learning.clipasm
```

## 6. Check the timeline edit

Open `generated/learning.mp4`. The meadow retains its original movement. The
evening now has a subtler zoom.

Names have uses beyond reuse. When named values reach a composition, they can
become stable placement paths for later timeline edits.

Next, [reuse a scene style across source files](07-reusable-program.md).
