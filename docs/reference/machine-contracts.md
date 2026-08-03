# Machine-readable contracts

ClipAsm emits several JSON documents. It supports only three as external
contracts. Always read the version field before decoding a document.

## Supported integrations

| Document | Current version | Produced or consumed by | Intended use |
| --- | ---: | --- | --- |
| Compiled inspection JSON | `format_version: 24` | `clipasm inspect` | source-analysis and diagnostic tooling |
| Render manifest | `format_version: 4` | successful native render | automation and render provenance |
| External-program request | `protocol_version: 3` | trusted external executable | implementing an external program |

A versioned JSON document is not an authoring format. ClipAsm does not accept
these documents as source input.

## Compiled inspection JSON

`clipasm inspect SOURCE` and the Rust `CompiledProgram::compiled_json` method
produce the same media-independent document. It includes project settings,
compiled operations, known domains, ordered outputs, names, source origins, and
the compiled structure hash.

Source origins are inspection metadata, not semantic identity. In particular,
moving or reformatting an external File parameter does not change the structure
hash. This requires an unchanged authored path and unchanged resolved call.

Path-bearing inspection fields are JSON strings and therefore require valid
Unicode. Pure compilation and semantic identity can still represent native
non-Unicode paths. Only a compiled inspection JSON request for such a program
fails.

A consumer must support the exact `format_version`. A new version may add,
remove, rename, or reinterpret fields.

## Render manifest

A successful native render writes `<output>.manifest.json` beside the MP4. It
records:

- Manifest and engine versions.
- The compiled semantic hash.
- Project Video and Audio settings.
- The published Video pixel and color encoding.
- The result fingerprint and exact Video domain.
- Whether the Video carries meaningful Audio.
- FFmpeg and FFprobe version summaries.
- The cache mode and number of verified working artifacts reused. Cache-none
  renders report zero reused artifacts.
- The execution materialization mode (`all` or `fused`) and number of rendered
  jobs.

It deliberately excludes local source paths, executable recipes, and cache
locations.

## External-program request

A reachable external implementation receives one JSON object on standard input.
Protocol version 3 contains:

- Named prepared inputs with artifact paths, types, exact domains, and audio
  state.
- Resolved Integer, Keyword, and File parameters.
- An output object containing the path the process must create, the complete
  working Video encoding, and the signed-16-bit working Audio encoding.
- Project Video and Audio settings.
- Resolved FFmpeg and FFprobe executable paths.

The process does not return JSON. It creates the requested file and exits with
status zero. ClipAsm then probes and verifies dimensions, duration, audio, pixel
format, bit depth, primaries, transfer, matrix, and range against the request.
An implementation must reject protocol versions it does not support.

Paths refer to native host paths, but JSON strings carry them. Every path in an
external-program request must therefore be valid Unicode. Native ClipAsm
operations do not share this JSON limitation. The executable runs with the
user's permissions. This protocol is not a sandbox.

## Internal formats

**Prepared inspection JSON** (`format_version: 16`) and the **Browser render plan**
(`version: 3`, `recipe_contract: 9`) are internal to matching ClipAsm components.
They may help when you debug ClipAsm. They are not persistence or interchange
contracts.

**Cache entry metadata** is a **Private implementation detail**. Do not read,
edit, copy, or construct cache sidecars as an integration mechanism.

## Consumer rules

For supported documents:

1. Read the version field first.
2. Accept only versions your software explicitly supports.
3. Ignore unknown fields only when doing so is safe for that decoder.
4. Review and test every version change.
5. Never use JSON object ordering as identity.
