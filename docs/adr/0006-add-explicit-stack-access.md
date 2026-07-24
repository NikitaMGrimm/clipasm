---
status: accepted
---

# Add explicit stack access

This record refines the local-stack wording in ADRs 0002 and 0005. Their
program lifecycle, source-program result, and isolation decisions remain in
force; nested registered bodies now use scoped views over one evaluation stack.

ClipAsm evaluates nested bodies over one physical typed evaluation stack. Each
body frame tracks a visible suffix and an owned suffix. Programs consume missing
inputs from the owned suffix by default. An invocation may opt into
`stack_access: visible` to consume farther down the visible suffix, stopping at
the nearest visibility boundary. Values captured this way become owned by the
body and may be finalized with its other results.

Stack access is generic invocation metadata rather than an ordinary program
parameter. Every program descriptor explicitly declares its default, and every
current program defaults to `owned`. An authored override applies only to that
invocation; child invocations independently use their own override or descriptor
default. Source programs expose the same metadata and explicitly default to
`owned`. Named clips and inline input bodies remain isolated.

This design adds opt-in stack-language power without making it ambient. A
visible body can expose earlier values, but the child operation that consumes
them must also be visible. A later owned variadic program such as `concat`
consumes only the suffix already captured by the body rather than all still
visible values. An owned descendant creates a new boundary that visible
grandchildren cannot pierce.

The compiler centralizes these rules in an invariant-owning evaluation-stack
abstraction. Program implementations still receive fully resolved typed calls
and never manipulate frame indices. Body finalizers receive only the body's
owned suffix and continue to return one typed output. Multiple program outputs
remain a separate future change.

The alternative of making every nested body unrestricted was rejected because
existing `glue`, `join`, `during`, named-clip, and inline-input meanings would
become context-sensitive by default. Automatically inheriting a visible setting
was also rejected because a distant `concat`, `flash`, or `join` could acquire
ambient access without an annotation at the consuming invocation.
