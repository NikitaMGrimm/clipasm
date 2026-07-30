# Contributing

Use the [language reference](docs/reference/language/index.md) for public syntax
and behavior. Use [architecture](docs/architecture.md) for phase
responsibilities. Use the [change guide](docs/development/change-guide.md) to
find affected code, tests, documentation, and identity versions.

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

When you add or change a built-in diagnostic, update its typed catalog entry and
production construction sites together. Define its title, category, retry
guidance, and explanation. Add focused coverage and regenerate the diagnostic
reference.

The [documentation maintenance guide](docs/development/documentation.md)
contains the required generator commands and consistency review.

Use Conventional Commits, for example `feat: add trim program`. Pull request
titles must use the same format so squash merges preserve a semantic history.

## Tests

Run focused tests while you edit. Prefer integration tests under `tests/` for
observable behavior. Use unit tests for private invariants. Follow nearby test
style instead of adding a different pattern.

Before review or handoff, run:

```console
./scripts/check.sh
```

Report the exact limitation when a required check cannot run.

## Rust

- Prefer correctness and clarity over compactness.
- Prefer narrow visibility and exhaustive matches.
- Handle fallible user input and runtime operations explicitly.
- Reserve `expect`, `unreachable!`, and assertions for established internal invariants.
- Avoid unchecked `unwrap()` and unsafe code on paths that can fail at runtime.
- Document every unsafe block with a `SAFETY` comment.
- Explain non-obvious reasons and invariants.
- Do not narrate visible code.
- Verify that Clippy findings are new before you treat them as pre-existing.
- Fix Clippy findings when practical.
- For justified exceptions, prefer `#[expect(..., reason = "...")]` over
  `#[allow(...)]`.
- Avoid speculative traits, context objects, and generic machinery without shown reuse.
- Prefer an existing abstraction over a parallel implementation.
- Fix the owning abstraction instead of adding a local workaround.

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
- you considered semantic, format, protocol, and cache versions where relevant
- generated files and lockfile changes are intentional
- `./scripts/check.sh` passes or you report the exact limitation
- you disclosed material AI assistance under `AI_POLICY.md`

## Releases

Prepare the version from a new branch based on a clean, synchronized `main`
branch.

1. Switch to `main`:

   ```console
   git switch main
   ```

2. Update `main` with a fast-forward:

   ```console,ignore
   git pull --ff-only
   ```

3. Create the release branch:

   ```console
   git switch -c feat/release-X-Y-Z
   ```

4. Prepare the release:

   ```console
   python scripts/prepare_release.py X.Y.Z
   ```

5. Check the repository:

   ```console
   ./scripts/check.sh
   ```

6. Commit the release:

   ```console,ignore
   git commit -am "release: X.Y.Z"
   ```

7. Push the release branch:

   ```console,ignore
   git push -u origin feat/release-X-Y-Z
   ```

The preparation script updates the root package, the playground package, and the
lockfile as one checked operation. It never commits, tags, pushes, or publishes.

8. Open a pull request titled `release: X.Y.Z`.
9. Merge the pull request.
10. Switch to `main`:

    ```console
    git switch main
    ```

11. Update `main` with a fast-forward:

    ```console,ignore
    git pull --ff-only
    ```

12. Wait for all `main` CI checks to succeed.
13. Verify that the tag matches `v` followed by the version in `Cargo.toml`:

    ```console
    python scripts/package_release.py verify --tag vX.Y.Z
    ```

14. Create the annotated tag:

    ```console,ignore
    git tag -a vX.Y.Z -m "ClipAsm X.Y.Z"
    ```

15. Push the tag:

    ```console,ignore
    git push origin vX.Y.Z
    ```

The tag starts the Release workflow. Before crates.io publication, the workflow
repeats repository verification. It checks the default and dependency-light
Rust API surfaces against crates.io. It runs the portable Rust suite on Linux,
macOS, and Windows. It then builds the native archives.

The workflow uses crates.io trusted publishing. It creates the GitHub release
only after every required job succeeds.

Do not move or reuse a pushed release tag. After an aborted tagged release, use
a new patch version.

## Examples and fixtures

Keep examples small, readable, and runnable. Prefer deterministic text fixtures
such as PPM images. Keep generated media, render outputs, manifests, and caches
untracked. Use `local/` for personal experiments.
