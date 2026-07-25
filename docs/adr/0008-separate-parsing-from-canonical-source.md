---
status: accepted
---

# Separate parsing from canonical source

ClipAsm keeps a crate-private canonical authored model between the native
language and the compiler.

The lexer, parser, package loader, and lowerer own `.clipasm` grammar, file
loading, positional-expression expansion, and language sugar. They produce a
linked `SourcePackage`. The compiler consumes only that package and does not
read authored files or interpret surface syntax.

Every source location retains its source unit and filesystem base. Relative
paths therefore resolve from the file that authored them, including imported
program defaults and external executable commands.

Canonical source is not a public construction API. Its purpose is to keep
language concerns out of linking, type inference, stack binding, semantic
evaluation, and rendering without freezing an external builder interface.

Compiled JSON remains a downstream serialization of compiled semantics. It is
not accepted as authored input and is not derived directly from internal Rust
struct layouts.

## Consequences

- Surface sugar disappears before compilation.
- Compiler behavior is independent of parser implementation details.
- Source paths and diagnostics remain accurate across imported units.
- Language evolution does not require exposing compiler internals publicly.
