---
status: accepted
---

# Support ordered program outputs

ClipAsm programs have an ordered sequence of typed outputs. Existing built-ins
retain their established behavior and each declares exactly one Video output,
but the common program interface permits zero or multiple outputs. Direct
lowerers and body finalizers return values in descriptor order, and the
evaluator appends every returned value to the evaluation stack in that order.
The last returned value is therefore the new stack top.

ADR 0009 later scopes named values to each authored-program invocation. The
namespace rules below remain unchanged within one such local scope.

A source program is not implicitly reduced. Its outputs are the complete final
owned suffix of its body, including an empty suffix. Pure `validate` and
`compile` accept zero, one, or multiple source outputs. A header `output` path
is publication metadata and requires exactly one Video output; preflight and
rendering remain singular publication phases.

An item with one output may use `id`. An item with multiple outputs may use
`ids: [name, ...]`, which must completely name the outputs in bottom-to-top
stack order. `id` and `ids` are mutually exclusive. Zero-output items cannot be
named. Output names use the existing global namespace and forward-reference,
duplicate-name, and dependency-cycle rules. Omitting output names never removes,
reorders, or discards values.

Compiled semantic identity includes the ordered source-output hashes, so
reordering outputs changes identity. The compiled document stores `outputs`
instead of one `result`; compiled format version 7 originally recorded that
incompatible change. ADR 0008 later supersedes the document boundary with
compiled format version 8 while preserving the ordered-output contract.
Prepared plans remain singular and their format and cache versions do not
change.

Named clips still require exactly one Video. Inline fixed-input bodies still
require exactly one value of the port type. Existing body programs retain their
one-output finalization behavior. Callable or imported YAML programs, their
signatures, and program-call cycle detection remain separate future work.

Requiring authored output declarations on source programs was rejected because
the compiler already evaluates an acyclic composition of statically described
program effects and therefore knows the exact ordered output sequence. An
optional checked interface declaration may be considered with callable YAML
programs, where separate compilation or recursion could make it useful.
