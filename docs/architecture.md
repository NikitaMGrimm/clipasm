# Architecture

```text
authoring representation
  -> frontend parsing and desugaring
Canonical source package + linked source programs
  -> program catalog linking and checked-source construction
Checked source + canonical source
  -> typed call binding and stack evaluation
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

## Frontends and canonical source

`source` owns the representation-neutral authored model: linked source
packages, source units, imports, root project/publication settings, source
program signatures, bodies, invocations, references, literals, and output
bindings. The compiler consumes only this canonical model.

`frontend::yaml` parses restricted YAML and lowers it into canonical source.
It owns YAML mappings, scalar styles, reserved fields, the `program` header,
postfix and primary-argument sugar, duplicate keys, anchors, aliases, tags, and
document-count restrictions. Those surface details disappear before
compilation. This internal boundary permits future frontends to provide
different syntax and sugar without changing compiler behavior. The canonical
source structs are intentionally opaque to external crates today; no stable
public builder API is promised before a second frontend demonstrates what it
needs.

## Compilation

Before evaluation, the compiler links each source unit's local program
namespace and checks every authored program in import dependency order,
including programs the root never invokes. This pass resolves program IDs,
effective stack access, argument and body contracts, named-value types, and
ordered output types into compiler-owned checked-source metadata paired with
the immutable canonical source. Named outputs whose generic type is not yet
known are represented as deferred declarations. A recursive declaration resolver
follows aliases and source-level reference dependencies, respects body-port
shadowing, detects cycles, and infers self-contained producers through the same
body checker used by ordinary compilation. Imported definitions and built-ins
then share
one runtime catalog and one call binder.

`compiler` evaluates the source body and every nested body as a typed postfix
stack program over one physical heterogeneous evaluation stack. Each stack
occurrence records its owning body depth, and recursive frames record the nearest
visibility boundary. The source body starts empty and returns its complete ordered final owned values. Evaluation traverses canonical source
and checked metadata together, so it does not repeat program-name lookup,
effective-access resolution, output-signature discovery, or structural body
validation. One shared binder resolves explicit inputs in descriptor order,
evaluates inline fixed-input bodies on isolated evaluation stacks, consumes
missing inputs by exact concrete type from the invocation's accessible values,
and converts authored parameters to their declared Rust types. Before binding, the checker resolves the single closed type parameter used by
type-preserving built-ins and stores the resulting concrete signature in checked
source. A body-inferred program such as bare `glue` may defer that resolution
until its checked body has produced one homogeneous timeline type, including
when the result is named or referenced before its declaration. The evaluator
does not repeat type inference. Program implementations therefore
receive a fully resolved call rather than frontend-layer arguments, generic
types, or stack-frame metadata.

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

Compiled JSON is produced by an explicit downstream document adapter. It is a
serialized view of compiled semantics, not an authored source representation,
and its schema is not derived implicitly from the internal `CompiledProgram`
layout.

## Programs

All programs are runtime `ProgramDefinition` values in one crate-private
catalog. Each definition contains typed inputs, typed parameters, an ordered
output sequence, a semantic version, and an explicit default stack access.
Implementations are built-in direct lowerers, built-in body preparers, or
authored source programs. Body programs additionally expose a declarative body
contract used by common type inference.

Direct programs lower immediately. Body programs prepare initial values and a
requested-duration context. Their resolved fixed graph inputs are exposed in
the body as lexical references named after the ports; argument expressions are
evaluated before that child scope is entered. The evaluator opens one recursive
frame on the
shared evaluation stack, executes the body once, extracts only entries owned by
that frame in physical order, and gives them to a program-owned finalizer that
returns the
definition's declared output sequence.

An authored program uses the same caller-side binder. Its invocation then
opens an isolated local scope and empty local stack. Bound graph inputs and
scalar parameters become immutable local bindings. Local clips and `id`/`ids`
bindings do not escape; only the complete ordered final owned values return to the caller. Internal references use typed symbol identities, while public root
names remain a separate compiled interface.

External root bindings enter compilation through `EntrypointBindings`. A bound
root Video input is adapted to an isolated canonical body invoking the native
`video` program, then evaluated and bound through the ordinary program catalog,
call binder, and type checks. Scalar text is converted by the shared parameter
binder. Binding spans carry the caller's path base, so the CLI can resolve
supplied media, `File` parameters, and output destinations from its working
directory without rewriting authored source.

Registered programs are:

- direct: `image`, `video`, `audio`, `extract_audio`, `set_audio`, `concat`,
  `repeat`, `trim`, `zoom`, `wobble`, `flash`
- body: `join`, `glue`, `during`

Lowering is restricted to a scoped `GraphBuilder`; every generated operation
inherits the active program's semantic version and origin. Adding a program
does not require parser or evaluator program-name control flow.

Exhaustive matches over the closed semantic-operation and prepared-primitive
enums are healthy: each owner must handle every supported operation. Branching
on registered program names in parser or evaluator logic is unhealthy; program
behavior belongs in registry definitions and their direct or body
implementations.

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

The prepared plan has exact frame domains for Video nodes and exact sample
domains for Audio nodes. Audio is normalized to 48 kHz stereo. Working Video
artifacts always contain one lossless normalized audio stream, using silence for
semantically silent Videos, while semantic audio presence controls final MP4
publication.

## Rendering

`render` verifies the prepared FFmpeg and FFprobe build identities and source
hashes again, reuses only verified cached artifacts, renders missing
FFV1+FLAC Video intermediates and FLAC Audio intermediates in Matroska, and
exports one H.264/yuv420p MP4 with AAC when the result Video has audio.

The cache lives under `.clipasm/cache/` beside the entrypoint source. Output and
manifest files are staged as temporary siblings and committed through one
rollback-capable in-process publication transaction after verification.

## Ownership rules

- Canonical source owns representation-neutral authored structures and source
  locations.
- Each frontend owns its surface grammar, reserved syntax, and desugaring.
- Compiler binding owns signature enforcement and parameter conversion.
- Checked-source construction owns linked program resolution, static body
  validation, and authored output inference.
- Programs own operation signatures, body lifecycles, and semantic versions.
- Semantic graph construction owns graph-local validity.
- Compilation owns typed stack evaluation, dependency resolution, and pure
  domain inference.
- Preflight owns media and tool discovery.
- Rendering owns artifact execution and publication.

Do not move media I/O into compilation or backend policy into semantic Video
domains.
