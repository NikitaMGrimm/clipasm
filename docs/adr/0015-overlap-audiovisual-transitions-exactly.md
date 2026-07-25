---
status: accepted
---

# Overlap audiovisual transitions on exact boundaries

## Context

A real crossfade displays the end of one Video at the same time as the start of
another. It is not a cut followed by an effect on the latter clip. The output is
therefore shorter than ordinary concatenation, and attached Audio must overlap
on the same timeline interval as the picture.

Video duration is integral project frames while Audio duration is integral
samples. At fractional frame/sample ratios, independently rounding an overlap
from zero can disagree with the sample boundaries of its position in the final
Video. A global Video-frame output limit can also stop FFmpeg before the small
Audio tail required by coverage rounding has been encoded.

## Decision

`crossfade` is one concrete native Video program with `before` and `after`
inputs and an optional `duration`. The default is 500 milliseconds. The authored
duration becomes the smallest project-frame count that covers it and must cover
at least one frame. The overlap may not exceed either input.

For input frame counts `before`, `after`, and `overlap`, the output domain is:

```text
before + after - overlap
```

Rendering selects the exact Video prefix, both exact overlap regions, and the
exact suffix. The overlap uses a frame-indexed linear blend: its first frame is
the complete `before` picture and its final frame is the complete `after`
picture. A one-frame overlap is a defined equal blend.

Audio uses the cumulative frame-to-sample mapper from ADR 0013. Prefix,
overlap, and suffix sample ranges are derived from their absolute output frame
boundaries. The two overlap regions are normalized to the exact output overlap
sample count, faded linearly, delayed to the overlap start, and mixed on one
full-length output sample timeline. The suffix is likewise phase-adjusted and
delayed to its exact cumulative boundary. Meaningful attached-audio state is the
logical OR of the inputs; normalized silence follows the same render path.

Native Video filters are responsible for producing finite exact frame streams.
Working-artifact and final-export commands do not impose a separate
`-frames:v` limit. Artifact verification remains the authority for exact frame
and sample counts. This avoids truncating a coverage-rounded Audio tail at the
last Video timestamp.

The renderer policy change invalidates old cached artifacts, so cache execution
format version 8 records it. Compiled and prepared semantic formats do not
change for existing workflows.

## Consequences

- Crossfade duration, picture overlap, Audio overlap, and output shortening use
  one exact frame-domain contract.
- Fractional frame/sample ratios do not make Audio placement depend on source
  segmentation or packet boundaries.
- `flash` remains a non-overlapping cut effect with its existing summed domain.
- Additional overlapping transitions may reuse the established timing rules,
  but no generic transition kind or runtime transition framework is introduced
  until another operation demonstrates shared semantics.
