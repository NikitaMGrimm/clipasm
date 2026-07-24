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

Every callable ClipAsm construct is a registered typed program. A program is
either direct, meaning it lowers one resolved call immediately, or body-based,
meaning it prepares one nested body evaluation and finalizes that body's owned
suffix into an ordered output sequence. Both kinds use the same descriptors, typed parameter
binding, explicit-input rules, implicit stack binding, output checks,
semantic versions, and constrained semantic graph builder.

This preserves the literal language model without pretending that a program
which owns an unevaluated body is identical to one that lowers immediately.
`join`, `glue`, and `during` therefore do not receive parser or evaluator
branches based on their names. Body programs instead use the common
prepare, evaluate once, and finalize lifecycle. Surface forms such as postfix
`during` normalize into ordinary invocations before compilation.

The registry was originally static and remains crate-private. ADR 0009 later
made its definitions runtime-owned so authored source programs can join the
same catalog without adding plugins, a public registration API, or dynamically
extensible semantic operations.

The source-program header is definition syntax rather than a callable registry
entry. Its executable body still uses the same item parser, binder, evaluator,
graph builder, IDs, and references as registered program bodies. Inline fixed
inputs reuse that evaluator through isolated bodies; neither case requires
parser or evaluator branches on a registered program name.

Named clips remain declarations, and `$name` remains a reference expression;
neither is callable, so neither is represented as a program.
