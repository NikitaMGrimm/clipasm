# Architecture

```text
authoring representation
  -> frontend parsing and desugaring
Canonical source package + linked source programs
  -> program catalog linking and checked-source construction
Self-contained checked source
  -> checked stack evaluation
Semantic graph + ordered source outputs
  -> compiled JSON adapter (optional)
Compiled semantic document
  -> preflight
Prepared primitive plan + one result
  -> renderer and cache
Optional entrypoint publication
  -> MP4 + manifest
```

The reasons behind the load-bearing boundaries are recorded in the
[architecture decision records](adr/).

## Language and canonical source

`source` owns the lowered authored model: linked source packages, source units,
imports, root project/publication settings, source program signatures, bodies,
invocations, references, literals, and output bindings. The compiler consumes
only this model.

The native `.clipasm` language is the sole supported source language. Its lexer,
parser, package loader, and lowerer own surface grammar and sugar. Those details
disappear before compilation. The source structs remain intentionally opaque;
the project does not promise a stable alternate-frontend or external builder
API.

The current YAML parser remains temporary migration scaffolding while native
language support and tests are implemented. It will be removed rather than
maintained as a second frontend.

`StackBlock` is a structural canonical-source item rather than a registered
program. It evaluates a nested body in a child stack frame and returns every
remaining value owned by that frame, preserving order and types. Blocks default
to owned access in the native language; explicit visible access uses the normal
stack visibility boundary rules. Output bindings apply to the block's complete
ordered result sequence, while IDs declared inside remain program-wide.

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

Before evaluation, the compiler validates the complete linked source-unit
graph, including the root and every import target, rejects cycles, and derives a
deterministic dependency-first order from the graph itself. `SourcePackage`
unit storage order is therefore not semantic. Every unit is checked, including
units the root never invokes, while checked programs remain stored by
their `SourceUnitId` for evaluation. The compiler then links each unit's local program
namespace. Canonical bodies first become a compiler-owned resolved draft that
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
resolution and records every invocation's concrete signature and stack-binding
plan together with the ordered output types of clips and the source body. There
is no separate concrete type or stack interpreter. Imported definitions and
built-ins share the resulting runtime catalog.

Checked-source materialization allocates compact local and parameter identities,
resolves graph and scalar references, parses scalar literals, and assigns
lexical body-port identities. Inputs, parameters, stack plans, and body-port
identities remain aligned to typed descriptor slots throughout checked source.
It consumes the resolver's concrete signatures, stack plans, and output types
rather than recomputing them. Checked items are complete when constructed; no
later source/checked lockstep pass repairs references or output bindings. Inline
input bodies and lexical body-port aliases are represented directly in checked
source. Canonical bodies and invocations are not retained for ordinary
evaluation.

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
signature in checked source. A body-inferred program such as bare `glue`
contributes its homogeneous owned body outputs to that same inference, including
when the result is named or referenced before its declaration. The evaluator
does not repeat type inference. Program implementations therefore receive a
fully resolved call rather than frontend-layer arguments, generic types, or
stack-frame metadata.

Every program descriptor explicitly declares a default `StackAccess`. Generic
invocation metadata may override it with `stack_access: owned|visible`.
`owned` binding is limited to values owned by the current frame. `visible`
binding may also consume enclosing values down to the frame's visibility
boundary. Values of unrelated types remain ordered and untouched. The setting is per invocation and does not propagate to child calls. Direct built-ins and source programs default to
`owned`; the native body programs `join`, `glue`, and `during` default to
`visible`, so they may bind through an enclosing body boundary and expose that
same visible suffix to independently visible descendants.

The crate-private `semantic` module owns graph operations, draft and compiled
nodes, origins, graph construction, graph-local type checks, and semantic
version propagation. Compilation retains references for dependency analysis,
infers every domain knowable without media I/O, and produces a structure hash
that identifies language and graph semantics rather than the package release.
Entrypoint `output` metadata remains separate from the semantic result and its
structure hash.

Project media formats are invariant-protected model values: Video dimensions,
frame-rate components, audio sample rate, and channel count are positive by
construction. Canonical frontends may carry representable raw settings such as
a zero dimension, but the compiler owns semantic project-format validation so
future frontends cannot bypass it. Video domains compose an exact frame count
with a `VideoSpec`; Audio domains compose an exact sample count with an
`AudioSpec`. Explicit serialization adapters preserve the established flat JSON
schemas.

Compiled JSON is produced by an explicit downstream document adapter. It is a
serialized view of compiled semantics, not an authored source representation,
and its schema is not derived implicitly from the internal `CompiledProgram`
layout.

## Programs

All programs are runtime `ProgramDefinition` values in one crate-private
catalog. Each definition contains typed inputs, typed parameters, an ordered
output sequence, a semantic version, and an explicit default stack access.
Implementations are built-in direct lowerers, body implementations that own
their preparer and declarative body contract, authored source programs, or
external implementations that own their runtime specification. Implementation-
specific data is carried by the applicable variant rather than optional
sidecars on every definition.

Direct programs lower immediately. Body programs prepare initial values and a
requested-duration context. Their resolved fixed graph inputs are exposed in
the body as lexical references named after the ports; argument expressions are
evaluated before that child scope is entered. The evaluator opens one recursive
frame on the
shared evaluation stack, executes the body once, extracts only entries owned by
that frame in physical order, and gives them to a program-owned finalizer that
returns the
definition's declared output sequence.

An authored program receives the same concrete resolved-call interface. Its
invocation opens an isolated local scope and empty local stack. Bound graph inputs and
scalar parameters become immutable local bindings. Local clips and `id`/`ids`
bindings do not escape; only the complete ordered final owned values return to the caller. Internal references use typed symbol identities, while public root
names remain a separate compiled interface.

External root bindings enter compilation through `EntrypointBindings` after
checked-source construction. The adapter validates names and concrete checked
input/parameter types directly, lowers bound Video paths through the registered
native `video` implementation, and constructs the root resolved call without
reconstructing canonical source. Scalar text uses the same parameter conversion
module as authored literals. Binding spans carry the caller's path base, so the CLI can resolve
supplied media, `File` parameters, and output destinations from its working
directory without rewriting authored source.

Registered programs are:

- direct: `image`, `video`, `audio`, `extract_audio`, `set_audio`, `concat`,
  `repeat`, `trim`, `zoom`, `wobble`, `flash`, `crossfade`
- body: `join`, `glue`, `during`

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
implementations. See [ADR 0014](adr/0014-keep-native-operations-phase-owned.md).

The YAML `program` header is frontend syntax, not a registered invocation.
The evaluator treats its body uniformly without granting any registered
program, including `glue`, a privileged source-file role. Pure compilation may
produce zero, one, or multiple ordered outputs. Publication finds exactly one
Video among them and permits any number of auxiliary Audio outputs.

`frontend::yaml` also owns file-backed package loading for the current
representation: import paths are resolved relative to the importing file,
parsed source units are deduplicated by canonical path, and import cycles are
rejected. The resulting imports and source-program interfaces are canonical
source data; compilation does not branch on YAML or open files.

### External programs

A canonical source package may carry external program specifications and local
source-unit aliases. The YAML loader obtains them from JSON manifests, but the
compiler consumes the representation-neutral package catalog. Each specification
is converted into an ordinary runtime program definition, so checking and
binding remain shared with every other implementation.

External evaluation adds a pure semantic node. Preflight is the first phase that
resolves the executable, verifies executable permissions, hashes its bytes, and
turns the node into an exact prepared primitive. The first protocol declares that
one Video input supplies the output domain and meaningful-audio state. Rendering
reverifies the executable hash, sends one JSON request over standard input, and
verifies the resulting working artifact before cache commit.

The renderer starts the executable directly with `Command`; it does not construct
a shell command. Executable content belongs to prepared identity, while the
authored command, parameters, and graph inputs belong to compiled semantic
identity.

## Preflight

`preflight` is the first phase allowed to inspect assets or external tools. It:

- resolves each authored path relative to the source unit that contains it
- hashes reachable source files
- validates image and video contracts
- resolves video-source durations
- verifies FFmpeg and FFprobe capabilities
- lowers reachable semantic nodes, including `replace_range`, to compact
  renderer primitives
- assigns content fingerprints and an execution namespace

The prepared-plan model is separate from preflight orchestration. It represents
Video and Audio nodes as separate structural variants. A Video variant always
carries a Video primitive, exact frame domain, and attached-audio state; an
Audio variant always carries an Audio primitive and exact sample domain. Wrong-
media operations and missing domains are therefore not representable in a
prepared node. Each prepared variant owns its canonical input order, which
prepared fingerprinting reuses. The serialized prepared-plan adapter preserves
the existing `kind`, `value_type`, `domain`, `audio_domain`, and `has_audio`
fields.

Preflight keeps one exhaustive semantic-operation dispatcher. Media, timeline,
effect, transition, and external modules implement the individual preparation
rules while shared graph lookup, exact-domain access, node construction, and
fingerprinting remain centralized.

Audio is normalized to 48 kHz stereo. Working Video artifacts always contain
one lossless normalized audio stream, using silence for semantically silent
Videos, while semantic audio presence controls final MP4 publication.

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
input to global output boundaries. See
[ADR 0013](adr/0013-map-frame-and-sample-boundaries.md) and
[ADR 0015](adr/0015-overlap-audiovisual-transitions-exactly.md).

## Rendering

`render` verifies the prepared FFmpeg and FFprobe build identities and source
hashes again, reuses only verified cached artifacts, renders missing
FFV1+FLAC Video intermediates and FLAC Audio intermediates in Matroska, and
exports one H.264/yuv420p MP4 with AAC when the result Video has audio.
Cache and publication orchestration remain in `render`. The native executor
keeps one exhaustive prepared-primitive dispatcher and delegates media, Audio,
timeline, effect, transition, external-process, and export work to focused
modules. One shared render context owns FFmpeg command initialization, upstream
artifact lookup, temporary output naming, failure cleanup, and atomic cache
replacement, so operation modules cannot diverge on execution lifecycle.

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
modules; there is no generic process runner, operation trait hierarchy, or
renderer backend interface.

The cache lives under `.clipasm/cache/` beside the entrypoint source. Per-artifact
file locks serialize validation and replacement across ClipAsm processes without
blocking unrelated fingerprints. Output and manifest files are staged as
temporary siblings and committed under a destination-specific file lock through
one rollback-capable publication transaction after verification.

The YAML frontend, import loader, and compiler enforce explicit nesting limits
for authored structures. Semantic graph dependency, hashing, and domain passes
remain iterative so graph depth is not limited by the Rust call stack.

## Ownership rules

- Canonical source owns representation-neutral authored structures and source
  locations.
- Each frontend owns its surface grammar, reserved syntax, and desugaring.
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
- Rendering owns artifact execution and publication.

Do not move media I/O into compilation or backend policy into semantic Video
domains.
