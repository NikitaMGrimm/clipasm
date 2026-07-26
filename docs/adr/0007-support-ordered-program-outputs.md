---
status: accepted
---

# Support ordered program outputs

ClipAsm programs have an ordered sequence of typed outputs. Direct lowerers and
body finalizers return values in descriptor order, and the evaluator appends
them to the stack in that order. The last returned value is therefore the new
stack top.

A source program is not implicitly reduced. Its outputs are the complete
ordered values owned by its body, including an empty result. Pure validation
and compilation accept zero, one, or multiple outputs. Publication separately
requires exactly one Video and permits auxiliary Audio outputs.

An item with one output may use `as name`; an item with multiple outputs uses
`as (name, ...)` and must name the complete sequence in bottom-to-top stack
order. Zero-output items cannot be named. Names use the containing authored
program invocation's namespace and its forward-reference, duplicate-name, and
dependency-cycle rules. Omitting names never removes, reorders, or discards
values.

Compiled semantic identity includes ordered source-output hashes, so reordering
outputs changes identity. Compiled serialization stores the ordered sequence.
Prepared plans remain singular because rendering selects exactly one Video for
publication. Inline fixed-input bodies still require exactly one value of the
port type.

Requiring separate authored output declarations was rejected because the
compiler already evaluates an acyclic composition of statically described
program effects and knows the exact output sequence. Separate compilation or
recursion could justify an explicit interface later.
