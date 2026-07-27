# Imports and external programs

Imports make another `.clipasm` file callable under a local name. The imported
file may be implemented in ClipAsm or may declare a trusted external executable.

## Imports

```clipasm
import "programs/polish.clipasm" as polish
import "programs/brighten.clipasm" as brighten
```

An alias is required. The path resolves from the file containing the import.
Aliases are local, are not re-exported, and cannot shadow built-in programs.
Import cycles are errors.

Each imported source file defines one callable program with its own local stack,
inputs, parameters, and names. Callers use the same syntax regardless of its
implementation:

```clipasm
video("assets/scene.mp4")
polish(8%)
```

## External implementations

A source file may declare an external implementation instead of an executable
ClipAsm body:

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

### Fields

- `executable` is either a source-relative path or a bare name found through the
  platform command lookup.
- `arguments` is an ordered list of literal strings and `file("...")` values.
- `semantic_version` is a positive author-controlled version for output meaning.
- `preserve` names the Video input whose exact duration and meaningful-audio
  state the single Video output must retain.

A `file("...")` argument resolves from the external source file. ClipAsm hashes
declared executable and File bytes during preflight and checks them again before
execution. It passes the executable and argument vector separately rather than
constructing a shell command.

External programs currently support fixed Video or Audio inputs; Integer, File,
or Keyword parameters; and exactly one Video output. Defaults are applied before
execution.

An external implementation file cannot also contain executable statements or
imports. Put composition in a separate wrapper file and import the external
program there.

Validation remains media- and process-free. Rendering sends a versioned JSON
request over standard input and verifies the produced media afterward. External
programs are not sandboxed; read [External programs and the trust boundary](../../concepts/external-programs-and-trust.md)
before running one.
