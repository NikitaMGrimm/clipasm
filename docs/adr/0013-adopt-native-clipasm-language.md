---
status: accepted
---

# Adopt the native ClipAsm language

ClipAsm uses one dedicated `.clipasm` source language. Maintaining multiple
authoring syntaxes would duplicate grammar, diagnostics, examples, and tests
without improving the compiler model.

The language uses one invocation form:

```text
@access name<Type>(arguments) { body } as output
```

Each component is optional where the called construct permits it. Stack access
is prefix-only. Explicit `Video` or `Audio` type arguments constrain ordinary
compiler inference rather than replacing it.

File declarations precede executable statements. They cover the language
version, root configuration, imports, external implementations, graph inputs, and
scalar parameters. One callable source program is defined per file. Imports and
imported programs require a local alias regardless of implementation.

Positional graph-producing expressions lower to preceding stack items in source
order. Positional scalar expressions bind scalar parameters in descriptor
order. Named graph arguments retain explicit isolated-input semantics. The
parser records structure only; program lookup, graph classification, type
inference, and stack binding occur later.

Plain braces define a structural stack block. It defaults to visible access,
evaluates a child stack frame, and returns every remaining child-owned value in
order. Ordinary child programs still keep their own defaults; `@owned { ... }`
is the explicit isolation form. A stack block is not a registered program or a
lexical name scope.

`clip` is language sugar. It lowers to a `glue` result followed by an owned
`drop`. An explicit access modifier applies to the generated `glue`; an output
binding names that result before cleanup. Surface provenance keeps diagnostics
and normal explain output attributed to `clip` while hiding the generated
cleanup operation.

The implementation is split into lexer, syntax tree, parser, package loader,
lowerer, and sugar expansion. The parser does not recognize registered program
names, and sugar expansion produces ordinary canonical source items in memory.

## Consequences

- ClipAsm has one documented source language and one package loader.
- Program descriptors remain authoritative for input and parameter order.
- Stack blocks group ordered outputs without changing ordinary call semantics;
  explicit owned blocks provide isolation.
- Sugar can expand structurally without generated text or reparsing.
- Canonical source remains an internal compiler boundary rather than a public
  extension interface.
