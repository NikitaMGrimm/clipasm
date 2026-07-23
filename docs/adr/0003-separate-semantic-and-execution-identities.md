---
status: accepted
---

# Separate semantic and execution identities

ClipAsm uses separate identities for meaning and execution. Compiled structure
hashes identify the authored language and semantic graph. Prepared semantic
hashes additionally include resolved source content and exact prepared domains.
The cache execution namespace separately identifies renderer compatibility,
including its format version, FFmpeg and FFprobe identities, and working-media
policy. Cargo package versions remain metadata rather than inputs to semantic or
cache identity.

This separation avoids two opposite errors: invalidating meaningful results for
an unrelated release-number change, and reusing artifacts produced under an
incompatible toolchain or media policy. Source spans, comments, YAML mapping
order, project location, and internal numeric node IDs therefore do not define
semantic identity. References hash as aliases of their targets.

When a program's lowering semantics change, increment that program's semantic
version. Increment compiled or prepared format versions when their canonical
identity changes incompatibly. Increment the cache format version when existing
artifacts are no longer safe to reuse.
