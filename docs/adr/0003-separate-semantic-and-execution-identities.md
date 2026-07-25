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
incompatible toolchain or media policy. Source spans, comments, named argument
order, project location, and internal numeric node IDs therefore do not define
semantic identity. Authored source selection does: the pure authored image or
video path belongs to compiled identity, while project relocation does not.
References hash as aliases of their targets.
The optional entrypoint output path is publication metadata and does not define
the compiled semantic hash.

Execution identity uses complete FFmpeg and FFprobe build fingerprints derived
from canonical executable bytes and normalized full `-version` output,
including configuration and linked-library versions. Executable location is
reported for diagnostics but does not define cache compatibility. Capability
validation is plan-scoped: the reachable prepared primitives declare their
required FFmpeg encoders, muxers, and filters after lowering. Features used only
by unreachable operations do not reject a plan, while the complete build
fingerprint still isolates artifacts produced by different tool builds.

When a program's lowering semantics change, increment that program's semantic
version. Increment compiled or prepared format versions when their canonical
identity changes incompatibly. Increment the cache format version when existing
artifacts are no longer safe to reuse.
