---
status: accepted
---

# Run registered external programs

## Context

Some useful media operations should remain ordinary scripts or standalone
binaries instead of becoming built-in Rust lowerers. They still need ClipAsm's
typed inputs, scalar binding, stack behavior, semantic identity, exact prepared
domains, cache safety, and output verification.

Executing a script during compilation would violate pure compilation and make
validation depend on the local machine. Treating arbitrary command strings as
shell source would also introduce platform-specific quoting and command
injection behavior. A long-lived plugin ABI or in-process dynamic library would
be disproportionate for the first external operation.

External implementation metadata cannot exist solely as transient parser state because the
compiler, preflight, renderer, and cache all need the validated specification.

## Decision

Every source unit owns one program implementation: either a ClipAsm body or a
native `external { ... }` declaration. External programs are ordinary
`.clipasm` source units and are imported with the same `import "..." as alias`
syntax as body programs. This keeps callers dependent on the program interface
rather than its implementation.

An external specification becomes an ordinary runtime `ProgramDefinition` with
`ProgramImplementation::External`. It uses the shared descriptor validator,
argument binder, exact typed inputs, scalar parameters, stack access, output
checks, and semantic version. External aliases remain local to the source unit
and may not collide with built-ins or authored imports.

The native declaration requires an `executable`, optional ordered `arguments`,
a positive `semantic_version`, and a `preserve` field naming one declared Video
input. Arguments are literal strings or explicit `file("...")` values.
External implementation files cannot also contain executable statements or
imports; composition belongs in a separate ClipAsm wrapper program.

The initial process protocol is deliberately closed:

- process protocol version 1;
- fixed Video or Audio inputs;
- Integer, File, and Keyword parameters;
- exactly one Video output;
- output domain and meaningful-audio state preserve one declared Video input;
- one executable with ordered literal or content-hashed file arguments.

Compilation reads the already loaded specification but never resolves or runs
its executable. Evaluation emits a pure `ExternalVideo` semantic node containing
the authored executable and arguments, bound parameters, named graph inputs,
and preserved input.

Preflight resolves the executable relative to its defining source unit or through
the platform command lookup, requires a regular file, hashes its bytes, resolves
and hashes every `file(...)` argument, lowers every input dependency, and copies
the exact domain and meaningful-audio state from the preserved Video input. The
executable and file-argument content hashes participate in prepared identity.

Rendering re-hashes the executable and file arguments before accepting cache
entries or executing the node. It passes the executable and arguments separately
and writes one versioned JSON request to standard input. ClipAsm does not build a
shell command string; normal platform process semantics still apply. The request
contains named input artifact paths and domains, bound parameters, a temporary
output path, project Video and Audio settings, and resolved FFmpeg and FFprobe
paths. A zero exit status indicates that the process wrote the output. ClipAsm
then applies its ordinary prepared-artifact verification before committing the
cache entry.
File parameters resolve from the source location that supplied the value,
become verified content-hashed prepared assets, and are re-hashed before cache
reuse or execution. The process request receives their resolved paths.

External code is trusted native code. Importing an external `.clipasm` program
is explicit, but rendering an unfamiliar project can execute that program and
should only be done for trusted sources. Validation and compilation do not
execute it. ClipAsm does not sandbox external programs, impose an execution
timeout, or attempt to prove that they terminate or behave deterministically.
An external process may hang, crash, consume arbitrary machine resources,
access the network or filesystem, or produce different results for identical
requests.

Cache identity covers the declared semantic version, executable and file-argument
bytes, bound parameters, upstream artifacts, project settings, and provided
FFmpeg/FFprobe identities. It cannot automatically discover imported modules,
environment variables, clocks, random input, network responses, or undeclared
files. Authors must declare file dependencies or increment the semantic version
whenever such dependencies change output semantics.

## Consequences

- External programs share the normal program model rather than creating a
  second call language.
- External programs share the same canonical catalog as built-ins and imported
  authored programs.
- Scripts can be launched through an explicit interpreter and `file(...)`
  argument while their callable interface remains native ClipAsm source.
- Executable bytes, file-argument bytes, parameters, and upstream artifacts
  invalidate cache identity.
- Nondeterministic or environmentally dependent external programs may reuse a
  cached prior result; reproducibility remains the external author's contract.
- Output-changing programs that do not preserve an input domain require a later
  protocol extension with explicit prepared-domain discovery.
- Multiple outputs, Duration/TimeRange parameters, variadic inputs, and shell
  command strings remain outside the initial protocol.

## Related decisions

- [ADR 0017](0017-snapshot-prepared-data-assets.md) changes declared File
  arguments and parameters from reverified authored paths to immutable
  plan-scoped snapshot paths in external protocol version 2.
