# RhythmCut domain context

This document is the source of truth for the foundation's domain language and
settled authoring semantics. `README.md` is the user-oriented introduction;
code and future design work should preserve the distinctions below.

## Glossary

- A **workflow** is one parsed versioned YAML document.
- An **item** is one executable entry in a sequence body.
- A **program** is a typed operation such as `image`, `video`, `repeat`, or
  `concat`.
- A **clip** is any finite Video value. A **named clip** is a clip bound to a
  name under `clips`. `clip` is not a program or invocation keyword.
- A **value** is an immutable typed graph result. A stack may contain multiple
  occurrences of the same value.
- A **reference expression** is `$name` (or its expanded `ref: $name` form). It
  reads a named value without consuming any stack occurrence.
- A **local stack** is the ordered sequence of value occurrences visible while
  executing one body.
- The **semantic graph** is the pure result of compilation. It retains semantic
  operations, references, source-independent frame domains, origins, and
  explain data. Video-file source durations remain deferred.
- The **prepared plan** is the preflight result. It binds assets and tools,
  resolves video-file durations, lowers root-reachable semantic operations to
  exact renderer primitives, and assigns content fingerprints.

Compiled structure and prepared semantic hashes identify language and graph
semantics, not the Cargo package release. Engine release versions remain plan
and manifest metadata. Artifact-cache namespaces are a separate execution
identity containing an explicit cache-format version, resolved FFmpeg/FFprobe
identities, and the working-media policy.

## Public syntax

A workflow has `version: 1`, an optional `project.video` mapping (`width`,
`height`, `fps`), an optional `clips` mapping, a required `timeline` sequence,
and an optional `output` path. Mapping order has no meaning.

An item can be:

- a plain reference expression, such as `- $opening`;
- an expanded reference with annotations, such as
  `{ref: $opening, id: copy}`;
- a no-argument program name, such as `- concat`;
- a program invocation using primary shorthand, such as `- repeat: 3`;
- a full argument mapping, such as
  `{image: {path: card.png, duration: 1s}}`;
- one of the structural compounds `then`, `join`, or `timeline`, whose value is
  a sequence body.

Only `id` and `during` may be sibling fields beside an invocation or expanded
reference. Program arguments belong inside the program mapping. `during` is
semantically compound but deliberately uses postfix same-item notation:

```yaml
- repeat: 2
  during: 4s..6s
```

A standalone `- during: ...` item is not supported.

`image` and `video` are zero-input Video source programs. An image requires an
authored duration. A video accepts its path and optional fit policy, while its
full intrinsic duration remains deferred so compilation stays media-pure.
Preflight requires exactly one decodable video stream and converts its duration
to the project frame rate. Source audio streams are ignored; prepared artifacts
and exports remain video-only. Both source programs support `cover`, `contain`,
and `stretch` fitting.

## Stack and compound semantics

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
- `during` consumes a base Video, executes the annotated item against only the
  selected range, requires one processed Video, and splices that result between
  the untouched prefix and suffix.
- The root timeline starts empty and concatenates all leftover Videos in order.

There is no hidden replacement, fallback input, or automatic reduction inside
named-clip and compound bodies.

## Names, references, and dependencies

Clip names and invocation `id` values share one namespace. Forward references
are allowed: they affect dependency resolution, not list execution order.
References create semantic graph dependencies and never move, remove, or
duplicate stack occurrences. Cycles and missing names are compile errors.

## Explicit non-goals

The foundation does not support audio output, transitions, decorative effects,
user-defined programs, plugins, runtime plan loading, a GUI, distributed
rendering, or multiple export profiles. Still-image sources must decode as
exactly one video frame with no audio. Rendering currently uses lossless
FFV1/Matroska intermediates and a final H.264/yuv420p MP4 export.
