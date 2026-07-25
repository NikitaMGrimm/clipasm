# Architecture decision records

Architecture decision records (ADRs) preserve the context, trade-offs, and
consequences of decisions that shape ClipAsm. Read the relevant records before
changing a phase boundary, semantic or execution identity, program model,
authoring contract, or media-timing rule.

ADRs explain why a decision was made; they are not the normative language
reference. Use
[`CONTEXT.md`](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTEXT.md)
for current domain language and settled authoring semantics, the
[language reference](../language-reference.md) for current public syntax and
behavior, and the [architecture](../architecture.md) for current phase
responsibilities.

Write a new ADR when a change creates or revises a durable boundary,
non-obvious trade-off, identity rule, or phase owner. Do not rewrite an accepted
record to make history look current. Record a superseding decision separately
and update the relationships and statuses of both records. Start from the
[ClipAsm ADR template](template.md).

> **Current stack model:** ADR 0010 supersedes ADR 0006. Read ADR 0006 only for
> its historical context. Current documentation must use per-occurrence
> ownership and exact-type binding from
> [ADR 0010](0010-add-typed-audio-and-body-input-scopes.md) and
> [`CONTEXT.md`](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTEXT.md#settled-stack-semantics),
> not ADR 0006's
> contiguous visible/owned suffix model.

## Language, compilation, and phase boundaries

| ADR | Status | Decision |
| --- | --- | --- |
| [0001](0001-keep-compilation-pure.md) | `accepted` | Keep compilation pure and defer media and tool inspection to preflight. |
| [0003](0003-separate-semantic-and-execution-identities.md) | `accepted` | Separate authored semantic identity, prepared identity, and renderer compatibility. |
| [0008](0008-separate-parsing-from-canonical-source.md) | `accepted` | Keep parsing and lowering separate from crate-private canonical source. |
| [0013](0013-adopt-native-clipasm-language.md) | `accepted` | Use one native `.clipasm` language and keep syntax and sugar language-owned. |
| [0015](0015-keep-native-operations-phase-owned.md) | `accepted` | Keep native operations closed, exhaustive, and owned by their phases. |

See the architecture's
[language and canonical-source](../architecture.md#language-and-canonical-source),
[compilation](../architecture.md#compilation), and
[ownership](../architecture.md#ownership-rules) sections for the current
responsibility map.

## Programs, source units, and stack evaluation

| ADR | Status | Decision |
| --- | --- | --- |
| [0002](0002-use-one-program-model.md) | `accepted` | Use one typed model for direct and body programs. |
| [0005](0005-treat-source-files-as-programs.md) | `accepted` | Treat a source file as a stack program and keep publication separate. |
| [0006](0006-add-explicit-stack-access.md) | **`superseded by ADR 0010`** | Historical explicit stack-access model; use [ADR 0010](0010-add-typed-audio-and-body-input-scopes.md) for current ownership semantics. |
| [0007](0007-support-ordered-program-outputs.md) | `accepted` | Give programs an ordered sequence of typed outputs. |
| [0009](0009-call-authored-source-programs.md) | `accepted` | Make imported authored source programs ordinary callable definitions with isolated invocations. |
| [0010](0010-add-typed-audio-and-body-input-scopes.md) | `accepted` | Add typed Audio, per-occurrence stack ownership, exact-type binding, and body-input aliases. |
| [0011](0011-add-type-preserving-timeline-programs.md) | `accepted` | Resolve type-preserving Video or Audio timeline programs during checking. |

For current vocabulary and behavior, see
[settled stack semantics](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTEXT.md#settled-stack-semantics)
and
[names, references, and dependencies](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTEXT.md#names-references-and-dependencies).
Older records can contain historical surface syntax; use the
[language reference](../language-reference.md) for authored examples.

## Media domains, execution, and rendering

| ADR | Status | Decision |
| --- | --- | --- |
| [0004](0004-quantize-source-duration-by-coverage.md) | `accepted` | Quantize source video duration to the smallest covering project-frame count. |
| [0012](0012-run-external-programs.md) | `accepted` | Run typed external programs only as trusted rendering-time executables. |
| [0014](0014-map-frame-and-sample-boundaries.md) | `accepted` | Map cumulative frame and sample boundaries exactly without a shared master tick. |
| [0016](0016-overlap-audiovisual-transitions-exactly.md) | `accepted` | Overlap crossfade picture and Audio on the same exact output boundaries. |
| [0017](0017-snapshot-prepared-data-assets.md) | `accepted` | Snapshot reachable data assets so preflight and execution consume the same bytes. |

See the architecture's [preflight](../architecture.md#preflight) and
[rendering](../architecture.md#rendering) sections for current execution
ownership. External executables are trusted code; the complete trust boundary
is recorded in [ADR 0012](0012-run-external-programs.md).
