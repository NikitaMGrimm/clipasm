# Contributing

Choose the canonical owner of a change before editing. Read
[CONTEXT.md](CONTEXT.md) before changing terminology or settled authoring
semantics, [the language reference](docs/language-reference.md) before changing
public syntax or language behavior, [the architecture](docs/architecture.md)
before moving responsibilities between phases, and the relevant
[architecture decision records](docs/adr/index.md) before revisiting a durable
decision.

Before implementing a change, use the
[change guide](docs/development/change-guide.md) to identify affected code,
tests, documentation, and identity versions.

Contributions may use AI assistance under the
[AI contribution policy](AI_POLICY.md). A human contributor remains accountable
for the submitted work.

## Setup

Install Rust 1.95 or newer. Full verification also requires FFmpeg, FFprobe,
mdBook, and Node.js on `PATH`.

## Contribution workflow

Use Conventional Commits for commit messages, for example `feat: add trim program`.

1. Inspect the existing worktree and preserve unrelated changes.
2. Identify the canonical document and affected phases through the change
   guide.
3. Implement the smallest coherent change without bypassing phase boundaries.
4. Add or update tests at the public or internal interface that owns the
   behavior.
5. Update the canonical documentation owner when behavior or terminology
   changes. Update affected examples and reader-oriented documentation, but link
   to the canonical owner instead of duplicating semantic definitions.
6. Run the complete repository check before handoff:

```console
./scripts/check.sh
```

## Documentation contributions

Read the
[documentation maintenance guide](docs/development/documentation.md) before
adding or substantially changing documentation. Identify the intended audience
and page type, verify every example and command the environment permits, and
keep relative links and book navigation current. A tutorial or guide may
explain canonical behavior for its reader, but it must not become a competing
language reference, architecture description, or decision record.

When behavior changes, update its canonical owner in the same contribution.
When terminology changes, update `CONTEXT.md` first and review the language
reference, architecture, diagnostics, examples, and other affected prose.

## Review-ready contributions

Before requesting review, confirm that:

- the change has a clear scope and excludes unrelated work
- tests cover changed behavior at the interface that owns it
- canonical documentation, examples, and commands agree with the change
- new or moved public pages are reachable from `docs/SUMMARY.md`
- local links are valid and no public page was unintentionally orphaned
- relevant ADR implications and identity-version changes were considered
- `./scripts/check.sh` passes, or the exact limitation and remaining checks are
  reported
- material AI assistance is disclosed as described in
  [AI_POLICY.md](AI_POLICY.md)

## Releases

A release tag must exactly match `v` followed by the version in `Cargo.toml`. The
tag workflow reruns the full repository check, dependency policy, and Cargo
publish dry run before building native Linux x64, macOS arm64, and Windows x64
archives. Each archive includes a SHA-256 checksum. The GitHub release is created
only after every build succeeds. Publishing to crates.io remains a separate
manual decision.

## Examples and fixtures

Keep committed examples small, readable, and representative of the public
language. Run the documented commands that exercise a changed example and
update `docs/examples.md` when adding or changing a committed source program.

Prefer deterministic text fixtures such as PPM images. Keep generated media,
render outputs, manifests, and caches untracked. Use `local/` for personal media
experiments.

Rendering tests may skip when FFmpeg or FFprobe is unavailable, but contributors
with those tools installed should run the complete check before handoff.
