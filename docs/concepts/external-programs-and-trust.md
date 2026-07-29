# External programs and the trust boundary

An external program looks like an ordinary typed ClipAsm program to its caller,
but rendering delegates one operation to another executable.

> **Warning:** a reachable external program runs with your user permissions.
> Importing or validating it does not execute it; rendering does.

## What validation checks

Validation checks the declaration, inputs, parameters, defaults, and ordinary
call behavior. It records the external operation without locating or executing
the executable.

The current interface supports fixed Video or Audio inputs, Integer, File, or
Keyword parameters, and exactly one Video output. The declaration identifies the
input whose duration and audio state the output preserves.

## What rendering checks

Before execution, ClipAsm locates and hashes the executable, declared File
arguments, and File-valued parameters. It sends a versioned JSON request over
standard input and passes the executable and arguments separately rather than
building a shell command.

A zero exit status is not enough: ClipAsm probes the produced artifact and
checks it against the declared Video result before accepting it.

An external implementation must complete the work needed for its declared output
before the direct process exits. ClipAsm contains the invocation in a dedicated
process group on Unix and a Job Object on Windows, then terminates remaining
managed descendants when the direct process finishes. Work that deliberately
escapes that managed group or job is outside the protocol contract.

## What ClipAsm cannot make safe

ClipAsm does not sandbox the process, limit its runtime, or prevent access to the
filesystem, network, environment, or other processes. It cannot discover hidden
inputs such as:

- environment variables;
- clocks or random state;
- network responses;
- imported modules;
- undeclared files.

Persistent caching assumes an external implementation is deterministic with
respect to everything ClipAsm identifies: executable and declared File bytes,
`semantic_version`, arguments, parameters, project settings, and input artifact
bytes. Repeating the same identified invocation must produce equivalent output.
Clock time, randomness, network responses, mutable environment state, and
undeclared files violate that contract and can make both the external node and
cached descendants stale or mutually inconsistent.

Authors must declare File dependencies where supported and update
`semantic_version` whenever other output-affecting behavior changes. An
implementation that cannot satisfy this deterministic contract should not rely
on persistent reuse; remove the project's `.clipasm/` state before each render
until ClipAsm exposes an explicit nonpersistent execution policy.

Hashing reduces accidental drift but does not create an immutable snapshot. A
file can still change between the final hash and the external process reading
it.

Review and trust every declaration, executable, script, and file argument before
rendering. See [Review and run an external program](../guides/external-programs.md)
for a safe workflow and [External implementations](../reference/language/imports-and-external-programs.md#external-implementations)
for exact declaration rules.
