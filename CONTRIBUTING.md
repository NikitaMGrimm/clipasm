# Contributing

Use the [language reference](docs/reference/language/index.md) for public syntax
and behavior, [architecture](docs/architecture.md) for phase responsibilities,
and [ADRs](docs/adr/index.md) before revisiting a durable decision. Use the
[change guide](docs/development/change-guide.md) to find affected code, tests,
documentation, and identity versions.

AI-assisted contributions are welcome under [AI_POLICY.md](AI_POLICY.md). The
human contributor remains accountable for the submitted work.

## Setup

Install Rust 1.95 or newer. Full verification also requires FFmpeg, FFprobe,
mdBook, and Node.js on `PATH`.

## Workflow

1. Inspect the worktree and preserve unrelated changes.
2. Find the existing owner of the behavior before editing.
3. Implement the smallest coherent root-cause fix without bypassing phase boundaries.
4. Add or update tests at the interface that owns the behavior.
5. Update the canonical documentation and affected examples in the same change.
6. Review the final diff and run the complete repository check.

When adding or changing a built-in diagnostic, update its typed catalog entry
and production construction sites together, define its title, category, retry
guidance, stability, and explanation, add focused coverage, and regenerate the
diagnostic reference. See the [documentation maintenance guide](docs/development/documentation.md)
for the required generator commands and compatibility review.

Use Conventional Commits, for example `feat: add trim program`.

## Tests

Run focused tests while iterating. Prefer integration tests under `tests/` for
observable behavior and unit tests for private invariants. Follow nearby test
style rather than introducing a parallel pattern.

Before review or handoff, run:

```console
./scripts/check.sh
```

Report the exact limitation when a required check cannot run.

## Rust

- Prefer correctness and clarity over compactness.
- Prefer narrow visibility and exhaustive matches.
- Handle user-controlled and runtime fallibility explicitly.
- Reserve `expect`, `unreachable!`, and assertions for established internal invariants.
- Avoid unchecked `unwrap()` and unsafe code on runtime-fallible paths; document every unsafe block with a `SAFETY` comment.
- Explain non-obvious reasons and invariants; do not narrate visible code.
- Verify whether Clippy findings are new before treating them as pre-existing. Fix them when practical; prefer `#[expect(..., reason = "...")]` over `#[allow(...)]` for justified exceptions.
- Avoid speculative traits, context objects, and generic machinery without demonstrated reuse.
- Prefer an existing abstraction over a parallel implementation, but fix the owning abstraction rather than adding a local workaround.

Breaking changes are acceptable before 1.0 when they make the language or
architecture substantially simpler.

## Documentation

Read the [documentation maintenance guide](docs/development/documentation.md)
before adding or substantially changing documentation. Give each page one job,
link to the canonical owner for exact rules, verify examples and commands, and
keep public pages reachable from `docs/SUMMARY.md`.

## Review

Before requesting review, confirm that:

- the change excludes unrelated work
- tests cover changed behavior
- public behavior, architecture, examples, and diagnostics agree
- semantic, format, protocol, and cache versions were considered where relevant
- generated files and lockfile changes are intentional
- `./scripts/check.sh` passes or the exact limitation is reported
- material AI assistance is disclosed under `AI_POLICY.md`

## Releases

Prepare the version from a clean, synchronized `main` branch:

```console
python scripts/prepare_release.py X.Y.Z
./scripts/check.sh
git commit -am "release: X.Y.Z"
git push origin main
```

The preparation script updates the root package, playground package, and lockfile
as one checked operation. It never commits, tags, pushes, or publishes. Wait for
all `main` CI checks to succeed before creating an annotated tag that exactly
matches `v` followed by the version in `Cargo.toml`:

```console
python scripts/package_release.py verify --tag vX.Y.Z
git tag -a vX.Y.Z -m "ClipAsm X.Y.Z"
git push origin vX.Y.Z
```

The tag starts the Release workflow. Before crates.io publication, that workflow
repeats repository verification and the portable Rust suite on Linux, macOS, and
Windows, then builds native archives. It publishes through crates.io trusted
publishing and creates the GitHub release only after every required job succeeds.
Do not move or reuse a pushed release tag; use a new patch version after an
aborted tagged release.

## Examples and fixtures

Keep examples small, readable, and runnable. Prefer deterministic text fixtures
such as PPM images. Keep generated media, render outputs, manifests, and caches
untracked; use `local/` for personal experiments.
