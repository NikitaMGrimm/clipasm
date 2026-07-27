# Validate and inspect a program

Use validation while editing a source package, then inspect its compiled JSON
document when you need to understand what compilation produced. Neither command
opens media files, probes media, invokes FFmpeg or FFprobe, or executes an
external program.

This guide uses the starter scenic sequence. Run the commands from an
initialized project directory, such as the one created in the
[first-render guide](../getting-started/first-render.md). In a repository
checkout, replace `main.clipasm` with `examples/scenic-sequence.clipasm`.

## Validate the source package

```console,ignore
clipasm validate main.clipasm
```

Validation parses and checks the complete source package, evaluates its stack
programs, and infers every domain available from authored data. A successful
result confirms that the source is well formed and type-correct. It does not
confirm that authored media paths exist or that the tools needed for rendering
are available.

If validation fails, start with the reported source location and construct.
Fix that error before inspecting or rendering the program.

## Inspect the compiled JSON document

```console,ignore
clipasm inspect main.clipasm
```

Inspection prints a versioned downstream serialization of compiled semantics as
JSON. It is not canonical source or an authoring format. Check `format_version`
before using it in tooling; the compatibility rules are in
[Machine-readable contracts](../reference/machine-contracts.md#compiled-inspection-json).
The useful categories are:

- project Video and Audio settings;
- semantic graph nodes, their operations, and value types;
- source-independent Video frame or Audio sample domains;
- ordered source-program outputs and named values;
- source origins and the authored operations represented by the graph;
- the configured publication output, when one is declared.

Use this structure to confirm, for example, that three authored images feed one
concatenation and that the resulting Video is the source program's output.
Inspection is a view of compilation, not a prepared plan or a preview of
rendered media.

## Continue to rendering

Run `render` only when you are ready for preflight to resolve assets and tools:

```console,ignore
clipasm render main.clipasm
```

For the phase boundaries, read
[Compilation, preflight, and rendering](../concepts/pipeline.md). See the
[language reference](../reference/language/index.md) for exact language behavior and
the [examples catalog](../examples.md) for more programs to validate.
