---
status: superseded by ADR 0010
---

# Add explicit stack access

This record refines the local-stack wording in ADRs 0002 and 0005. Their
program lifecycle and isolation decisions remain in force; nested registered
bodies now use scoped views over one evaluation stack. ADR 0007 later replaces
the single source result with an ordered output suffix.

ClipAsm evaluates nested bodies over one physical typed evaluation stack. Each
body frame tracks a visible suffix and an owned suffix. `owned` binding consumes
only the owned suffix. `visible` binding may consume farther down the visible
suffix, stopping at the nearest visibility boundary. Values captured this way
become owned by the body and may be finalized with its other results.

Stack access is generic invocation metadata rather than an ordinary program
parameter. Every program descriptor explicitly declares its default. Direct
built-ins and source programs default to `owned`; the native body programs
`join`, `glue`, and `during` default to `visible`. An authored override applies
only to that invocation, and child invocations independently use their own
override or descriptor default. Named clips and inline input bodies remain
isolated.

For a body program, stack access controls both its own missing-input binding and
the visibility boundary of its nested body. A default-visible body may bind its
inputs through an enclosing body boundary and exposes that same visible suffix
to descendants. A descendant still consumes outside its local owned suffix only
when its own access is `visible`. Setting the body invocation to `owned`
establishes a new boundary.

The compiler centralizes these rules in an invariant-owning evaluation-stack
abstraction. Program implementations still receive fully resolved typed calls
and never manipulate frame indices. Body finalizers receive only the body's owned suffix. ADR 0007 generalizes
their return type to an ordered declared output sequence while preserving every
existing body's one-output behavior.
