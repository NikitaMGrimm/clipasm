# Imports and external programs

## Imports

```clipasm
import "programs/polish.clipasm" as polish
import "programs/brighten.clipasm" as brighten
```

Aliases are required. Paths resolve relative to the declaring source file.
Imported source files are ordinary callable programs with isolated local stacks
and names. Import cycles are errors. Callers use the same import syntax whether
the imported program is implemented in ClipAsm or by an external executable.

## External implementations

A source file may replace its executable ClipAsm body with one external
implementation:

```clipasm
clipasm 1

input video: Video
param amount: Integer = 15

external {
    executable = "python3"
    arguments = [file("brighten.py")]
    semantic_version = 1
    preserve = video
}
```

`executable` resolves relative to this source file when it contains a path, or
through the platform command lookup for a bare name. `arguments` is an ordered
list of literal strings and `file("...")` values. File arguments resolve from
this source file and are hashed during preflight. External protocol version 1
passes their resolved paths. Rendering rehashes declared files when the
external node is reached, but does not snapshot them or prevent a concurrent
change after that check. ClipAsm passes the executable and arguments separately
rather than constructing a shell command string; normal platform process
semantics still apply. `semantic_version` must be positive and is part of
semantic identity. `preserve` names the declared Video input whose exact
timeline domain and meaningful-audio state the single Video output preserves.

External programs currently accept fixed Video or Audio inputs and Integer,
File, or Keyword parameters. File values resolve from the source that supplied
them and are hashed during preflight. Native defaults are applied before
execution. An external program cannot also contain executable statements or
imports; use a separate ClipAsm wrapper program for composition. Compilation
remains pure. Preflight resolves and hashes the executable, and rendering sends
a versioned JSON request over standard input.
