# ClipAsm

ClipAsm is a strict, typed YAML language whose source files are stack programs
returning one Video. It normalizes authored YAML, compiles the source program
into a pure semantic graph, prepares result-reachable media and tools, and
renders an MP4 through verified cached artifacts.

The current language supports still-image and video-file sources, named clips,
references, inline fixed-input bodies, concatenation, repetition, effects, and
the `join`, `timeline`, and `during` body programs. Audio output, plugins,
imports, and user-defined program signatures are outside the current
foundation.

Start with:

- the [source-program reference](workflow-reference.md) to write ClipAsm YAML;
- the [examples](examples.md) to run representative source programs;
- the [architecture](architecture.md) to understand compiler phases;
- the [change guide](development/change-guide.md) to locate implementation,
  tests, documentation, and identity impact.

Rust applications embedding ClipAsm should use the separately generated
rustdoc API reference.
