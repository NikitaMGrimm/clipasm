# From source to published video

ClipAsm separates understanding a program from inspecting its media and from
executing it. That separation lets the language and compiler answer structural
questions even when media files or rendering tools are unavailable.

This page is a mental model. The
[architecture](../architecture.md) owns exact phase responsibilities, and
[ADR 0001](../adr/records.md#keep-compilation-pure) records why compilation stays
media-pure.

## The pipeline at a glance

```text
native .clipasm source
  -> language front end
canonical source package
  -> compilation
semantic graph + ordered source-program outputs
  -> preflight of the publishable Video's reachable graph
prepared plan
  -> rendering, verification, and publication
MP4 + manifest
```

People often use *compilation* for the whole path from authored source to
compiled semantics. Internally, the architecture draws a finer boundary: the
native-language front end lexes and parses source, loads its package, and lowers
surface sugar before the compiler receives canonical source. This keeps grammar
and sugar out of semantic evaluation without making the front end media-aware.

## The language front end builds a package

One root source unit and its imports form a source package. The front end
resolves that authored structure and lowers it to the internal canonical source
model. Canonical source preserves source locations and path bases, but it is not
a public builder API or another authoring format.

At this point, ClipAsm has interpreted language structure, not media content.
See [Source programs and imports](source-programs-and-imports.md) for how source
units become callable programs.

## Compilation determines meaning

Compilation checks the complete linked package, including imported source units
that the root never calls. It resolves program calls, references, types, stack
bindings, body contracts, named outputs, and the ordered outputs of each source
program. Checked source is then evaluated into a semantic graph.

The result describes what the program means without opening authored media or
running FFmpeg, FFprobe, or an external program. Facts derivable from authored
data can already be exact. Media-derived facts can remain deferred; for
example, a video-file source may not have an exact project-frame count yet.

Pure validation and compilation allow a source program to return zero, one, or
multiple ordered values. That is a compilation property, not a promise that
every such result can be published.

## Preflight prepares reachable work

Preflight is the first phase allowed to inspect assets and tools. For the graph
reachable from the Video selected for publication, it resolves authored paths,
hashes source assets, probes media, derives exact domains, checks the required
FFmpeg capabilities, resolves external executables, and lowers semantic nodes
to renderer primitives.

This gives two deliberately different scopes:

- compilation checks every linked source unit, even if it is never called
- preflight inspects only assets, operations, and capabilities reachable from
  the result being prepared

An unused imported program must therefore be valid source, but its unused media
does not make preflight reject an otherwise reachable plan.

The output of preflight is the prepared plan: resolved assets and tools,
exact media domains, and the primitives the renderer can execute.

## Rendering executes and publishes

Rendering verifies the prepared tool identities, then plans backward from the
result. A verified cache artifact satisfies that node and prunes its upstream
subtree; a miss makes the node's inputs part of the execution frontier. Source
files and external executables are rehashed when their node is reached, and
missing renderer primitives are executed in topological order. Reached
[external programs](external-programs-and-trust.md) are trusted native code.
Produced artifacts are checked before they enter the cache or publication
transaction.

Publication chooses exactly one Video output by type. A source program may also
return Audio values, but those are auxiliary and are not published. A program
with no Video or more than one Video can still be valid for pure compilation
while being invalid as a render entrypoint.

The optional output path belongs to entrypoint publication, not to the semantic
graph or its identity. Rendering publishes the verified Video as MP4 and writes
its sibling manifest.

## Where to find exact rules

- The [architecture](../architecture.md) defines phase terminology and
  specifies the front end, compiler,
  preflight, renderer, cache, and publication responsibilities.
- [ADR 0001](../adr/records.md#keep-compilation-pure) explains the pure compilation
  boundary.
- [ADR 0003](../adr/records.md#separate-semantic-and-execution-identities)
  explains why meaning, prepared content, and execution compatibility have
  separate identities.
- The [language reference](../language-reference.md) owns current authored
  syntax and CLI binding forms.
