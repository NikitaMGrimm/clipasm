# Build the scenic sequence

In this tutorial, you will use an initialized project to understand one idea at
a time: project settings, image values, ordered stack statements, and `concat`.
You will predict an outcome, validate it, make one safe deliberate error, and
repair it. Create a fresh lesson project from the directory where you keep your
projects, then enter it:

```console,ignore
clipasm init scenic-video
cd scenic-video
```

Open `main.clipasm` in an editor. It is your ordinary project file, not a
managed template; ClipAsm will not rewrite it after initialization. The
[CLI reference](../reference/cli.md#init) defines the bundled starter's
compatibility and lifecycle.

## 1. Predict the project timeline

Read the configuration:

```clipasm
config {
    video {
        width = 320
        height = 180
        fps = 24
    }
    output = "generated/scenic-sequence.mp4"
}
```

**Predict:** this selects a 320x180 project at 24 frames per second and tells
rendering where to publish the MP4. It does not load an image.

Validate without changing the file:

```console,ignore
clipasm validate main.clipasm
```

**Observe:** validation succeeds with 108 frames. That confirms the complete
program has an authored duration; it has still not checked whether the image
files can be opened. The exact declaration rules are in
[configuration and declarations](../language-reference.md#configuration-and-declarations).

## 2. Predict the values on the stack

Now read the first three statements in the executable block:

```clipasm
image("assets/morning.png", 1500ms, contain)
image("assets/meadow.png", 1500ms, contain)
image("assets/evening.png", 1500ms, contain)
```

**Predict:** each `image` statement produces one 1.5-second Video. The paths
are relative to `main.clipasm`, and `contain` states how each image fits the
project frame. Three such scenes at 24 fps should account for 108 frames.

**Observe:** the previous validation result is that prediction: `3 × 1.5 × 24`
is 108. The [built-in program table](../language-reference.md#built-in-programs)
owns the exact `image` signature.

## 3. Join those values

The final statement is:

```clipasm
concat
```

**Predict:** `concat` consumes the accessible Video values in their statement
order and returns one combined Video. Rendering should show morning, meadow,
then evening.

Test a harmless diagnostic before rendering. Change only `concat` to
`concatt`, save, and validate:

```console,ignore
clipasm validate main.clipasm
```

**Observe:** validation fails at `concatt` because it is not a known program.
No media is opened and no output is written. Restore the spelling to `concat`,
save, and validate again:

```console,ignore
clipasm validate main.clipasm
```

**Observe:** it is valid again with 108 frames. This is a safe way to use a
source-location diagnostic while learning: repair the source before rendering.

## 4. Change one duration and render

Change the meadow duration, and nothing else, from `1500ms` to `1s`.

**Predict:** the sequence becomes four seconds, so validation should report 96
frames at 24 fps. Check it, then render:

```console,ignore
clipasm validate main.clipasm
clipasm render main.clipasm
```

**Observe:** validation reports 96 frames, and
`generated/scenic-sequence.mp4` has a shorter middle scene. You may restore
`1500ms` whenever you want the original 4.5-second sequence again.

## What you learned

You used project configuration, created Video values with `image`, relied on
statement order, and used `concat` to produce one output. You also used pure
validation to diagnose and repair a source error before rendering. Next, [build
a reusable composition](reusable-composition.md), or consult the
[language reference](../language-reference.md) for exact syntax and stack
behavior.
