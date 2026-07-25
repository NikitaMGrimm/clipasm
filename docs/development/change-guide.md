# Change guide

Use this guide before implementation to identify the code, tests,
documentation, and identity versions affected by a change. It routes work to
canonical owners; it does not replace the
[architecture](../architecture.md),
[YAML frontend reference](../workflow-reference.md), root `CONTEXT.md`, or
records under `docs/adr/`.

Start with `CONTEXT.md`, then use the affected change below to identify the
relevant architecture, YAML frontend reference, ADRs, tests, and identity
versions. Follow `CONTRIBUTING.md` and run `./scripts/check.sh` before handoff.

## Source map

| Path | Responsibility |
|---|---|
| `src/source` | Canonical authored programs, entrypoints, and source-unit context |
| `src/frontend/yaml` | Restricted YAML parsing, surface invariants, and desugaring |
| `src/program` | Definitions, typed calls, lowerers, and body lifecycles |
| `src/semantic.rs` | Semantic operations and constrained graph builder |
| `src/compiler` | Checked-source construction, binding, stack evaluation, references, domains, and hashes |
| `src/format` | Explicit downstream serialized document formats |
| `src/preflight` | Assets, tools, exact typed domains, and prepared Video/Audio primitives |
| `src/render` | Cache execution, FFmpeg, verification, and publication |
| `src/model` | Invariant-protected value, time, and video types |
| `tests` | Public compiler, preflight, CLI, and render contracts |
| `examples` | Small runnable source programs |

## Change-impact matrix

| Change | Primary code | Required review |
|---|---|---|
| Change canonical source semantics | `src/source`, compiler | every frontend, source-body contracts, [architecture](../architecture.md), semantic identity |
| Change source-package linking or unit identity | `src/source`, `src/compiler/link.rs`, `src/compiler/check.rs` | every frontend, root/import target validation, import cycles, storage-order independence, compiled identity |
| Change checked-source construction | `src/compiler/draft.rs`, `src/compiler/typecheck.rs`, `src/compiler/check.rs`, `src/compiler/checked.rs` | declaration dependencies, signatures, stack plans, evaluator interface, every frontend through canonical source |
| Change YAML source-program or header syntax | `src/frontend/yaml` | canonical lowering, examples, [YAML frontend reference](../workflow-reference.md), [ADR 0005](../adr/0005-treat-source-files-as-programs.md) |
| Change inline input syntax or evaluation | frontend, `src/compiler/typecheck.rs`, `src/compiler/check.rs`, `src/compiler/evaluate.rs` | descriptor-order resolution, isolated-stack tests, requested-frame inheritance, global IDs and dependencies |
| Add a direct program | `src/program/builtins/direct.rs`, registry | normalization, binding, domains, [YAML frontend reference](../workflow-reference.md), semantic version |
| Add a body program | `src/program/builtins/body.rs`, registry | body syntax, stack contract, finalizer tests, [YAML frontend reference](../workflow-reference.md) |
| Change an existing program | its definition and lowerer/finalizer | semantic version, domain tests, [YAML frontend reference](../workflow-reference.md), hashes |
| Change YAML syntax | `src/frontend/yaml` | canonical-source tests, compiler contracts, examples, [YAML frontend reference](../workflow-reference.md) |
| Add a frontend | `src/frontend`, `src/source` | canonical equivalence tests, source locations, relative paths, CLI selection |
| Change compiled JSON | `src/format/json.rs` | format version, compiler contracts, [ADR 0003](../adr/0003-separate-semantic-and-execution-identities.md) |
| Change call or stack binding | `src/compiler/typecheck.rs`, `src/compiler/stack.rs`, `src/compiler/entrypoint.rs` | checked argument plans, root bindings, direct/body contracts, variadics, stack diagnostics |
| Add a semantic operation | `src/semantic.rs` | domain inference, fingerprinting, preflight lowering, prepared tests |
| Change semantic identity | affected program or graph operation | [ADR 0003](../adr/0003-separate-semantic-and-execution-identities.md), semantic version, compiled/prepared/cache versions |
| Change preflight behavior | `src/preflight` | [pure-compile boundary](../adr/0001-keep-compilation-pure.md), capability tests, prepared-plan identity |
| Change prepared primitive or rendering | `src/preflight/mod.rs`, `src/preflight/lower.rs`, `src/preflight/identity.rs`, `src/render` | Video/Audio variant invariants, serialized prepared shape, semantic/cache identity, FFmpeg requirements, render integration |
| Change CLI behavior | `src/cli.rs`, library boundary | CLI tests, `README.md`, Rustdoc |
| Change Rust public API | `src/lib.rs`, exported types | Rustdoc, doctests, compatibility implications |
| Change terminology | `CONTEXT.md` first | [YAML frontend reference](../workflow-reference.md), architecture, diagnostics, code names |
| Add a tool or dependency | `Cargo.toml`, CI | README requirements, execution identity where relevant |
| Change repository layout | affected files | this guide, `AGENTS.md` links, mdBook summary |

## Final review checklist

Before handoff, ask:

- Did public YAML behavior change?
- Did source-program result or entrypoint publication behavior change?
- Did a domain term or settled semantic rule change?
- Did a phase boundary or durable decision change?
- Does a program semantic version need incrementing?
- Do compiled, prepared, or cache format versions need review?
- Did a new semantic operation update every exhaustive owner?
- Did requirements, CI, examples, Rustdoc, or the YAML frontend reference change?
- Does repository navigation still point to the correct canonical owner?
- Did `./scripts/check.sh` pass?
