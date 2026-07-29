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

The native CLI owns `clipasm.toml` discovery and decoding. When a source path is
omitted, it searches upward from the caller's current directory and resolves the
manifest's relative entrypoint and project root. An explicit source path bypasses
project discovery. The manifest selects a source package and the host location
for project-local `.clipasm/` state only; it does not contribute language
configuration, semantic identity, media settings, output settings, or renderer
policy.

### Authored model

`source` owns the lowered authored model: linked source packages, source units,
imports, root project/publication settings, source program signatures, bodies,
invocations, references, literals, and output bindings. The compiler consumes
only this model.

### Parsing and lowering

The native `.clipasm` language is the sole supported source language. Its
normative EBNF, lexer, parser, package loader, and lowerer own surface grammar
and sugar. The handwritten recursive-descent parser mirrors the grammar's
productions and produces a source-located scalar-expression tree without
performing arithmetic. Those details disappear before compilation. The source
structs remain intentionally opaque; the project does not promise a stable
external builder API.

Parsing erases empty call parentheses into an empty argument sequence. It
preserves whether a body was authored until program linking resolves the
implementation variant. Checked-source construction gives every body program an
empty draft body when braces were omitted; direct, authored, and external
programs continue to reject caller-supplied bodies. `clip` applies the same
normalization while lowering its sugar expansion.

`StackBlock` is a structural canonical-source item rather than a registered
program. It evaluates through the same checked body machinery as program bodies;
the [composition reference](reference/language/composition-forms.md#stack-blocks)
owns its authored behavior.

### Surface provenance

Every canonical item also carries one surface origin separate from its
executable target. The origin records the authored construct name and span,
ordered sugar-expansion frames, and whether the item belongs in normal explain
output. Compiler draft and checked source preserve that value unchanged.
Diagnostics and semantic source origins use the authored construct, while
program lookup and execution continue to use the actual invocation target.
Generated helpers may therefore remain executable and diagnosable without
appearing as user-authored operations. Surface provenance is excluded from
semantic and cache identity.

## Compilation

### Package linking and type resolution

Before evaluation, the compiler validates the complete linked source-unit
graph, including the root and every import target, rejects cycles, and derives a
deterministic dependency-first order from the graph itself. `SourcePackage`
unit storage order is therefore not semantic. Every unit is checked, including
units the root never invokes, while checked programs remain stored by
their `SourceUnitId` for evaluation. The compiler then links each unit's local program
namespace. Canonical bodies first become a compiler-owned linked draft that
records program IDs, effective stack access, descriptor-ordered input and
parameter roles, and validated body presence once.
Declaration collection and dependency discovery consume that draft before one
compiler-owned type resolver assigns stable variables to graph-valued locals
and generic invocations. The resolver owns selectors, explicit input types,
body contracts, normal stack binding, generic outputs, and names attached to
those outputs. Exploratory source-order passes narrow the variables
monotonically, retrying stack choices that depend on unresolved forward types.
Forward references therefore participate in the same resolution path as
ordinary stack values, while dependency cycles remain explicit errors.

After the fixpoint stabilizes, the same recursive resolver performs final
resolution. The resulting resolved draft owns the linked draft, every
invocation's concrete signature and stack-binding plan, and the ordered output
types of structural stack blocks and the source body. There is no separate
concrete type or stack interpreter. Imported definitions and built-ins share
the resulting runtime catalog.

### Checked source and scalar evaluation

Checked-source materialization allocates compact local and parameter identities,
resolves graph and scalar references, statically checks scalar operator types,
and assigns lexical body-port identities. A separate exact scalar evaluator
reduces Number expressions, applies Integer refinements, and evaluates typed
Duration and TimeRange composition when a checked invocation is bound. Parsing
therefore knows expression structure but not rational arithmetic or parameter
constraints. Inputs, parameters, stack plans, and body-port
identities remain aligned to typed descriptor slots throughout checked source.

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
without runtime scope maps. Parent aliases are visible in descendants; nested
aliases do not escape; sibling bodies may reuse a name; and declarations may not
shadow a visible alias or collide with a program input, parameter, or named graph
value.

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
output types rather than recomputing them. Checked items are complete when
constructed; no
later source/checked lockstep pass repairs references or output bindings. Inline
input bodies and lexical body-port aliases are represented directly in checked
source. Canonical bodies and invocations are not retained for ordinary
evaluation.

### Stack evaluation and root bindings

`compiler` evaluates the checked body and every nested body as a typed postfix
stack program over one physical heterogeneous evaluation stack. Each stack
occurrence records its owning body depth, and recursive frames record the nearest
visibility boundary. The source body starts empty and returns its complete
ordered final owned values. Evaluation traverses only checked source, so it does
not repeat program-name lookup, reference lookup, argument classification,
scalar conversion, effective-access resolution, output-signature discovery,
structural body validation, or stack selection. It applies the stored stack
plan, evaluates checked inline input bodies on isolated evaluation stacks, and
constructs one descriptor-indexed resolved call consumed by every direct, body,
authored, and external implementation. Static names, cardinalities, and
parameter types remain owned by the program descriptor; the resolved signature
stores only concrete input and output types. Runtime input cardinality is
structural, and parameters remain aligned to typed slots rather than being
rebuilt into name maps. Named accessor methods used by built-ins resolve through
the descriptor as convenience adapters only.

Root values supplied by a CLI or another host after source checking use a
dedicated entrypoint adapter that translates public names into the same ordered
call ABI once. External programs likewise use the ordered call internally and
reconstruct named input and parameter maps only at the semantic and process
protocol boundary. The type resolver resolves the single closed Video-or-Audio
selector used by type-preserving built-ins and stores the resulting concrete
signature in checked source. A bare `concat` contributes its homogeneous stack
inputs to that same inference, including when a containing stack-block result is
named or referenced before its declaration. The evaluator
does not repeat type inference. Program implementations therefore receive a
fully resolved call rather than surface arguments, generic types, or
stack-frame metadata.

Every program descriptor explicitly declares a default `StackAccess`; checked
invocation metadata may override it. The evaluator applies that plan using
frame ownership and visibility boundaries. The
[stack-binding reference](reference/language/stack-binding.md) owns the public
access rules.

### Semantic graph and timeline identity

The crate-private `semantic` module owns graph operations, draft and compiled
nodes, origins, graph construction, graph-local type checks, and semantic
version propagation. Generic timeline programs remain generic in this graph:
there is one typed semantic `Repeat`, `Concat`, `Slice`, and `ReplaceRange` for
both Video and Audio. Concrete ranges carry an invariant-checked native frame or
sample range; deferred ranges carry one exact expression and derive their media
type from the bound input. Preflight is the sole phase that dispatches these
operations into media-specific prepared Video or Audio primitives. Compilation
retains references for dependency analysis,
infers every domain knowable without media I/O, and produces a structure hash
that identifies language and graph semantics rather than the package release.
Private typed identity documents enumerate the semantic fields of every
operation explicitly. They contain authored values and upstream hashes, never
source locations or inspection-only metadata. UTF-8 paths retain their ordinary
string identity while non-UTF-8 native paths use an explicit platform-tagged
byte or wide-unit encoding; private hashing therefore does not inherit JSON
inspection's Unicode-path limitation. Identity hashing and streaming file
content hashing are owned by one crate-private utility rather than by the
compiler, so preflight, cache, and tool identity do not depend on compiler
implementation details.
Timeline marker arithmetic is normalized into exact linear expressions in
seconds. Each extent term is scaled from its semantic value's native unit:
project frames for Video and project samples for Audio. A trim whose marker
range depends on unprobed media remains a typed deferred semantic slice; a
`during` replacement may likewise remain one typed deferred replacement. Their
extent terms are semantic dependencies and their identities use upstream value
hashes rather than engine-assigned IDs. Preflight resolves those terms to
ordinary exact frame or sample ranges after probing, then reuses existing native
slice and concat primitives. Evaluated stack occurrences carry a separate
timeline-view identity with symbolic extent and one canonical ordered child
sequence. Composition splices the children of unnamed occurrences into the
parent and preserves named occurrences as selector boundaries. The named
selector index is derived centrally from that sequence and stores child indexes
for every immediate occurrence of each spelling; it never copies offsets or
child-view identity. Exact paths require one occurrence at
each level; no label origin shadows another. Selector evaluation in a
timeline-anchored call may search this canonical graph for one uniquely
matching descendant suffix; aliases and explicitly rooted selectors do not use
that contextual search. The search uses capped zero/one/multiple dynamic
programming over the timeline-view DAG, so repeated shared views do not expand
into an exponential occurrence walk. Selector failures format the canonical
occurrence graph as a bounded root-relative tree;
mixed-root failures carry the two originating trees through scalar evaluation.
Program definitions declare their layout mapping, and registry validation checks
that every non-fresh mapping has one type-compatible output, every mapped input
slot exists, and every body shape matches its mapping before compilation. Body
prepare functions are checked again against their declared initial-value
contract when invoked. Identity mappings preserve an input view. Repeat mappings
preserve the complete layout for the `repeat(1)` alias; larger counts create a
fresh unindexed root whose extent is the exact input extent multiplied by the
count. Direct concat and body-concat finalizers create cumulative placements
from evaluated occurrences, media-neutral crop mappings retain and rebase only
fully contained child regions, replacement mappings splice
unaffected base regions with a nested replacement view, and transitions create
operation-owned regions. Crossfade maps `before` and `after` into overlapping
coordinates and exposes their shared `overlap`; flash cut maps its inputs
sequentially. `during` rejects a surviving base placement named `replacement`
because that spelling is reserved by its result contract. Anonymous concat
identity and associativity follow from the one normalization rule rather than
from operation-specific exceptions. Media values and timeline views therefore
remain distinct. Entrypoint `output` metadata remains separate from the semantic
result and its structure hash.

### Formats and host adapters

Project media formats are invariant-protected model values: Video dimensions,
frame-rate components, audio sample rate, and channel count are positive by
construction. Language lowering may carry representable raw settings such as
a zero dimension, but the compiler owns semantic project-format validation so
the language layer cannot bypass it. Video domains compose an exact frame count
with a `VideoSpec`; Audio domains compose an exact sample count with an
`AudioSpec`. Explicit serialization adapters preserve the established flat JSON
schemas.

Compiled JSON is produced by an explicit downstream document adapter. It is a
versioned inspection view of compiled semantics, not an authored source
representation, and its schema is not derived implicitly from the internal
`CompiledProgram` layout. Executable semantic values do not implement the
inspection schema themselves; the adapter deliberately includes source origins
that semantic identity excludes. Render manifests and external-program requests are
also versioned integration contracts. Prepared inspection JSON and browser
render plans remain host-internal; cache metadata remains private. Canonical
versions and support levels live in `src/contracts.rs`.

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
Implementations are built-in direct lowerers, body implementations that own
their preparer and declarative body contract, authored source programs, or
external implementations that own their runtime specification. Implementation-
specific data is carried by the applicable variant rather than optional
sidecars on every definition.

### Built-in and authored execution

Direct programs lower immediately. Body programs prepare initial values and an
optional requested Video extent. The extent is an exact concrete frame count
when compilation knows it and otherwise a symbolic timeline expression that
preflight resolves from prepared media domains. Their resolved fixed graph
inputs are exposed in the body as lexical references named after the ports;
argument expressions are
evaluated before that child scope is entered. The evaluator opens one recursive
frame on the
shared evaluation stack, executes the body once, extracts only entries owned by
that frame in physical order, and gives them to a program-owned finalizer that
returns the
definition's declared output sequence.

An authored program receives the same concrete resolved-call interface. Its
invocation opens an isolated local scope and empty local stack. Bound graph inputs and
scalar parameters become immutable local bindings. Local graph names do not
escape; only the complete ordered final owned values return to the caller.
Internal references use typed symbol identities, while public root names remain
a separate compiled interface.

### Entrypoint bindings

External root bindings enter compilation through `EntrypointBindings` after
checked-source construction. The adapter validates names and concrete checked
input/parameter types directly, lowers bound Video and Audio paths through the
registered native media-source implementations, and constructs the root resolved call without
reconstructing canonical source. Scalar text uses the same parameter conversion
module as authored literals. Binding spans carry the caller's path base, so the CLI can resolve
supplied media, `File` parameters, and output destinations from its working
directory without rewriting authored source.

### Built-in catalog and semantic operations

Registered programs are:

- direct: `image`, `video`, `audio`, `extract_audio`, `set_audio`, `concat`,
  `repeat`, `trim`, `drop`, `zoom_in`, `flash_cut`, `crossfade`
- body: `join`, `during`

Lowering is restricted to a scoped `GraphBuilder`; every generated operation
inherits the active program's semantic version and origin. Built-in declarations
are grouped by responsibility under `program::builtins`: media sources, explicit
Audio adaptations, generic timeline operations, visual effects, transitions,
and body programs. Small programs share a family module; a substantial program
may use a focused module within its phase. Adding a program does not require
parser or evaluator program-name control flow.

Semantic operations are structurally typed: each variant owns its result type
and canonical dependency order. Traversal, named-reference discovery, and
compiled fingerprinting consume that dependency authority rather than
reconstructing topology independently. Pure Video domain inference is a
separate compiler owner from final graph assembly.

Exhaustive matches over the closed semantic-operation and prepared-primitive
enums are healthy: each phase keeps one dispatcher that must handle every
supported operation. Operation-specific work may live in family modules, but a
native operation is not a dynamically registered cross-phase plugin. Branching
on registered program names in parser or evaluator logic is unhealthy; program
behavior belongs in registry definitions and their direct or body
implementations.

### Source programs and imports

Native file declarations are language syntax, not registered invocations. The
evaluator treats the executable body uniformly without granting any registered
program a privileged source-file role. Pure compilation may
produce zero, one, or multiple ordered outputs. Publication finds exactly one
Video among them and permits any number of auxiliary Audio outputs.

The native package loader resolves import paths relative to the importing file,
deduplicates source units by canonical path, rejects cycles, and lowers each
file only after its dependencies expose their callable input and parameter
shapes. The resulting imports, source-program interfaces, and implementation
kinds are canonical source data; compilation never opens source files.

### External programs

A source unit owns either a ClipAsm body or one native `external { ... }`
implementation. Both are linked through ordinary source imports. Canonical
source retains the external executable, ordered arguments, semantic version,
and preserved input;
the compiler converts it into an ordinary runtime program definition, sharing
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
Executable and file-argument bytes belong to prepared identity; authored
executable, arguments, parameters, and graph inputs belong to compiled semantic
identity. Persistent reuse assumes external implementations are deterministic
for that complete identified input. Clock, random, network, environment, or
undeclared-file dependencies are outside the cache contract and must be reflected
by a changed declared file or `semantic_version`; the current renderer has no
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
carries a Video primitive, exact frame domain, and attached-audio state; an
Audio variant always carries an Audio primitive and exact sample domain. Wrong-
media operations and missing domains are therefore not representable in a
prepared node. Each prepared variant owns its canonical input order, which
prepared fingerprinting reuses. `PreparedPlan::prepared_json` owns the explicit,
versioned local inspection document and preserves the `kind`, `value_type`,
`domain`, `audio_domain`, and `has_audio` fields. Renderer-owned plan, node,
operation, asset, and tool types do not implement `Serialize`; adding private
execution state therefore cannot silently change the inspection format.

Preflight keeps one exhaustive semantic-operation dispatcher. Media, timeline,
effect, transition, and external modules implement the individual preparation
rules while shared graph lookup, exact-domain access, node construction, and
fingerprinting remain centralized. FFmpeg discovery records the executable build
identity before media inspection. After lowering, an exhaustive prepared-primitive
pass derives the encoders, muxers, and filters required by that graph and its final
export. Missing capabilities for unreachable operations do not reject the plan;
external programs remain responsible for extra FFmpeg features they invoke. The
artifact-cache profile and its contract revision join the FFmpeg and FFprobe
build identities in the execution namespace. Native encoders and the native
working container are required only when the prepared graph contains native
nodes; external artifacts may use other encodings when they satisfy the
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
and video files. The browser renderer turns that plan into the versioned
execution document. Audio-file sources, imports, and external programs are
explicitly unsupported in the browser.

### Exact media domains

Audio is normalized to the configured stereo project sample rate, which
defaults to 48 kHz. Working Video artifacts always contain one lossless
normalized audio stream, using silence for semantically silent Videos, while
semantic audio presence controls final MP4 publication. Standalone source-audio
duration comes from its declared stream timeline where available and is
converted to covering samples on that project grid. A decoded-count/source-rate
duration is used only when timeline metadata is absent.

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
source assets, reuses only verified cached artifacts, renders missing native
FFV1+FLAC Video intermediates and FLAC Audio intermediates in Matroska, verifies
external-program artifacts against the same prepared media shape, and exports
one H.264/yuv420p MP4 with AAC when the result Video has audio.
FFmpeg/FFprobe metadata and capability output is captured with fixed limits,
long-running commands retain only bounded diagnostic stderr, and exact Audio
sample counts are consumed as a bounded line stream rather than one complete
frame document. Media-tool execution has no fixed deadline because valid render
time scales with the input and operation graph. The
sibling render manifest has its own versioned schema and records only project
media properties, semantic/result identity, tool version summaries, and cache
statistics; it does not serialize the executable prepared plan or local paths.
Cache and publication orchestration remain in `render`. One exhaustive
prepared-primitive dispatcher delegates media, Audio, timeline, effect, and
transition argument construction to focused modules and returns a typed FFmpeg
recipe. Recipes distinguish literal arguments, source assets, and prepared
artifacts; they contain no shell commands or host paths.

The native adapter materializes recipes with platform paths and owns FFmpeg
process setup, upstream artifact lookup, policy-driven output construction,
temporary naming, failure cleanup, and atomic cache replacement. External
programs bypass FFmpeg recipes, use the versioned process protocol, and must
satisfy artifact verification rather than the native encoding policy.

### Execution planning and cache validation

Before execution, a private execution plan walks backward from the prepared
result. Each prepared node owns one exact physical working-artifact contract:
Video includes its exact Video domain plus the normalized physical Audio domain
stored in every working Video artifact, while Audio includes its exact Audio
domain. That contract participates in the node fingerprint and is verified
before a new cache sidecar can be committed. A later cache entry becomes a
dependency barrier when its versioned sidecar identifies the current execution
namespace and node fingerprint and its recorded SHA-256 matches the artifact.
A miss expands to the node's canonical prepared inputs. Actions then run in stable
topological order, rechecking planned misses under their per-artifact lock so a
concurrent renderer can satisfy them without duplicate work. Source assets,
external executables, and declared external files are rehashed when their node
is reached; a verified downstream artifact makes the pruned upstream subtree
irrelevant. FFmpeg/FFprobe identity verification and final export remain
unconditional.
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
coverage-rounded Audio at the final Video timestamp; artifact verification
checks the exact resulting frame and sample counts instead. Artifact
verification, locking, and rollback-capable publication remain separate deep
modules. One private process-lifecycle module owns bounded pipe retention,
temporary executable-busy retries, and kill-and-reap cleanup. Media tools receive
a closed standard input instead of inheriting the caller's terminal. Unix
children run in dedicated process groups and Windows children run in Job
Objects; after the direct process exits, remaining members are terminated so
descendants cannot outlive a completed invocation or keep inherited pipes open.
Command recipes, protocols, and diagnostics remain in their phase adapters.
There is no generic
command runner, operation trait hierarchy, or renderer backend interface.

### Browser rendering

The browser adapter materializes the same recipes against a private virtual
filesystem and executes them sequentially in a dedicated worker. It verifies
the exact stream shape, frame count, and Audio sample count after every step,
deletes artifacts after their last use, and returns a verified MP4. Runtime and
work limits are browser policy rather than semantic Video limits: they cover
prepared-operation count, Video pixel-frames, and aggregate Audio samples. The
pinned single-threaded FFmpeg WebAssembly runtime loads only when rendering
starts; cancellation terminates the worker. Browser rendering has no persistent
cache.

### Cache and publication

Project renders keep the cache under `.clipasm/cache/` at the discovered
manifest root. Explicit standalone sources use `.clipasm/cache/` beside the
entrypoint source. The compiler retains the canonical path identity of every
linked package source, including imported units outside the result-reachable
graph. Native preflight and rendering protect those identities separately from
the prepared-node resource traversal. Publication destinations and private
cache artifacts, sidecars, and lock paths are rejected when they alias a source
program, asset, or external executable. Lock files also reject symlink paths
before opening them. Per-artifact file locks serialize validation and
replacement across ClipAsm processes without blocking unrelated fingerprints.
Exact media verification happens once, before
the versioned sidecar for those bytes is committed. Later hits rehash the complete
artifact and require the sidecar to identify the current execution namespace and
node fingerprint. This detects accidental corruption and shape-compatible swaps
without repeatedly decoding already certified bytes. The cache remains trusted
local state rather than an authenticated boundary: an actor able to replace both
artifact and sidecar can define that local state. Locks coordinate ClipAsm
processes but do not snapshot a path against unrelated local mutation between
validation and later use. Output and manifest files are staged as temporary
siblings and committed under a destination-specific file lock through one
rollback-capable publication transaction after verification.

The native parser, package loader, and compiler enforce explicit nesting limits
for authored structures. Semantic graph dependency, hashing, and domain passes
remain iterative so graph depth is not limited by the Rust call stack.

## Ownership rules

- Canonical source owns lowered authored structures and source locations.
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
