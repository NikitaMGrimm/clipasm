# Review and run an external program

An external program exposes an ordinary typed ClipAsm interface but performs its
work in another executable. Treat it like any other native program you are
considering for execution.

> **Warning:** rendering a reachable external program executes it with your user
> permissions. ClipAsm does not sandbox it, impose a timeout, or prevent file,
> network, or process access.

This guide uses `examples/external-brighten.clipasm`. Run commands from the
repository root. The result is a two-second brightened MP4.

## Before you start

Use a ClipAsm source checkout with Python 3, FFmpeg, and FFprobe on `PATH`.
External programs are advanced, trusted integrations. Do not render code that
you have not reviewed.

## 1. Review the project-controlled code

Open these project files before rendering:

- `examples/external-brighten.clipasm`, the wrapper
- `examples/programs/brighten/program.clipasm`, the external declaration
- `examples/programs/brighten/brighten.py`, the executed script

The declaration chooses `python3` and passes the script as a declared file
argument. It promises that the output keeps the input Video's exact duration
and audio state. The script receives a versioned JSON request on standard
input. It uses the FFmpeg path that ClipAsm provides.
The request also provides the exact Video pixel/color encoding and Audio sample
encoding required for the output artifact. ClipAsm probes those fields after
the process exits.

The `python3`, FFmpeg, and FFprobe binaries are also executable dependencies.

## 2. Confirm the executable dependencies

Confirm that the command lookup in your environment resolves to installations
you trust.

## 3. Validate the ClipAsm source

```console,ignore
clipasm validate examples/external-brighten.clipasm
```

## 4. Inspect the compiled program

```console,ignore
clipasm inspect examples/external-brighten.clipasm
```

These commands check the package and typed call. They do not locate or execute
Python, the script, or FFmpeg. They cannot tell you whether the code is safe.

## 5. Render only after review

```console,ignore
clipasm render examples/external-brighten.clipasm
```

Before execution, ClipAsm resolves and hashes the executable, declared File
arguments, and File-valued parameters. It hashes reached dependencies again,
sends the request, and verifies the produced media before accepting it. The
example writes `examples/external-brighten.mp4`.

## 6. Check the rendered video

Open `examples/external-brighten.mp4`. Confirm that it is a two-second
brightened version of the input. External code can still read undeclared state.
A successful render does not make an unreviewed program safe or reproducible.

See [External implementations](../reference/language/imports-and-external-programs.md#external-implementations)
for the declaration and [External programs and the trust boundary](../concepts/external-programs-and-trust.md)
for the security and reproducibility model.
