---
status: accepted
---

# Add typed Audio and body input scopes

## Context

Audio and Video need independent graph values. A stack model based on contiguous
typed suffixes makes unrelated types block one another, while automatically
detaching every Video's audio would make stack shape depend on hidden media
properties.

Body programs also need stable references to their bound inputs after an
operation consumes the corresponding stack occurrences.

## Decision

The semantic graph has two closed value types: `Video` and `Audio`. Video is a
picture timeline with optional synchronized audio; standalone Audio is a finite
timeline on the project sample grid. Compilation remains media-pure.

The evaluation stack is one ordered sequence whose occurrences carry body
ownership independently. Missing fixed inputs bind from last port to first,
selecting the nearest accessible value of the exact required type. A missing
variadic input consumes every accessible value of its type in physical order.
Other types remain in place, and implicit binding never adapts types.

Every body program exposes its resolved fixed graph inputs as immutable lexical
references named after the ports. Argument expressions are evaluated before
those aliases are introduced, so an alias cannot affect its own binding. The
aliases derive from descriptors and resolved calls, not registered program
names.

Explicit graph-input boundaries allow two direct contextual adaptations:

- `Video` to `Audio` extracts synchronized audio; a silent Video yields
  matching-duration silence.
- `Audio` to `Video` creates a project-sized black picture carrying that Audio.

Each adaptation is an explicit graph node. Implicit stack inputs and program or
body outputs never adapt.

Project Audio is stereo at a configurable positive sample rate, defaulting to
48 kHz. Exact Audio domains use sample counts. Working Video artifacts always
contain normalized lossless audio, including silence, so ordinary Video
operations can preserve timing uniformly. Semantic audio presence alone
controls whether final publication includes audio. Standalone Audio uses the
same normalized project format.

`set_audio` starts Audio at zero and conforms it to the Video duration by
trimming or padding with silence. A render entrypoint permits exactly one Video
plus any number of auxiliary Audio outputs; preflight follows only the
publishable Video.

## Consequences

- Mixed Audio and Video stacks bind deterministically without unrelated values
  blocking one another.
- Body programs can reuse their bound inputs through lexical port references.
- Existing visual operations preserve attached audio unless a program
  explicitly extracts or replaces it.
- New Audio effects can use ordinary typed graph operations without changing
  stack ownership or body scoping.
