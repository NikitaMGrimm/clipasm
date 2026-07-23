---
status: accepted
---

# Quantize source duration by coverage

A full-duration video source produces the smallest integral number of project
frames whose duration covers the complete source interval.

For a source duration `n/d` seconds and project frame rate `p/q`, the prepared
frame count is:

```text
ceil(n * p / (d * q))
```

The implementation uses checked `u128` multiplication and quotient/remainder
ceiling division. It does not add `denominator - 1` before division because
that addition can overflow.

Consequently, a source is never shortened during frame-rate conversion.
Prepared output may exceed the source duration by less than one project frame,
and the final decoded image may be held for that partial final frame. Durations
already aligned to project frames remain unchanged. Preflight owns this
media-derived calculation, and rendering pads and trims explicitly to the same
prepared frame count.
