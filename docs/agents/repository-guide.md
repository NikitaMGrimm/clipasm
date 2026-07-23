# Repository guide

This page routes implementation work to the document, module, and tests that
own it. It complements the phase model in the
[architecture chapter](../architecture.md) rather than repeating it.

## Read order

1. `README.md` introduces the supported product and local commands.
2. `CONTEXT.md` owns canonical domain and language terminology.
3. [`docs/workflow-reference.md`](../workflow-reference.md) owns public YAML
   behavior and program examples.
4. [`docs/architecture.md`](../architecture.md) owns phase and module
   responsibilities.
5. [`docs/adr/`](../adr/0001-keep-compilation-pure.md) records durable
   architectural decisions and their reasons.
6. `CONTRIBUTING.md` owns implementation practices and verification.
7. `AGENTS.md` is the mandatory routing entry point for context-free agents.

## Source map

| Path | Ownership |
|---|---|
| `src/language.rs` | Built-in registry assembly and grammar-name invariants |
| `src/syntax` | Restricted YAML parsing, source spans, and normalization |
| `src/program` | Program descriptors, typed calls, direct lowering, and body lifecycles |
| `src/semantic.rs` | Closed semantic operations and constrained graph construction |
| `src/compiler` | Call binding, stack evaluation, references, domains, and semantic hashes |
| `src/preflight.rs` | Assets, tools, media-derived facts, and prepared primitives |
| `src/render` | Cache execution, FFmpeg commands, verification, and publication |
| `src/model` | Exact frames, video properties, IDs, and value types |
| `tests` | Public language, compiler, preflight, CLI, and rendering contracts |
| `examples` | Small runnable language demonstrations and local test assets |

## Change impact map

### Add a direct program

Review the definition and lowerer in `src/program/builtins/direct.rs`, registry
assembly, typed binding tests, semantic lowering and domains, and the workflow
reference. Add backend tests only if the program requires a new preparation or
rendering contract.

### Add a body program

Review the definition, body preparation, finalizer, normalization form, local
stack tests, semantic lowering, and workflow reference. The evaluator should
continue to execute the generic prepare → evaluate once → finalize lifecycle.

### Add a semantic operation

Review `SemanticNodeKind`, the constrained `GraphBuilder`, domain inference,
compiled fingerprinting, preflight lowering, prepared-plan tests, and
architecture documentation. A semantic operation does not automatically
require a new renderer primitive.

### Change semantic identity

Review the affected program semantic version, compiled format version,
prepared format version, cache format version, identity tests, and
[ADR 0003](../adr/0003-separate-semantic-and-execution-identities.md). Do not
use the Cargo package version as a semantic or cache input.

### Change YAML syntax

Review restricted raw parsing, normalization, registry syntax metadata,
parse-versus-compile error ownership, compiler contract tests, examples, and
the workflow reference.

## Common mistakes

- Do not put media or tool I/O in compilation.
- Do not special-case registered program names in parser or evaluator control
  flow.
- Do not treat YAML mapping order as execution order.
- Do not add renderer policy to semantic Video domains.
- Do not add public plugin machinery for built-in programs.
- Do not update hashes by adding package-version strings.
- Do not bypass the constrained `GraphBuilder`.
- Do not discard or absorb unrelated worktree changes.

Operational agent conventions remain in
[`issue-tracker.md`](issue-tracker.md),
[`triage-labels.md`](triage-labels.md), and [`domain.md`](domain.md).
