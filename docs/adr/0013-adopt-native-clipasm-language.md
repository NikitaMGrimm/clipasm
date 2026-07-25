## Status

Accepted

## Context

ClipAsm currently accepts a restricted YAML representation. The project is
unreleased, and maintaining YAML alongside a native language would duplicate
syntax, documentation, examples, diagnostics, and tests without improving the
compiler model.

The compiler already consumes an internal authored-source representation rather
than YAML nodes directly. That boundary remains useful for separating parsing
and source-language sugar from linking, type checking, stack binding, semantic
evaluation, and rendering.

The existing `program.clips` declaration is represented separately throughout
parsing, source construction, type inference, checking, and evaluation. Its
behavior can instead be expressed by ordinary named operations and stack
consumption.

## Decision

`.clipasm` is the only supported source language. YAML will be removed without
a compatibility mode or stable alternate-frontend API.

The native language uses one universal invocation form:

```text
@access name<Type>(arguments) { body } as output
```

Every component is optional when permitted by the called program. Stack access
is prefix-only. Explicit `Video` or `Audio` type arguments constrain ordinary
compiler inference rather than replacing it.

File declarations precede executable statements. The initial declaration forms
cover the language version, root configuration, imports, external manifests,
graph inputs, and scalar parameters. One source program is defined per file.
Imports and external manifests use a required local alias:

```clipasm
import "programs/polish.clipasm" as polish
external "programs/brighten.json" as brighten
```

Ordinary output IDs remain program-wide, immutable, and unique. Nested bodies
may declare IDs that are referenced elsewhere in the same source program.
Fixed body-input aliases remain lexical and may shadow program-wide IDs for the
duration of their body.

Positional graph-producing expressions are lowered to ordinary preceding stack
items in source order. Positional scalar expressions bind scalar parameters in
descriptor order. Named graph arguments retain explicit input-boundary
semantics. The parser records syntax only; graph-value classification, type
inference, and stack binding remain later semantic work.

Plain braces define a structural stack block:

```clipasm
{
    first_operation
    second_operation
} as (first, second)
```

A stack block defaults to owned access, evaluates its body in a child stack
frame, and returns every remaining child-owned value to its parent in order. It
is not a registered program and does not introduce a name scope.

`clip` is native-language sugar. It lowers structurally to an owned `glue`
unless an explicit access modifier is supplied, followed by an owned `drop`.
Any output binding belongs to the generated `glue`. The compiler has no
clip-specific representation.

The native implementation is organized as lexer, syntax tree, parser, package
loader, and lowerer. The parser does not recognize registered built-in names.
Sugar expansion operates on structured syntax nodes and produces ordinary
compiler source items.

## Consequences

- ClipAsm has one documented source language and one package loader.
- The internal source representation remains crate-private compiler input, not
  a promised frontend extension API.
- The old named-clip declaration and its dedicated compiler pipeline are
  removed.
- YAML remains only as temporary migration scaffolding until native parsing and
  test coverage replace it, then its dependency and implementation are deleted.
- Program descriptors remain the authority for input and parameter order.
- Stack blocks provide explicit isolation without silently wrapping ordinary
  calls or changing partial stack binding.
