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

YAML is only one frontend. External registration therefore cannot exist solely
as parser state or YAML-specific execution syntax.

## Decision

Canonical `SourcePackage` data owns external program specifications. Each source
unit maps local aliases to those specifications. A frontend is responsible for
loading or constructing the specifications before compilation. The YAML
frontend exposes this through `program.externals`, whose manifest paths resolve
relative to the declaring YAML source.

An external specification becomes an ordinary runtime `ProgramDefinition` with
`ProgramImplementation::External`. It uses the shared descriptor validator,
argument binder, exact typed inputs, scalar parameters, stack access, output
checks, and semantic version. External aliases remain local to the source unit
and may not collide with built-ins or authored imports.

The initial manifest and protocol are deliberately closed:

- JSON manifest format version 1;
- process protocol version 1;
- fixed Video or Audio inputs;
- Integer and Keyword parameters;
- exactly one Video output;
- output domain and meaningful-audio state preserve one declared Video input;
- one directly executable command path, with no implicit shell or argument
  interpolation.

Compilation reads the already loaded specification but never resolves or runs
its command. Evaluation emits a pure `ExternalVideo` semantic node containing
the authored command, bound parameters, named graph inputs, and preserved input.

Preflight resolves the command relative to its manifest or from `PATH`, requires
an executable regular file, hashes its bytes, lowers every input dependency,
and copies the exact domain and meaningful-audio state from the preserved Video
input. The executable content hash participates in the prepared node identity.

Rendering re-hashes the executable before accepting cache entries or executing
the node. It launches the executable directly, writes one versioned JSON request
to standard input, and never invokes a shell. The request contains named input
artifact paths and domains, bound parameters, a temporary output path, project
Video and Audio settings, and resolved FFmpeg and FFprobe paths. A zero exit
status indicates that the process wrote the output. ClipAsm then applies its
ordinary prepared-artifact verification before committing the cache entry.

External code is trusted native code. Importing a manifest is explicit, but
rendering an unfamiliar project can execute that program and should only be done
for trusted sources. Validation and compilation do not execute it.

## Consequences

- External programs share the normal program model rather than creating a
  second call language.
- Future frontends can populate the same canonical external catalog without
  adopting YAML manifests or syntax.
- Scripts can be authored in any language that produces an executable and can
  read JSON from standard input.
- Script bytes, parameters, and upstream artifacts invalidate cache identity.
- Output-changing programs that do not preserve an input domain require a later
  protocol extension with explicit prepared-domain discovery.
- Multiple outputs, File/Duration/TimeRange parameters, variadic inputs,
  interpreter-plus-argument command declarations, and shell execution remain
  outside the initial protocol.
