---
status: accepted
---

# Keep native operations closed and phase-owned

## Context

A native operation participates in several different contracts: program
binding, semantic graph structure, pure domain inference, compiled identity,
prepared lowering, prepared identity, and renderer execution. Colocating all of
those concerns behind one operation trait or dynamic registry would make the
phase boundaries less visible and could turn missing engine support into a
runtime failure.

Keeping every implementation in one large file also scales poorly. Program
definitions, preflight preparation, and FFmpeg execution have different natural
groupings and substantially different amounts of code.

## Decision

Native semantic and prepared operations remain closed enums. Every phase keeps
one exhaustive dispatch over the closed operation set, so adding a variant
requires the compiler to identify every owner that must support it.

An operation owns structural facts that all phases must agree on. Semantic
operations derive their result type and canonical dependency order from the
operation variant. Prepared operations likewise expose canonical input order.
Traversal and identity code consume those authorities instead of independently
reconstructing graph topology.

Implementation details are organized by responsibility inside each phase:

- built-in declarations are grouped into sources, Audio adaptations, timeline
  operations, effects, transitions, and body programs;
- compiler finalization and pure domain inference are separate owners;
- the prepared-plan model is separate from preflight orchestration;
- preflight retains one exhaustive dispatcher and delegates operation-specific
  work to media, timeline, effect, transition, and external modules;
- rendering retains one exhaustive dispatcher and delegates to the same broad
  families through one shared execution context.

The execution context owns FFmpeg command initialization, dependency artifact
lookup, temporary output placement, failure cleanup, and atomic cache commit.
Operation modules own only their media command and filter construction.

YAML shorthand metadata remains frontend-owned. It is not added to canonical
program definitions, because another frontend may choose different syntax.

## Consequences

- Missing support for a new native operation remains a compile-time exhaustive-
  match failure.
- Result type and dependency ordering cannot drift between traversal,
  fingerprinting, and serialization owners.
- Complex operations may receive focused modules without requiring one file per
  small program or one cross-phase feature module.
- Phase boundaries remain explicit: compiler code does not know FFmpeg policy,
  and renderer code does not own authored program binding.
- Native operations are not runtime plugins. Authored and external programs
  continue to use the runtime program catalog without weakening the closed
  engine primitive contract.
