# Machine-readable contracts

ClipAsm emits several JSON documents, but only three are intended for external
consumers. Always read the version field before decoding a document.

## Supported integrations

| Document | Current version | Produced or consumed by | Intended use |
| --- | ---: | --- | --- |
| Compiled inspection JSON | `format_version: 21` | `clipasm inspect` | source-analysis and diagnostic tooling |
| Render manifest | `format_version: 1` | successful native render | automation and render provenance |
| External-program request | `protocol_version: 1` | trusted external executable | implementing an external program |

A versioned JSON document is not an authoring format. ClipAsm does not accept
these documents as source input.

## Compiled inspection JSON

`clipasm inspect SOURCE` and the Rust `CompiledProgram::compiled_json` method
produce the same media-independent document. It includes project settings,
compiled operations, known domains, ordered outputs, names, source origins, and
the compiled structure hash.

A consumer must support the exact `format_version`. A new version may add,
remove, rename, or reinterpret fields.

## Render manifest

A successful native render writes `<output>.manifest.json` beside the MP4. It
records:

- manifest and engine versions;
- the compiled semantic hash;
- project Video and Audio settings;
- the result fingerprint and exact Video domain;
- whether the Video carries meaningful Audio;
- FFmpeg and FFprobe version summaries;
- cache hit and miss counts.

It deliberately excludes local source paths, executable recipes, and cache
locations.

## External-program request

A reachable external implementation receives one JSON object on standard input.
Protocol version 1 contains:

- named prepared inputs with artifact paths, types, exact domains, and audio
  state;
- resolved Integer, Keyword, and File parameters;
- the output path the process must create;
- project Video and Audio settings;
- resolved FFmpeg and FFprobe executable paths.

The process does not return JSON. It creates the requested file and exits with
status zero; ClipAsm then probes and verifies the artifact. An implementation
must reject protocol versions it does not support.

Paths are native host paths, and the executable runs with the user's
permissions. This protocol is not a sandbox.

## Internal formats

**Prepared inspection JSON** (`format_version: 11`) and the **Browser render plan**
(`version: 1`, `recipe_contract: 1`) are internal to matching ClipAsm components. They may be useful while debugging ClipAsm, but they are not
persistence or interchange contracts.

**Cache entry metadata** is a **Private implementation detail**. Do not read,
edit, copy, or construct cache sidecars as an integration mechanism.

## Consumer rules

For supported documents:

1. Read the version field first.
2. Accept only versions your software explicitly supports.
3. Ignore unknown fields only when doing so is safe for that decoder.
4. Review and test every version change.
5. Never use JSON object ordering as identity.
