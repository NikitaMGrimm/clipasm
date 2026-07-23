# Contributing

Read [CONTEXT.md](CONTEXT.md) before changing language semantics,
[docs/architecture.md](docs/architecture.md) before moving responsibilities
between phases, and the relevant records under [docs/adr](docs/adr) before
revisiting an architectural decision.

## Setup

Install a Rust toolchain compatible with edition 2024. Rendering tests also
require FFmpeg and FFprobe on `PATH`.

```console
cargo build
cargo test --all-targets
```

## Quality gate

Run before submitting changes:

```console
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo test --doc
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
mdbook build
```

Render integration tests skip when FFmpeg or FFprobe is unavailable.

## Documentation ownership

| Subject | Canonical owner |
|---|---|
| Rust API behavior | Rustdoc in `src/` |
| YAML language behavior | `docs/workflow-reference.md` |
| Architecture | `docs/architecture.md` |
| Durable trade-offs | `docs/adr/` |
| Domain terminology | `CONTEXT.md` |
| Agent navigation | `AGENTS.md` and `docs/agents/` |

Update the owning document when behavior changes. Link to it elsewhere instead
of adding a second explanation.

## Repository map

| Path | Responsibility |
|---|---|
| `src/language.rs` | built-in registry assembly and grammar-name invariants |
| `src/syntax` | restricted YAML parsing, source spans, and normalization |
| `src/program` | program descriptors, typed resolved calls, and body lifecycles |
| `src/semantic.rs` | semantic operations and constrained graph construction |
| `src/compiler` | call binding, stack evaluation, references, domains, and hashing |
| `src/preflight.rs` | assets, tools, duration resolution, prepared primitive plan |
| `src/render` | cache execution, FFmpeg commands, output publication |
| `src/model` | invariant-protected value, time, and video types |
| `tests` | public compiler, preflight, rendering, and language contracts |

## Adding a program

1. Add a static `ProgramDefinition` in
   `src/program/builtins/direct.rs` or `src/program/builtins/body.rs`.
2. Declare its inputs, typed parameters, primary parameter, output, semantic
   version, and optional postfix syntax.
3. Implement a direct lowerer or a body preparer and finalizer using the
   constrained `GraphBuilder`.
4. Register the definition in `src/program/builtins/mod.rs`.
5. Add tests for normalization, parameter binding, stack consumption, output
   type, domains, and semantic-version propagation where relevant.
6. Update [docs/workflow-reference.md](docs/workflow-reference.md).

Do not add parser or evaluator branches for program names. Add preflight or
render tests only when the program requires a new backend contract.

## Adding a semantic operation

Prefer lowering a program through existing operations. Add a new
`SemanticNodeKind` only when the semantic graph cannot faithfully express the
program otherwise. A new operation requires exhaustive updates to domain
inference, fingerprints, preflight lowering, and their tests; it does not imply
a new renderer primitive.

## Versioning identities

Increase a program's semantic version when the same authored invocation would
produce different graph semantics.

Increase the compiled or prepared format version when the corresponding
canonical identity changes incompatibly. Increase the cache format version
when existing artifacts are no longer safe to reuse.

Package versions are metadata, not semantic or cache identities.

## Compiler invariants

1. One invocation has one output.
2. Every direct or body program has one external output.
3. Explicit inputs read names; implicit inputs consume local stack occurrences.
4. References are immutable and never destructively consumed.
5. List order matters; mapping order does not.
6. `then` starts with one preceding Video.
7. `during` starts with the selected range.
8. `join` starts with the previous two Videos.
9. `timeline` finalizes with ordered concatenation.
10. Named clip bodies do not receive timeline finalization.
11. There is no hidden replacement behavior.
12. Graph structure is known after pure compilation; exact duration is known after preflight.
13. Programs receive typed resolved calls and lower through trusted builder
    operations.
14. Pure compilation performs no asset or external-tool I/O.
15. Preflight hashes reachable assets and owns renderer-only policy.
16. Output and manifest publication use temporary siblings and atomic replacement.

Use `local/` for untracked media experiments. Keep committed examples small
and reproducible.
