# ClipAsm

ClipAsm is a strict, typed stack language with a representation-neutral
canonical source model. The current YAML frontend desugars authored YAML into
that model; the compiler then builds a pure semantic graph, prepares
result-reachable media and tools, and renders an MP4 through verified cached
artifacts.

The current language supports still-image and video-file sources, named clips,
references, inline fixed-input bodies, concatenation, repetition, effects, and
the `join`, `glue`, and `during` body programs. Audio output, plugins,
imports, callable authored programs, and user-defined signatures are outside
the current foundation.

Start with:

- the [YAML frontend reference](workflow-reference.md) to write ClipAsm YAML;
- the [examples](examples.md) to run representative source programs;
- the [architecture](architecture.md) to understand compiler phases;
- the [change guide](development/change-guide.md) to locate implementation,
  tests, documentation, and identity impact.

Rust applications embedding ClipAsm should use the separately generated
rustdoc API reference.
