# Documentation maintenance

ClipAsm documentation serves readers with different goals without creating
multiple sources of truth. Decide who a page is for, what job it performs, and
which canonical document owns its claims before writing it.

## Choose the page type

- A **tutorial** guides a learner through a successful experience in a deliberate
  order. Introduce a construct when the learner first needs it.
- A **how-to guide** helps a reader who already has some context complete one
  concrete task.
- **Reference** states exact syntax and behavior for lookup. Keep it complete,
  structured, and direct.
- **Explanation** builds a mental model and discusses relationships or
  trade-offs without redefining normative behavior.
- **Development documentation** helps contributors change or verify the project
  while preserving its boundaries.

A page may link across these modes, but it should have one primary job. Prefer a
small complete page over a broad outline or placeholder.

## Respect canonical ownership

Use the existing owner for each kind of claim:

| Owner | Authority |
| --- | --- |
| [`CONTEXT.md`](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTEXT.md) | Canonical domain language and settled authoring semantics |
| [Language reference](../language-reference.md) | Normative public syntax and language behavior |
| [Architecture](../architecture.md) | Phase responsibilities and internal architecture |
| [Architecture decision records](../adr/index.md) | Durable decisions, trade-offs, relationships, and status |
| [Change guide](change-guide.md) | Change ownership and impact routing |
| `examples/` ([catalog](../examples.md)) | Committed runnable source examples |
| [`CONTRIBUTING.md`](https://github.com/NikitaMGrimm/clipasm/blob/main/CONTRIBUTING.md) | Human contribution workflow |
| [`AGENTS.md`](https://github.com/NikitaMGrimm/clipasm/blob/main/AGENTS.md) | Repository operating instructions for agents |
| [`AI_POLICY.md`](https://github.com/NikitaMGrimm/clipasm/blob/main/AI_POLICY.md) | Human accountability for AI-assisted contributions |

Reader-oriented pages should summarize the minimum context their audience needs
and link to the owner for exact detail. Do not copy a semantic definition into
several pages or turn compiled JSON, implementation structures, or an ADR's
historical syntax into a second public language.

If canonical documents appear to disagree, use the safer current statement,
record the exact conflicting paths, and resolve the owners directly. Do not
infer the answer from incidental implementation behavior or silently redesign
an accepted decision.

## Use canonical terminology

Use the names defined in `CONTEXT.md`, including the capitalization of `Video`,
`Audio`, ClipAsm, FFmpeg, and FFprobe. Preserve distinctions such as source
program versus external program, graph input versus scalar parameter, clip
block versus stack block, and compilation versus preflight versus rendering.

Change terminology in `CONTEXT.md` first. Then review the language reference,
architecture, diagnostics, examples, and reader-oriented pages for affected
uses. Avoid introducing a synonym merely to vary the prose.

## Verify examples and commands

Prefer a committed example over a parallel invented version. Keep excerpts
small enough that a reader can connect them to the complete source.

For every command:

1. State the directory or other prerequisite from which it runs.
2. Run the exact command when the environment permits.
3. Explain what success means without inventing output or error text.
4. Report any unavailable tool, asset, or platform condition instead of
   claiming verification.

Use `clipasm` fences for ClipAsm source and `console` fences for terminal
sessions. Do not commit generated media, render output, manifests, or caches.

### Keep exact CLI output executable

When a page promises exact CLI output, write the command and output as one
terminal transcript beginning with `$`. Register the page in
`tests/documented_cli.rs`; normal `cargo test` then runs the command and rejects
stale output. Mark command-only blocks on a registered page as
`console,ignore` so the transcript test does not execute them.

Update registered transcripts after an intentional CLI change with:

```console
TRYCMD=overwrite cargo test --locked --test documented_cli
```

Review the Markdown diff before committing it. Register only deterministic,
side-effect-free commands; do not execute installation, rendering, or file
mutation from a documentation test.

## Link and navigate

Use relative links within the published book. Use stable repository links for
root files outside `docs/`. Link to a section or canonical owner instead of
repeating its contents. After adding, moving, or renaming a page:

- update inbound links
- add every public page to `docs/SUMMARY.md`
- ensure a public page is not unintentionally orphaned
- keep internal agent-operation pages reachable from an intentional agent or
  contributor entry point rather than presenting them as end-user chapters
- build the book and inspect the affected reader journey

## Record durable decisions

Write an ADR when a change establishes or revisits a durable architectural
boundary, non-obvious trade-off, identity rule, or phase owner. Keep the public
ADR set focused on active decisions and let Git preserve obsolete history. When
a decision changes, transfer any still-relevant rationale to the replacement,
remove the superseded record and its inbound links, and state how the new
decision is confirmed. Use the [ADR template](../adr/template.md).

## Mark volatile material selectively

A freshness note can help on a maintainer page whose details are expected to
change. State what was verified and against which source or condition; update
the note when rechecking the content. Do not add dates to stable explanations
or use a freshness marker as a substitute for verification.

## Add an interactive ClipAsm example

Put a normal `clipasm` code block immediately before an empty
`<div data-clipasm-playground></div>`. The published book progressively replaces
that pair with the browser editor; readers without JavaScript still see the
source. Prefer an mdBook `{{#include}}` of a committed example so the book and
repository do not drift.

The browser adapter deliberately exposes only media-pure, single-file
compilation. Do not imply that the playground opens assets, resolves imports,
runs external programs, or renders video. Changes to its response must update
the response version in `playground/src/lib.rs` and
`theme/clipasm-playground.js`.

To build the WebAssembly assets locally, install the pinned target and binding
tool, build the book, and run the asset script:

```console
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.126 --locked
mdbook build
./scripts/build_playground.sh
```

Serve `target/book` over HTTP to test the worker; browsers do not load the
WebAssembly module correctly from a `file://` URL. CI checks the adapter and
JavaScript, while the Pages workflow builds the same assets before deployment.

## Check the result

During documentation work, run targeted checks early:

```console
mdbook build
python3 scripts/check_docs.py
git diff --check
```

The documentation checker validates repository-local Markdown targets, confirms
that public book pages are represented in `docs/SUMMARY.md`, and checks links
and anchors in the generated HTML book. Individual ADRs are intentionally
indexed through `docs/adr/index.md`, while internal `docs/agents/` pages remain
outside the public book navigation.

Before handoff, run the complete repository check:

```console
./scripts/check.sh
```

Review the final diff for broken links, missing navigation, terminology drift,
duplicated rules, unsupported claims, unverified commands, generated artifacts,
and unrelated changes. If a check cannot run, report the exact limitation and
all checks that did run.
