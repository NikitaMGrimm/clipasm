---
status: accepted
date: 2026-07-25
---

# 0017: Snapshot prepared data assets

## Context and problem statement

Preflight previously represented a prepared asset as an authored filesystem
path plus a content hash. Rendering re-hashed that path before passing it to
FFmpeg or an external process, but the consumer opened the path separately.
Another process or ordinary project edit could therefore replace the file
between verification and use.

The prepared plan is an in-memory value, compilation must remain media-pure,
and external executables and media tools retain platform-specific loader and
adjacent-resource behavior that copying can change.

## Decision outcome

Preflight snapshots every reachable media asset, external `file(...)` argument,
and File parameter into private plan-scoped storage while hashing it. Media
probing and rendering consume the snapshot. The resolved authored path remains
provenance for diagnostics, inspection, and destructive-output collision
checks, but later changes to that path do not change an existing prepared plan.

Snapshots are installed under generated digest-based names that preserve the
authored extension. Equal content with the same extension shares one snapshot
within a plan. Cloned prepared plans share ownership of the private storage,
which is removed after the last clone is dropped.

FFmpeg, FFprobe, and external executables are not copied. Their existing build
or executable identities and rendering-time verification remain in force
because relocating an executable can change dynamic-library lookup, script
behavior, signing, or adjacent-resource discovery.

External protocol version 2 records that declared File values are delivered as
immutable snapshot paths rather than authored paths. Prepared format version 10
includes that protocol revision in external prepared identity.

## Consequences

- Probing, prepared identity, and execution refer to the same captured bytes.
- A prepared plan remains executable when an authored data file changes or is
  removed.
- Preflight performs one bounded-memory copy of each distinct reachable asset
  and temporarily requires corresponding disk space.
- Snapshot paths are deliberately opaque and must not be treated as authored
  project locations by external programs.
- The storage protects against ordinary path mutation, not malicious code
  already running with the user's permissions.
- A persistent cross-run content store remains deferred because it would
  require locking, corruption recovery, pinning, garbage collection, and an
  on-disk compatibility policy.

## Confirmation

- The snapshot module owns streamed capture, digest naming, deduplication, and
  lifetime tests.
- Preflight probes snapshot paths and rendering has no data-asset path back to
  authored bytes.
- Integration tests mutate media, external script, and File-parameter sources
  after preflight and render from the captured plan.
- Prepared JSON continues to expose only authored provenance and content hash.

## Related decisions and supersession

- Related to [ADR 0001](0001-keep-compilation-pure.md).
- Refines the data-asset execution rules in
  [ADR 0012](0012-run-external-programs.md).
- Preserves the semantic/execution distinction from
  [ADR 0003](0003-separate-semantic-and-execution-identities.md).
