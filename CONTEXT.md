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
- **Checked source** is compiler-owned metadata paired with canonical source
  after program names, effective stack access, argument contracts, body
  contracts, named-value types, and ordered output types have been resolved.
- A **source unit** identifies one authored input, its diagnostic name, and its
  optional filesystem base for relative paths.
- A **source package** is one linked collection of source units with one root
  unit. Imports expose another unit's source program under a local alias.
- A **source program** is one callable canonical stack program. Its executable
  body starts empty and returns its ordered final owned values, including zero
  values.
- The **root source unit** may additionally declare project and publication
  settings for compilation and rendering. Imported units may not.
- A **program header** is the required first source item. It declares the
  language version, project settings, named clips, and optional entrypoint
  output path; it is not executable.
- An **item** is one executable entry in a sequence body.
- A **program** is a typed callable construct with declared inputs,
  parameters, and an ordered output sequence. Current built-ins declare exactly
  one Video or Audio output.
- A **direct program** produces its output without evaluating a nested body.
- A **body program** evaluates one nested body with a program-defined initial
  value sequence and finalization rule.
- A **clip** is any finite Video value. A Video carries picture plus optional
  synchronized audio.
- **Audio** is a finite standalone normalized audio timeline.
- A **named clip** is a clip declaration bound to a name under `clips`; `clip`
  is not a program or invocation keyword.
- A **value** is an immutable typed graph result. A stack may contain multiple
  occurrences of the same value.
- A **reference expression** is `$name`. It reads a named value without
  consuming any stack occurrence. In a scalar parameter position it may also
  forward a declared scalar parameter of the required type.
- The **evaluation stack** is the ordered sequence of value occurrences used
  while compiling one source program.
- A body's **visible values** are stack entries whose ownership lies within
  the nearest visibility boundary.
- A body's **owned values** are stack entries produced for that body. Ownership
  is tracked per value occurrence rather than as a contiguous suffix.
- **Stack access** is generic invocation metadata with values `owned` and
  `visible`. It does not propagate to child invocations.
- The **semantic graph** is the pure result of compilation. Media-derived facts
  such as a video-file source duration may remain deferred.
- The **compiled JSON document** is a downstream serialization of compiled
  semantics. It is not canonical source and is not a frontend input format.
- A source program's **outputs** are its ordered final owned values. An
  entrypoint configured with `output` must contain exactly one Video output;
  any additional Audio outputs are auxiliary and are not published.
- An **inline input body** is an isolated body that supplies one explicit fixed
  graph input to an invocation.
- The **prepared plan** is the preflight result with reachable assets, tools,
  exact domains, and renderer primitives resolved.

## Settled stack semantics

Sequence order is executable order; frontend mapping order has no executable
meaning. The stack is one physical ordered sequence containing typed value
occurrences. Each occurrence records which active body owns it.

Missing fixed inputs are bound from last port to first port. Each port consumes
the nearest accessible value of its exact declared type. Values of other types
are skipped without moving and retain their relative order. A missing variadic
input consumes all accessible values of its declared type in physical order.
Explicit inputs read named values or evaluate isolated inline bodies and consume
nothing from the caller stack.

Every program definition explicitly declares a default stack access. Direct
built-ins and source programs default to `owned`; `join`, `glue`, and `during`
default to `visible`. `owned` binding can consume only entries owned by the
current body. `visible` binding may also consume entries owned by enclosing
bodies down to the nearest visibility boundary. A body invocation with
`stack_access: owned` establishes a new boundary. Settings remain per invocation
and do not propagate to children.

Every body program exposes each resolved fixed graph input as a local immutable
reference named after its port. Arguments are evaluated in the caller scope
before those aliases are introduced. Thus an inner `during.video: $video` reads
the outer `$video`, while `$video` inside the inner body names the inner bound
port. The body itself is structural invocation data, not a port. Programs with
no fixed inputs, such as `glue`, expose no aliases.

- A named clip is isolated, starts empty, and must leave exactly one Video.
- `join` resolves one homogeneous timeline type, starts its body with two bound
  values of that type, exposes `$before` and `$after`, and concatenates the
  body's owned values in order.
- A nested `glue` starts with no owned values, infers one homogeneous timeline
  type from its body unless selected explicitly, and concatenates those values.
- `during` exposes its complete bound `$video`, starts the body stack with only
  the selected range, requires one processed owned Video, and splices it back.

A source-program body starts empty and returns all final owned values in physical
order. Zero, one, or multiple outputs are valid for pure validation and
compilation. Publication finds exactly one Video output by type and ignores any
auxiliary Audio outputs. Named clips, inline inputs, and body contracts remain
strict.

Implicit stack binding always requires exact types. `trim`, `repeat`, `concat`,
`drop`, `join`, and `glue` are type-preserving over Video or Audio. Unary programs select the
nearest accessible compatible value. `concat` consumes one homogeneous typed
view; `type: Video` or `type: Audio` selects it explicitly, while bare `concat`
is an error when both timeline types are accessible. Generic explicit inputs
must match exactly and never adapt.

Explicit fixed concrete graph inputs may apply one direct contextual adaptation
at the port boundary: `Video` to `Audio` extracts synchronized audio, while
`Audio` to `Video` creates a black project-sized Video carrying that Audio.
Adaptations are semantic graph nodes; program outputs and body outputs are never
adapted. Nested explicit boundaries may compose direct adaptations.

An inline input body starts empty, inherits the enclosing requested-frame
context, and must leave exactly one value accepted by its input port after any
direct adaptation.

A full-duration video source is quantized to the smallest integral project frame
count that covers its complete source interval. Audio domains use exact sample
counts at the canonical 48 kHz stereo project format; frame/sample conversion
also uses checked coverage rounding.

## Names, references, and dependencies

Clip names and invocation output names share one namespace. `id` names the
single output of a one-output item. `ids` completely names a multi-output item
in bottom-to-top stack order. Forward references affect dependency resolution,
not list execution order. References create semantic graph dependencies and
never move, remove, or duplicate stack occurrences. Cycles, missing names, and
duplicate names are compile errors.

Each source-program invocation has an isolated local namespace containing its
declared inputs, parameters, clips, and invocation output names. Inputs are
local graph values; parameters are local scalar values. A scalar parameter is
not a stack value, and a graph value is not a scalar parameter. Local names do
not escape the invocation; only the program's ordered outputs return to the
caller.

Imports use explicit local aliases. They are not re-exported, may not shadow
built-ins, and resolve relative to the importing source unit. Import cycles,
including self-import and multi-file cycles, are rejected before compilation.
Recursive source-program calls are therefore unsupported.

Relative authored paths resolve from the source unit containing the authored
value. Entrypoint publication and cache placement use the entrypoint source
unit. Literal defaults authored in imported programs retain the imported
unit's source base; caller-supplied values retain the caller's source base.
Every linked source program is checked even when the root does not invoke it.
