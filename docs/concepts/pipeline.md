# From source to published video

Three commands expose the main stages of ClipAsm:

| Command | Opens media? | Runs tools or external programs? | Main result |
| --- | --- | --- | --- |
| `validate` | No | No | source and type check |
| `inspect` | No | No | compiled JSON |
| `render` | Yes, when reachable | Yes, when required | MP4 and manifest |

This separation lets you catch source problems quickly and lets unused media
stay unopened.

## 1. Read and check the source

ClipAsm parses the root `.clipasm` file and its imports, then checks every linked
source program. It resolves calls, arguments, types, stack inputs, names, and
ordered outputs.

This stage does not open authored media. Durations written directly in source
can already be exact; durations that depend on a video or audio file remain
unknown until rendering.

An unused imported program must still be valid source because the complete
linked package is checked.

## 2. Prepare reachable media and tools

During `render`, preflight starts from the one Video selected for publication
and follows only the work needed to produce it. It resolves paths, hashes source
assets, probes media, checks required FFmpeg capabilities, and locates reachable
external executables.

This means an unused import can contain an unused missing media file without
blocking rendering, as long as the imported source itself is valid.

## 3. Execute, verify, and publish

ClipAsm reuses verified cached artifacts when possible and executes the missing
operations in dependency order. External programs reached here run as trusted
native code.

Produced media is checked before it enters the cache or replaces the published
output. A successful render writes the MP4 and a sibling manifest.

Rendering requires exactly one Video output. Additional Audio outputs may exist,
but they are not published separately.

## Terms used in reference pages

- **compiled program**: the checked media-independent result used by `inspect`;
- **preflight**: media and tool resolution performed by `render`;
- **prepared plan**: the exact reachable work after preflight;
- **publication**: verification and final replacement of the MP4 and manifest.

See the [command-line reference](../reference/cli.md) for exact command behavior.
