# ClipAsm

ClipAsm is a typed, stack-based language for building Video and Audio graphs.
The native `.clipasm` loader lowers source into an internal authored model; the
compiler then creates a pure semantic graph, preflight resolves reachable media
and tools, and the renderer produces verified cached artifacts and an MP4.

Start with:

- the [language reference](workflow-reference.md) for syntax and stack rules;
- the [examples](examples.md) for runnable programs;
- the [architecture](architecture.md) for compiler phases;
- the [change guide](development/change-guide.md) for implementation ownership.

The generated rustdoc is the API reference for embedding ClipAsm in Rust.
