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
- A **clip block** is native-language sugar that evaluates a stack block ending
  in an owned `concat`, may bind the result with `as`, and removes the resulting
  stack occurrence with an owned `drop`. It is lowered before compilation and
  is not a registered program.
- A **stack block** is the structural `{ ... }` item. It evaluates a child stack
  frame and returns every remaining value owned by that frame in order. It is
  not a registered program or a lexical name scope.
- A **value** is an immutable typed graph result. A stack may contain multiple
  occurrences of the same value.
- A **reference expression** is `$name`. It reads a named value without
  consuming any stack occurrence. In a scalar parameter position it may also
  forward a declared scalar parameter of the required type.
- A **Number** is a dimensionless exact reduced rational scalar. Authored
  integers, decimals, percentages, and arithmetic never use binary floating
  point.
- An **Integer** is the refinement of Number whose reduced denominator is one.
  Integer constraints are checked after exact expression evaluation.
- A **Duration** is an exact time quantity distinct from Number. The postfix
  units `ms` and `s` construct a Duration from an Integer expression.
- A **timeline view** is compiler-owned marker metadata for one authored
  timeline occurrence. It is distinct from semantic media identity, so two
  placements may share one immutable value while retaining different marker
  roots.
- A **placement marker** is a named closed-open child region of a composed
  Video timeline. Explicit output bindings name placements; one uniquely
  referenced value may also contribute its reference name implicitly.
- A timeline view retains one canonical ordered child sequence. Anonymous
  composition layers are transparent and contribute their children directly;
  naming an occurrence creates a deliberate selector boundary. Selector lookup
  is a derived spelling-to-occurrences index rather than separately authored
  state.
- A placement spelling is addressable at one selector level only when exactly
  one occurrence has that spelling. Explicit, inferred, and operation-created
  names do not shadow one another. Duplicates remain visible in diagnostics and
  are ambiguous.
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
- The **prepared plan** is the preflight result with reachable data assets and
  tools resolved and hashed, exact domains derived, and renderer primitives
  selected.
- An **FFmpeg recipe** is a renderer-owned, closed argument description for one
  prepared primitive or final export. Native and browser hosts materialize the
  same recipe against their own paths and runtime.

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
built-ins and source programs default to `owned`; `join` and `during`
default to `visible`. `owned` binding can consume only entries owned by the
current body. `visible` binding may also consume entries owned by enclosing
bodies down to the nearest visibility boundary. A body invocation with
`stack_access: owned` establishes a new boundary. Settings remain per invocation
and do not propagate to children.

Empty call parentheses are optional. A body-capable construct with no authored
body receives an empty body, so `join`, `join()`, `join {}`, and `join() {}`
have the same semantics. The same omission rule applies to `clip` sugar.
Ordinary input binding, required parameters, body contracts, and generated
operations determine whether the resulting empty invocation is valid.
Constructs that do not accept a caller body still reject authored braces.

Every body program exposes each resolved fixed graph input as a local immutable
reference named after its port. Arguments are evaluated in the caller scope
before those aliases are introduced. Thus an inner `during.video: $video` reads
the outer `$video`, while `$video` inside the inner body names the inner bound
port. The body itself is structural invocation data, not a port.

- `join` resolves one homogeneous timeline type, starts its body with two bound
  values of that type, exposes `$before` and `$after`, and concatenates the
  body's owned values in order.
- `during` exposes its complete bound `$video`, starts the body stack with only
  the selected range, requires one processed owned Video, and splices it back.
- A plain stack block starts a visible child frame and returns its complete
  ordered owned remainder. `@owned { ... }` establishes an explicit visibility
  boundary.
- A clip block defaults its generated stack block to owned access. An explicit
  access modifier applies to that block; its generated `concat` and cleanup
  `drop` remain owned.

A source-program body starts empty and returns all final owned values in physical
order. Zero, one, or multiple outputs are valid for pure validation and
compilation. Publication finds exactly one Video output by type and ignores any
auxiliary Audio outputs. Inline inputs and body contracts remain strict.

Implicit stack binding always requires exact types. `trim`, `repeat`, `concat`,
`drop` and `join` are type-preserving over Video or Audio. Unary programs select the
nearest accessible compatible value. `concat` consumes one homogeneous typed
view; `<Video>` or `<Audio>` selects it explicitly, while bare `concat`
is an error when both timeline types are accessible. Generic explicit inputs
must match exactly and never adapt.

Explicit fixed concrete graph inputs may apply one direct contextual adaptation
at the port boundary: `Video` to `Audio` extracts synchronized audio, while
`Audio` to `Video` creates a black project-sized Video carrying that Audio.
Adaptations are semantic graph nodes; program outputs and body outputs are never
adapted. Nested explicit boundaries may compose direct adaptations.

## Settled scalar semantics

Scalar expressions use conventional precedence: ranges, sums, products, unary
signs, postfix operators, then primaries from loosest to tightest. Parentheses
may group any scalar expression. `+`, `-`, `*`, and `/` operate on Number;
Duration additionally supports unary signs and addition or subtraction with
Duration. `..` constructs a TimeRange from two Duration expressions.

Postfix `%` divides a Number by 100 and may repeat without a language-specific
restriction. Postfix `ms` and `s` require the immediately preceding expression
to evaluate to Integer and construct an exact Duration. Consequently
`(6 / 2)ms` is three milliseconds, `(5 / 2)ms` fails Integer refinement, and
`5 / 2ms` is rejected as undefined Number/Duration division. Duration
parameters reject negative or sub-nanosecond results at their representation
boundary.

Scalar parameter references participate in expressions without becoming stack
values. Expressions evaluate before the target parameter constraint, so
`repeat(6 / 2)` is valid while `repeat(5 / 2)` reports the evaluated value
`2.5` and its exact fraction `5/2`. Canonical reduced rational values define
semantic identity; equivalent forms such as `8%`, `0.08`, and `2 / 25` hash
identically.

`name = expression` declares an immutable scalar alias with an inferred scalar
type and no stack effect. Aliases, parameters, inputs, and graph output names
share one program-wide namespace. Alias references may be forward. Alias
expressions are checked and evaluated on demand only when a scalar parameter use
reaches them; transitive references are followed at that point and reached
cycles are rejected. An unused alias may therefore contain an unknown name, an
invalid operator, division by zero, mixed timeline roots, or an invalid value
without affecting compilation. Declaration syntax and duplicate names remain
eager because they determine whether a binding exists. Timeline bounds,
alignment, and parameter constraints are likewise validated only at the final
use. Timeline selectors inside aliases are explicitly rooted and do not borrow
contextual roots from later invocations.

Timeline selectors use `::` and remain frame-native. A selector such as
`$edit::credits::start` addresses a boundary in the marker layout rooted at
`$edit`; nested placement paths are permitted. A placement selector without a
boundary, such as `$edit::credits`, denotes that placement's complete
closed-open range. `::middle` denotes its exact arithmetic midpoint and need
not itself be frame-aligned until used as a frame boundary. Two selector
boundaries with the same root construct a TimeRange without converting through
the nanosecond Duration grid. Coordinates with the same root may be added or
subtracted, scaled or divided by Number, and offset by Duration. Coordinate
arithmetic is exact and may temporarily leave the owning timeline; frame
alignment, range ordering, and bounds are checked only when the result is
consumed as a TimeRange. A bound timeline provides context for a selector
suffix such as `$interview::start` or `$chapter::interview::start`. The suffix
may begin at any uniquely matching addressable descendant; multiple matches are
ambiguous and require more leading placement names or the owning timeline.
Explicitly rooted selectors remain exact paths, and aliases never borrow this
invocation-local context. The consuming timeline input must have the same root.
`concat` and the `join` body finalizer create canonical placement layouts
from their actual surviving occurrences. Anonymous composition is associative
and transparent: blocks, redundant one-input concatenations, and regrouping do
not change selector paths. A named occurrence remains one nested selector
boundary. Identity mappings copy layouts, while
Video `trim` rebases only child placements whose complete regions are provably
contained by the selected range; partial or uncertain placements disappear.
`during` retains base placements provably before its selected range, shifts
placements provably after it by the replacement-duration delta, drops
intersecting or uncertain placements, and exposes the inserted body as the
reserved `replacement` region with its nested layout. If a base placement named
`replacement` survives the edit, `during` rejects the structural collision
instead of shadowing either occurrence. Transition mappings are operation-owned:
`flash_cut` exposes sequential `before` and `after` regions, while `crossfade`
exposes overlapping `before` and `after` regions plus their shared `overlap`.
Transition input regions retain their nested layouts. Marker coordinates are
canonical linear expressions in exact seconds.
Known frame boundaries reduce to constants; unknown Video extents remain terms
referencing semantic values. Video `trim` and `during` may carry such ranges
through the compiled graph, where preflight substitutes probed project-frame
domains and then validates exact alignment, ordering, and final bounds.
`during` also propagates the selected extent as a symbolic requested-duration
context, allowing duration-inheriting images in its body to remain pure until
preflight resolves their concrete frame counts.

An inline input body starts empty, inherits the enclosing requested Video
extent, and must leave exactly one value accepted by its input port after any
direct adaptation. The extent is concrete when known and otherwise remains an
exact symbolic expression until preflight.

A full-duration video source is quantized to the smallest integral project frame
count that covers its complete source interval. Audio domains use exact sample
counts at the configured stereo project sample rate, which defaults to 48 kHz;
frame/sample conversion also uses checked coverage rounding.

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

Every source program has one implementation: either a ClipAsm body or an
`external { ... }` declaration. Both are imported through ordinary `import`
declarations and use the ordinary program interface, binder, defaults, stack,
and semantic-version rules.

The initial protocol supports fixed Video or Audio inputs, Integer, File, and
Keyword parameters, and one Video output preserving one declared Video input's
exact domain and meaningful-audio state. Compilation remains pure. An external
declaration names one executable plus ordered literal or `file(...)` arguments.
Preflight resolves and hashes the executable, file arguments, and File
parameters relative to their source. Rendering re-hashes dependencies reached
by the execution plan, passes the executable and resolved argv separately,
sends a versioned JSON request, and verifies the artifact. ClipAsm does not
construct a shell command string, while normal platform process semantics still
apply. External programs cannot also contain statements or imports; composition
belongs in a ClipAsm wrapper. External executables are trusted code.
