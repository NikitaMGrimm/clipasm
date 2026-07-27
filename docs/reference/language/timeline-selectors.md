# Timeline selectors and ranges

Composed Video and Audio timelines expose native-grid placement markers. Video
boundaries are exact project frames; Audio boundaries are exact project samples.
Explicit names on values that reach a final concatenation become placement names:

```clipasm
clip {
    image("title.png", 1s) as intro
    image("credits.png", 2s) as credits
} as edit

$edit
during($edit::credits) {
    zoom_in(2%)
}
```

Selector paths may be nested, such as
`$edit::chapter::interview::start`. A placement selector without a final
boundary denotes its complete closed-open range. Terminal `::start`,
`::middle`, and `::end` select exact coordinates and remain reserved as boundary
words. A placement with one of those spellings is selected with an additional
boundary component, for example
`$edit::middle::start..$edit::middle::end`; bare `$edit::middle` always remains
the midpoint of `$edit`. A uniquely placed bare reference contributes
its reference name implicitly, and identity-preserving programs such as
`zoom_in` retain that marker. When an operation has already bound its timeline,
a selector may omit leading ancestors when the remaining suffix identifies one
addressable descendant. For example, `$interview::start` or
`$chapter::interview::start` may stand for a longer path under the bound root.
Multiple matches are ambiguous and require more leading names or the owning
timeline. Lookup is performed over the shared view DAG without expanding every
reused occurrence. Explicitly rooted selectors remain exact paths.

Selector structure follows names rather than operation history. Anonymous
composition layers are transparent, so these forms expose the same direct
`a` and `b` placements:

```clipasm
image("a.png", 1s) as a
image("b.png", 1s) as b
concat
concat as edit
```

```clipasm
image("a.png", 1s) as a
image("b.png", 1s) as b
join { concat } as edit
```

Both accept `$edit::a` and `$edit::b`. Anonymous one-input concatenation,
stack-block boundaries, and associative regrouping do not add path components.
Naming an occurrence does create a boundary:

```clipasm
image("a.png", 1s) as a
image("b.png", 1s) as b
concat as pair
image("c.png", 1s) as c
concat as edit

trim(value=$edit, range=$edit::pair::a)
```

Here `$edit::a` is invalid because the authored `pair` boundary cannot be
skipped. A name continues to denote the exact view captured at its declaration,
even when later anonymous composition wraps that occurrence.

At one parent level, a placement spelling is addressable only when exactly one
occurrence has that spelling. Explicit `as` labels, inferred bare-reference
labels, and operation-created labels do not shadow one another. Any duplicate
spelling is ambiguous and needs a distinct explicit name.

Marker ranges must be used with the timeline that owns their root. `join`
preserves the exact views of untouched inputs and exposes named values created
by its body as placements in the joined result. `trim` and `during` accept rooted
marker ranges for both Video and Audio. `trim`
preserves child placements only when their complete closed-open region is
provably inside the selected range, rebasing their starts to the trimmed
timeline. Partially surviving or symbolically uncertain placements are omitted.
A trimmed occurrence keeps its own placement label when later composed.

Audio uses the same selector, contextual-suffix, and interval-replacement rules:

```clipasm
audio("intro.wav") as intro
audio("song.wav") as song
join as mix

during(timeline=$mix, range=$mix::song) {
    repeat(2)
}
```

`during` splices timeline layouts as well as media. Base placements fully before
the replaced range keep their coordinates. Placements fully after it shift by
the replacement-duration delta. Placements that intersect the replaced range,
or whose side cannot be proven symbolically, are omitted. The inserted body is
available as `::replacement`, retaining its nested layout. That spelling is
reserved by the `during` result contract: if a base placement named
`replacement` survives the edit, compilation reports
`E_TIMELINE_PLACEMENT_CONFLICT` instead of shadowing either occurrence. A base
placement with that name is permitted when the selected range removes it.

Transitions expose operation-owned regions. `flash_cut` provides sequential
`::before` and `::after` regions. `crossfade` provides `::before`, `::after`,
and the shared `::overlap` region:

```clipasm
image("before.png", 2s)
image("after.png", 2s)
crossfade(500ms) as transition

trim(range=$transition::overlap)
```

The `before` and `after` regions retain their nested placement layouts, so a
path such as `$transition::before::title` remains addressable. Their ranges
overlap in a crossfade and remain sequential in a flash cut. All normal
boundaries, including `::middle`, apply to these regions.

Timeline coordinates use exact rational arithmetic. Coordinates with the same
root may be added or subtracted, Number may scale them, and Duration may offset
them:

```clipasm
during(
    50% * ($edit::intro::start + $edit::credits::start)
        ..($edit::credits::end - 500ms)
) {
    zoom_in(2%)
}
```

Intermediate coordinates may be negative or beyond the owning timeline. Exact
native-grid alignment, ordering, and final bounds are checked only when the
expression is consumed as a TimeRange. `::middle` is therefore valid as an
exact rational coordinate even when it falls between frames or samples, but
using an unaligned value reports the applicable frame- or sample-alignment
error. Video and Audio `trim` retain marker expressions whose boundaries depend
on unprobed media and resolve them during preflight after the referenced source
domains are known. The prepared operation contains an ordinary exact frame or
sample range. `during` uses the same deferred native-range model and lowers to
existing slice and concat primitives. A Video `during` body receives the selected
extent symbolically, so an `image` without an explicit duration inherits that
media-dependent extent and is resolved to a concrete frame count during
preflight. Audio `during` does not reinterpret a sample extent as a Video frame
request.

Aliases make long marker expressions reusable:

```clipasm
credits_lead_in = $edit::credits::start - 500ms
credits_end = $edit::credits::end

during($credits_lead_in..$credits_end) {
    zoom_in(2%)
}
```

Timeline selectors inside aliases require their explicit root. Contextual
suffix lookup such as `$interview::start` or `$chapter::interview::start` is
available directly in a timeline-anchored call, but an alias should use the
complete rooted path so its meaning is independent of later uses.

Timeline selector diagnostics print the compiler's actual rooted occurrence
layout. Each child includes its root-relative closed-open range. The tree is the
canonical selector layout, not a record of anonymous operation wrappers.
Genuinely unnamed leaves and ambiguous labels are marked as not directly
addressable. Mixed-root arithmetic shows both roots, and a marker range used
with the wrong input shows the marker root beside the bound input layouts.
Diagnostic trees are capped at 64 occurrences and 12 nesting levels.
