# Architecture decision records

Architecture decision records (ADRs) describe the active durable decisions that
shape ClipAsm. Read the relevant records before changing a phase boundary,
identity, program model, authoring contract, or media-timing rule.

ADRs explain constraints and trade-offs; they are not the normative language
reference or an implementation history. Git retains superseded decisions. Use
[`CONTEXT.md`](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTEXT.md)
for current domain language and settled authoring semantics, the
[language reference](../language-reference.md) for current public syntax and
behavior, and the [architecture](../architecture.md) for current phase
responsibilities.

Write an ADR for a durable boundary, non-obvious trade-off, identity rule, or
phase owner. Keep accepted records focused on the active decision and remove
superseded records from the public book after transferring any still-relevant
rationale. Start from the [ClipAsm ADR template](template.md).

## Language, compilation, and phase boundaries

| ADR | Status | Decision |
| --- | --- | --- |
| [0001](0001-keep-compilation-pure.md) | `accepted` | Keep compilation pure and defer media and tool inspection to preflight. |
| [0003](0003-separate-semantic-and-execution-identities.md) | `accepted` | Separate authored semantic identity, prepared identity, and renderer compatibility. |
| [0008](0008-separate-parsing-from-canonical-source.md) | `accepted` | Keep parsing and lowering separate from crate-private canonical source. |
| [0013](0013-adopt-native-clipasm-language.md) | `accepted` | Use one native `.clipasm` language and keep syntax and sugar language-owned. |
| [0015](0015-keep-native-operations-phase-owned.md) | `accepted` | Keep native operations closed, exhaustive, and owned by their phases. |
| [0018](0018-evaluate-scalar-expressions-exactly.md) | `accepted` | Evaluate Number and Duration expressions exactly before parameter constraints. |

See the architecture's
[language and canonical-source](../architecture.md#language-and-canonical-source),
[compilation](../architecture.md#compilation), and
[ownership](../architecture.md#ownership-rules) sections for the current
responsibility map.

## Programs, source units, and stack evaluation

| ADR | Status | Decision |
| --- | --- | --- |
| [0002](0002-use-one-program-model.md) | `accepted` | Use one typed call model for every program implementation. |
| [0005](0005-treat-source-files-as-programs.md) | `accepted` | Treat a source file as a stack program and keep publication separate. |
| [0007](0007-support-ordered-program-outputs.md) | `accepted` | Give programs an ordered sequence of typed outputs. |
| [0009](0009-call-authored-source-programs.md) | `accepted` | Make imported authored source programs ordinary callable definitions with isolated invocations. |
| [0010](0010-add-typed-audio-and-body-input-scopes.md) | `accepted` | Add typed Audio, per-occurrence stack ownership, exact-type binding, and body-input aliases. |
| [0011](0011-add-type-preserving-timeline-programs.md) | `accepted` | Resolve type-preserving Video or Audio timeline programs during checking. |

For current vocabulary and behavior, see
[settled stack semantics](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTEXT.md#settled-stack-semantics)
and
[names, references, and dependencies](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTEXT.md#names-references-and-dependencies).
Use the [language reference](../language-reference.md) for authored examples.

## Media domains, execution, and rendering

| ADR | Status | Decision |
| --- | --- | --- |
| [0004](0004-quantize-source-duration-by-coverage.md) | `accepted` | Quantize source video duration to the smallest covering project-frame count. |
| [0012](0012-run-external-programs.md) | `accepted` | Run typed external programs only as trusted rendering-time executables. |
| [0014](0014-map-frame-and-sample-boundaries.md) | `accepted` | Map cumulative frame and sample boundaries exactly without a shared master tick. |
| [0016](0016-overlap-audiovisual-transitions-exactly.md) | `accepted` | Overlap crossfade picture and Audio on the same exact output boundaries. |
| [0017](0017-run-ffmpeg-recipes-through-host-adapters.md) | `accepted` | Share closed FFmpeg recipes between explicit native and browser runtime adapters. |

See the architecture's [preflight](../architecture.md#preflight) and
[rendering](../architecture.md#rendering) sections for current execution
ownership. External executables are trusted code; the complete trust boundary
is recorded in [ADR 0012](0012-run-external-programs.md). The native/browser
runtime boundary is recorded in
[ADR 0017](0017-run-ffmpeg-recipes-through-host-adapters.md).
