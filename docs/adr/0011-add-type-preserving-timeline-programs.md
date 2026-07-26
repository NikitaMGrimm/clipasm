---
status: accepted
---

# Add type-preserving timeline programs

## Context

Audio and Video are finite timelines, so trim, repeat, and concatenation should
share semantics without routing Audio through a black Video. A mixed stack also
makes broad variadic consumption ambiguous unless the selected type is explicit
or uniquely inferable.

## Decision

A built-in may declare one closed type selector over the semantic value types.
The compiler resolves it once per invocation and stores a concrete checked
signature; authored program interfaces remain explicitly typed.

The type-preserving programs are:

- `trim<T: Timeline>(value: T, range: TimeRange) -> T`
- `repeat<T: Timeline>(value: T, count: Integer) -> T`
- `concat<T: Timeline>(values: T...) -> T`
- `drop<T: Value>(value: T) -> []`
- `join<T: Timeline>(before: T, after: T, body) -> T`
- `glue<T: Timeline>(body) -> T`

`Timeline` currently means Video or Audio. Generic inputs require exact types
and do not use contextual adaptation. Audio operations use the exact project
sample grid; Video operations use project frames.

Unary calls infer the nearest accessible compatible type. `concat` consumes one
homogeneous typed stack view; bare `concat` is an error when both Audio and
Video are accessible. `drop` removes the nearest accessible value or the
nearest value selected explicitly.

`join` requires homogeneous inputs and body outputs. `glue` infers from one or
more homogeneous body outputs. An explicit `<Video>` or `<Audio>` selector
resolves genuine ambiguity. Naming, forward references, lexical body ports,
explicit inputs, and stack binding constrain the same compiler-owned variables,
so naming never changes stack effects or inference.

Inference narrows the finite type domain monotonically and retries deferred
stack choices after later constraints make progress. A concrete stack choice
waits when a nearer unresolved value could change the selected occurrence.
Dependency cycles remain errors even when an explicit selector fixes their
types. Evaluation receives only concrete signatures and stored stack plans; it
does not repeat inference.

## Consequences

- Audio timeline operations stay sample-native and avoid unrelated picture
  work.
- Existing Video operation identities remain type-specific and predictable.
- Mixed broad-stack reductions require an explicit or uniquely inferable type.
- Checked source owns generic resolution without introducing a general-purpose
  public generic type system.
