---
status: accepted
---

# Add typed Audio and body input scopes

## Context

ClipAsm originally had one `Video` value type and represented each body's
accessible values as contiguous visible and owned suffixes. That model cannot
express independent Audio values without making unrelated types block one
another. Custom audiovisual transitions also need to reuse a body program's
bound inputs after a visual operation consumes their stack occurrences.

Automatic detachment of every Video's audio would make stack shape depend on
hidden media properties and would complicate ordinary visual editing. A generic
conversion search would make implicit stack behavior difficult to predict.

## Decision

The semantic graph has two finite value types: `Video` and `Audio`. A Video is a
picture timeline with optional synchronized audio. A standalone Audio is a
finite normalized audio timeline. Compilation remains media-pure; preflight is
the first phase that inspects streams and exact source durations.

The evaluation stack remains one physical ordered sequence. Each value carries
body ownership independently. Missing fixed inputs bind from last port to first
port, selecting the nearest accessible value of the exact required type. A
missing variadic input consumes all accessible values of its declared type in
physical order. Values of other types remain in place. Implicit binding never
adapts types.

Every body program exposes its resolved fixed graph inputs inside its body as
immutable local references named after the ports. Invocation arguments are
evaluated in the caller scope before those aliases are introduced. The aliases
therefore shadow outer names only inside the body and cannot self-reference
while their own port expressions are evaluated. The execution body is
structural invocation data, not an input port. Body aliases are derived from
program descriptors and resolved calls rather than registered program names.

Explicit graph-input boundaries allow exactly two direct contextual
adaptations:

- `Video` to `Audio` extracts the synchronized audio timeline; a silent Video
  yields matching-duration silence.
- `Audio` to `Video` creates a project-sized black picture carrying that Audio.

Each adaptation is an explicit semantic graph node. One boundary performs at
most one direct adaptation; nested explicit boundaries may compose them.
Program outputs, body outputs, and implicit stack inputs are never adapted.

The initial user-visible Audio programs are:

- `audio(path) -> Audio`
- `extract_audio(video: Video) -> Audio`
- `set_audio(audio: Audio, video: Video) -> Video`

`set_audio` starts the Audio at zero and uses the Video duration. Excess Audio
is trimmed and short Audio is padded with silence.

The canonical Audio format is initially fixed at 48 kHz stereo. Exact Audio
domains use integral sample counts. Conversions between frame and sample
durations use checked ceiling division so a finite source interval is never
shortened.

Working Video artifacts always contain one normalized lossless audio stream,
including compressed silence for semantically silent Videos. This lets every
existing Video operation preserve, trim, repeat, splice, or concatenate audio
with the picture timeline using one renderer contract. Semantic `has_audio`
state determines whether final MP4 publication includes AAC audio. Standalone
Audio artifacts use normalized lossless audio in Matroska.

A render entrypoint is valid when its ordered outputs contain exactly one Video
and any number of auxiliary Audio values. Preflight follows only the unique
Video's reachable graph. Nested body contracts remain exact and do not permit
unconsumed Audio values to escape accidentally.

## Consequences

- Custom visual and audio transition branches can reuse `$before` and `$after`
  after a visual transition consumes their stack occurrences.
- Adding a new body program automatically exposes its fixed input names without
  parser, checker, or evaluator program-name branches.
- Mixed Audio and Video stacks remain deterministic, and unrelated values do
  not block exact-type binding.
- Existing visual programs preserve attached audio by default; users detach or
  replace audio only when they explicitly request it.
- Compiled, prepared, cache, and affected program semantic versions change
  because graph identity and artifact contracts now include Audio semantics.
- Future audio effects can be ordinary typed graph operations without changing
  stack binding or body scoping again.
