---
status: accepted
---

# Call authored source programs

## Context

Canonical source already models source files as typed stack programs. Reusable
authored programs therefore become ordinary callable program definitions rather
than includes or textual macros.

The existing built-in registry was static, source compilation had one global
name scope, and program output types were known only from built-in descriptors.
Calling authored programs requires linked source units, runtime-owned program
definitions, isolated invocation scopes, and output inference before callers
can be type checked.

## Decision

The native loader produces a `SourcePackage` containing one root source unit and
its linked imported units. Each source unit defines one callable source program.
Imports bind explicit local aliases to other source units; aliases are not
re-exported and may not shadow built-ins or sugar.

The compiler builds one runtime program catalog containing built-ins and
authored definitions. Authored definitions use the same input ports, parameter
types, default stack access, ordered outputs, and caller-side binder as
built-ins. Their outputs are inferred in import dependency order from the
complete ordered final owned values of each body.

An authored invocation opens an isolated local scope and an empty local stack.
Bound inputs become local graph-value bindings. Bound parameters become local
scalar bindings. Inputs, parameters, body aliases, and output bindings share one
local namespace and do not escape the invocation. Only the ordered program
outputs return to the caller.

Semantic references use typed internal symbol identities. Public root names
remain a separate compiled interface. Compiled JSON serializes a reference's
resolved semantic target rather than an internal symbol identity. This changes
the compiled format to version 9.

Import paths are resolved relative to the importing file, parsed units are
deduplicated by canonical filesystem path, and import cycles are rejected during
package loading. Imported files may not declare root-only project or output
settings. Recursive authored-program calls are not supported.

The authored interface supports ordered fixed `Video` or `Audio` inputs and the
existing shared scalar parameter types: `Integer`, `File`, `Duration`,
`TimeRange`, and `Keyword`. It does not add authored variadic inputs,
caller-supplied bodies, re-exports, or recursion.

## Consequences

- Imported `.clipasm` calls use ordinary program binding and stack semantics.
- The same source definition can be imported under several aliases or called
  repeatedly without merging invocation-local names.
- Literal file defaults retain the defining source unit's path base; values
  supplied by a caller retain the caller's path base.
- Import cycles such as `one -> two -> three -> one` fail before output
  inference or semantic evaluation.
- Supporting recursion later would require a separate decision covering
  termination, output inference, and graph representation.
