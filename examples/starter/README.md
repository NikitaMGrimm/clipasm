# ClipAsm starter

This project is ready to render:

```console
clipasm render
```

For a faster source-only check that does not open media, run:

```console
clipasm validate
```

Open `generated/scenic-sequence.mp4` to see the result. Edit `main.clipasm`,
optionally validate the change, and render again.

`clipasm.toml` keeps verified working artifacts between renders by default. Set
`render.cache = "none"` when you want each render to use temporary artifacts
only.

`render.materialization = "all"` keeps one working artifact per prepared node.
Set it to `"fused"` to combine compatible FFmpeg-only graph regions. Cache
hits, duplicated physical streams, temporal joins, external programs, and
branches with different materialized endpoints remain artifact boundaries.

The files are ordinary ClipAsm project files. `clipasm init` creates them but
does not manage or update them afterward.
