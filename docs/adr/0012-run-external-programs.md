---
status: accepted
---

# Run registered external programs

## Context

Some media operations are better kept as scripts or standalone binaries. They
still need typed ClipAsm calls, pure compilation, prepared identity, cache-aware
execution, and artifact verification. Shell command strings, in-process dynamic
libraries, and a long-lived plugin ABI would add unnecessary quoting, safety,
or compatibility problems.

## Decision

A source unit owns either a ClipAsm body or one native `external { ... }`
implementation. External programs are imported and called through the ordinary
program model. Their declaration requires an executable, optional ordered
literal or `file(...)` arguments, a positive semantic version, and one preserved
Video input. External implementation files cannot also contain statements or
imports; composition belongs in a ClipAsm wrapper.

Protocol version 1 is deliberately closed:

- fixed Video or Audio inputs;
- Integer, File, and Keyword parameters;
- exactly one Video output;
- the output preserves one declared Video input's exact domain and
  meaningful-audio state.

Compilation validates the declaration and emits a pure external semantic node
without resolving or running the executable. Preflight resolves and hashes the
executable, file arguments, and File parameters; prepares graph inputs; and
copies the preserved Video contract.

When cache-aware planning reaches the external node, rendering rehashes its
declared files before cache reuse or process launch. A verified downstream
artifact may prune the node. Execution passes the executable and argument
vector separately, never constructs a shell command, and sends one versioned
JSON request over standard input. The request contains named input artifacts,
bound parameters, resolved File paths, a temporary output path, project
settings, and FFmpeg/FFprobe paths. ClipAsm verifies the resulting artifact
before cache commit.

External code is trusted native code. ClipAsm does not sandbox it, impose a
timeout, prove termination, or require determinism. It may access arbitrary
local and network resources with the user's permissions.

Prepared identity includes the executable and declared file content hashes
observed during preflight, bound parameters, upstream artifacts, project
settings, and tool identities. Rehashing detects ordinary later changes but is
not atomic with the process opening a path. ClipAsm does not snapshot these
files or defend against hostile concurrent filesystem mutation.

Identity cannot discover undeclared files, imported modules, environment
variables, clocks, randomness, or network responses. Authors must declare file
dependencies and increment the semantic version when undeclared dependencies or
implementation meaning changes.

## Consequences

- External implementations reuse ordinary program binding and stack semantics.
- Executables and arguments remain separate, avoiding shell-source
  construction.
- Reproducibility remains the external author's responsibility.
- Multiple outputs, Duration/TimeRange parameters, variadic inputs, and
  output-domain discovery require later protocol decisions.
