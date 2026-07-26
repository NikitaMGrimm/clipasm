---
status: accepted
---

# Use one program model

Every callable operation resolves through one crate-private catalog of typed
`ProgramDefinition` values. Built-ins, authored source programs, and external
implementations share descriptors, argument binding, stack access, output
checks, semantic versions, and the resolved-call interface.

A direct implementation lowers one resolved call immediately. A body
implementation prepares initial values and context, evaluates its nested body
once, and finalizes the body's ordered owned outputs. An authored source program
uses the same call interface with an isolated local stack and scope. An external
implementation lowers to a semantic node and runs only during rendering.

This common model does not erase meaningful lifecycle differences. It keeps
program-specific behavior in definition variants while preventing parser or
evaluator branches on registered names such as `join`, `glue`, or `during`.

File declarations, structural stack blocks, references, and `clip` sugar are
language structures rather than registered programs. Sugar may generate
ordinary invocations before compilation without becoming a catalog entry.

## Consequences

- One binder and resolved-call ABI serve every callable implementation.
- Adding an authored or external program does not create a second call
  language.
- Native semantic and prepared operations remain closed engine primitives, not
  runtime plugins.
