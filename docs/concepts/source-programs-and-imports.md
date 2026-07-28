# Source programs and imports

Every `.clipasm` file defines one callable source program. Importing a file makes
that program available under a local alias; it does not paste the file's text
into the caller.

## One file, one interface

A source program may declare Video or Audio inputs and scalar parameters, then
return the values left by its body. The root file may additionally configure the
project and publication output.

Imported files cannot set root project or output configuration.

## Imports create local aliases

```clipasm
import "programs/polish.clipasm" as polish
```

The path is relative to the file containing the import. The alias is required,
local to that file, and cannot replace a built-in name. Import cycles and
recursive source-program calls are rejected.

The imported program is called like a built-in:

```clipasm
video("assets/scene.mp4")
polish(10%)
```

## Calls are isolated

Each call gets its own local stack, inputs, parameters, and names. Those names do
not leak back to the caller. Only the program's final ordered values return.
Calling the same imported program twice therefore creates two independent
invocations.

## Paths keep their source

A relative path stays attached to where it was authored:

- an import path resolves from the importing file;
- a media or default File path resolves from the file containing it;
- a value supplied by a caller keeps the caller's path base;
- a CLI-supplied path resolves from the current working directory.

This allows a reusable imported program to keep assets beside its own source.

## The complete package is checked

Validation checks every linked imported source program, even when the root does
not call it. Rendering later opens only media and tools reachable from the Video
being published.

Follow [Import and call a source program](../guides/import-a-program.md) for the
complete task workflow. Chapter 7,
[Reuse a scene style across source files](../learn/07-reusable-program.md),
introduces imports within the learning project. See
[Imports](../reference/language/imports-and-external-programs.md#imports) for
exact syntax.
