# 6. Change a named scene after assembly

The edit is now assembled, but its structure is still useful. In this chapter
you will name the finished edit, select its evening placement, and apply an
effect after composition without writing a brittle time range.

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

Validate:

```console,ignore
clipasm validate learning.clipasm
```

The result remains one 108-frame Video.

## 2. Select the evening placement

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
body's result is then spliced back into the original timeline.

## 3. Render the revised edit

```console,ignore
clipasm validate learning.clipasm
clipasm render learning.clipasm
```

Validation still reports 108 frames. Open `generated/learning.mp4`; the meadow
retains its original movement, and the evening now has a subtler zoom.

Names are useful beyond reuse: when named values reach a composition, they can
become stable placement paths for later timeline edits.

Next, [reuse a scene style across source files](07-reusable-program.md).
