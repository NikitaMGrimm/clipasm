---
status: accepted
---

# Separate semantic and execution identities

ClipAsm uses separate identities for meaning and execution. Compiled structure
hashes identify the authored language and semantic graph. Prepared semantic
hashes additionally include content hashes observed while resolving source
assets during preflight and exact prepared domains. The cache execution
namespace separately identifies renderer compatibility, including its
artifact-contract revision, artifact-cache policy, and FFmpeg and FFprobe
identities. The policy covers the verified working media shape and the native
renderer's codec and container choices; external-program artifacts may use any
encoding that satisfies the verified prepared-artifact contract. The final
export profile is execution policy but does not define intermediate-cache
compatibility because publication always re-exports the result. Cargo package
versions remain metadata rather than inputs to semantic or cache identity.

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
identity changes incompatibly. Increment the artifact-contract revision when a
renderer or filter change makes existing working artifacts unsafe to reuse.
Native working codec or container changes and working pixel-format, extension,
or channel-layout changes alter the namespace structurally. Export-only changes
do not invalidate compatible working artifacts.
