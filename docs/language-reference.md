# ClipAsm language reference

A source file uses the `.clipasm` extension and begins with:

```clipasm
clipasm 1
```

Declarations come next. Executable statements begin after the declarations;
declarations cannot appear later in the file.

## Layout

Spaces, tabs, and indentation are ignored. Newlines separate statements and
configuration fields. There is no semicolon syntax.

A block containing one statement may fit on one line:

```clipasm
clip { image("title.png", 2s) } as title
```

Multiple statements require newlines:

```clipasm
clip {
    image("title.png", 2s)
    zoom(8)
} as title
```

This is invalid because the two statements have no separator:

```clipasm
clip { image("title.png", 2s) zoom(8) }
```

Newlines are allowed inside parentheses around comma-separated arguments.
Comments begin with `#` and continue to the end of the line.

## Configuration and declarations

```clipasm
clipasm 1

config {
    video {
        width = 1920
        height = 1080
        fps = 30000/1001
    }
    output = "generated/final.mp4"
}

input source: Video
param title: File = "assets/title.png"
param duration: Duration = 2s
param count: Integer
param range: TimeRange
param fit: Keyword(cover, contain, stretch) = contain
```

Graph input types are `Video` and `Audio`. Scalar parameter types are
`Integer`, `File`, `Duration`, `TimeRange`, and a declared `Keyword(...)` set.
Parameters without defaults are required when another program or the CLI calls
the source program.

Only the root file may set project video configuration or an output path.

## Imports

```clipasm
import "programs/polish.clipasm" as polish
import "programs/brighten.clipasm" as brighten
```

Aliases are required. Paths resolve relative to the declaring source file.
Imported source files are ordinary callable programs with isolated local stacks
and names. Import cycles are errors. Callers use the same import syntax whether
the imported program is implemented in ClipAsm or by an external executable.

## External implementations

A source file may replace its executable ClipAsm body with one external
implementation:

```clipasm
clipasm 1

input video: Video
param amount: Integer = 15

external {
    command = "./brighten.py"
    semantic_version = 1
    preserve = video
}
```

`command` is executed directly without a shell and resolves relative to this
source file when it contains a path. `semantic_version` must be positive and is
part of semantic identity. `preserve` names the declared Video input whose exact
timeline domain and meaningful-audio state the single Video output preserves.

External programs currently accept fixed Video or Audio inputs and Integer,
File, or Keyword parameters. File values resolve from the source that supplied
them and are content-hashed during preflight. Native defaults are applied before execution. An external
program cannot also contain executable statements or imports; use a separate
ClipAsm wrapper program for composition. Compilation remains pure. Preflight
resolves and hashes the executable, and rendering sends a versioned JSON request
over standard input.

## Statements

The general statement shape is:

```text
@access name<Type>(arguments) { body } as output
```

Each part is optional when the target permits it. Examples:

```clipasm
image("title.png", 2s, contain)
concat<Audio>
@visible repeat(2)
during(1s..3s) {
    wobble(2)
}
operation as result
operation as (first, second)
```

`@owned` and `@visible` control stack access. Direct programs default to owned
access. `join`, `glue`, and `during` default to visible access. An enclosing
owned boundary cannot be pierced by an inner visible invocation.

`<Video>` or `<Audio>` constrains a generic invocation. Usually the compiler
infers the type; an explicit argument is useful for ambiguity or deliberate
selection.

## Arguments and stack binding

Scalar positional arguments bind scalar parameters in declaration order.
Named scalar arguments use `=`:

```clipasm
image("title.png", duration=2s, fit=contain)
```

Graph-valued positional expressions are evaluated as preceding statements in
the current stack frame, preserving source order:

```clipasm
flash(
    image("before.png", 2s),
    image("after.png", 2s),
    4,
)
```

behaves like:

```clipasm
image("before.png", 2s)
image("after.png", 2s)
flash(4)
```

Omitted graph inputs bind from the accessible stack by type. Fixed inputs bind
from the nearest compatible values, working from the last port to the first.
Variadic programs such as `concat` consume all accessible values of the chosen
type.

Named graph inputs are isolated explicit input bodies:

```clipasm
set_audio(
    video=video("picture.mp4"),
    audio=audio("sound.wav"),
)
```

A positional graph expression and a named graph input cannot be mixed in the
same call. Named scalar arguments may still accompany positional graph
expressions.

## References and output names

```clipasm
image("title.png", 2s) as title
$title
```

Output names are immutable, program-wide, and unique. Names declared in nested
bodies remain available throughout the containing source program. Forward
references are allowed when dependencies can be resolved; cycles are diagnosed.
Body-input aliases such as `$before`, `$after`, and `$video` temporarily shadow
program-wide names while their body is active.

`as name` requires one output. `as (first, second)` names an exact ordered
multi-output result. Naming does not remove a value from the stack.

## Stack blocks

A structural block creates a child stack frame and returns every remaining value
owned by that frame, in order:

```clipasm
{
    video("picture.mp4")
    audio("sound.wav")
} as (picture, sound)
```

A plain block is visible, but ordinary programs inside still default to owned
access and therefore cannot consume older outer values. An inner operation that
deliberately uses `@visible` may reach outward without also annotating the block.
Use `@owned { ... }` when the block itself must establish a visibility boundary.
A stack block is not a lexical name scope.

## `clip` sugar

`clip` builds one homogeneous Video or Audio value and removes its initial stack
occurrence while preserving an optional name:

```clipasm
clip {
    image("title.png", 2s)
    zoom(8)
} as opening

$opening
```

It lowers in memory to a visible user operation backed by `glue`, followed by a
hidden owned cleanup. Diagnostics and explain output name the authored `clip`;
generated helper names are not exposed. An explicit access modifier applies to
the generated `glue`, while cleanup remains owned.

## Built-in programs

`?` marks an optional scalar parameter. Generic programs operate on Video or
Audio and may use `<Video>` or `<Audio>`.

| Program | Inputs and parameters | Output |
| --- | --- | --- |
| `image` | `path: File`, `duration: Duration?`, `fit: cover|contain|stretch?` | Video |
| `video` | `path: File`, `fit?` | Video |
| `audio` | `path: File` | Audio |
| `trim` | `value: T`, `range: TimeRange` | T |
| `repeat` | `value: T`, `count: Integer` | T |
| `concat` | `values: T...` | T |
| `drop` | `value: T` | none |
| `zoom` | `video: Video`, `percent: Integer?` | Video |
| `wobble` | `video: Video`, `pixels: Integer?` | Video |
| `flash` | `before: Video`, `after: Video`, `frames: Integer?` | Video |
| `crossfade` | `before: Video`, `after: Video`, `duration: Duration?` | Video |
| `extract_audio` | `video: Video` | Audio |
| `set_audio` | `video: Video`, `audio: Audio` | Video |
| `join` | `before: T`, `after: T`, body | T |
| `glue` | body | T |
| `during` | `video: Video`, `range: TimeRange`, body | Video |

Defaults are: image/video fit `cover`, zoom `8`, wobble `3`, crossfade `500ms`,
and flash the smallest frame count covering `160ms`.

`image.duration` may be omitted only when the surrounding body supplies a
requested duration; otherwise it is required.

`glue` starts its body empty and concatenates the homogeneous body remainder.
`join` starts with `$before` and `$after` and concatenates its homogeneous body
remainder. `during` starts with the selected range as `$video`, requires exactly
one processed Video, and splices it back into the original Video.

## CLI bindings

```console
clipasm validate template.clipasm \
  --video-input source=footage.mp4 \
  --arg range=1s..3s \
  --arg count=2

clipasm render template.clipasm \
  --video-input source=footage.mp4 \
  --arg range=1s..3s \
  --arg count=2 \
  --output final.mp4
```

CLI paths resolve from the caller's working directory. Authored paths resolve
from the source file that contains them.
