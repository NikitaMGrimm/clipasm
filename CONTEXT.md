# ClipAsm domain context

This document is the source of truth for the foundation's domain language and
settled authoring semantics. `README.md` is the user-oriented introduction;
implementation decisions belong in `docs/architecture.md` and `docs/adr/`.

## Glossary

- A **workflow** is one parsed versioned YAML document.
- An **item** is one executable entry in a sequence body.
- A **program** is a typed callable construct with declared inputs,
  parameters, and one output.
- A **direct program** produces its output without evaluating a nested body.
- A **body program** evaluates one nested body with a program-defined initial
  local stack and finalization rule.
- A **clip** is any finite Video value. A **named clip** is a clip bound to a
  name under `clips`. `clip` is not a program or invocation keyword.
- A **value** is an immutable typed graph result. A stack may contain multiple
  occurrences of the same value.
- A **reference expression** is `$name`. It reads a named value without
  consuming any stack occurrence.
- A **local stack** is the ordered sequence of value occurrences visible while
  executing one body.
- The **semantic graph** is the pure result of compilation. Video-file source
  durations may remain deferred.
- The **prepared plan** is the preflight result with reachable assets, tools,
  exact domains, and renderer primitives resolved.

## Public syntax

A workflow has `version: 1`, an optional `project.video` mapping (`width`,
`height`, `fps`), an optional `clips` mapping, a required `timeline` sequence,
and an optional `output` path. Mapping order has no meaning.

An item can be:

- a plain reference expression, such as `- $opening`;
- a no-argument program name, such as `- concat`;
- a program invocation using primary shorthand, such as `- repeat: 3`;
- a full argument mapping, such as
  `{image: {path: card.png, duration: 1s}}`;
- a body-program invocation such as `then`, `join`, `timeline`, or `during`.

`id` is the only item annotation. Program arguments belong inside the program
mapping. `during` additionally declares postfix syntax:

```yaml
- repeat: 2
  during: 4s..6s
```

The canonical full form is also available:

```yaml
- during:
    range: 4s..6s
    body:
      - repeat: 2
```

`image` and `video` are zero-input Video source programs. An image requires an
authored duration unless its body context supplies a requested duration. A
video accepts its path and optional fit policy, while its full intrinsic
duration remains deferred so compilation stays media-pure.
Preflight requires exactly one decodable video stream and converts its duration
to the project frame rate. Source audio streams are ignored; prepared artifacts
and exports remain video-only. Both source programs support `cover`, `contain`,
and `stretch` fitting.

## Stack and body-program semantics

List order is executable stack order. Items run from first to last. Missing
fixed inputs consume the required suffix of the current local stack while
preserving signature order. A missing variadic input consumes all remaining
local occurrences in order. Explicit inputs read named values and consume
nothing.

- A named clip starts with an empty local stack and must leave exactly one
  Video. Its body does not receive timeline finalization.
- `then` consumes one preceding Video, executes its body with that value as the
  local stack, and must leave exactly one Video.
- `join` consumes two preceding Videos, executes its body with both in order,
  and must leave exactly one Video.
- Nested `timeline` starts empty and concatenates its leftover Videos in order.
- `during` consumes a base Video, executes its body against only the
  selected range, requires one processed Video, and splices that result between
  the untouched prefix and suffix.
- The root timeline starts empty and concatenates all leftover Videos in order.

There is no hidden replacement, fallback input, or automatic reduction inside
named-clip and body-program bodies.

## Names, references, and dependencies

Clip names and invocation `id` values share one namespace. Forward references
are allowed: they affect dependency resolution, not list execution order.
References create semantic graph dependencies and never move, remove, or
duplicate stack occurrences. Cycles and missing names are compile errors.

## Explicit non-goals

The foundation does not support audio output, transitions, decorative effects,
user-defined programs, plugins, runtime plan loading, a GUI, distributed
rendering, or multiple export profiles. Still-image sources must decode as
exactly one video frame with no audio.
