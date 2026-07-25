---
status: accepted
---

# Use one program model

ADR 0006 refines references below to a body's local stack: registered bodies
now receive scoped visible and owned suffixes over one evaluation stack. ADR
0007 generalizes the common output contract from one value to an ordered typed
sequence while existing programs remain one-output. The prepare, evaluate, and
finalize lifecycle remains unchanged.

ADR 0009 supersedes this record's original static-registry limitation by
introducing runtime-owned authored program definitions in the same crate-private
catalog. The one-program model and direct/body lifecycle remain unchanged.

Every executable program call resolves to a registered typed program. A program is
either direct, meaning it lowers one resolved call immediately, or body-based,
meaning it prepares one nested body evaluation and finalizes that body's owned
suffix into an ordered output sequence. Both kinds use the same descriptors, typed parameter
binding, explicit-input rules, implicit stack binding, output checks,
semantic versions, and constrained semantic graph builder.

This preserves the literal language model without pretending that a program
which owns an unevaluated body is identical to one that lowers immediately.
`join`, `glue`, and `during` therefore do not receive parser or evaluator
branches based on their names. Body programs instead use the common
prepare, evaluate once, and finalize lifecycle. Native sugar may generate
ordinary invocations before compilation without becoming a registered program.

The registry was originally static and remains crate-private. ADR 0009 later
made its definitions runtime-owned so authored source programs can join the
same catalog without adding plugins, a public registration API, or dynamically
extensible semantic operations.

File declarations are definition syntax rather than callable registry entries.
The executable source body still uses the same binder, evaluator, graph builder,
IDs, and references as registered program bodies. Inline fixed inputs reuse that
evaluator through isolated bodies; neither case requires evaluator branches on
a registered program name.

`clip` is language sugar, structural stack blocks are canonical items, and
`$name` is a reference expression; none is represented as a registered program.
