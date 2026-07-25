# Contributing

Read [CONTEXT.md](CONTEXT.md) before changing terminology or settled semantics,
[docs/architecture.md](docs/architecture.md) before moving responsibilities
between phases, and relevant records under [docs/adr](docs/adr) before
revisiting a durable decision.

Before implementing a change, use the
[change guide](docs/development/change-guide.md) to identify affected code,
tests, documentation, and identity versions.

## Setup

Install Rust 1.95 or newer. Full verification also
requires FFmpeg, FFprobe, and mdBook on `PATH`.

## Contribution workflow

Use Conventional Commits for commit messages, for example `feat: add trim program`.

1. Inspect the existing worktree and preserve unrelated changes.
2. Identify the canonical document and affected phases through the change
   guide.
3. Implement the smallest coherent change without bypassing phase boundaries.
4. Add or update tests at the public or internal interface that owns the
   behavior.
5. Update the canonical documentation owner when behavior or terminology
   changes; link to it elsewhere instead of duplicating it.
6. Run the complete repository check before handoff:

```console
./scripts/check.sh
```

## Releases

A release tag must exactly match `v` followed by the version in `Cargo.toml`. The
tag workflow reruns the full repository check, dependency policy, and Cargo
publish dry run before building native Linux x64, macOS arm64, and Windows x64
archives. Each archive includes a SHA-256 checksum. The GitHub release is created
only after every build succeeds. Publishing to crates.io remains a separate
manual decision.

## Examples and fixtures

Keep committed examples small, readable, and representative of the public language.
Update `docs/examples.md` when adding or changing a committed source program.

Prefer deterministic text fixtures such as PPM images. Keep generated media,
render outputs, manifests, and caches untracked. Use `local/` for personal media
experiments.

Rendering tests may skip when FFmpeg or FFprobe is unavailable, but contributors
with those tools installed should run the complete check before handoff.
