# Change guide

Use this guide to find the canonical owner of a change. Start with `CONTEXT.md`,
then review the relevant architecture, language reference, ADRs, tests, and
identity versions. Run `./scripts/check.sh` before handoff.

## Source map

| Path | Responsibility |
| --- | --- |
| `src/language` | Native lexer, parser, loader, lowering, and sugar |
| `src/source` | Lowered authored packages and source locations |
| `src/program` | Program descriptors, typed calls, and implementations |
| `src/compiler` | Linking, checked source, type inference, stack evaluation, references, domains, and hashes |
| `src/semantic` | Typed semantic operations and graph construction |
| `src/format` | Serialized compiled and prepared formats |
| `src/preflight` | Assets, tools, exact media domains, and prepared primitives |
| `src/render` | Cache execution, FFmpeg, verification, and publication |
| `src/model` | Invariant-protected value, time, audio, and video types |
| `tests` | Public compiler, preflight, CLI, and render contracts |
| `examples` | Small runnable `.clipasm` programs |

`src/frontend/yaml` is temporary migration scaffolding. Do not add features to
it; migrate remaining coverage and delete it.

## Change-impact matrix

| Change | Primary code | Required review |
| --- | --- | --- |
| Change `.clipasm` grammar | `src/language/lexer.rs`, `parser.rs`, `syntax.rs` | lowering, diagnostics, language reference, examples, parser tests |
| Change lowering or sugar | `src/language/lower.rs`, `sugar.rs` | surface provenance, canonical source, semantic equivalence tests |
| Change package loading | `src/language/loader.rs`, `src/source` | path bases, deduplication, cycles, aliases, external manifests, imported-program tests |
| Change canonical source | `src/source`, compiler | draft/checked IR, every traversal, semantic identity |
| Change checked-source construction | `src/compiler/draft.rs`, `typecheck.rs`, `check.rs`, `checked.rs` | dependencies, signatures, stack plans, evaluator interface |
| Add or change a direct program | matching `src/program/builtins` module | descriptor order, semantic version, domains, prepared lowering, rendering, identities, language reference |
| Add or change a body program | `src/program/builtins/body.rs` | body contract, access default, finalizer, lexical aliases, tests, language reference |
| Change call or stack binding | `src/program/call.rs`, compiler stack/typecheck/evaluate | descriptor slots, cardinality, root and authored calls, diagnostics |
| Add a semantic operation | `src/semantic`, compiler domain, preflight, render | exhaustive dispatch, canonical inputs, serialized formats, identities |
| Change semantic identity | affected program or operation | semantic version, compiled/prepared/cache versions, ADR 0003 |
| Change preflight behavior | `src/preflight` | pure-compile boundary, capability tests, prepared identity |
| Change media formats or frame/sample mapping | `src/model`, preflight, render | ADR 0013, exact-domain tests, serialized shape, cache identity |
| Change CLI behavior | `src/cli.rs` | CLI tests, README, examples, Rustdoc |
| Change Rust public API | `src/lib.rs`, exported types | Rustdoc, doctests, compatibility implications |
| Change terminology | `CONTEXT.md` first | language reference, architecture, diagnostics, code names |
| Add a dependency | `Cargo.toml`, CI | requirements and execution identity where relevant |

## Final review

- Does the language reference still describe the parser exactly?
- Are authored names, spans, and sugar diagnostics preserved?
- Did program signatures or stack behavior change?
- Does a semantic or format version need incrementing?
- Did every exhaustive semantic/prepared/render owner change together?
- Are examples, Rustdoc, and CLI help current?
- Did `./scripts/check.sh` pass?
