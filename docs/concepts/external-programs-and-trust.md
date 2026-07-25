# External programs and the trust boundary

An external program gives a typed ClipAsm source program an implementation in a
separate executable. Callers still use an ordinary import and the ordinary
program interface; the difference appears only when ClipAsm prepares and
renders the semantic graph.

External programs are an explicit trust boundary. Read this page before
rendering a project that imports one. The
[language reference](../language-reference.md#external-implementations) owns
the current declaration syntax, and
[ADR 0012](../adr/0012-run-external-programs.md) records the complete execution
and cache contract.

## Compilation records meaning without execution

An external implementation file declares typed inputs and parameters, an
executable with ordered arguments, a positive semantic version, and the Video
input whose domain its output preserves. It is imported through the same local
alias mechanism as a ClipAsm-bodied source program.

During compilation, the external call becomes a pure semantic graph node.
Validation and compilation do not resolve or execute its program. The call
still participates in ordinary type checking, defaults, stack binding, and
semantic identity.

The initial external protocol is deliberately narrow:

- fixed Video or Audio inputs
- Integer, File, or Keyword parameters
- exactly one Video output
- an output with the exact domain and meaningful-audio state of one declared
  Video input

An external implementation file cannot also contain executable statements or
imports. Put composition in a separate ClipAsm wrapper program.

## Preflight resolves and hashes dependencies

Preflight is the first phase that resolves an external executable. A path is
resolved relative to the external source unit; a bare executable name uses the
platform command lookup. Preflight requires a regular file and hashes its
bytes.

It also resolves and hashes explicit `file(...)` arguments and bound File
parameters, prepares upstream graph inputs, and copies the exact prepared domain
from the declared preserved Video input. These bytes and resolved dependencies
participate in prepared identity. Preflight does not run the external program.

## Rendering re-verifies, executes, and checks

Before using a cache entry or starting the process, rendering re-hashes the
executable and declared files. It passes the executable and its argument vector
separately rather than constructing a shell command string, and sends a
versioned JSON request over standard input.

A successful exit only means the process claims to have written its result.
ClipAsm still verifies the produced artifact against the prepared media
contract before committing it to the cache.

Separating the executable from its arguments avoids treating authored text as
shell source. It does not make the executable safe or isolated.

## Treat external code as native code

> **Warning:** An external program is trusted native code. Importing or
> validating it does not execute it, but rendering a reachable call runs the
> executable with the user's permissions. Render only projects and external
> programs you trust.

ClipAsm does not sandbox an external process, impose an execution timeout, prove
that it terminates, or require deterministic behavior. The process may hang,
crash, consume arbitrary machine resources, access the filesystem or network,
or produce different output for identical requests. Normal platform process
semantics still apply even though ClipAsm does not construct a shell command.

## Cache identity cannot discover hidden inputs

Cache identity covers the declared semantic version, executable and
`file(...)` bytes, File parameters, bound scalar parameters, upstream artifacts,
project settings, and the provided FFmpeg and FFprobe identities.

Preflight captures declared File values into private plan-scoped snapshots.
External protocol version 2 receives those opaque snapshot paths, so external
code must not use their directory or generated basename as authored project
metadata. The authored extension is retained for format and interpreter
selection.

It cannot automatically discover:

- imported modules or undeclared files opened by the executable
- environment variables
- clocks, random input, or other process state
- filesystem or network responses not declared through the interface

An environmentally dependent or nondeterministic program may therefore reuse a
cached prior result. Authors must declare file dependencies where the protocol
allows it and increment `semantic_version` whenever an undeclared dependency
changes output semantics. Reproducibility remains the external program
author's contract.

## Where to find exact rules

- [`CONTEXT.md`](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTEXT.md#external-programs)
  defines external-program terminology and the supported interface.
- The [language reference](../language-reference.md#external-implementations)
  owns the declaration form and path behavior.
- [ADR 0012](../adr/0012-run-external-programs.md) records execution,
  verification, cache identity, protocol limits, and the trust boundary.
- [ADR 0017](../adr/0017-snapshot-prepared-data-assets.md) records immutable
  data-asset capture and snapshot lifetime.
- The architecture's
  [external-program](../architecture.md#external-programs),
  [preflight](../architecture.md#preflight), and
  [rendering](../architecture.md#rendering) sections own phase
  responsibilities.
