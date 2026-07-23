# Architecture

```text
restricted YAML
  -> syntax parsing and normalization
Workflow
  -> typed call binding and stack evaluation
Semantic graph
  -> preflight
Prepared primitive plan
  -> renderer and cache
MP4 + manifest
```

The reasons behind the load-bearing boundaries are recorded in the
[architecture decision records](adr/).

## Syntax

`syntax` parses restricted YAML, retains source spans, and normalizes every
executable item to either a reference or a registered program invocation.
Body shorthand, postfix syntax, and the root `timeline` disappear during this
normalization. Duplicate keys, anchors, aliases, custom tags, and multiple YAML
documents are rejected. Parsing does not open media files.

## Compilation

`compiler` evaluates every program body as a typed postfix stack. One shared
binder resolves explicit inputs, consumes missing inputs from the local stack,
and converts authored parameters to their declared Rust types. Program
implementations therefore receive a fully resolved call rather than
syntax-layer arguments.

The crate-private `semantic` module owns graph operations, draft and compiled
nodes, origins, graph construction, graph-local type checks, and semantic
version propagation. Compilation retains references for dependency analysis,
infers every domain knowable without media I/O, and produces a structure hash
that identifies language and graph semantics rather than the package release.

## Programs

All programs are static `ProgramDefinition` values in one crate-private
registry. Each definition contains typed inputs, typed parameters, one output,
a semantic version, and either a direct lowerer or a body preparer.

Direct programs lower immediately. Body programs prepare one initial local
stack and requested-duration context, the evaluator executes their body once,
and a program-owned finalizer reduces the resulting stack to one value.

Foundation programs are:

- direct: `image`, `video`, `concat`, `repeat`
- body: `then`, `join`, `timeline`, `during`

Lowering is restricted to a scoped `GraphBuilder`; every generated operation
inherits the active program's semantic version and origin. Adding a program
does not require parser or evaluator program-name control flow.

Exhaustive matches over the closed semantic-operation and prepared-primitive
enums are healthy: each owner must handle every supported operation. Branching
on registered program names in parser or evaluator logic is unhealthy; program
behavior belongs in registry definitions and their direct or body
implementations.

## Preflight

`preflight` is the first phase allowed to inspect assets or external tools. It:

- resolves paths relative to the workflow
- hashes reachable source files
- validates image and video contracts
- resolves video-source durations
- verifies FFmpeg and FFprobe capabilities
- lowers reachable semantic nodes, including `replace_range`, to
  `image_video`, `video_source`, `slice`, and `concat`
- assigns content fingerprints and an execution namespace

The prepared plan has exact domains for every node.

## Rendering

`render` verifies source hashes again, reuses only verified cached artifacts,
renders missing FFV1/Matroska intermediates, and exports one H.264/yuv420p MP4.

The cache lives under `.clipasm/cache/` beside the workflow. Output and
manifest files are written to temporary siblings and atomically replaced after
verification.

## Ownership rules

- Language assembly owns registry and syntax-name invariants.
- Syntax owns YAML shape and descriptor-driven normalization.
- Compiler binding owns signature enforcement and parameter conversion.
- Programs own operation signatures, body lifecycles, and semantic versions.
- Semantic graph construction owns graph-local validity.
- Compilation owns typed stack evaluation, dependency resolution, and pure
  domain inference.
- Preflight owns media and tool discovery.
- Rendering owns artifact execution and publication.

Do not move media I/O into compilation or backend policy into semantic Video
domains.
