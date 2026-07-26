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
    zoom_in(8%)
} as title
```

This is invalid because the two statements have no separator:

```clipasm
clip { image("title.png", 2s) zoom_in(8%) }
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
    audio {
        sample_rate = 48000
    }
    output = "generated/final.mp4"
}

input source: Video
param title: File = "assets/title.png"
param duration: Duration = 2s
param amount: Number = 8%
param count: Integer
param range: TimeRange
param fit: Keyword(cover, contain, stretch) = contain
```

Graph input types are `Video` and `Audio`. Scalar parameter types are `Number`,
`Integer`, `File`, `Duration`, `TimeRange`, and a declared `Keyword(...)` set.
Parameters without defaults are required when another program or the CLI calls
the source program.

Only the root file may set project media configuration or an output path.
Current limits: project audio is stereo, and publication is MP4 only.

## Scalar expressions

Number values are exact reduced rationals. Integer literals, decimal literals,
percentages, arithmetic, and scalar parameter references never pass through
binary floating point:

```clipasm
param by: Number = 8%
param count: Integer = 6 / 2

image("title.png", 1s)
zoom_in($by)
repeat($count)
```

Operators use this precedence from loosest to tightest:

1. the TimeRange operator `..`
2. addition and subtraction
3. multiplication and division
4. unary `+` and `-`
5. postfix `%`, `ms`, and `s`
6. primary values and parenthesized expressions

Postfix operators may repeat. `%` divides a Number by 100, so `800%%`, `8%`,
`0.08`, and `2 / 25` are the same exact value and have the same semantic
identity.

Integer is the refinement of Number whose exact reduced denominator is one.
Constraints apply after evaluation:

```clipasm
repeat(6 / 2) # valid: evaluates to Integer 3
repeat(5 / 2) # error: evaluates to 2.5, exactly 5/2
```

`ms` and `s` require an Integer result and construct Duration. They bind to the
immediately preceding expression:

```clipasm
image("short.png", (6 / 2)ms) # 3ms
image("bad.png", (5 / 2)ms)   # error: ms requires Integer
image("bad.png", 5 / 2ms)     # error: Number / Duration is undefined
```

Duration is distinct from Number. It supports unary signs, addition with
Duration, and subtraction of Duration:

```clipasm
image("long.png", 100s - 100ms)
during((1s + 500ms)..3s) { repeat(2) }
```

Duration parameters must ultimately be nonnegative and exactly representable
on ClipAsm's nanosecond authoring grid. See the
[normative grammar](language-grammar.md#scalar-expressions) for the complete
syntax.

Immutable scalar aliases name inferred scalar expressions without adding a
value to the media stack:

```clipasm
length = 500ms
count = 6 / 2

image("card.png", $length)
repeat($count)
```

Aliases may refer forward to other aliases. Their expressions are checked and
evaluated only when an actual scalar use reaches them, including transitive
references. An unused alias may contain an unknown reference, invalid operator,
division by zero, dependency cycle, mixed timeline roots, or an invalid value
without causing an error. The same problem is diagnosed when a parameter use
reaches that alias. Declaration syntax and duplicate names remain eager.
Parameters, graph values, inputs, and aliases share one program-wide name
namespace.

Composed Video timelines expose frame-native placement markers. Explicit names
on the values that reach a clip's final concatenation become placement names:

```clipasm
clip {
    image("title.png", 1s) as intro
    image("credits.png", 2s) as credits
} as edit

$edit
during($edit::credits) {
    zoom_in(2%)
}
```

Selector paths may be nested, such as
`$edit::chapter::interview::start`. A placement selector without a final
boundary denotes its complete closed-open range. `::start`, `::middle`, and
`::end` select exact coordinates. A uniquely placed bare reference contributes
its reference name implicitly, and identity-preserving programs such as
`zoom_in` retain that marker. When an operation has already bound its timeline,
a selector may omit leading ancestors when the remaining suffix identifies one
addressable descendant. For example, `$interview::start` or
`$chapter::interview::start` may stand for a longer path under the bound root.
Multiple matches are ambiguous and require more leading names or the owning
timeline. Explicitly rooted selectors remain exact paths.

Selector structure follows names rather than operation history. Anonymous
composition layers are transparent, so these forms expose the same direct
`a` and `b` placements:

```clipasm
image("a.png", 1s) as a
image("b.png", 1s) as b
concat
concat as edit
```

```clipasm
image("a.png", 1s) as a
image("b.png", 1s) as b
join { concat } as edit
```

Both accept `$edit::a` and `$edit::b`. Anonymous one-input concatenation,
stack-block boundaries, and associative regrouping do not add path components.
Naming an occurrence does create a boundary:

```clipasm
image("a.png", 1s) as a
image("b.png", 1s) as b
concat as pair
image("c.png", 1s) as c
concat as edit

trim(value=$edit, range=$edit::pair::a)
```

Here `$edit::a` is invalid because the authored `pair` boundary cannot be
skipped. A name continues to denote the exact view captured at its declaration,
even when later anonymous composition wraps that occurrence.

At one parent level, a placement spelling is addressable only when exactly one
occurrence has that spelling. Explicit `as` labels, inferred bare-reference
labels, and operation-created labels do not shadow one another. Any duplicate
spelling is ambiguous and needs a distinct explicit name.

Marker ranges must be used with the timeline that owns their root. `join`
preserves the exact views of untouched inputs and exposes named values created
by its body as placements in the joined result. Both
`during` and Video `trim` accept rooted marker ranges. Video `trim` preserves
child placements only when their complete closed-open region is provably inside
the selected range, rebasing their starts to the trimmed timeline. Partially
surviving or symbolically uncertain placements are omitted. A trimmed occurrence
keeps its own placement label when it is later inserted into a clip.

`during` splices timeline layouts as well as media. Base placements fully before
the replaced range keep their coordinates. Placements fully after it shift by
the replacement-duration delta. Placements that intersect the replaced range,
or whose side cannot be proven symbolically, are omitted. The inserted body is
available as `::replacement`, retaining its nested layout. That spelling is
reserved by the `during` result contract: if a base placement named
`replacement` survives the edit, compilation reports
`E_TIMELINE_PLACEMENT_CONFLICT` instead of shadowing either occurrence. A base
placement with that name is permitted when the selected range removes it.

Transitions expose operation-owned regions. `flash_cut` provides sequential
`::before` and `::after` regions. `crossfade` provides `::before`, `::after`,
and the shared `::overlap` region:

```clipasm
image("before.png", 2s)
image("after.png", 2s)
crossfade(500ms) as transition

trim(range=$transition::overlap)
```

The `before` and `after` regions retain their nested placement layouts, so a
path such as `$transition::before::title` remains addressable. Their ranges
overlap in a crossfade and remain sequential in a flash cut. All normal
boundaries, including `::middle`, apply to these regions.

Timeline coordinates use exact rational arithmetic. Coordinates with the same
root may be added or subtracted, Number may scale them, and Duration may offset
them:

```clipasm
during(
    50% * ($edit::intro::start + $edit::credits::start)
        ..($edit::credits::end - 500ms)
) {
    zoom_in(2%)
}
```

Intermediate coordinates may be negative or beyond the owning timeline. Exact
frame alignment, ordering, and final bounds are checked only when the expression
is consumed as a TimeRange. `::middle` is therefore valid as an exact rational
coordinate even when it falls between frames, but using that value as a frame
boundary reports an alignment error. Video `trim` retains marker expressions
whose boundaries depend on unprobed media and resolves them during preflight
after the referenced source domains are known. The prepared operation still
contains an ordinary exact frame range. `during` uses the same deferred range
model. Its body receives the selected extent symbolically, so an `image`
without an explicit duration inherits that media-dependent extent and is
resolved to a concrete frame count during preflight.

Aliases make long marker expressions reusable:

```clipasm
credits_lead_in = $edit::credits::start - 500ms
credits_end = $edit::credits::end

during($credits_lead_in..$credits_end) {
    zoom_in(2%)
}
```

Timeline selectors inside aliases require their explicit root. Contextual
suffix lookup such as `$interview::start` or `$chapter::interview::start` is
available directly in a timeline-anchored call, but an alias should use the
complete rooted path so its meaning is independent of later uses.

Timeline selector diagnostics print the compiler's actual rooted occurrence
layout. Each child includes its root-relative closed-open range. The tree is the
canonical selector layout, not a record of anonymous operation wrappers.
Genuinely unnamed leaves and ambiguous labels are marked as not directly
addressable. Mixed-root arithmetic shows both roots, and a marker range used
with the wrong input shows the marker root beside the bound input layouts.
Diagnostic trees are capped at 64 occurrences and 12 nesting levels.

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
    executable = "python3"
    arguments = [file("brighten.py")]
    semantic_version = 1
    preserve = video
}
```

`executable` resolves relative to this source file when it contains a path, or
through the platform command lookup for a bare name. `arguments` is an ordered
list of literal strings and `file("...")` values. File arguments resolve from
this source file and are hashed during preflight. External protocol version 1
passes their resolved paths. Rendering rehashes declared files when the
external node is reached, but does not snapshot them or prevent a concurrent
change after that check. ClipAsm passes the executable and arguments separately
rather than constructing a shell command string; normal platform process
semantics still apply. `semantic_version` must be positive and is part of
semantic identity. `preserve` names the declared Video input whose exact
timeline domain and meaningful-audio state the single Video output preserves.

External programs currently accept fixed Video or Audio inputs and Integer,
File, or Keyword parameters. File values resolve from the source that supplied
them and are hashed during preflight. Native defaults are applied before
execution. An external program cannot also contain executable statements or
imports; use a separate ClipAsm wrapper program for composition. Compilation
remains pure. Preflight resolves and hashes the executable, and rendering sends
a versioned JSON request over standard input.

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
    zoom_in(2%)
}
operation as result
operation as (first, second)
```

At statement position, a zero-argument call may omit `()`, as in `concat` or
`operation as result`. Inside an argument expression, write `producer()`;
an unparenthesized identifier there is a scalar atom, not a program call.

For a construct that accepts a body, omitting braces means an empty body.
Empty call parentheses and empty bodies may be omitted independently, so these
are equivalent:

```clipasm
join
join()
join {}
join() {}
```

Normal input binding and body-output contracts still apply. For example, bare
`join` still requires two accessible matching timelines. `clip`, `clip()`,
`clip {}`, and `clip() {}` likewise share one empty expansion, whose generated
`concat` reports the ordinary missing-input error. Programs that do not accept
a caller-supplied body still reject braces.

`@owned` and `@visible` control stack access. Direct programs default to owned
access. `join` and `during` default to visible access. An enclosing owned
boundary cannot be pierced by an inner visible invocation.

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
flash_cut(
    image("before.png", 2s),
    image("after.png", 2s),
    160ms,
)
```

behaves like:

```clipasm
image("before.png", 2s)
image("after.png", 2s)
flash_cut(160ms)
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
    zoom_in(8%)
} as opening

$opening
```

It lowers in memory to a user-attributed stack block with a hidden owned
`concat`, followed by a hidden owned cleanup. Diagnostics and explain output
name the authored `clip`; generated helper operations are not exposed. The
generated block defaults to owned stack access; an explicit access modifier
applies to the block, while its `concat` and cleanup remain owned.

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
| `zoom_in` | `video: Video`, `by: Number?` | Video |
| `flash_cut` | `before: Video`, `after: Video`, `duration: Duration?` | Video |
| `crossfade` | `before: Video`, `after: Video`, `duration: Duration?` | Video |
| `extract_audio` | `video: Video` | Audio |
| `set_audio` | `video: Video`, `audio: Audio` | Video |
| `join` | `before: T`, `after: T`, body | T |
| `during` | `video: Video`, `range: TimeRange`, body | Video |

Defaults are: image/video fit `cover`, zoom_in `8%`, crossfade `500ms`, and
`flash_cut` `160ms`.

`zoom_in.by` is the exact final fractional increase in picture scale. For a
multi-frame Video, the first frame remains at 100%, and the scale increases
linearly across the complete Video to `100% + by` on the last frame. Thus
`zoom_in(8%)` ends at 108%; it does not mean 8% scale or an eight-times zoom.

`flash_cut.duration` and `crossfade.duration` both become the smallest whole
project-frame count covering the authored duration. A positive duration that
falls between frame boundaries therefore rounds up; `0ms` is invalid.

`image.duration` may be omitted only when the surrounding body supplies a
requested duration; otherwise it is required.

`join` starts with `$before` and `$after` and concatenates its homogeneous body
remainder. `during` starts its body stack with the selected range, exposes the
complete bound input as `$video`, requires exactly one processed Video, and
splices it back into the original Video.

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
