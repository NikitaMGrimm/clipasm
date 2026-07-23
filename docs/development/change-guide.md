# Change guide

Use this guide before implementation to identify the code, tests,
documentation, and identity versions affected by a change. It routes work to
canonical owners; it does not replace the
[architecture](../architecture.md), [workflow reference](../workflow-reference.md),
root `CONTEXT.md`, or records under `docs/adr/`.

Start with `CONTEXT.md`, then use the affected change below to identify the
relevant architecture, workflow reference, ADRs, tests, and identity versions.
Follow `CONTRIBUTING.md` and run `./scripts/check.sh` before handoff.

## Source map

| Path | Responsibility |
|---|---|
| `src/language.rs` | Registry assembly and grammar-name invariants |
| `src/syntax` | Restricted YAML, spans, and normalization |
| `src/program` | Definitions, typed calls, lowerers, and body lifecycles |
| `src/semantic.rs` | Semantic operations and constrained graph builder |
| `src/compiler` | Binding, stack evaluation, references, domains, and hashes |
| `src/preflight.rs` | Assets, tools, exact domains, and prepared primitives |
| `src/render` | Cache execution, FFmpeg, verification, and publication |
| `src/model` | Invariant-protected value, time, and video types |
| `tests` | Public compiler, preflight, CLI, and render contracts |
| `examples` | Small runnable workflows |

## Change-impact matrix

| Change | Primary code | Required review |
|---|---|---|
| Add a direct program | `src/program/builtins/direct.rs`, registry | normalization, binding, domains, [workflow reference](../workflow-reference.md), semantic version |
| Add a body program | `src/program/builtins/body.rs`, registry | body syntax, stack contract, finalizer tests, [workflow reference](../workflow-reference.md) |
| Change an existing program | its definition and lowerer/finalizer | semantic version, domain tests, [workflow reference](../workflow-reference.md), hashes |
| Change YAML syntax | `src/syntax`, language metadata | compiler contracts, examples, [workflow reference](../workflow-reference.md), compiled-format identity |
| Change call or stack binding | `src/compiler/bind.rs` | direct/body contracts, explicit inputs, variadics, stack diagnostics |
| Add a semantic operation | `src/semantic.rs` | domain inference, fingerprinting, preflight lowering, prepared tests |
| Change semantic identity | affected program or graph operation | [ADR 0003](../adr/0003-separate-semantic-and-execution-identities.md), semantic version, compiled/prepared/cache versions |
| Change preflight behavior | `src/preflight.rs` | [pure-compile boundary](../adr/0001-keep-compilation-pure.md), capability tests, prepared-plan identity |
| Change prepared primitive or rendering | preflight, `src/render` | cache format, FFmpeg requirements, render integration |
| Change CLI behavior | `src/cli.rs`, library boundary | CLI tests, `README.md`, Rustdoc |
| Change Rust public API | `src/lib.rs`, exported types | Rustdoc, doctests, compatibility implications |
| Change terminology | `CONTEXT.md` first | [workflow reference](../workflow-reference.md), architecture, diagnostics, code names |
| Add a tool or dependency | `Cargo.toml`, CI | README requirements, execution identity where relevant |
| Change repository layout | affected files | this guide, `AGENTS.md` links, mdBook summary |

## Final review checklist

Before handoff, ask:

- Did public YAML behavior change?
- Did a domain term or settled semantic rule change?
- Did a phase boundary or durable decision change?
- Does a program semantic version need incrementing?
- Do compiled, prepared, or cache format versions need review?
- Did a new semantic operation update every exhaustive owner?
- Did requirements, CI, examples, Rustdoc, or the workflow reference change?
- Does repository navigation still point to the correct canonical owner?
- Did `./scripts/check.sh` pass?
