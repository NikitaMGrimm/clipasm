# ClipAsm domain context

This document is the source of truth for canonical domain language and settled
authoring semantics. Public YAML forms and program arguments are defined in
`docs/workflow-reference.md`; phase ownership belongs in
`docs/architecture.md`, and durable trade-offs belong in `docs/adr/`.

## Glossary

- A **workflow** is one parsed versioned YAML document.
- An **item** is one executable entry in a sequence body.
- A **program** is a typed callable construct with declared inputs,
  parameters, and one output.
- A **direct program** produces its output without evaluating a nested body.
- A **body program** evaluates one nested body with a program-defined initial
  local stack and finalization rule.
- A **clip** is any finite Video value.
- A **named clip** is a clip declaration bound to a name under `clips`; `clip`
  is not a program or invocation keyword.
- A **value** is an immutable typed graph result. A stack may contain multiple
  occurrences of the same value.
- A **reference expression** is `$name`. It reads a named value without
  consuming any stack occurrence.
- A **local stack** is the ordered sequence of value occurrences visible while
  executing one body.
- The **semantic graph** is the pure result of compilation. Media-derived facts
  such as a video-file source duration may remain deferred.
- The **prepared plan** is the preflight result with reachable assets, tools,
  exact domains, and renderer primitives resolved.

## Settled stack semantics

Sequence order is executable order; YAML mapping order has no executable
meaning. Missing fixed inputs consume the exact required suffix of the current
local stack while preserving signature order. A missing variadic input consumes
all remaining local occurrences in order. Explicit inputs read named values and
consume nothing.

- A named clip starts with an empty local stack and must leave exactly one
  Video. It does not receive timeline finalization.
- `then` starts its body with one preceding Video and requires one Video.
- `join` starts its body with two preceding Videos in order and requires one
  Video.
- A nested or root `timeline` starts empty and concatenates leftover Videos in
  order.
- `during` starts its body with only the selected range, requires one processed
  Video, and splices that result between the untouched prefix and suffix.

A full-duration video source is quantized to the smallest integral project
frame count that covers its complete source interval.

There is no hidden replacement, fallback input, or automatic reduction inside
named-clip or body-program bodies.

## Names, references, and dependencies

Clip names and invocation `id` values share one namespace. Forward references
affect dependency resolution, not list execution order. References create
semantic graph dependencies and never move, remove, or duplicate stack
occurrences. Cycles, missing names, and duplicate names are compile errors.
