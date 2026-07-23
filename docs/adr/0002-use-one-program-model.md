---
status: accepted
---

# Use one program model

Every callable ClipAsm construct is a registered typed program. A program is
either direct, meaning it lowers one resolved call immediately, or body-based,
meaning it prepares one nested body evaluation and finalizes that body's local
stack into one output. Both kinds use the same descriptors, typed parameter
binding, explicit-reference rules, implicit stack binding, output checks,
semantic versions, and constrained semantic graph builder.

This preserves the literal language model without pretending that a program
which owns an unevaluated body is identical to one that lowers immediately.
`join`, `timeline`, and `during` therefore do not receive parser or evaluator
branches based on their names. Body programs instead use the common
prepare, evaluate once, and finalize lifecycle. Surface forms such as postfix
`during` normalize into ordinary invocations before compilation.

The registry remains static and crate-private. This gives built-in programs a
uniform extension path without committing the foundation to plugins, a public
registration API, or dynamically extensible semantic operations.

Named clips remain declarations, and `$name` remains a reference expression;
neither is callable, so neither is represented as a program.
