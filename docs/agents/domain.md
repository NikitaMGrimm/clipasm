# Domain docs

Generic engineering skills should:

1. Read `AGENTS.md` for repository entry rules.
2. Use terminology from the root `CONTEXT.md`.
3. Consult relevant records under `docs/adr/`.
4. Use `docs/development/change-guide.md` for source and impact navigation.

If a context or ADR path does not exist, proceed silently. Producer workflows
create them lazily when decisions are resolved.

## Use the glossary's vocabulary

When output names a domain concept, use the term defined in `CONTEXT.md`. Do
not drift to synonyms the glossary explicitly avoids.

If the needed concept is absent, reconsider whether it belongs to the project
or note the genuine glossary gap.

## Flag ADR conflicts

Surface any contradiction with an accepted ADR rather than silently overriding
it.
