# Machine-readable contracts

ClipAsm has several JSON boundaries, but they serve different audiences. Check
the document's own version field before consuming it; a higher version means the
shape or interpretation changed.

| Boundary | Current version | Support level | Intended consumer |
| --- | ---: | --- | --- |
| Compiled inspection JSON | `format_version: 20` | Versioned inspection contract | `clipasm inspect` users and diagnostic tooling |
| Render manifest | `format_version: 1` | Versioned published contract | Render automation and provenance tooling |
| External-program request | `protocol_version: 1` | Versioned integration protocol | Trusted external program implementations |
| Prepared inspection JSON | `format_version: 11` | Host-internal inspection format | Rust hosts debugging a prepared plan |
| Browser render plan | `version: 1`, `recipe_contract: 1` | Bundled-host contract | ClipAsm's browser worker and matching runtime |
| Cache entry metadata | private version | Private implementation detail | ClipAsm only |

A version number does not make a document an authoring format. Do not edit one
of these documents and feed it back into the compiler.

## Compiled inspection JSON

`clipasm inspect SOURCE` and [`CompiledProgram::compiled_json`](https://docs.rs/clipasm/latest/clipasm/compiler/struct.CompiledProgram.html#method.compiled_json)
produce the same pure, media-independent document. It includes project media
settings, semantic nodes, ordered outputs, names, source origins, and the
compiled structure hash.

Consumers may rely on the shape only when `format_version` is exactly the
version they support. A format-version change may add, remove, rename, or
reinterpret fields. The document is not canonical source and is not accepted as
compiler input.

## Render manifest

A successful native render publishes `<output>.manifest.json` beside the MP4.
The manifest is deliberately smaller than a prepared plan. It records:

- the manifest and engine versions;
- the compiled semantic hash;
- project Video and Audio settings;
- the result fingerprint, exact Video domain, and meaningful-audio flag;
- FFmpeg and FFprobe version summaries;
- cache hit and miss counts.

It excludes local source paths, executable recipes, and cache locations. Tools
may archive and compare manifests after checking `format_version`.

## External-program request

A reachable trusted external implementation receives one JSON object on
standard input. Protocol version 1 contains:

- `protocol_version`;
- named prepared inputs with artifact path, value type, exact domain, and audio
  state;
- resolved Integer, Keyword, and File parameters;
- the output path the process must create;
- project Video and Audio settings;
- resolved FFmpeg and FFprobe executable paths.

The process writes no response document. Success means exiting with status zero
after creating the requested output; ClipAsm then probes and verifies that
artifact. An implementation must reject unsupported protocol versions rather
than guessing.

Paths are native host paths and the process runs with the user's permissions.
This protocol is not a sandbox.

## Host-internal formats

Prepared inspection JSON exposes resolved paths, tool identities, renderer
primitives, fingerprints, and cache metadata. It is useful for debugging a Rust
host, but it is not a persistence or interchange promise.

The browser render plan and its recipe contract are supported only between the
matching ClipAsm browser adapter, bundled worker, and declared runtime versions.
They may evolve with that host without becoming a general external API.

Cache entry metadata is private. Do not read, edit, copy, or construct cache
sidecars as an integration mechanism.

## Compatibility rule

For the three supported contracts—compiled inspection JSON, render manifests,
and external-program requests—consumers must:

1. Read the version field first.
2. Accept only explicitly supported versions.
3. Ignore unknown fields when that is safe for their decoder.
4. Treat a version change as requiring review and tests.

ClipAsm may improve whitespace, field ordering, and human-readable text without
changing semantic meaning. JSON object ordering must not be used as identity.
