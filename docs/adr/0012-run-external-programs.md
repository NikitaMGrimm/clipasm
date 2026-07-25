---
status: accepted
---

# Run registered external programs

The external-program architecture remains current. References to YAML and
multiple frontends are historical; the native `.clipasm` loader now owns the
`external "manifest.json" as alias` declaration.

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

External registration cannot exist solely as transient parser state because the
compiler, preflight, renderer, and cache all need the validated specification.

## Decision

Canonical `SourcePackage` data owns external program specifications. Each source
unit maps local aliases to those specifications. The native loader reads the
manifest before compilation, and manifest paths resolve relative to the
declaring `.clipasm` source.

An external specification becomes an ordinary runtime `ProgramDefinition` with
`ProgramImplementation::External`. It uses the shared descriptor validator,
argument binder, exact typed inputs, scalar parameters, stack access, output
checks, and semantic version. External aliases remain local to the source unit
and may not collide with built-ins or authored imports.

The initial manifest and protocol are deliberately closed:

- JSON manifest format version 2;
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
for trusted sources. Validation and compilation do not execute it. ClipAsm does
not sandbox external programs, impose an execution timeout, or attempt to prove
that they terminate or behave deterministically. An external process may hang,
crash, consume arbitrary machine resources, access the network or filesystem,
or produce different results for identical requests.

Cache identity covers the declared semantic version, executable bytes, bound
parameters, upstream artifacts, project settings, and provided FFmpeg/FFprobe
identities. It cannot automatically discover interpreter versions, imported
modules, environment variables, clocks, random input, network responses, or
undeclared files. Authors must change the executable bytes or increment the
manifest semantic version whenever such dependencies change output semantics.

## Consequences

- External programs share the normal program model rather than creating a
  second call language.
- External programs share the same canonical catalog as built-ins and imported
  authored programs.
- Scripts can be authored in any language that produces an executable and can
  read JSON from standard input.
- Script bytes, parameters, and upstream artifacts invalidate cache identity.
- Nondeterministic or environmentally dependent external programs may reuse a
  cached prior result; reproducibility remains the external author's contract.
- Output-changing programs that do not preserve an input domain require a later
  protocol extension with explicit prepared-domain discovery.
- Multiple outputs, File/Duration/TimeRange parameters, variadic inputs,
  interpreter-plus-argument command declarations, and shell execution remain
  outside the initial protocol.
