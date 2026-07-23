# Domain docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

- **`CONTEXT.md`** at the repo root, or
- **`CONTEXT-MAP.md`** at the repo root if it exists—it points at one `CONTEXT.md` per context. Read each one relevant to the topic.
- **`docs/adr/`**—read ADRs that touch the area being changed.

If any of these files do not exist, proceed silently. The producer workflow creates them lazily when terms or decisions are resolved.

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
