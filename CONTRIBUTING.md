# Compiler invariants

Keep these rules visible when changing the language or compiler:

1. One invocation has one output.
2. One compound has one external output.
3. Explicit inputs read names; implicit inputs consume local stack occurrences.
4. References are immutable and never destructively consumed.
5. List order matters; mapping order does not.
6. `then` starts with one preceding value.
7. `during` starts with the selected range.
8. `join` starts with the previous two videos.
9. `timeline` finalizes with ordered concatenation.
10. Named clip bodies do not receive timeline finalization.
11. There is no hidden replacement behavior.
12. Graph structure and exact duration are known before rendering.
13. Programs lower through trusted primitive builder operations.
14. Surface macros remain visible in origins but disappear from executable IR.
15. Deferred features do not belong in the public schema.

An ordinary program belongs in the static registry and needs a fixed descriptor,
argument validation, lowering through the compiler’s trusted graph operations,
and semantic tests. Structural compounds remain an explicit closed enum owned
by the stack evaluator.

