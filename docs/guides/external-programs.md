# Review and run an external program

An external program exposes an ordinary typed ClipAsm interface but performs its
work in another executable. Treat it like any other native program you are
asked to run.

> **Warning:** rendering a reachable external program executes it with your user
> permissions. ClipAsm does not sandbox it, impose a timeout, or prevent file,
> network, or process access.

This guide uses `examples/external-brighten.clipasm`. Run commands from the
repository root.

## Review everything that can execute

Open these files before rendering:

- `examples/external-brighten.clipasm`, the wrapper;
- `examples/programs/brighten/program.clipasm`, the external declaration;
- `examples/programs/brighten/brighten.py`, the executed script.

The declaration chooses `python3`, passes the script as a declared file
argument, and promises that the output keeps the input Video's exact duration
and audio state. The script receives a versioned JSON request on standard input
and uses the FFmpeg path provided by ClipAsm.

This example requires Python 3, FFmpeg, and FFprobe on `PATH`.

## Check the ClipAsm source without executing it

```console,ignore
clipasm validate examples/external-brighten.clipasm
clipasm inspect examples/external-brighten.clipasm
```

These commands check the package and typed call. They do not locate or execute
Python, the script, or FFmpeg, and they cannot tell you whether the code is safe.

## Render only after review

```console,ignore
clipasm render examples/external-brighten.clipasm
```

Before execution, ClipAsm resolves and hashes the executable and declared file
arguments. It hashes them again when the external operation is reached, sends
the request, and verifies the produced media before accepting it. The example
writes `examples/external-brighten.mp4`.

A program can still depend on undeclared state such as environment variables,
network responses, imported modules, random values, clocks, or other files.
Those dependencies are invisible to the cache. External-program authors must
declare supported file dependencies and change `semantic_version` whenever
other output-affecting behavior changes.

See [External implementations](../reference/language/imports-and-external-programs.md#external-implementations)
for the declaration and [External programs and the trust boundary](../concepts/external-programs-and-trust.md)
for the security and reproducibility model.
