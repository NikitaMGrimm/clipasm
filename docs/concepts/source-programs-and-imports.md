# Source programs and imports

Each authored ClipAsm input defines one callable source program. Imports connect
those programs through typed interfaces rather than copying text or merging
their stacks and names.

This page explains the relationship between files, packages, and calls.
The [language reference](../language-reference.md#imports) owns current syntax
and behavior, while
[ADR 0009](../adr/0009-call-authored-source-programs.md) records the import and
invocation design.

## Units form one linked package

A **source unit** identifies one authored input, its diagnostic name, and an
optional filesystem base for relative paths. A **source package** is one linked
collection of source units with one root unit. Each unit contributes one
callable **source program**.

The root source unit is special only where a project needs one owner. It may
declare project media configuration and publication settings. Imported units
may declare their callable inputs and parameters, but they may not declare
root-only project or output settings.

Declarations precede executable items. A source program's executable body
starts with an empty local stack and returns every final value occurrence owned
by that body, in order. Zero, one, or multiple outputs are valid for pure
compilation; publication separately requires exactly one Video.

## Imports bind local aliases

An import gives another unit's source program an explicit local alias:

```clipasm
import "programs/polish.clipasm" as polish
```

Calling `polish` then uses the same typed input, parameter, stack-binding, and
ordered-output model as a built-in call. Whether the imported program has a
ClipAsm body or an external implementation does not change the caller's import
syntax.

Aliases are local to the importing unit. They are not re-exported and may not
shadow built-ins. Import cycles, including self-imports, are rejected, so
recursive source-program calls are not supported.

An import is therefore not a textual include. The imported program keeps its
own body, interface, path base, and local namespace.

## Every call is isolated

Invoking a source program opens an empty local stack and an isolated namespace.
Bound Video or Audio inputs become local graph values. Scalar parameters become
local scalar values rather than stack entries. Output bindings and body-input
aliases also stay local to that invocation.

Only the source program's ordered outputs return to the caller. Local names do
not leak, and calling the same imported definition more than once does not merge
the calls' stacks or names.

This isolation also explains why a source program defaults to `owned` stack
access. Missing inputs bind from the caller according to the ordinary call
interface, then evaluation proceeds within the program's own local stack.

## Paths keep their author

Relative authored paths resolve from the source unit containing the authored
value. This remains true across calls:

- a literal default in an imported program keeps the imported unit's path base
- a value supplied by the caller keeps the caller's path base
- an import path resolves relative to the unit declaring the import
- entrypoint publication and cache placement use the entrypoint source unit
- paths supplied through the CLI resolve from the caller's working directory

The path base follows the authored value rather than whichever source program
happens to consume it. This makes reusable programs independent of the root
project's directory layout.

## Linking checks more than execution reaches

Before evaluation, compilation validates the complete linked source-unit graph,
rejects cycles, and checks every linked source program, even when the root never
calls it. An unused import therefore cannot hide invalid source.

That rule is intentionally broader than preflight. Compilation checks the whole
package without opening media; preflight later inspects only the assets and
tools reachable from the Video being prepared. See
[From source to published video](pipeline.md) for the phase distinction.

## Where to find exact rules

- The [language reference](../language-reference.md#configuration-and-declarations)
  specifies declarations, source programs, root-only configuration, imports,
  inputs, parameters, namespaces, and path behavior.
- [ADR 0005](../adr/0005-treat-source-files-as-programs.md) separates source
  program outputs from entrypoint publication.
- [ADR 0009](../adr/0009-call-authored-source-programs.md) records linked
  packages, aliases, callable authored programs, and isolated invocations.
- The [architecture](../architecture.md#language-and-canonical-source) describes
  package loading and canonical source ownership.
