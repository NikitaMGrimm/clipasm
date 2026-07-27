# Language reference

ClipAsm source files use the `.clipasm` extension and language version 1. Use
these focused reference pages to look up exact authored behavior:

- [Files and configuration](files-and-configuration.md) defines source layout,
  declarations, project media configuration, and output configuration.
- [Scalar values and expressions](scalar-values-and-expressions.md) defines
  exact scalar types, operators, and aliases.
- [Timeline selectors and ranges](timeline-selectors.md) defines placement
  paths, coordinates, marker ranges, and their diagnostics.
- [Imports and external programs](imports-and-external-programs.md) defines
  source imports and native external implementations.
- [Statements and calls](statements-and-calls.md) defines invocation syntax,
  bodies, and generic type selection.
- [Stack binding](stack-binding.md) defines argument binding, stack access, and
  graph-input selection.
- [Names, blocks, and `clip`](names-blocks-and-clip.md) defines graph
  references, output names, structural blocks, and `clip` sugar.
- The [built-in program reference](../programs/index.md) provides one stable
  page for every built-in program.
- The [command-line reference](../cli.md) defines CLI source and root bindings.
- The [formal grammar](../../language-grammar.md) is the normative EBNF grammar
  for language version 1.

New documentation should link directly to the focused canonical page rather
than the legacy compatibility route.
