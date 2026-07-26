# Documentation work

Use this guide for documentation-only work and documentation reviews. Start
with `AGENTS.md`, then read `CONTEXT.md`,
`docs/development/change-guide.md`, and `CONTRIBUTING.md`. Preserve unrelated
worktree changes and follow any narrower scope the user assigns.

## Route every claim to its owner

| Claim | Canonical owner |
| --- | --- |
| Domain language and settled authoring semantics | `CONTEXT.md` |
| Current public syntax and language behavior | `docs/language-reference.md` |
| Phase responsibilities and internal architecture | `docs/architecture.md` |
| Durable decisions, trade-offs, and statuses | `docs/adr/` |
| Change ownership and affected surfaces | `docs/development/change-guide.md` |
| Runnable committed source programs | `examples/` |
| Human contribution workflow | `CONTRIBUTING.md` |
| Human accountability for AI-assisted contributions | `AI_POLICY.md` |
| Repository operating instructions for agents | `AGENTS.md` |

Tutorials, guides, landing pages, and explanations may summarize canonical
material for a reader. Link to the owner for exact detail instead of creating a
competing definition. ADRs describe active durable decisions; Git preserves
superseded history. ADRs do not override the current language reference.

## Give each page one primary job

- **Tutorial:** lead a learner through a successful experience in a deliberate
  order.
- **How-to guide:** help a reader with some context complete one concrete task.
- **Reference:** state exact behavior for lookup.
- **Explanation:** build a mental model or discuss relationships and trade-offs.
- **Development documentation:** help contributors change and verify the
  project safely.

Use the [documentation maintenance guide](../development/documentation.md) for
editorial and maintenance details.

## Stay inside a documentation-only boundary

When a task excludes implementation review:

- do not inspect or modify `src/`, tests, or implementation behavior
- do not infer language behavior, output, errors, or guarantees from incidental
  code structure
- do not use implementation code to settle a contradiction between canonical
  documents
- do not silently redesign an accepted ADR
- do not invent features, compatibility or support promises, performance
  claims, security guarantees, output, or error messages
- do not treat canonical source or compiled JSON as a public authoring format

Use the safer current canonical statement when owners disagree and report the
conflicting paths. Escalate a change to canonical behavior rather than hiding it
inside reader-oriented prose.

## Verify the reader journey

Prefer committed examples and run every documented command the environment
permits. State prerequisites and the working directory, explain what success
means, and report unavailable tools or assets without fabricating a result.

For links and navigation:

- use relative repository links
- update inbound links after a move or rename
- put every new public page in `docs/SUMMARY.md`
- check for unintentionally orphaned public pages
- keep internal agent pages reachable from an intentional agent entry point
- run `mdbook build` and inspect the affected route through the book

Use `clipasm` fences for ClipAsm source and `console` fences for terminal
sessions. Keep generated media, render output, manifests, and caches untracked.

## Hand off verifiable work

Report:

- files added, moved, and changed
- canonical sources used for consequential claims
- commands and checks run, with exact results
- commands, examples, or claims that could not be verified
- unresolved contradictions with exact paths
- risks, intentional deferrals, and required follow-up
- confirmation that assigned scope and unrelated worktree changes were
  preserved

Run `git diff --check` during editing and `./scripts/check.sh` before final
handoff. If the full check cannot run, give the exact limitation and list the
checks that did pass. Review the final status and diff for accidental code,
test, dependency, generated-file, or unrelated changes.
