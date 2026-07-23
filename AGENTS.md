# Working in ClipAsm

This file is the mandatory routing entry point for implementation work. Follow
the linked source of truth instead of reconstructing project decisions from
code or prior conversation.

## Read first

1. `CONTEXT.md` for canonical domain and language terminology.
2. Relevant records under `docs/adr/` before revisiting durable decisions.
3. `docs/architecture.md` for phase and module ownership.
4. `docs/workflow-reference.md` for public YAML behavior.
5. `CONTRIBUTING.md` for implementation and verification rules.
6. `docs/agents/repository-guide.md` for detailed source navigation.

## Task routing

| Task | Start here |
|---|---|
| Change YAML syntax or normalization | `src/syntax` |
| Add a direct program | `src/program/builtins/direct.rs` |
| Add a body program | `src/program/builtins/body.rs` |
| Change call or stack binding | `src/compiler/bind.rs` |
| Change program execution | `src/compiler/evaluate.rs` |
| Add a semantic operation | `src/semantic.rs`, then resolution, hashing, and preflight |
| Change media or tool discovery | `src/preflight.rs` |
| Change caching or FFmpeg execution | `src/render` |
| Change public terminology | `CONTEXT.md` |
| Revisit an architectural decision | Relevant ADR first |

## Architectural guardrails

- Compilation is pure.
- Every callable construct is a registered program.
- Parser and evaluator control flow do not branch on program names.
- Direct and body programs share typed call binding.
- Programs lower only through the constrained semantic graph builder.
- Preflight owns assets, tools, and media-derived facts.
- Rendering consumes prepared primitives.
- Mapping order is never executable order.
- Preserve unrelated dirty worktree changes.

## Quality gate

Run every command before handing off implementation work:

```console
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo test --doc
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
mdbook build
git diff --check
```

## Repository-specific agent practices

- Issues and PRDs use local Markdown under `.scratch/`; see
  `docs/agents/issue-tracker.md`.
- Triage uses the five canonical roles mapped in
  `docs/agents/triage-labels.md`.
- Domain-document discovery and ADR-conflict rules live in
  `docs/agents/domain.md`.
- Treat handoffs as working notes and verify consequential claims against code,
  tests, canonical documents, and accepted ADRs.
- Preserve unrelated existing changes; do not stage, commit, publish, or verify
  them as part of a narrower task.
