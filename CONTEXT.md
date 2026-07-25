# ClipAsm domain context

This document is the source of truth for canonical domain language and settled
authoring semantics. The native `.clipasm` grammar is implemented under
`src/language`; phase ownership belongs in
`docs/architecture.md`, and durable trade-offs belong in `docs/adr/`.

## Glossary

- The **native language** is the sole supported `.clipasm` authoring syntax.
  Its lexer, parser, package loader, and lowerer own grammar and sugar.
- **Canonical source** is the lowered authored `ClipAsm` model consumed by the
  compiler. It is an internal phase boundary, not a public construction API.
- **Checked source** is the compiler-owned executable representation derived
  from canonical source after program names, references, effective stack
  access, arguments, body contracts, stack bindings, named-value types, and
  ordered output types have been resolved.
- A **source unit** identifies one authored input, its diagnostic name, and its
  optional filesystem base for relative paths.
- A **source package** is one linked collection of source units with one root
  unit. Imports expose another unit's source program under a local alias.
- A **source program** is one callable canonical stack program. Its executable
  body starts empty and returns its ordered final owned values, including zero
  values.
- The **root source unit** may additionally declare project and publication
  settings for compilation and rendering. Imported units may not.
- A **file declaration** is non-executable source metadata such as the required
  `clipasm 1` version line, configuration, imports, externals, inputs, and
  parameters. All declarations precede executable statements.
- An **item** is one executable entry in a sequence body.
- A **program** is a typed callable construct with declared inputs,
  parameters, and an ordered output sequence.
- A **direct program** produces its output without evaluating a nested body.
- A **body program** evaluates one nested body with a program-defined initial
  value sequence and finalization rule.
- An **external program** is a typed registered program whose semantic node is
  compiled purely and whose trusted executable runs only during rendering.
- A **Video** is a finite picture timeline with optional synchronized audio.
- **Audio** is a finite standalone normalized audio timeline.
- A **clip block** is native-language sugar that evaluates a `glue` body, may
  bind the result with `as`, and removes the resulting stack occurrence with an
  owned `drop`. It is lowered before compilation and is not a registered
  program.
- A **stack block** is the structural `{ ... }` item. It evaluates a child stack
  frame and returns every remaining value owned by that frame in order. It is
  not a registered program or a lexical name scope.
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
  semantics. It is not canonical source and is not an authoring format.
- A source program's **outputs** are its ordered final owned values. An
  entrypoint configured with `output` must contain exactly one Video output;
  any additional Audio outputs are auxiliary and are not published.
- An **inline input body** is an isolated body that supplies one explicit fixed
  graph input to an invocation.
- The **prepared plan** is the preflight result with reachable assets, tools,
  exact domains, and renderer primitives resolved.

## Settled stack semantics

Sequence order is executable order; named argument order has no executable
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

- `join` resolves one homogeneous timeline type, starts its body with two bound
  values of that type, exposes `$before` and `$after`, and concatenates the
  body's owned values in order.
- A nested `glue` starts with no owned values, infers one homogeneous timeline
  type from its body unless selected explicitly, and concatenates those values.
- `during` exposes its complete bound `$video`, starts the body stack with only
  the selected range, requires one processed owned Video, and splices it back.
- A plain stack block starts a visible child frame and returns its complete
  ordered owned remainder. `@owned { ... }` establishes an explicit visibility
  boundary.
- A clip block defaults its generated `glue` to owned access. An explicit
  access modifier applies to that `glue`; the generated cleanup `drop` remains
  owned.

A source-program body starts empty and returns all final owned values in physical
order. Zero, one, or multiple outputs are valid for pure validation and
compilation. Publication finds exactly one Video output by type and ignores any
auxiliary Audio outputs. Inline inputs and body contracts remain strict.

Implicit stack binding always requires exact types. `trim`, `repeat`, `concat`,
`drop`, `join`, and `glue` are type-preserving over Video or Audio. Unary programs select the
nearest accessible compatible value. `concat` consumes one homogeneous typed
view; `<Video>` or `<Audio>` selects it explicitly, while bare `concat`
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

Output bindings share one program-wide namespace. `as name` names one output;
`as (first, second)` completely names a multi-output item in bottom-to-top stack
order. IDs declared in nested bodies and stack blocks remain available
throughout the containing source program. Duplicate names are errors rather
than lexical shadowing; only body-input aliases shadow while their body is
active. Forward references affect dependency resolution, not statement
execution order. Naming attaches local identities to already-produced outputs
and never changes type inference, input selection, stack effects, or body
semantics. Generic types are inferred from explicit type arguments, explicit
inputs, body contracts, and normal type-directed stack binding. A type argument
is required only for genuine ambiguity, deliberate selection, or an
irreducible inference dependency. References create semantic graph dependencies and never move,
remove, or duplicate stack occurrences. Cycles, missing names, and duplicate
names are compile errors.

Each source-program invocation has an isolated local namespace containing its
declared inputs, parameters, and output bindings. Inputs are
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

## External programs

Canonical source packages may contain external program specifications and
source-unit-local aliases to them. External calls use the ordinary descriptor,
binder, stack, reference, output, and semantic-version rules. The native
`external "manifest.json" as alias` declaration loads the manifest before
lowering; the compiler consumes the resulting ordinary package catalog.

The initial external protocol supports fixed Video or Audio inputs, Integer and
Keyword parameters, and one Video output whose exact domain and meaningful-audio
state preserve one declared Video input. Compilation does not resolve or run the
command. Preflight resolves and hashes it; rendering executes it directly with a
versioned JSON request and verifies the produced artifact. No implicit shell is
used. External executables are trusted native code.
