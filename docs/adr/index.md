# Architecture decision records

Architecture decision records (ADRs) describe the active durable decisions that
shape ClipAsm. Read the relevant records before changing a phase boundary,
identity, program model, authoring contract, or media-timing rule.

ADRs explain constraints and trade-offs; they are not the normative language
reference or an implementation history. Git retains superseded decisions. Use
the [language reference](../language-reference.md) for current public syntax and
behavior, and the [architecture](../architecture.md) for current phase
responsibilities and internal terminology.

Write an ADR for a durable boundary, non-obvious trade-off, identity rule, or
phase owner. Keep accepted records focused on the active decision and remove
superseded records from the public book after transferring any still-relevant
rationale. Start from the [ClipAsm ADR template](template.md).

## Language, compilation, and phase boundaries

| ADR | Status | Decision |
| --- | --- | --- |
| [0001](records.md#keep-compilation-pure) | `accepted` | Keep compilation pure and defer media and tool inspection to preflight. |
| [0003](records.md#separate-semantic-and-execution-identities) | `accepted` | Separate authored semantic identity, prepared identity, and renderer compatibility. |
| [0008](records.md#separate-parsing-from-canonical-source) | `accepted` | Keep parsing and lowering separate from crate-private canonical source. |
| [0013](records.md#adopt-the-native-clipasm-language) | `accepted` | Use one native `.clipasm` language and keep syntax and sugar language-owned. |
| [0015](records.md#keep-native-operations-closed-and-phase-owned) | `accepted` | Keep native operations closed, exhaustive, and owned by their phases. |
| [0018](records.md#evaluate-scalar-expressions-exactly) | `accepted` | Evaluate Number and Duration expressions exactly before parameter constraints. |
| [0019](records.md#0019-model-rooted-timeline-layouts-separately-from-media-values) | `accepted` | Keep rooted authored timeline layouts separate from media identity and resolve symbolic boundaries during preflight. |

See the architecture's
[language and canonical-source](../architecture.md#language-and-canonical-source),
[compilation](../architecture.md#compilation), and
[ownership](../architecture.md#ownership-rules) sections for the current
responsibility map.

## Programs, source units, and stack evaluation

| ADR | Status | Decision |
| --- | --- | --- |
| [0002](records.md#use-one-program-model) | `accepted` | Use one typed call model for every program implementation. |
| [0005](records.md#treat-source-files-as-programs) | `accepted` | Treat a source file as a stack program and keep publication separate. |
| [0007](records.md#support-ordered-program-outputs) | `accepted` | Give programs an ordered sequence of typed outputs. |
| [0009](records.md#call-authored-source-programs) | `accepted` | Make imported authored source programs ordinary callable definitions with isolated invocations. |
| [0010](records.md#add-typed-audio-and-body-input-scopes) | `accepted` | Add typed Audio, per-occurrence stack ownership, exact-type binding, and body-input aliases. |
| [0011](records.md#add-type-preserving-timeline-programs) | `accepted` | Resolve type-preserving Video or Audio timeline programs during checking. |

For current behavior, see the language reference sections on
[arguments and stack binding](../language-reference.md#arguments-and-stack-binding)
and [references and output names](../language-reference.md#references-and-output-names).

## Media domains, execution, and rendering

| ADR | Status | Decision |
| --- | --- | --- |
| [0004](records.md#quantize-source-duration-by-coverage) | `accepted` | Quantize source video duration to the smallest covering project-frame count. |
| [0012](records.md#run-registered-external-programs) | `accepted` | Run typed external programs only as trusted rendering-time executables. |
| [0014](records.md#map-frame-and-sample-boundaries-cumulatively) | `accepted` | Map cumulative frame and sample boundaries exactly without a shared master tick. |
| [0016](records.md#overlap-audiovisual-transitions-on-exact-boundaries) | `accepted` | Overlap crossfade picture and Audio on the same exact output boundaries. |
| [0017](records.md#0017-run-ffmpeg-recipes-through-host-adapters) | `accepted` | Share closed FFmpeg recipes between explicit native and browser runtime adapters. |

See the architecture's [preflight](../architecture.md#preflight) and
[rendering](../architecture.md#rendering) sections for current execution
ownership. External executables are trusted code; the complete trust boundary
is recorded in [ADR 0012](records.md#run-registered-external-programs). The native/browser
runtime boundary is recorded in
[ADR 0017](records.md#0017-run-ffmpeg-recipes-through-host-adapters).
