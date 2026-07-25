# Review and run an external program

An external program keeps an ordinary ClipAsm interface while delegating its
implementation to a script or executable. This guide uses the committed
`examples/external-brighten.clipasm` wrapper.

> **External programs are trusted native code.** Rendering can execute them
> with the current user's access to the filesystem, network, processes, and
> other machine resources. ClipAsm does not sandbox them, impose an execution
> timeout, or prove that they terminate or behave deterministically. Do not
> render an unfamiliar project until you have reviewed and trust every external
> declaration, executable, script, and declared file argument.

Run all commands from the repository root.

## Review the external implementation

Before running the example, open and inspect both:

- `examples/programs/brighten/program.clipasm`, the external declaration;
- `examples/programs/brighten/brighten.py`, the Python program it executes.

The declaration names `python3` as its executable, passes `brighten.py` as a
content-hashed file argument, declares its semantic version, and states that
the output preserves the `video` input's domain. The Python program reads the
versioned request from standard input and invokes the FFmpeg path supplied by
ClipAsm.

Also inspect the wrapper, `examples/external-brighten.clipasm`. It imports the
external source program under the local alias `brighten`, creates a Video, and
calls `brighten(amount=15)`.

This example requires Python 3, FFmpeg, and FFprobe on `PATH`.

## Perform the pure checks

Validation and inspection compile the declaration into a pure semantic node.
They do not resolve or execute `python3` or `brighten.py`:

```console
cargo run -- validate examples/external-brighten.clipasm
cargo run -- inspect examples/external-brighten.clipasm
```

These checks confirm the ClipAsm package and its typed call. They cannot confirm
that the executable is available, that its undeclared environment is
reproducible, or that running it is safe.

## Render only after review

> Run the following command only after you trust the declaration and Python
> program described above.

```console
cargo run -- render examples/external-brighten.clipasm
```

During preflight, ClipAsm resolves and hashes the executable and declared file
argument and prepares its input dependencies. During rendering, it re-verifies
those hashes, executes the external program with a versioned JSON request, and
verifies the artifact before accepting it. The example publishes
`examples/external-brighten.mp4`.

Cache identity cannot discover undeclared inputs such as environment variables,
clock or random state, network responses, imported modules, or arbitrary files.
The external-program author remains responsible for declaring file dependencies
and updating the semantic version when output meaning changes.

See [External implementations](../language-reference.md#external-implementations)
for the normative declaration and protocol constraints,
[Pure compilation and external-program trust](../concepts/external-programs-and-trust.md)
for the trust model, and the [examples catalog](../examples.md#external-program)
for the canonical command listing.
