# Stack values, ownership, and visibility

A ClipAsm source program is a typed stack program. The stack makes composition
concise, while types, per-occurrence ownership, and explicit visibility
boundaries keep nested bodies predictable.

This page explains the model.
[`CONTEXT.md`](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTEXT.md#settled-stack-semantics)
owns the exact stack semantics, and
[ADR 0010](../adr/0010-add-typed-audio-and-body-input-scopes.md) records the
current typed ownership design.

## Values and occurrences are different

A **value** is an immutable typed result in the semantic graph. A **value
occurrence** is one place where that value participates in the evaluation
stack. The stack is one physical ordered sequence containing Video and Audio
occurrences.

This distinction matters because stack operations act on occurrences, while
references identify graph values. Naming a result does not move it, change its
type, or alter its stack effect.

## Implicit inputs bind by exact type

When an invocation omits graph inputs, ClipAsm binds them from the accessible
stack. Fixed ports are considered from last to first. Each port takes the
nearest accessible occurrence of its exact declared type. Occurrences of other
types stay in place and retain their order.

This excerpt from
[`examples/crossfade.clipasm`](https://github.com/NikitaMGrimm/clipasm/blob/main/examples/crossfade.clipasm)
leaves two Videos for `crossfade`:

```clipasm
image("assets/morning.png", 2s)
image("assets/evening.png", 2s)
crossfade(500ms)
```

The program's last port, `after`, binds first to the nearest Video: the evening
image. Its `before` port then binds to the morning image. On a heterogeneous
stack, Audio occurrences would be skipped without moving while these Video
ports bind.

A missing variadic input consumes every accessible occurrence of its selected
type in physical order. Generic operations such as `concat` still require one
homogeneous Video or Audio view; the
[language reference](../language-reference.md#arguments-and-stack-binding)
defines the exact authored forms and ambiguity rules.

Explicit graph inputs behave differently. A named reference or an isolated
inline input body supplies the port directly and consumes nothing from the
caller's stack. Contextual Video/Audio adaptation is possible only at an
explicit fixed-input boundary; implicit binding always requires an exact type.

## Ownership follows each occurrence

Every stack occurrence records which active body owns it. A body's **owned
values** are occurrences produced for that body. Its **visible values** are the
occurrences it may reach within the nearest visibility boundary.

Stack access is invocation metadata:

- `owned` binding can consume only occurrences owned by the current body
- `visible` binding may also reach occurrences owned by enclosing bodies, up to
  the nearest visibility boundary

The setting applies to one invocation and does not propagate to its children.
Direct built-ins and source programs default to `owned`; `join`, `glue`, and
`during` default to `visible`.

A plain stack block opens a visible child frame and returns every remaining
occurrence owned by that frame in order. An `@owned { ... }` block establishes
a visibility boundary. The block is structural: it is not a registered program
or a lexical name scope.

## References do not consume stack occurrences

An output binding attaches a local name to a value that an item already
produced. A reference expression such as `$picture` reads that named value and
creates a semantic graph dependency without consuming, moving, or removing a
stack occurrence.

Output names are immutable and unique throughout one source-program invocation.
A name introduced inside a nested body or stack block remains available in the
containing source program; ordinary braces do not create a lexical name scope.
Forward references affect dependency resolution, not statement execution
order, and dependency cycles are errors.

Body programs add one narrow exception. Their fixed graph inputs appear inside
the body as local aliases such as `$before`, `$after`, or `$video`. Those aliases
temporarily shadow an outer name while the body is active. Arguments are
evaluated before the aliases are introduced, so an argument cannot accidentally
self-reference the port it is defining.

Each invocation of an imported source program has its own isolated local
namespace. Inputs, parameters, body aliases, and output bindings do not escape;
only the program's ordered outputs return to its caller.

## Where to find exact rules

- [`CONTEXT.md`](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTEXT.md#settled-stack-semantics)
  owns binding,
  ownership, visibility, naming, and adaptation semantics.
- The [language reference](../language-reference.md#statements) owns current
  access modifiers, blocks, references, output bindings, and call syntax.
- [ADR 0010](../adr/0010-add-typed-audio-and-body-input-scopes.md) replaces the
  earlier contiguous stack-access model with typed per-occurrence ownership.
- [ADR 0011](../adr/0011-add-type-preserving-timeline-programs.md) explains
  checked Video-or-Audio type inference.
- The [architecture](../architecture.md#compilation) describes how checked stack
  plans are evaluated.
