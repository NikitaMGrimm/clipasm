---
status: accepted
date: 2026-07-26
---

# 0017: Run FFmpeg recipes through host adapters

## Context and problem statement

Native rendering executes prepared primitives through local FFmpeg processes,
files, verification, and cache orchestration. The browser playground needs the
same operation and timing behavior in a virtual filesystem, but cannot use
native paths, processes, or external programs.

Duplicating FFmpeg filters in JavaScript would let the two renderers drift.
A general renderer-backend interface would expose an extension boundary even
though native operations are deliberately closed and phase-owned.

## Decision outcome

The renderer owns one closed, exhaustive conversion from prepared primitives to
typed FFmpeg recipes. A recipe contains literal arguments plus typed asset and
artifact references; it is not a shell command or a public plugin interface.

The native adapter materializes references as platform paths and retains cache,
locking, process, verification, and publication ownership. The browser adapter
materializes references as virtual paths, runs a pinned single-threaded FFmpeg
WebAssembly runtime in a worker, verifies exact artifact contracts, and returns
the final MP4 without a persistent cache.

Browser preparation remains pure. The host supplies normalized virtual asset
paths and SHA-256 facts for the immutable blobs it will mount. For a video-file
source, the browser worker also checks decodability and returns bounded FFprobe
stream metadata; Rust validates that document and derives the exact project
frame domain before constructing the plan. Standalone Audio-file sources remain
unsupported. External programs remain native-only trusted executables.

The browser plan identifies its recipe contract, runtime versions, and encoding
policy. Browser work, file sizes, elapsed time, and retained logs are bounded;
cancellation discards the render worker and its virtual filesystem.

## Consequences

- Native and browser rendering share operation arguments, codecs, and exact
  frame/sample mapping.
- Browser rendering downloads about 31 MiB on first use and is slower than
  native rendering because the selected runtime is single-threaded.
- A video-file source loads that runtime during probing, before recipe
  construction.
- The FFmpeg WebAssembly core is separately distributed under
  GPL-2.0-or-later; its version, integrity, license, and source information must
  remain visible and pinned.
- Adding another host requires an explicit architecture decision. This record
  does not establish a generic renderer backend.

## Confirmation

The prepared-primitive dispatcher and recipe argument enum remain exhaustive.
Native recipe tests assert exact materialization, browser preparation tests
assert the versioned plan and artifact contracts, and CI builds the pinned
WebAssembly playground. Browser behavior is exercised against the scenic
sequence and an uploaded video source before release.

## Related decisions

- [ADR 0001](0001-keep-compilation-pure.md)
- [ADR 0003](0003-separate-semantic-and-execution-identities.md)
- [ADR 0012](0012-run-external-programs.md)
- [ADR 0014](0014-map-frame-and-sample-boundaries.md)
- [ADR 0015](0015-keep-native-operations-phase-owned.md)
- [ADR 0016](0016-overlap-audiovisual-transitions-exactly.md)
