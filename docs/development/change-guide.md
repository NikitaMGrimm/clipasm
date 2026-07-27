# Change guide

Use this guide when a change affects public behavior, crosses phase boundaries,
or may change an identity contract. The language reference owns public syntax
and behavior, while architecture owns phase responsibilities. Run
`./scripts/check.sh` before handoff.

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
| `playground` | WebAssembly compilation/preparation adapter and pinned browser render runtime |
| `theme` | Book playground, virtual-file binding, worker lifecycle, preview, and presentation |
| `tests` | Public compiler, preflight, CLI, and render contracts |
| `examples` | Small runnable `.clipasm` programs |

## Change-impact matrix

| Change | Primary code | Required review |
| --- | --- | --- |
| Change `.clipasm` grammar | `src/language/lexer.rs`, `parser.rs`, `syntax.rs` | lowering, diagnostics, language reference, examples, parser tests |
| Change lowering or sugar | `src/language/lower.rs`, `sugar.rs` | surface provenance, canonical source, semantic equivalence tests |
| Change package loading | `src/language/loader.rs`, `src/source` | path bases, deduplication, cycles, aliases, external implementations, imported-program tests |
| Change canonical source | `src/source`, compiler | draft/checked IR, every traversal, semantic identity |
| Change checked-source construction | `src/compiler/draft.rs`, `typecheck.rs`, `check.rs`, `checked.rs` | dependencies, signatures, stack plans, evaluator interface |
| Add or change a direct program | matching `src/program/builtins` module | catalog reference facts, descriptor order, semantic version, domains, prepared lowering, rendering, identities, generated program reference |
| Add or change a body program | `src/program/builtins/body.rs` | catalog reference facts, body contract, access default, finalizer, lexical aliases, tests, generated program reference |
| Change call or stack binding | `src/program/call.rs`, compiler stack/typecheck/evaluate | descriptor slots, cardinality, root and authored calls, diagnostics |
| Add a semantic operation | `src/semantic`, compiler domain, preflight, render | exhaustive dispatch, canonical inputs, FFmpeg capability requirements, serialized formats, identities |
| Change semantic identity | affected program or operation | semantic version and compiled/prepared/cache versions |
| Change preflight behavior | `src/preflight` | pure-compile boundary, capability tests, prepared identity |
| Change media formats or frame/sample mapping | `src/model`, preflight, render | exact-domain tests, serialized shape, and cache identity |
| Change CLI behavior | `src/cli.rs` | CLI tests, README, examples, Rustdoc |
| Add or change a built-in diagnostic | diagnostic catalog and emitting phase | title, category, retry guidance, `explain` output, standalone diagnostic reference, and focused tests |
| Change Rust public API | `src/lib.rs`, exported types | Rustdoc, doctests, and downstream impact |
| Change a machine-readable boundary | serialization owner and `src/contracts.rs` | support level, version bump, reference page, consumers, serialization tests |
| Change browser compilation or rendering | `playground`, `src/preflight/browser.rs`, `src/render/browser.rs`, `src/render/execute`, `theme` | pure-compilation boundary, virtual paths and hashes, recipe/runtime versions, artifact contracts, work limits, licensing, WebAssembly and book builds |
| Change public terminology | `docs/reference/language/` or the built-in catalog | generated program reference, concepts, diagnostics, examples |
| Change internal terminology | `docs/architecture.md` | code names and development docs |
| Add a dependency | `Cargo.toml`, CI | requirements and execution identity where relevant |

## Final review

- Does the language reference still describe the parser exactly?
- Are authored names, spans, and sugar diagnostics preserved?
- Did program signatures or stack behavior change?
- Does a semantic or machine-contract version need incrementing?
- Did every exhaustive semantic/prepared/render owner change together?
- Are examples, Rustdoc, and CLI help current?
- Does every changed built-in diagnostic have a generated reference entry and matching `explain` output?
- Did `./scripts/check.sh` pass?
