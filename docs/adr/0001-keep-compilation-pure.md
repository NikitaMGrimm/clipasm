---
status: accepted
---

# Keep compilation pure

ClipAsm separates compilation, preflight, and rendering. Compilation parses and
type-checks the source program, evaluates its bodies, builds the semantic graph,
infers every domain available from authored data, and computes structural
identity without opening media or invoking external tools. Preflight is the
first phase allowed to resolve files, probe media, inspect FFmpeg and FFprobe,
and lower the reachable semantic graph into an exact prepared plan. Rendering
executes only that prepared plan.

This boundary keeps compilation deterministic, fast, and usable for validation
when assets or tools are unavailable. It also makes deferred facts explicit:
for example, a video-file source may have an unknown frame count after
compilation and an exact frame count after preflight. Moving media inspection
into compilation would make language validation depend on the local machine and
would mix authoring semantics with execution policy.

Consequently, code that reads assets or tools belongs in preflight, and renderer
constraints such as export pixel format do not belong in semantic Video
domains.
