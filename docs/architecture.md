# Architecture

This page maps phase ownership and the internal contracts between phases. Use
the [language reference](reference/language/index.md) for exact authored
behavior and the [change guide](development/change-guide.md) to find affected
code, tests, documentation, and identities.

```text
native .clipasm source
  -> lexing, parsing, package loading, and structural lowering
Canonical source package + linked source programs
  -> program catalog linking and checked-source construction
Self-contained checked source
  -> checked stack evaluation
Semantic graph + ordered source outputs (`CompiledProgram`)
  |-> compiled JSON adapter (optional inspection view)
  `-> preflight through native or browser asset host
Prepared primitive plan + one result
  -> closed renderer-owned FFmpeg recipes
  -> native process/cache adapter -> published MP4 + manifest
  -> browser worker/VFS adapter -> preview and downloadable MP4
```

## Language and canonical source

### CLI project discovery

The native CLI owns `clipasm.toml` discovery and decoding. When the caller omits
a source path, the CLI searches upward from the current directory. It resolves
the manifest's relative entrypoint and project root. An explicit source path
bypasses project discovery.

The manifest selects a source package. It also selects the host location for
project-local `.clipasm/` state. The manifest does not contribute language
configuration, semantic identity, media settings, output settings, or renderer
policy.

### Authored model

`source` owns the lowered authored model. This model includes linked source
packages, source units, imports, root settings, source program signatures,
bodies, invocations, references, literals, and output bindings. The compiler
consumes only this model.

The neutral `catalog` module owns the shared signature vocabulary. This
vocabulary includes input and parameter slots, cardinality, stack access,
scalar parameter types, and value signatures. The `source`, `language`, and
`program` modules use this vocabulary. The catalog does not depend on source
packages, program implementations, or semantic graphs.

### Parsing and lowering

The native `.clipasm` language is the sole supported source language. Its
normative EBNF, lexer, parser, package loader, and lowerer own surface grammar
and sugar.

The handwritten recursive-descent parser mirrors the grammar's productions. It
produces a source-located scalar-expression tree without arithmetic. Those
details disappear before compilation. The source structs remain intentionally
opaque. The project does not promise a stable external builder API.

Parsing erases empty call parentheses into an empty argument sequence. It
preserves authored body presence until program linking resolves the
implementation variant.

When the caller omits braces, checked-source construction gives each body
program an empty draft body. Direct, authored, and external programs continue
to reject caller-supplied bodies. `clip` applies the same normalization while
lowering its sugar expansion.

`StackBlock` is a structural canonical-source item, not a registered program. It
uses the same checked body evaluation as program bodies. The
[composition reference](reference/language/composition-forms.md#stack-blocks)
owns its authored behavior.

### Surface provenance

Every canonical item also carries one surface origin separate from its
executable target. The origin records the authored construct name and span,
ordered sugar-expansion frames, and whether the item belongs in normal explain
output. Compiler draft and checked source preserve that value unchanged.
Diagnostics and semantic source origins use the authored construct, while
program lookup and execution continue to use the actual invocation target.
Generated helpers may therefore remain executable and diagnosable without
appearing as user-authored operations. Semantic and cache identity exclude
surface provenance.

## Compilation

### Package linking and type resolution

Before evaluation, the compiler validates the complete linked source-unit
graph. The graph includes the root and every import target. The compiler rejects
cycles and derives a deterministic dependency-first order from the graph.
`SourcePackage` unit storage order is therefore not semantic.

The compiler checks every unit, including units that the root never invokes.
It stores checked programs by their `SourceUnitId` for evaluation. The compiler
then links each unit's local program namespace.

Canonical bodies first become a compiler-owned linked draft. The draft records
program IDs, effective stack access, descriptor-ordered roles, and validated
body presence once. Declaration collection and dependency discovery consume
that draft.

Then, one compiler-owned type resolver assigns stable variables to graph-valued
locals and generic invocations. The resolver owns selectors, explicit input
types, body contracts, normal stack binding, generic outputs, and output names.
Exploratory source-order passes narrow the variables monotonically. They retry
stack choices that depend on unresolved forward types.
Forward references therefore participate in the same resolution path as
ordinary stack values, while dependency cycles remain explicit errors.

After the fixpoint stabilizes, the same recursive resolver performs final
resolution. The resolved draft owns the linked draft. It also owns each
invocation's concrete signature and stack-binding plan. It stores the ordered
output types of structural stack blocks and the source body.

There is no separate concrete type or stack interpreter. Imported definitions
and built-ins share the runtime catalog.

### Checked source and scalar evaluation

Checked-source materialization allocates compact local and parameter identities.
It resolves graph and scalar references and checks scalar operator types. It
also assigns lexical body-port identities.

A separate exact scalar evaluator reduces Number expressions and applies Integer
refinements. It evaluates typed Duration and TimeRange composition when a checked
invocation is bound. Parsing therefore knows expression structure, but not
rational arithmetic or parameter constraints. Checked source keeps inputs,
parameters, stack plans, and body-port identities aligned to typed descriptor
slots.

Authored durations retain their wall-clock or project-frame unit family through
parameter binding. The invariant-protected time model owns textual parsing for
both families. Built-ins convert wall-clock durations through the configured
rate, while project-frame durations lower directly to the existing semantic
frame counts and ranges. Audio consumers map those cumulative project-frame
boundaries onto the project sample grid. The authored spelling therefore does
not introduce a second semantic range representation or alter compiled identity.

Immutable scalar aliases are canonical zero-output items. Every draft body has
one scalar scope identified independently from graph-value scope. The compiler
predeclares all aliases in that body, links the scope to its parent, and assigns
compact alias identities. This permits forward references and lexical captures
without runtime scope maps. Parent aliases are visible in descendants.

Nested aliases do not escape. Sibling bodies may reuse a name. Declarations cannot
shadow a visible alias. They also cannot collide with a program input,
parameter, or named graph value.

Checked-source materialization eagerly resolves every alias reference, checks
operator types, and diagnoses dependency cycles in the lexical environment of
the declaring body. The resulting checked alias table is dense: every identity
owns one complete checked expression. Invocation evaluation reduces an alias
through the existing exact scalar evaluator only when a parameter use reaches
it. Value-dependent failures such as division by zero, mixed timeline roots,
native-grid bounds, and destination parameter constraints therefore remain
use-time checks. Timeline selectors in aliases may capture lexical body ports
but never borrow a contextual timeline root from a later invocation.

Materialization consumes the resolver's concrete signatures, stack plans, and
output types instead of recalculating them. Checked items are complete when
constructed. No later source-and-checked lockstep pass repairs references or
output bindings.

Checked source directly represents inline input bodies and lexical body-port
aliases. It does not retain canonical bodies and invocations for ordinary
evaluation.

### Stack evaluation and root bindings

`compiler` evaluates each checked body as a typed postfix stack program. All
bodies use one physical heterogeneous evaluation stack. Each stack occurrence
records its owning body depth. Recursive frames record the nearest visibility
boundary. The source body starts empty and returns its complete ordered final
owned values.

Evaluation traverses only checked source. It does not repeat lookup, argument
classification, scalar conversion, access resolution, signature discovery,
body validation, or stack selection.

The evaluator applies the stored stack plan. It evaluates checked inline input
bodies on isolated evaluation stacks. It constructs one descriptor-indexed
resolved call for every implementation variant.

The program descriptor owns static names, cardinalities, and parameter types.
The resolved signature stores only concrete input and output types. Runtime
input cardinality is structural. Parameters remain aligned to typed slots
instead of name maps. Built-in named accessor methods use the descriptor only
as convenience adapters.

After source checking, a dedicated entrypoint adapter accepts root values from
a CLI or another host. The adapter translates public names into the ordered
call ABI once.

External programs also use the ordered call internally. They reconstruct named
input and parameter maps only at the semantic and process protocol boundary.

The type resolver resolves the closed Video-or-Audio selector for
type-preserving built-ins. It stores the concrete signature in checked source.
A bare `concat` contributes its homogeneous stack inputs to the same inference.
This also applies when code names or references a containing stack-block result
before its declaration.

The evaluator does not repeat type inference. Program implementations receive a
fully resolved call, not surface arguments, generic types, or stack-frame
metadata.

Every program descriptor explicitly declares a default `StackAccess`. Checked
invocation metadata may override it. The evaluator applies that plan with
frame ownership and visibility boundaries. The
[stack-binding reference](reference/language/stack-binding.md) owns the public
access rules.

### Semantic graph and timeline identity

The crate-private `semantic` module owns graph operations, draft and compiled
nodes, origins, graph construction, graph-local type checks, and semantic
version propagation. Generic timeline programs remain generic in this graph.
One typed semantic `Repeat`, `Concat`, `Slice`, and `ReplaceRange` supports both
Video and Audio.

Concrete ranges carry an invariant-checked native frame or sample range.
Deferred ranges carry one exact expression. They derive their media type from
the bound input. Preflight dispatches these operations into media-specific
prepared Video or Audio primitives.

Compilation retains references for dependency analysis. It infers each domain
that it can determine without media I/O. The structure hash identifies language
and graph semantics, not the package release.

Private typed identity documents list the semantic fields of each operation.
They contain authored values and upstream hashes. They never contain source
locations or inspection-only metadata.

UTF-8 paths retain their ordinary string identity. Non-UTF-8 native paths use an
explicit platform-tagged byte or wide-unit encoding. Thus, private hashing does
not inherit the Unicode-path limitation of JSON inspection.

One crate-private utility owns identity hashing and streaming file-content
hashing. The compiler does not own this utility. Thus, preflight, cache, and
tool identity do not depend on compiler implementation details.

Timeline marker arithmetic uses exact linear expressions in seconds. Each
extent term uses the native unit of its semantic value. Video uses project
frames, and Audio uses project samples.

A trim with a marker range that depends on unprobed media remains a typed,
deferred semantic slice. A `during` replacement can also remain a typed,
deferred replacement. These extent terms are semantic dependencies. Their
identities use upstream value hashes, not engine-assigned IDs. Preflight resolves
the terms to exact frame or sample ranges after probing. It then reuses the
existing native slice and concat primitives.

Evaluated stack occurrences carry a separate timeline-view identity. This
identity has a symbolic extent and one canonical ordered child sequence.
Composition adds the children of unnamed occurrences to the parent. It preserves
named occurrences as selector boundaries.

The named selector index comes from that sequence. It stores child indexes for
each immediate occurrence of a spelling. It never copies offsets or child-view
identity. Exact paths require one occurrence at each level. No label origin
shadows another.

A timeline-anchored call can search this canonical graph for one unique matching
descendant suffix. Aliases and explicitly rooted selectors do not use that
contextual search. The search uses capped zero, one, or multiple dynamic
programming over the timeline-view DAG. Thus, repeated shared views do not cause
an exponential occurrence walk. Selector failures show the canonical occurrence
graph as a bounded root-relative tree.

Mixed-root failures carry the two originating trees through scalar evaluation.
Program definitions declare their layout mapping. Registry validation checks
that each non-fresh mapping has one type-compatible output. It also checks each
mapped input slot and body shape before compilation.

Registry validation checks body prepare functions against their declared
initial-value contract again when invoked. Identity mappings preserve an input
view. Repeat mappings
preserve the complete layout for the `repeat(1)` alias. Larger counts create an
unindexed root. Its extent equals the exact input extent times the count.

Direct concat and body-concat finalizers create cumulative placements from
evaluated occurrences. Media-neutral crop mappings retain and rebase only fully
contained child regions. Replacement mappings splice unaffected base regions
with a nested replacement view. Transitions create operation-owned regions.

Crossfade maps `before` and `after` into overlapping coordinates. It exposes
their shared `overlap`. Flash cut maps its inputs sequentially. `during` rejects
a surviving base placement named `replacement`. Its result contract reserves
that spelling.

Anonymous concat identity and associativity follow from one normalization rule,
not operation-specific exceptions. Thus, media values and timeline views remain
distinct. Entrypoint `output` metadata remains separate from the semantic result
and its structure hash.

### Formats and host adapters

Project media formats are invariant-protected model values: Video dimensions,
frame-rate components, color interpretation, audio sample rate, and channel
count are valid by construction. Language lowering may carry representable raw
settings such as a zero dimension. The compiler owns semantic project-format
validation, so the language layer cannot bypass it. Video domains compose an
exact frame count with a `VideoSpec`. Audio domains compose an exact sample
count with an `AudioSpec`.

### Explicit color pipeline

Semantic color and physical encoding have separate owners. `VideoSpec` carries
the project `ColorSpec`: primaries, transfer characteristic, matrix
coefficients, and numeric range. Authored source selects the closed
`sdr_bt709` profile rather than independently combining those fields.
`RenderPolicy` separately owns codec, container, pixel format, bit depth,
subsampling, and chroma location. Both contracts serialize at their relevant
identity boundaries.

The foundation profile is opaque SDR BT.709. Persistent working Video is
lossless FFV1 in Matroska with `yuv444p10le`, BT.709 primaries, BT.709 transfer,
BT.709 matrix coefficients, and limited range. Final Video is H.264
`yuv420p`, 8-bit BT.709 limited range with left chroma location. FFmpeg output
options tag both representations, and native and browser artifact verification
checks the complete tuple, bit depth, pixel format, and final chroma location.
Working Audio is lossless FLAC carrying explicitly quantized signed 16-bit PCM
at the project sample rate and stereo channel layout. Every logical Audio node
ends in that representation, and artifact verification checks the sample
format and bit depth as well as rate, channels, timestamps, and sample count.

Preflight resolves source interpretation before recipe generation. Untagged
opaque RGB stills use the language's sRGB convention. JPEG Y'CbCr stills use
full-range BT.601 matrix coefficients with centered chroma. Conflicting still
metadata, alpha, and embedded ICC profiles are rejected until the corresponding
conversion or compositing policy exists. Video-file sources must carry complete
BT.709 primaries, transfer, matrix, range, and any required chroma location.
Missing video metadata is unknown; ClipAsm does not infer it from resolution,
codec, or file name. PQ, HLG, and HDR mastering metadata are rejected because
gamut conversion is not HDR-to-SDR tone mapping.

One renderer color module owns every zscale conversion. Source fitting,
resizing, `zoom_in` interpolation, `flash_cut`, and Video `crossfade` convert to
full-range `gbrpf32le` display-linear BT.709 RGB before doing pixel arithmetic,
then convert back to the canonical working signal. Here display-linear means
zimg's BT.1886-style display EOTF for BT.709 mastered Video, not inverse camera
OETF or scene-linear radiance. The conversion fixes nominal peak luminance at
100 cd/m² and disables zimg's approximate-gamma path. Routing-only operations
such as trim, repeat, and concat preserve the working representation. Export
uses an explicit zscale depth and chroma conversion with dithering; `setparams`
is used only after sample conversion to establish frame metadata.

An explicit downstream document adapter produces compiled JSON. It is a
versioned inspection view of compiled semantics, not an authored source
representation. Its schema does not derive implicitly from the internal
`CompiledProgram` layout. Executable semantic values do not implement the
inspection schema themselves. The adapter deliberately includes source origins
that semantic identity excludes.

Render manifests and external-program requests are also versioned integration
contracts. Prepared inspection JSON and browser
render plans remain host-internal. Cache metadata remains private. Canonical
versions and support levels live in `src/contracts.rs`.

The root crate has three explicit build surfaces. With no Cargo features it
contains the language, source model, compiler, semantic graph, public reference
catalog, compiled inspection adapter, and pure browser preparation/render-recipe
adapters. The `native` feature adds filesystem/tool preflight and native
rendering. The default `cli` feature includes `native` plus command-line parsing
and project discovery. Native process, temporary-file, and platform process-group
dependencies are optional and do not enter the dependency-light base library.

The `playground` workspace crate is a downstream browser host adapter. It keeps
WebAssembly bindings out of the core crate and exposes versioned in-memory
compilation and browser preparation responses. The book accepts one source unit
and supplies immutable virtual files and their hashes. Native package loading
and external processes do not enter the browser boundary.

## Programs

### Program definitions

All programs are runtime `ProgramDefinition` values in one crate-private
catalog. Each definition contains typed inputs, typed parameters, an ordered
output sequence, a semantic version, and an explicit default stack access.
Implementations include built-in direct lowerers and authored source programs.
They also include body and external implementations. Body implementations own
their preparer and declarative body contract. External implementations own their
runtime specification.

The applicable variant carries implementation-specific
data instead of optional sidecars.

### Built-in and authored execution

Direct programs lower immediately. Body programs prepare initial values and an
optional requested Video extent. The extent is an exact concrete frame count
when compilation knows it and otherwise a symbolic timeline expression that
preflight resolves from prepared media domains. The body exposes their resolved
fixed graph inputs as lexical references named after the ports. The evaluator
evaluates argument expressions before it enters that child scope.

The evaluator opens one recursive
frame on the shared evaluation stack. It executes the body once. It extracts
only entries that the frame owns, in physical order. A program-owned finalizer
receives those entries and returns the definition's declared output sequence.

An authored program receives the same concrete resolved-call interface. Its
invocation opens an isolated local scope and empty local stack. Bound graph inputs and
scalar parameters become immutable local bindings. Local graph names do not
escape. Only the complete ordered final owned values return to the caller.
Internal references use typed symbol identities, while public root names remain
a separate compiled interface.

### Entrypoint bindings

External root bindings enter compilation through `EntrypointBindings` after
checked-source construction. The adapter validates names and concrete checked
input and parameter types. It lowers bound Video and Audio paths through the
registered native media-source implementations. It constructs the root resolved
call without reconstructing canonical source.

Scalar text uses the same parameter conversion module as authored literals.
Binding spans carry the caller's path base. Thus, the CLI can resolve supplied
media, `File` parameters, and output destinations from its working directory.
It does not rewrite authored source.

### Built-in catalog and semantic operations

Registered programs are:

- direct: `image`, `video`, `audio`, `extract_audio`, `set_audio`, `concat`,
  `repeat`, `trim`, `drop`, `zoom_in`, `flash_cut`, `crossfade`
- body: `join`, `during`

A scoped `GraphBuilder` limits lowering. Every generated operation inherits the
active program's semantic version and origin. The catalog groups built-in
declarations by responsibility under `program::builtins`: media sources, explicit
Audio adaptations, generic timeline operations, visual effects, transitions,
and body programs. Small programs share a family module. A substantial program
may use a focused module within its phase. Adding a program does not require
parser or evaluator program-name control flow.

Semantic operations are structurally typed: each variant owns its result type
and canonical dependency order. Traversal, named-reference discovery, and
compiled fingerprinting consume that dependency authority rather than
reconstructing topology independently. Pure Video domain inference is a
separate compiler owner from final graph assembly.

Topological traversal is semantic-graph infrastructure rather than a compiler
service. Compilation and native or browser preflight consume the same
semantic-owned traversal, so downstream phases do not reach back into compiler
internals.

Exhaustive matches over the closed semantic-operation and prepared-primitive
enums are healthy: each phase keeps one dispatcher that must handle every
supported operation. Operation-specific work may live in family modules, but a
native operation is not a dynamically registered cross-phase plugin. Branching
on registered program names in parser or evaluator logic is unhealthy. Program
behavior belongs in registry definitions and their direct or body
implementations.

### Source programs and imports

Native file declarations are language syntax, not registered invocations. The
evaluator treats the executable body uniformly without granting any registered
program a privileged source-file role. Pure compilation may
produce zero, one, or multiple ordered outputs. Publication finds exactly one
Video among them and permits any number of auxiliary Audio outputs.

The native package loader resolves import paths relative to the importing file.
It deduplicates source units by canonical path and rejects cycles. It lowers
each file after its dependencies expose their callable input and parameter
shapes. The resulting imports, source-program interfaces, and implementation
kinds are canonical source data. Compilation never opens source files.

### External programs

A source unit owns either a ClipAsm body or one native `external { ... }`
implementation. Ordinary source imports link both forms. Canonical
source retains the external executable, ordered arguments, semantic version,
and preserved input.
The compiler converts it into an ordinary runtime program definition, sharing
the normal checking, defaults, binding, and stack behavior.

External implementation files have no executable body and cannot import other
programs. Composition uses a ClipAsm wrapper.

External evaluation adds a pure semantic node. Preflight is the first phase that
resolves and hashes the executable and turns the node into an exact prepared
primitive. File arguments and File parameters follow the same source-relative
resolution, hashing, and collision rules as other data assets. Rendering
re-hashes reached dependencies, passes resolved paths, executable, and argv
separately, sends the versioned JSON request, and verifies the artifact before
cache commit.

Executable and file-argument bytes belong to prepared identity. Authored
executable, arguments, parameters, and graph inputs belong to compiled semantic
identity. Persistent reuse assumes external implementations are deterministic
for that complete identified input. Clock, random, network, environment, or
undeclared-file dependencies are outside the cache contract. Authors must record
such changes in a declared file or `semantic_version`. The current renderer has no
per-external cache opt-out.

## Preflight

### Responsibilities

`preflight` is the first phase allowed to inspect assets or external tools. It:

- resolves each authored path relative to the source unit that contains it
- resolves and hashes reachable data assets
- validates image and video contracts
- resolves video-source durations
- verifies FFmpeg identity and only the capabilities required by the reachable
  prepared operations and final export
- selects one renderer policy whose artifact-cache profile defines cache
  compatibility and whose export profile defines publication
- lowers reachable semantic nodes, including `replace_range`, to compact
  renderer primitives
- assigns content fingerprints and an execution namespace

### Prepared plan

The prepared-plan model is separate from preflight orchestration. It represents
Video and Audio nodes as separate structural variants. A Video variant always
carries a Video primitive, exact frame domain, and attached-audio state. An
Audio variant always carries an Audio primitive and exact sample domain. Wrong-
media operations and missing domains are therefore not representable in a
prepared node.

Each prepared variant owns its canonical input order, which
prepared fingerprinting reuses. `PreparedPlan::prepared_json` owns the explicit,
versioned local inspection document and preserves the `kind`, `value_type`,
`domain`, `audio_domain`, and `has_audio` fields. Renderer-owned plan, node,
operation, asset, and tool types do not implement `Serialize`. Adding private
execution state therefore cannot silently change the inspection format.

Preflight keeps one exhaustive semantic-operation dispatcher. Media, timeline,
effect, transition, and external modules implement the individual preparation
rules while shared graph lookup, exact-domain access, node construction, and
fingerprinting remain centralized. FFmpeg discovery records the executable build
identity before media inspection. After lowering, an exhaustive prepared-primitive
pass derives the encoders, muxers, and filters required by that graph and its final
export. Missing capabilities for unreachable operations do not reject the plan.
External programs remain responsible for extra FFmpeg features they invoke.

The artifact-cache profile and its contract revision join the FFmpeg and FFprobe
build identities in the execution namespace. Preflight requires native encoders
and the native working container only when the prepared graph contains native
nodes. External artifacts may use other encodings when they satisfy the
verified media contract. Export-only policy changes reuse compatible working
artifacts because publication is always performed afresh.

### Native and browser hosts

Native preflight supplies filesystem assets, media probes, and tool identities.
Browser preflight accepts normalized virtual assets plus host-computed hashes and
bounded probe documents without opening media or invoking tools itself. The
browser worker probes and decode-checks the same immutable blob it will mount,
using the authored still-image, video-file, or combined source roles. Browser
preflight validates the returned document against the same source contracts as
native preflight and derives exact project-frame domains for video files. It
reuses the same prepared lowering for operations reachable from still images
and video files.

The browser renderer turns that plan into the versioned execution document.
Audio-file sources, imports, and external programs are
explicitly unsupported in the browser.

### Exact media domains

Preflight normalizes Audio to the configured stereo project sample rate, which
defaults to 48 kHz. Working Video artifacts always contain one lossless
normalized audio stream, using silence for semantically silent Videos, while
semantic audio presence controls final MP4 publication. Standalone source-audio
duration comes from its declared stream timeline where available and is
converted to covering samples on that project grid. Preflight uses a
decoded-count/source-rate duration only when timeline metadata is absent.

Video and Audio retain their native duration grids rather than sharing a
least-common-denominator tick type. One exact rational timeline mapper converts
cumulative frame boundaries to covering sample boundaries and converts sample
durations back to covering frame counts. A Video segment from frame `a` to
frame `b` receives samples between the mapped absolute boundaries, not a fresh
rounding of `b - a`. Video concatenation, joins, slices, extraction, audio-on-
black adaptation, and repeat rendering all use this policy. Adjacent segments
therefore telescope to the exact combined sample count, so arbitrary source
segmentation cannot accumulate audio drift. Crossfade uses the same mapper for
its shortened prefix, overlap, and suffix, including phase-adjusting the latter
input to global output boundaries.

## Rendering

### Recipe generation and native execution

`render` verifies the prepared FFmpeg and FFprobe build identities and reached
source assets. Persistent mode reuses only verified cached artifacts. Cache-none
mode renders the same working contracts into private temporary storage without
reading or writing persistent entries. Both modes render native
FFV1/yuv444p10le+FLAC Video intermediates and signed-16-bit FLAC Audio
intermediates in Matroska. It verifies external-program artifacts against the same prepared
media shape and complete color encoding. The external request states the exact
working output encoding. When the result Video has audio, ClipAsm exports one
explicitly converted and tagged H.264/yuv420p BT.709 MP4 with AAC.

ClipAsm captures FFmpeg and FFprobe metadata and capability output with fixed
limits. Long-running commands retain only bounded diagnostic stderr. ClipAsm
consumes exact Audio sample counts as a bounded line stream, not one complete
frame document. Media-tool execution has no fixed deadline because valid render
time scales with the input and operation graph. The
sibling render manifest has its own versioned schema and records only project
media properties, semantic/result identity, tool version summaries, and cache
statistics. It does not serialize the executable prepared plan or local paths.

Cache and publication orchestration remain in `render`. One exhaustive
prepared-primitive dispatcher delegates media, Audio, timeline, effect, and
transition argument construction to focused modules and returns a typed FFmpeg
recipe. Recipes distinguish literal arguments, source assets, and prepared
artifacts. They contain no shell commands or host paths.

The native adapter materializes recipes with platform paths. It owns FFmpeg
process setup, upstream artifact lookup, policy-driven output construction,
temporary naming, failure cleanup, and atomic cache replacement. External
programs bypass FFmpeg recipes, use the versioned process protocol, and must
satisfy artifact verification rather than the native encoding policy.

### Execution planning and cache validation

Before execution, a private execution plan walks backward from the prepared
result. Each prepared node owns one exact physical working-artifact contract.
Video includes its exact Video domain, color interpretation, physical encoding,
and normalized physical Audio domain.
Every working Video artifact stores that Audio domain. Audio includes its exact
Audio domain.

The contract participates in the node fingerprint. ClipAsm verifies it before
it commits a new cache sidecar. A later cache entry becomes a dependency barrier
when its versioned sidecar identifies the current execution namespace and node
fingerprint. Its recorded SHA-256 must also match the artifact.

A persistent-cache miss expands to the node's canonical prepared inputs.
Actions then run in stable topological order, rechecking planned misses under
their per-artifact lock so a concurrent renderer can satisfy them without
duplicate work. Cache-none execution expands the complete reachable graph,
verifies every artifact, and deletes each non-result artifact after its final
consumer. Shared graph inputs remain available until all consumers finish. The
planner rehashes source assets, external executables, and declared external
files when it reaches their node. A verified persistent downstream artifact
makes the pruned upstream subtree irrelevant. FFmpeg/FFprobe identity
verification and final export remain unconditional.

Rehashing detects ordinary changes but does not snapshot files or make the
check atomic with a renderer or external process opening the path.

### Exact audio and video execution

Video joins normalize each child audio stream to its cumulative allocation
before concat. Fractional Video repeats remain compact and timestamp repeated
audio segments at cumulative boundaries so FFmpeg distributes unavoidable
sample corrections through the timeline. Crossfade places faded Audio regions
on one exact full-length sample timeline rather than deriving placement from
packet boundaries.

Every native Video filter produces a finite exact frame stream. Working and
final encoders do not impose a second `-frames:v` cutoff, which could terminate
coverage-rounded Audio at the final Video timestamp. Artifact verification
checks the exact resulting frame and sample counts instead. Artifact
verification, locking, and rollback-capable publication remain separate deep
modules. One private process-lifecycle module owns bounded pipe retention,
temporary executable-busy retries, and kill-and-reap cleanup.

Media tools receive
a closed standard input instead of inheriting the caller's terminal. Unix
children run in dedicated process groups. Authored external programs run in Job
Objects on Windows.

Trusted FFmpeg and FFprobe invocations retain direct child handling on Windows.
This avoids Job Object setup on each media probe and render
step. ClipAsm terminates remaining managed descendants after the direct process
exits. Command recipes, protocols, and diagnostics remain in their phase adapters.
There is no generic command runner, operation trait hierarchy, or renderer
backend interface.

### Browser rendering

The browser adapter materializes the same recipes against a private virtual
filesystem. It executes them sequentially in a dedicated worker. It verifies
the exact stream shape, frame count, and Audio sample count after every step.
It deletes artifacts after their last use and returns a verified MP4. Runtime and
work limits are browser policy rather than semantic Video limits: they cover
prepared-operation count, Video pixel-frames, and aggregate Audio samples.

The pinned single-threaded FFmpeg WebAssembly runtime loads only when rendering
starts. Cancellation terminates the worker. Browser rendering has no persistent
cache.

### Cache and publication

Project renders in persistent mode keep the cache under `.clipasm/cache/` at
the discovered manifest root. Explicit standalone sources use
`.clipasm/cache/` beside the entrypoint source. Cache-none renders use a private
directory beside the publication destination and remove it at the end; this
temporary materialization is separate from cache retention policy. Existing
persistent entries are left untouched. The compiler retains the canonical path
identity of every linked package source, including imported units outside the
result-reachable graph. Native preflight and rendering protect those identities
separately from the prepared-node resource traversal. Rendering rejects
publication destinations, private cache artifacts, sidecars, and lock paths
when they alias a source program, asset, or external executable. Lock files
also reject symlink paths before opening them.

Per-artifact file locks serialize persistent-cache validation and
replacement across ClipAsm processes without blocking unrelated fingerprints.

Exact media verification happens once. ClipAsm then commits the versioned
sidecar for those bytes. Later hits rehash the complete
artifact and require the sidecar to identify the current execution namespace and
node fingerprint. This detects accidental corruption and shape-compatible swaps
without repeatedly decoding already certified bytes. The cache remains trusted
local state rather than an authenticated boundary: an actor able to replace both
artifact and sidecar can define that local state. Locks coordinate ClipAsm
processes but do not snapshot a path against unrelated local mutation between
validation and later use.

ClipAsm stages output and manifest files as temporary siblings. It commits them
under a destination-specific file lock through one
rollback-capable publication transaction after verification.

The native parser, package loader, and compiler enforce explicit nesting limits
for authored structures. Iterative semantic graph dependency, hashing, and
domain passes avoid a graph-depth limit from the Rust call stack.

## Ownership rules

- Canonical source owns lowered authored structures and source locations.
- The neutral catalog owns program-signature vocabulary shared across authored
  source, parsing, and executable program definitions.
- The native CLI owns project-manifest discovery and entrypoint selection.
- The native language layer owns surface grammar, package loading, callable
  argument elaboration, and structural sugar.
- The entrypoint adapter owns validation and conversion of root values supplied
  after source checking.
- Checked-source construction owns linked program resolution, reference and
  argument resolution, scalar conversion, static body validation, stack plans,
  and authored output inference.
- Programs own operation signatures, body lifecycles, and semantic versions.
- Semantic graph construction owns graph-local validity.
- Compilation owns checked stack evaluation, semantic dependency resolution,
  and pure domain inference.
- Preflight owns media and tool discovery, exact typed domains, and prepared primitive construction.
- Rendering owns closed FFmpeg recipes, native artifact execution, cache, and publication.
- The browser host owns virtual file binding, WebAssembly execution,
  cancellation, and preview/download lifecycle.

Do not move media I/O into compilation or backend policy into semantic Video
domains.
