# Documentation maintenance

Give each page one job and keep exact rules in one place.

## Page types

- **Tutorial:** guide a learner through one successful path.
- **How-to guide:** complete one concrete task.
- **Reference:** state exact behavior for lookup.
- **Explanation:** build a mental model or explain trade-offs.
- **Development documentation:** help contributors change and verify the project.

## Canonical owners

| Claim | Owner |
| --- | --- |
| Public syntax and behavior | `docs/reference/language/` and `docs/language-grammar.md` |
| Built-in program facts and generated reference pages | The Rust built-in catalog |
| Built-in diagnostic facts and generated reference pages | The Rust diagnostic catalog |
| Machine-contract versions and support levels | `src/contracts.rs` and `docs/reference/machine-contracts.md` |
| Phase responsibilities and internal terms | `docs/architecture.md` |
| Durable decisions and trade-offs | `docs/adr/` |
| Change impact and identity review | `docs/development/change-guide.md` |
| Runnable source programs | `examples/` |
| Installed starter compatibility and lifecycle | `docs/reference/cli.md#init` |
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

## ADRs

Write an ADR for a durable phase boundary, identity rule, or non-obvious
trade-off. Start from `docs/adr/template.md`. Keep accepted ADRs focused on the
current decision and let Git preserve superseded history.

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
sources. It probes videos in the browser, but does not resolve imports, accept
standalone Audio-file sources, or run external programs.

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
`clipasm::reference`. Generated program and diagnostic pages state their
ownership; do not edit them or either delimited navigation block directly.

After an intentional program or diagnostic catalog change, update the checked-in
reference pages:

```console
cargo run --locked -p clipasm-reference-docs -- write
```

Check catalog facts and examples, generated ownership, navigation, obsolete
pages, anchors, links, and exact generated bytes without changing files:

```console
cargo run --locked -p clipasm-reference-docs -- check
```

### Built-in diagnostic compatibility and workflow

Released built-in diagnostic codes are durable machine-readable identifiers,
even while ClipAsm is pre-release. Their wording and source locations may
improve, but one code must continue to identify one diagnostic class. Splitting,
replacing, or retiring a released code requires a compatibility note. Internal
contract diagnostics state their weaker stability in the generated reference.
Custom diagnostics from embedding applications are outside ClipAsm's built-in
catalog.

When adding or changing a built-in diagnostic:

1. Choose or add its typed catalog identifier.
2. Define its title, category, retry guidance, stability, and explanation.
3. Use the typed identifier at each production construction site.
4. Add focused behavioral coverage.
5. Regenerate the reference pages with `write`.
6. Run the generator `check` and the full repository check.
7. Review compatibility before renaming or retiring a released code.

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
