# Examples

Run the examples below from the repository root.

`clipasm init` copies the canonical `scenic-sequence.clipasm` source bytes as
an initialized project's `main.clipasm`, along with its three PNG assets. See
[the starter README](starter/README.md) for commands to run outside a checkout.

The canonical explanation of each source program and its validation/render
commands is in [the examples chapter](../docs/examples.md).

All source assets are committed and intentionally small:

- three 320x180 lossless PNG illustrations;
- one two-second 320x180 H.264/Matroska video with gentle continuous motion;
- one external `.clipasm` program and a minimal executable Python/FFmpeg script.

No asset-generation step is required.

Generated media, outputs, manifests, and cache files are ignored by Git.
