# Documentation maintenance

Give each page one job and keep exact rules in one place.

## Page types

- **Tutorial:** teach through one complete, successful project. State the
  outcome and prerequisites, use numbered steps with observable results, and
  finish with a short recap and one next step.
- **How-to guide:** complete one concrete task. State the starting conditions,
  use numbered actions, verify success, and link to reference for alternatives
  and exact rules.
- **Reference:** state exact behavior for lookup. Organize around stable names,
  signatures, options, tables, and constraints rather than a learning sequence.
- **Explanation:** build a mental model or explain trade-offs. Link to reference
  when a precise rule matters instead of restating every edge case.
- **Development documentation:** help contributors change and verify the project.

Learning chapters may explain why each step matters. How-to guides assume a
reader who already has a goal and should not detour into a full lesson. Keep one
reader journey per page; link to a different page type when the reader's need
changes.

## Learning chapters

The user-facing tutorial sequence is presented as **Learn ClipAsm**, one ordered
set of chapters rather than a collection of independent tutorials.

- Continue the same project, source filename, assets, durations, and output path
  unless a chapter explicitly motivates a change.
- Begin from the preceding checkpoint and end in a valid, unambiguous edit
  state. Do not repeat a complete source listing when precise incremental edits
  are clearer.
- Introduce a construct only when the current edit needs it.
- Deliberately show an error only when it exposes an important invariant. Warn
  that failure is expected, identify the mismatch, repair it immediately, and
  validate the repaired source.
- Prefer normal stack binding in linear compositions. Do not introduce named
  graph inputs, stack blocks, `join`, or explicit access modes merely to cover
  syntax.
- End with an observable validation or render result, a short statement of what
  changed in the reader's model, and exactly one next chapter.
- Keep a runnable final checkpoint under `examples/` and validate it with the
  rest of the repository.

The browser playground is a separate quick-success path. Guides remain
task-oriented and may assume the language concepts taught by the ordered path.

## Canonical owners

| Claim | Owner |
| --- | --- |
| Public syntax and behavior | `docs/reference/language/` and `docs/language-grammar.md` |
| Built-in program facts and generated reference pages | The Rust built-in catalog |
| Built-in diagnostic facts and generated reference pages | The Rust diagnostic catalog |
| Machine-contract versions and support levels | `src/contracts.rs` and `docs/reference/machine-contracts.md` |
| Phase responsibilities and internal terms | `docs/architecture.md` |
| Change impact and identity review | `docs/development/change-guide.md` |
| Runnable source programs | `examples/` |
| Installed starter contents and lifecycle | `docs/reference/cli.md#init` |
| Repository development examples | `examples/` |
| Starter README and ignore rules | `examples/starter/` |
| Contribution workflow | `CONTRIBUTING.md` |

Reader-oriented pages may summarize these sources for their audience, but should
link to the owner for exact rules. Do not infer public guarantees from incidental
implementation details or turn compiled JSON into another authoring format.

Use established spelling and capitalization, including ClipAsm, Video, Audio,
FFmpeg, and FFprobe. Change terminology in its canonical owner first, then review
diagnostics, concepts, examples, and code names that use it.

## Examples and commands

Prefer committed examples. For every documented command:

1. State required tools or working directory when it is not obvious.
2. Run the exact command when the environment permits.
3. Explain success without inventing output.
4. Report unavailable tools or assets.

Use `clipasm` fences for ClipAsm source and `console` fences for terminal
sessions. Mark commands that change state or have nondeterministic output as
`console,ignore`; this includes installation, initialization, rendering, and
tool-version checks. Keep generated media, render output, manifests, and caches
untracked.

### Exact CLI output

When a page promises exact, deterministic CLI output, use one terminal
transcript beginning with `$` and register the page in
`tests/documented_cli.rs`. Mark command-only blocks and any side-effecting or
nondeterministic command on a registered page as `console,ignore`.

Regenerate intentional transcript changes with:

```console
TRYCMD=overwrite cargo test --locked --test documented_cli
```

Review the Markdown diff. Register only deterministic, side-effect-free commands.

## Links and navigation

- Use relative links inside the book.
- Use stable repository links for root files outside `docs/`.
- Update inbound links after moving or renaming a page.
- Add every public page to `docs/SUMMARY.md`.
- Do not leave public pages orphaned.
- Build the book and inspect the affected route.

## Interactive examples

Place a normal `clipasm` block immediately before:

```html
<div data-clipasm-playground></div>
```

Prefer an mdBook `{{#include}}` of a committed example. To bundle project files,
add both attributes:

```html
<div data-clipasm-playground
     data-clipasm-assets-base="playground/example-assets/"
     data-clipasm-assets='["assets/morning.png"]'></div>
```

The browser adapter accepts one source unit plus still-image and video-file
sources. It hashes, probes, and decode-checks every media source in the browser,
then validates each blob against the same still-image or video-file contract as
native preflight. It does not resolve imports, accept standalone Audio-file
sources, or run external programs.

Changes to compilation or preparation responses must update the response version
in `playground/src/lib.rs` and `theme/clipasm-playground.js`. Changes to recipe or
runtime compatibility must update browser plan metadata and the render worker
together.

Build the browser assets with the pinned tools:

```console
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.126 --locked
mdbook build
./scripts/build_playground.sh
```

Serve `target/book` over HTTP. Render the scenic example, cancel and restart it,
and test an uploaded replacement after changing the runtime lifecycle.

## Generated references

The non-published `clipasm-reference-docs` workspace tool renders the built-in
program index, one page per built-in, the diagnostic index, diagnostic category
pages, and their delimited navigation blocks in `docs/SUMMARY.md` from
`clipasm::reference`. Generated program pages and the standalone diagnostics page state their
ownership; do not edit them or either delimited navigation block directly.

After an intentional program or diagnostic catalog change, update the checked-in
reference output:

```console
cargo run --locked -p clipasm-reference-docs -- write
```

Check catalog facts and examples, generated ownership, navigation, obsolete
output, anchors, links, and exact generated bytes without changing files:

```console
cargo run --locked -p clipasm-reference-docs -- check
```

### Built-in diagnostic workflow

Built-in diagnostic codes are lookup keys shared by terminal output, `clipasm
explain`, and generated documentation. Custom diagnostics from embedding
applications are outside ClipAsm's built-in catalog.

When adding or changing a built-in diagnostic:

1. Choose or add its typed catalog identifier.
2. Define its title, category, retry guidance, and explanation.
3. Use the typed identifier at each production construction site.
4. Add focused behavioral coverage.
5. Regenerate the reference output with `write`.
6. Run the generator `check` and the full repository check.

This keeps the catalog, terminal `clipasm explain <CODE>` output, and generated
documentation together. Do not add a production error string first and defer
its catalog entry or documentation.

## Machine-readable contracts

Keep version values in `src/contracts.rs`; serialization owners must consume
those constants rather than declaring local copies. Update the machine-contract
reference whenever a supported document changes shape or meaning.

Compiled inspection JSON, render manifests, and external-program requests are
versioned integration contracts. Prepared inspection JSON and browser render
plans are host-internal. Cache metadata remains private. A version bump requires
focused serialization tests and a review of every consumer.

## Checks

Run targeted checks while editing:

```console
mdbook build
python3 scripts/check_docs.py
git diff --check
```

Before handoff, run:

```console
./scripts/check.sh
```

Review the final diff for broken links, missing navigation, duplicated rules,
unverified commands, generated artifacts, and unrelated changes.
