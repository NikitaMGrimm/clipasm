---
status: accepted
date: 2026-07-26
---

# 0019: Model rooted timeline layouts separately from media values

## Context and problem statement

Authored placements and transition regions must remain addressable even when
multiple stack occurrences share one immutable media value. Some boundaries are
known during pure compilation, while others depend on media domains that only
preflight may inspect. Marker names must not change rendered identity merely by
existing, and parser or evaluator behavior must not branch on registered program
names.

## Decision drivers

- Preserve pure compilation and exact native frame or sample boundaries.
- Keep authored occurrence identity distinct from semantic media identity.
- Make coordinate propagation an explicit program contract.
- Reject ambiguous or unrelated roots instead of searching provenance.
- Keep prepared plans and rendering fully concrete.

## Considered options

- Store markers directly on semantic media nodes. This collapses repeated
  occurrences and one-input compositions that intentionally share a value.
- Recover placements by searching value provenance. This makes repeated values
  ambiguous and gives undeclared operations accidental coordinate semantics.
- Preserve every composition operation as a selector-tree node. This makes
  redundant one-input concat, stack-block placement, and associative regrouping
  change otherwise equivalent selector paths.
- Maintain compiler-owned rooted timeline layouts and require programs to
  declare their mapping behavior, then normalize unnamed composition.

## Decision outcome

The compiler maintains a timeline-view sidecar for each evaluated occurrence.
A view owns an exact symbolic extent and one canonical ordered child sequence.
Its named selector index is derived centrally from those children. Selectors
resolve to coordinates or closed-open ranges rooted in one view.
Timeline coordinates are canonical linear expressions in exact seconds. Terms
may reference semantic Video or Audio extents and are scaled by seconds per
native unit: frames for Video and samples for Audio. Preflight resolves those
native domains before a marker range is consumed.

Media `ValueRef` identity remains unchanged. Repeated occurrences can therefore
share media while retaining distinct timeline views and placement names.

Direct and body program definitions declare `TimelineBehavior`. Identity
mappings copy the input layout, direct concat and body-concat mappings build
cumulative child placements from evaluated occurrences, and media-neutral crop
mappings rebase fully contained placements into the selected range, replacement
mappings keep provably unaffected base placements and insert the body output as
a nested `replacement` region, and transition mappings define operation-owned
regions. `crossfade` exposes `before`, `after`, and `overlap`; `flash_cut`
exposes sequential `before` and `after`. A placement that only partially
survives a crop or replacement, or whose relation cannot be proven from the
normalized symbolic expressions, is omitted rather than represented
inaccurately. Programs with fresh behavior, including external programs, do not inherit
layouts. The repeat mapping preserves the complete input layout when
`repeat(1)` lowers to the exact input value. Larger counts create a fresh root
without unindexed child placements, while deriving its extent exactly as the
input extent multiplied by the count. No parser or evaluator branch recognizes
a registered program name.

Composition treats an unnamed occurrence with children as transparent and
splices those children into the parent at the occurrence offset. A named
occurrence remains one selector boundary. This gives anonymous concat an
identity law and associative regrouping law: adding a one-input concat, moving
an equivalent stack block, or changing anonymous concat grouping does not alter
selector paths. The result still receives a distinct root view, so composition
does not retroactively merge the roots of original named values.

The selector index maps each immediate spelling to every occurrence with that
spelling. A selector step is valid only when the index contains exactly one
occurrence. Explicit output labels, inferred bare-reference labels, and
operation-created labels never shadow one another; adding a duplicate can make
a selector ambiguous but cannot silently redirect it. In a direct
timeline-consuming call, selector shorthand may match a unique descendant
suffix anywhere below a bound timeline. The compiler computes a capped
zero/one/multiple result with dynamic programming over the timeline-view DAG;
shared sublayouts are not expanded into their potentially exponential occurrence
tree. Multiple matching occurrences report `E_AMBIGUOUS_TIMELINE_PLACEMENT`.
Explicitly rooted selectors remain exact paths, and aliases never borrow
invocation-local context.

Operation-owned spellings that are part of a result contract are reserved when
that operation merges existing sibling placements. `during` therefore rejects
a surviving base placement named `replacement` with
`E_TIMELINE_PLACEMENT_CONFLICT`; a placement with that name is allowed when the
selected range removes it. This preserves the promised `::replacement` region
without introducing label precedence.

Diagnostics format the canonical child sequence rather than raw operation
history. They therefore show genuine unnamed leaves, ambiguous labels, exact
root-relative ranges, and named nesting, but omit transparent anonymous
composition wrappers. These trees are bounded before being attached as
diagnostic notes.

Generic timeline programs use one typed semantic operation per language
concept: `Repeat`, `Concat`, `Slice`, and `ReplaceRange` do not fork into Audio
copies. Concrete slice and replacement ranges carry an invariant-checked native
frame or sample range. Media-dependent Video or Audio trim and during ranges,
requested Video body extents, inherited image lengths, and replacements remain
symbolic semantic operations until preflight substitutes exact media domains.
Preflight is the sole phase that dispatches the generic graph into existing
media-specific prepared operations and validates alignment, ordering, and bounds.

Marker names and unused scalar aliases do not enter rendered semantic identity.
When a symbolic range is consumed, its normalized expression and referenced
upstream semantic hashes do enter the consuming operation's identity.

## Consequences

- Repeated media values and nested compositions retain deterministic placement
  identity without duplicating media nodes or forcing contextual lookup to
  expand shared occurrence trees.
- Equivalent anonymous composition rewrites retain the same selector paths;
  authored names remain the only ordinary nesting boundaries.
- Exact marker arithmetic works across known and probed Video and Audio domains
  without a nanosecond round trip or a shared master-tick grid.
- New timeline-changing programs must deliberately specify their coordinate
  mapping or remain fresh.
- Layout propagation adds compiler-side state and deferred semantic variants,
  but no symbolic behavior reaches the renderer.
- A one-count repeat is a true identity; larger repeats remain fresh until an
  explicit occurrence-indexing design can represent their child placements.
- Future mappings must be introduced explicitly rather than inferred from equal
  duration.

## Confirmation

Semantic integration tests cover explicit, implicit, nested, contextual,
transition, midpoint, root-mismatch, anonymous identity and associativity,
cross-program composition, strict placement uniqueness, ambiguity diagnostics,
Audio composition and crop propagation, selected-layout preservation inside
replacement bodies, shared-layout DAG lookup, canonical replacement ordering,
and reserved operation-name collisions.
Preflight contract tests cover deferred Video and Audio trim and during, inherited
image extents, replacement splicing, transition regions, native-grid alignment,
and bounds. `TimelineBehavior` owns program mapping declarations;
semantic-operation dependency traversal and fingerprinting exhaustively own
deferred expressions. The compiled format version changes whenever serialized
deferred operations change. `./scripts/check.sh` validates all native, browser,
documentation, and rendering owners.

## Related decisions

- Related to ADR 0001, pure compilation.
- Related to ADR 0003, semantic and execution identity.
- Related to ADR 0014, exact frame and sample boundaries.
- Related to ADR 0016, exact overlap transitions.
- Related to ADR 0018, exact scalar expressions.
