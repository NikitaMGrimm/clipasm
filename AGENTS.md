# Working in ClipAsm

- Read `CONTRIBUTING.md` before changing files.
- Use `docs/development/change-guide.md` for public, cross-phase, or identity-affecting changes.
- Read only the relevant language reference, architecture section, and ADRs.
- Preserve unrelated worktree changes.
- Treat handoffs as unverified working notes; verify consequential claims from code or tests.
- Do not silently override accepted ADRs.
- Review semantic, format, protocol, and cache identities when behavior crosses their boundaries.
- ALWAYS attempt to add or update a test for changed behavior.
- PREFER integration tests under `tests/` over unit tests when behavior is observable.
- ALWAYS read and copy the style of nearby tests before adding new cases.
- PREFER running specific tests while iterating; run `./scripts/check.sh` before handoff.
- NEVER update all dependencies in the lockfile; use `cargo update -p <package> --precise <version>`.
- Keep compilation media-pure; asset and tool inspection belongs to preflight.
- Do not branch on registered program names in parser, type-checker, or evaluator logic.
- Add agent rules only for non-obvious, repeated, actionable repository traps.
