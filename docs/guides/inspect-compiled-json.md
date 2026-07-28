# Inspect compiled JSON

Use `clipasm inspect` when you need to debug compiled graph structure or feed
ClipAsm's supported inspection format into another tool. It is not part of the
normal edit-and-render loop.

## Before you start

Start with a source file that passes `clipasm validate`. The examples below use
an initialized project's `main.clipasm`.

## 1. Inspect standard output

```console,ignore
clipasm inspect main.clipasm
```

The command writes compiled JSON to standard output without opening media,
running FFmpeg or FFprobe, or executing an external program.

## 2. Write a new JSON file

```console,ignore
mkdir -p local
clipasm inspect main.clipasm --output local/compiled.json
```

The destination must not already exist. Choose a new path or remove the old
debugging file before repeating the command; `inspect` never overwrites it.

## 3. Check the contract version

The document describes project settings, compiled operations and inputs, known
frame or sample counts, ordered outputs, named values, source origins, and the
configured publication path.

Inspection JSON is not `.clipasm` source, a render plan, or a preview. Read its
`format_version` before consuming it in software, and reject versions your tool
does not support.

See [Compiled inspection JSON](../reference/machine-contracts.md#compiled-inspection-json)
for the exact supported contract.
