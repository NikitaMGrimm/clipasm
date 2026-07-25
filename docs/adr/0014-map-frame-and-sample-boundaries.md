---
status: accepted
---

# Map frame and sample boundaries cumulatively

Video duration remains an integer number of project frames. Standalone Audio
duration remains an integer number of samples in the project audio format.
ClipAsm does not replace either native unit with a least-common-denominator
master tick.

One exact rational mapper owns every conversion between the two grids. For
frame boundary `f`, sample rate `s`, and reduced frame rate `p/q`, the covering
sample boundary is:

```text
B(f) = ceil(f * s * q / p)
```

A frame range `start..end` receives the sample range
`B(start)..B(end)`. Adjacent ranges therefore telescope exactly: splitting and
rejoining a Video timeline cannot accumulate independently rounded audio
lengths.

Video concatenation and joins normalize each input audio stream to its
allocation between cumulative frame boundaries before combining streams. Video
repeat keeps one compact semantic and prepared node; rendering timestamps each
repeated audio segment at its cumulative boundary and uses FFmpeg asynchronous
resampling to distribute unavoidable single-sample corrections instead of
leaving all drift at the end.

Authored Video times must still be exactly frame-aligned, and authored Audio
times must still be exactly sample-aligned. Coverage conversion is used only
where an operation explicitly requires enough frames or samples to contain a
native duration.

Standalone audio sources use their stream timeline duration when available. That
rational duration is mapped to the project sample grid with the same covering
policy. Decoded source-sample counts divided by the source sample rate are a
fallback only when stream duration metadata is absent; codec priming or discard
padding therefore does not redefine an otherwise declared timeline.

This changes execution behavior but not compiled or prepared semantics. The
cache execution format is therefore bumped, while compiled and prepared format
versions and semantic hashes remain unchanged.
