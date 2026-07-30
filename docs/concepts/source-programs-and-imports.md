# Source programs and imports

Every `.clipasm` file defines one callable source program. An import makes that
program available under a local alias. It does not paste the file's text into
the caller.

## One file, one interface

A source program may declare Video or Audio inputs and scalar parameters. It
returns the values left by its body. The root file may also configure the
project and publication output.

Imported files cannot set root project or output configuration.

## Imports create local aliases

```clipasm
import "programs/polish.clipasm" as polish
```

The path is relative to the file that contains the import. Each import requires
an alias that is local to that file. It cannot replace a built-in name. ClipAsm rejects
import cycles and recursive source-program calls.

Call the imported program like a built-in:

```clipasm
video("assets/scene.mp4")
polish(10%)
```

## ClipAsm isolates calls

Each call gets its own local stack, inputs, parameters, and names. Those names do
not leak back to the caller. Only the program's final ordered values return.
Calling the same imported program twice therefore creates two independent
invocations.

## Paths keep their source

A relative path keeps the source-file base from its authoring location:

- an import path resolves from the importing file
- a media or default File path resolves from the file that contains it
- a value supplied by a caller keeps the caller's path base
- a CLI-supplied path resolves from the current working directory

This allows a reusable imported program to keep assets beside its own source.

## ClipAsm checks the complete package

Validation checks every linked imported source program, even when the root does
not call it. Rendering later opens only media and tools reachable from the Video
that ClipAsm publishes.

Follow [Import and call a source program](../guides/import-a-program.md) for the
complete task workflow. Chapter 7,
[Reuse a scene style across source files](../learn/07-reusable-program.md),
introduces imports within the learning project. See
[Imports](../reference/language/imports-and-external-programs.md#imports) for
exact syntax.
