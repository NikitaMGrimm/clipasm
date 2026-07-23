# Domain docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

1. **`AGENTS.md`** for mandatory task routing and project guardrails.
2. **`docs/agents/repository-guide.md`** for source and change-impact maps.
3. **`CONTEXT.md`** at the repo root, or **`CONTEXT-MAP.md`** if it exists
   and points to multiple contexts.
4. Relevant records under **`docs/adr/`** before revisiting durable decisions.

If a context or ADR path does not exist, proceed silently. Producer workflows
create them lazily when terms or decisions are resolved.

## File structure

This repository uses the single-context layout:

```text
/
├── CONTEXT.md
├── docs/adr/
└── src/
```

## Use the glossary's vocabulary

When output names a domain concept, use the term defined in `CONTEXT.md`. Do not drift to synonyms the glossary explicitly avoids.

If the needed concept is absent, reconsider whether the term belongs to the project or note the genuine glossary gap.

## Flag ADR conflicts

Surface any contradiction with an existing ADR explicitly rather than silently overriding it.
