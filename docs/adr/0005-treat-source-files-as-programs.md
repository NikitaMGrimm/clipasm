---
status: accepted
---

# Treat source files as programs

A ClipAsm source file defines one typed stack program whose outputs are its
ordered final owned values. Non-executable declarations own language versioning,
project settings, imports, inputs, parameters, and optional entrypoint
publication. Executable statements form the source-program body.

The body starts with an empty evaluation stack and receives no implicit
finalizer. Zero, one, or multiple remaining owned values return in order.
Authors use `concat` or an explicit nested `glue` when several Videos should
become one Video; `glue` has no privileged source-file role.

Source outputs belong to compilation semantics. A root `output` path requires
exactly one Video, whose publication belongs to render orchestration. The path
is publication metadata, not a graph value or semantic-identity input. Calling
an imported source program returns its outputs without publishing anything;
imported files may not declare root-only project or output settings.

A fixed graph input may be supplied by an inline input body. That body starts
with an empty stack, inherits the enclosing requested-duration context,
evaluates ordinary items, and must leave exactly one value of the declared
type. It neither enters nor consumes the surrounding stack.

## Consequences

- Pure compilation may accept a source program with zero, one, or multiple
  outputs even when it is not a valid render entrypoint.
- Source programs and registered body programs share the stack evaluator
  without granting source files an implicit finalizer.
- Publication stays separate from authored-program calls and graph identity.
- Inline fixed-input bodies remain isolated and single-valued.
