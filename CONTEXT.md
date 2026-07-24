# ClipAsm domain context

This document is the source of truth for canonical domain language and settled
authoring semantics. Public YAML forms and program arguments are defined in
`docs/workflow-reference.md`; phase ownership belongs in
`docs/architecture.md`, and durable trade-offs belong in `docs/adr/`.

## Glossary

- A **frontend** translates one authoring representation into canonical source.
  YAML is the first frontend; future frontends may use different syntax and
  representation-specific sugar.
- **Canonical source** is the representation-neutral, fully desugared authored
  ClipAsm model consumed by the compiler.
- A **source unit** identifies one authored input, its diagnostic name, and its
  optional filesystem base for relative paths.
- A **source program** is one callable canonical stack program. Its executable
  body starts empty and returns its ordered final owned suffix, including zero
  values.
- A **source entrypoint** combines one source program with project and
  publication settings for root compilation and rendering.
- A **program header** is the required first source item. It declares the
  language version, project settings, named clips, and optional entrypoint
  output path; it is not executable.
- An **item** is one executable entry in a sequence body.
- A **program** is a typed callable construct with declared inputs,
  parameters, and an ordered output sequence. Every current built-in declares
  exactly one Video output.
- A **direct program** produces its output without evaluating a nested body.
- A **body program** evaluates one nested body with a program-defined initial
  value sequence and finalization rule.
- A **clip** is any finite Video value.
- A **named clip** is a clip declaration bound to a name under `clips`; `clip`
  is not a program or invocation keyword.
- A **value** is an immutable typed graph result. A stack may contain multiple
  occurrences of the same value.
- A **reference expression** is `$name`. It reads a named value without
  consuming any stack occurrence.
- The **evaluation stack** is the ordered sequence of value occurrences used
  while compiling one source program.
- A body's **visible suffix** is the part of the evaluation stack that its
  `visible` invocations may consume.
- A body's **owned suffix** is the part of the visible suffix that ordinary
  `owned` invocations and the body's finalizer may consume.
- **Stack access** is generic invocation metadata with values `owned` and
  `visible`. It does not propagate to child invocations.
- The **semantic graph** is the pure result of compilation. Media-derived facts
  such as a video-file source duration may remain deferred.
- The **compiled JSON document** is a downstream serialization of compiled
  semantics. It is not canonical source and is not a frontend input format.
- A source program's **outputs** are the ordered values in its final owned
  suffix. An entrypoint configured with `output` must have exactly one Video
  output, which is its render result.
- An **inline input body** is an isolated body that supplies one explicit fixed
  graph input to an invocation.
- The **prepared plan** is the preflight result with reachable assets, tools,
  exact domains, and renderer primitives resolved.

## Settled stack semantics

Sequence order is executable order; frontend mapping order has no executable
meaning. Missing fixed inputs consume the exact required suffix of the current
accessible suffix while preserving signature order. A missing variadic input
consumes the complete accessible suffix in order. Explicit inputs read named
values and consume nothing; a fixed input may instead evaluate an isolated
inline input body.

Every program definition explicitly declares a default stack access; all
current programs and source programs default to `owned`. An invocation may set
`stack_access: visible` to consume values below the current ownership frontier
down to the nearest visibility boundary. Captured values become owned by that
body. A child invocation independently uses its own explicit setting or program
default.

- A named clip is isolated, starts empty, and must leave exactly one Video. It
  does not receive glue finalization.
- `join` starts its body with two preceding Videos in order and concatenates
  the body's owned Videos in order.
- A nested `glue` starts with no owned values and concatenates the body's owned
  Videos in order.
- `during` starts its body with only the selected range, requires one processed
  owned Video, and splices that result between the untouched prefix and suffix.

A source-program body starts empty and returns its complete final owned suffix
without implicit reduction. Zero, one, or multiple outputs are valid for pure
validation and compilation. A header `output` path requires exactly one Video;
authors use `concat` or a nested `glue` when several Videos should become that
render result.

An inline input body also starts empty, inherits the enclosing requested-frame
context, and must leave exactly one value of its input port's declared type.
It is isolated from the enclosing invocation's evaluation stack.

A full-duration video source is quantized to the smallest integral project
frame count that covers its complete source interval.

There is no hidden replacement, parent-stack search, or automatic reduction
inside named-clip or body-program bodies. `visible` access operates only within
the current evaluation stack and cannot cross the nearest visibility boundary.

## Names, references, and dependencies

Clip names and invocation output names share one namespace. `id` names the
single output of a one-output item. `ids` completely names a multi-output item
in bottom-to-top stack order. Forward references affect dependency resolution,
not list execution order. References create semantic graph dependencies and
never move, remove, or duplicate stack occurrences. Cycles, missing names, and
duplicate names are compile errors.

Relative authored paths resolve from the source unit containing the authored
value. Entrypoint publication and cache placement use the entrypoint source
unit, while assets authored in future imported programs retain their own source
bases.
