---
status: accepted
---

# Separate frontends from canonical source

ClipAsm uses one representation-neutral canonical authored model between
authoring representations and compilation. A frontend parses and desugars its
surface representation into a `SourceEntryPoint` containing project and
publication settings plus a callable `SourceProgram`. The compiler consumes
only canonical source and does not open authored files or select a frontend.

YAML is the first frontend. YAML mappings, scalar styles, reserved fields,
header versioning, postfix forms, and other YAML-specific sugar belong under
`frontend::yaml` and disappear before compilation. Descriptor-dependent
interpretation of canonical argument literals, references, and bodies belongs
to the common compiler binder so future frontends cannot accidentally enforce
different program contracts. No generic frontend trait is introduced before a
second frontend demonstrates a need for one.

The canonical source package is an opaque public compiler input, not yet a
public construction API. Its fields, bodies, invocations, and builders remain
crate-private until a second frontend provides concrete requirements for a
stable external API. This preserves the architectural boundary without
prematurely freezing the authored model.

Every source span retains its source unit. A source unit carries a diagnostic
name, retained text, an optional backing filesystem path, and an optional base
directory. Relative authored paths resolve from the source unit containing the
authored value. Entrypoint publication and cache placement use the entrypoint
source unit. This permits future imported programs to retain their own path
contexts without making path resolution depend on YAML or on one root file.

`SourceProgram` contains callable stack semantics. `SourceEntryPoint` adds
root-only project and publication configuration. Frontend syntax versions do
not survive as compiler-facing program versions unless they represent an
actual canonical semantic distinction. Semantic origins own construct names so
future authored program names do not require static storage.

Compiled JSON remains a downstream serialized view of `CompiledProgram`; it is
not canonical authored source and is not read as a frontend representation.
Its schema is built through an explicit document adapter rather than by
deriving serialization from the internal compiler struct layout. Removing the
old frontend version field changes that document incompatibly, so compiled
format version 8 records this boundary.

This decision did not itself add imports, callable authored-program
registration, or program signatures. Those capabilities were subsequently
defined by [ADR 0009](0009-call-authored-source-programs.md). Native text syntax
and frontend plugins remain outside this decision.
