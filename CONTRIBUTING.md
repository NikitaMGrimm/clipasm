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
16. References are syntax expressions, never identity programs.
17. Pure compilation performs no asset or external-tool I/O.
18. Preflight hashes reachable assets and constructs renderer-only plans.
19. Semantic Video domains do not contain backend pixel formats.
20. Final output and manifest publication use temporary siblings and atomic rename.

An ordinary program belongs in one static registry definition containing its
fixed descriptor, parameter schema, and lowering function. Lowering uses the
compiler’s constrained `GraphBuilder`. Structural compounds remain an explicit
closed enum owned by the typed stack evaluator.
