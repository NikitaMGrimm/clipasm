# ClipAsm

ClipAsm is a strict, typed YAML language for compiling video workflows. It
normalizes authored YAML, compiles programs into a pure semantic graph,
prepares reachable media and tools, and renders an MP4 through verified cached
artifacts.

The current language supports still-image and video-file sources, named clips,
references, concatenation, repetition, and the `then`, `join`, `timeline`, and
`during` body programs. Audio output, transitions, effects, plugins, and
user-defined programs are outside the current foundation.

Start with:

- the [workflow reference](workflow-reference.md) to write ClipAsm YAML;
- the [examples](examples.md) to run representative workflows;
- the [architecture](architecture.md) to understand compiler phases;
- the [repository guide](agents/repository-guide.md) to locate implementation
  and test ownership.

Rust applications embedding ClipAsm should use the separately generated
rustdoc API reference.
