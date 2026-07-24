# Working in ClipAsm

This is the mandatory entry point for context-free agents. Follow canonical
documents rather than reconstructing decisions from code or prior conversation.

## Start here

1. Read `CONTEXT.md`.
2. Use `docs/development/change-guide.md` to scope the task.
3. Read the architecture, YAML frontend reference, and ADRs routed by that guide.
4. Follow `CONTRIBUTING.md`.

## Guardrails

- Preserve unrelated worktree changes.
- Treat handoffs as working notes and verify consequential claims.
- Do not redesign accepted ADR decisions silently.
- Keep compilation media-pure.
- Do not branch on registered program names in parser or evaluator logic.
- Run `./scripts/check.sh` before handoff.

## Agent operations

- Local issue workflow: `docs/agents/issue-tracker.md`
- Triage roles: `docs/agents/triage-labels.md`
- Domain-document conventions: `docs/agents/domain.md`
