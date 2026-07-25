---
status: proposed
date: YYYY-MM-DD
---

# NNNN: Decision title

Use this lightweight template for a new ClipAsm architecture decision. Replace
`NNNN` with the next record number and use a descriptive kebab-case filename.
Use a verified decision date; omit the `date` field rather than inventing one.
Keep the status current, for example `proposed`, `accepted`, `rejected`, or
`superseded by ADR NNNN`.

## Context and problem statement

What durable problem or constraint requires a decision? Describe the current
phase boundary, authoring contract, identity rule, or other relevant context.
Link the canonical documentation and existing ADRs that constrain the choice.

## Decision drivers (optional)

- Which properties or constraints matter most?
- Which invariants must remain true?

Omit this section when the drivers are already clear from the context.

## Considered options (optional)

- Option A
- Option B

Record only options that were genuinely considered and the trade-offs that
distinguish them. Omit this section rather than reconstructing discussion that
did not occur.

## Decision outcome

State the decision directly. Identify the owner of each new responsibility and
how the decision affects public authoring semantics, phase boundaries, or
identity when applicable.

## Consequences

- Positive consequence
- Negative consequence or accepted trade-off
- Follow-up constraint

Include both benefits and costs. Do not present an unresolved aspiration as an
accepted consequence.

## Confirmation

How will the repository keep this decision true? Name the applicable tests,
exhaustive phase dispatch, invariant-owning type, review step, semantic version,
compiled or prepared format version, cache execution version, or documentation
owner. Use only mechanisms that actually apply.

## Related decisions and supersession (optional)

- Supersedes ADR NNNN
- Superseded by ADR NNNN
- Related to ADR NNNN

Link each listed record. When this decision supersedes an earlier one, update
the earlier record's status or relationship explicitly without rewriting its
historical decision.
