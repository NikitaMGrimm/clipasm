---
status: accepted
---

# Treat source files as programs

ADR 0006 refines the stack-storage and visibility wording below. ADR 0007
supersedes the single-source-result restriction: source programs now return
their ordered final owned suffix, while publication still requires one Video.
Named clips and inline input bodies remain isolated and single-valued.

A ClipAsm source file defines one typed stack program whose outputs are its
ordered final owned suffix. The YAML document is a sequence: its required first item is a
non-executable `program` header, and every remaining item belongs to the
executable source-program body. The header owns language version bootstrapping,
project Video settings, local named-clip declarations, and the optional
entrypoint output path.

The source-program body starts with an empty evaluation stack and receives no
implicit finalizer. Zero, one, or multiple remaining owned values are returned
in order. Authors use `concat` or an explicit nested `glue` when several Videos
should become one Video. The
registered `glue` body program remains available, but it has no privileged
role in source-file evaluation.

The source program's ordered outputs belong to compilation semantics. A header
`output` path requires exactly one Video, whose publication belongs to
entrypoint render orchestration. The output path does not alter semantic graph identity and must
not become a graph value or operation. A future source-program invocation from
another YAML program will return the Video without publishing the imported
file's output default.

A fixed, single-value graph input may be supplied by an inline input body.
That body starts with an empty stack, inherits the enclosing requested-frame
context, evaluates ordinary items, and must leave exactly one value of the
declared input type. Its values do not enter or consume the surrounding local
stack. IDs and references inside it use the existing global named-value
namespace and dependency machinery.

This decision deliberately does not add source-program signatures, imports,
runtime-owned registry definitions, scalar-producing programs, or variadic
input bodies. ADR 0007 adds multiple outputs without adding callable YAML
programs. Those features can build on the same source body
and isolated input-body evaluator when their language contracts are defined.
