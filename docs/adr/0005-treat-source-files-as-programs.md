---
status: accepted
---

# Treat source files as programs

The file-as-program decision remains current. The YAML header and named-clip
surface syntax described below are historical and were replaced by the native
`.clipasm` declarations and `clip` sugar in ADR 0013.

ADR 0006 refines the stack-storage and visibility wording below. ADR 0007
supersedes the single-source-result restriction: source programs now return
their ordered final owned values, while publication still requires exactly one Video among the outputs.
Named clips and inline input bodies remain isolated and single-valued.

ADR 0009 subsequently adds source-program signatures, imports, runtime-owned
program definitions, and ordinary calls between authored source files. The
file-as-program and entrypoint-publication decisions below remain unchanged.

A ClipAsm source file defines one typed stack program whose outputs are its
ordered final owned values. Non-executable file declarations own language
versioning, project settings, imports, inputs, parameters, and optional
entrypoint publication. Executable statements form the source-program body.

The source-program body starts with an empty evaluation stack and receives no
implicit finalizer. Zero, one, or multiple remaining owned values are returned
in order. Authors use `concat` or an explicit nested `glue` when several Videos
should become one Video. The
registered `glue` body program remains available, but it has no privileged
role in source-file evaluation.

The source program's ordered outputs belong to compilation semantics. A header
`output` path requires exactly one Video, whose publication belongs to
entrypoint render orchestration. The output path does not alter semantic graph
identity and must not become a graph value or operation. As added by ADR 0009,
an invocation from another source program returns the authored outputs without
publishing an imported file's output default; imported files may not declare
that root-only setting.

A fixed, single-value graph input may be supplied by an inline input body.
That body starts with an empty stack, inherits the enclosing requested-frame
context, evaluates ordinary items, and must leave exactly one value of the
declared input type. Its values do not enter or consume the surrounding local
stack. IDs and references inside it use the existing global named-value
namespace and dependency machinery.

This decision deliberately did not add source-program signatures, imports,
runtime-owned registry definitions, scalar-producing programs, or variadic
input bodies. ADR 0007 added multiple outputs, and ADR 0009 later added callable
authored programs, signatures, imports, and runtime-owned definitions. Scalar
programs and variadic input bodies remain outside this decision.
