---
status: accepted
---

# Add type-preserving timeline programs

## Context

Audio and Video are both finite timelines. Operations such as trimming,
repetition, and concatenation have the same structural meaning for either type.
Routing Audio through `Audio -> black Video -> operation -> Audio` would add
unrelated picture work, quantize Audio edits to project frames, and obscure the
semantic graph.

A heterogeneous stack also makes an unqualified variadic operation ambiguous
when both Audio and Video are accessible. Choosing a preferred type or whichever
type happens to be nearest would make broad stack consumption difficult to
predict.

## Decision

A built-in may declare one closed type parameter constrained to the current
semantic value types. The compiler resolves that parameter once per invocation
and stores the concrete input and output signature in checked source. The
binder, evaluator, and program implementation receive only concrete types.
Authored program interfaces remain explicitly typed.

The type-preserving programs are:

- `trim<T: Timeline>(value: T, range: TimeRange) -> T`
- `repeat<T: Timeline>(value: T, count: Integer) -> T`
- `concat<T: Timeline>(values: T...) -> T`
- `drop<T: Value>(value: T) -> []`
- `join<T: Timeline>(before: T, after: T, body) -> T`
- `glue<T: Timeline>(body) -> T`

`Timeline` currently means Video or Audio. Generic inputs require exact types
and never use contextual adaptation.

For a missing unary input, the nearest accessible compatible value resolves
`T`. Explicit inputs resolve `T` from their exact type. Audio trim uses exact
48 kHz sample boundaries, while Video trim uses exact project-frame boundaries.

`concat` is homogeneous. An explicit `type: Video` or `type: Audio` selector
chooses the stack view and consumes every accessible value of that type in
physical order. Bare `concat` infers the type only when exactly one timeline
type is accessible; a mixed Audio/Video stack is an ambiguity error. Explicit
`values` must all have the same exact type.

`drop` returns no values. Its implicit form removes the nearest accessible
value; its selector can target the nearest Video or Audio specifically.

Checked-source construction assigns compact type variables to generic
invocations and graph-valued locals. Selectors, explicit inputs, body contracts,
ordinary type-directed stack binding, generic outputs, lexical body ports, and
names attached to outputs constrain the same variables. Naming therefore never
changes inference, input selection, or stack effects.

Inference narrows the closed Video-or-Audio domain monotonically and retries
deferred stack choices when later constraints make progress. A concrete stack
input skips values that cannot match and defers when a nearer unresolved value
could change the selected occurrence. A unary generic input selects the nearest
accessible value whose complete domain satisfies its constraint, so normal
proximity remains deterministic even before its concrete type is known.
`type:` remains necessary for genuine broad-stack ambiguity, deliberate typed
selection, or an irreducible inference dependency. Dependency cycles remain
cycles even when a selector makes their types concrete.

`join` resolves `T` from homogeneous explicit inputs, a selector, or a stack
view that can satisfy both fixed ports. It seeds the body with both values and
requires every owned body output to have that same type. When both Video and
Audio can satisfy the missing inputs, bare `join` is ambiguous.

`glue` has no graph inputs. An explicit selector fixes `T`; otherwise one or
more homogeneous body outputs constrain it. The same inference resolves named
and unnamed `glue`, including forward references and chains of named generic
bodies. Mixed outputs are rejected, and cycles remain dependency errors rather
than annotation requests.

Both finalizers use the same type-preserving graph concatenation as `concat`.
The checker owns the homogeneous-body guarantee; finalizer validation remains a
defensive boundary.

## Consequences

- Audio trim, repetition, concatenation, `join`, and `glue` use native
  sample-domain graph and renderer operations.
- Existing Video operation identities remain unchanged.
- Mixed stacks remain explicit at broad variadic reductions.
- Checked source, rather than runtime evaluation, owns generic type resolution.
- The descriptor model gains one small closed type parameter instead of a
  general-purpose generic type system.
