# Scalar values and expressions

ClipAsm evaluates numbers and durations exactly rather than with binary
floating point.

## Numbers and integers

Number values are reduced rational values. Integer literals, decimals,
percentages, arithmetic, and scalar references remain exact:

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
5. postfix `%`, `ms`, `s`, and `f`
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

## Durations

`ms`, `s`, and `f` require an Integer result and construct Duration. They bind
to the immediately preceding expression:

```clipasm
image("short.png", (6 / 2)ms) # 3ms
image("card.png", 15f)         # exactly 15 project video frames
image("bad.png", (5 / 2)ms)   # error: ms requires Integer
image("bad.png", 5 / 2ms)     # error: Number / Duration is undefined
```

`f` is resolved on the configured project video frame grid. It is useful for
machine-generated edits because boundaries remain exact even when one frame
cannot be represented on the nanosecond authoring grid:

```clipasm
config { video { fps = 30 } }

image("card.png", 15f)
trim(3f..15f)
flash_cut(3f)
```

For Video, a project-frame range is used directly. For Audio, each frame
boundary maps to the corresponding boundary on the configured project sample
grid. At 30 fps and 48 kHz, `3f..8f` therefore maps exactly to samples
`4800..12800`; cumulative boundaries do not drift.

Duration is distinct from Number. Both unit families support unary signs,
addition, and subtraction, but one expression cannot mix wall-clock and
project-frame values:

```clipasm
image("long.png", 100s - 100ms)
offset = -5f
image("exact.png", $offset + 20f)
during((1s + 500ms)..3s) { repeat(2) }
image("bad.png", 1s + 3f) # error: Duration families differ
```

Negative values are allowed as intermediate scalar results. At a program
parameter boundary, wall-clock Duration must be nonnegative and exactly
representable on ClipAsm's nanosecond authoring grid, while project-frame
Duration must be a nonnegative integer within the supported frame count. Both
endpoints of a range must use the same unit family.

Either family may offset a timeline coordinate. Project-frame offsets remain
on the frame grid until the final Video frame or Audio sample boundary is
resolved:

```clipasm
trim(
    range=($edit::start + 3f)..($edit::end - 3f),
)
```

See the
[normative grammar](../../language-grammar.md#scalar-expressions) for the
complete syntax.

## Scalar aliases

Immutable scalar aliases name inferred scalar expressions without adding a
value to the media stack:

```clipasm
length = 500ms
count = 6 / 2

image("card.png", $length)
repeat($count)
```

Each program body is a scalar scope. Aliases in that body may refer forward to
one another, and aliases from enclosing bodies remain visible. A nested alias
does not escape its body; sibling bodies may reuse the same name. Declaring an
alias that shadows a visible alias, or collides with a program input, parameter,
or named graph value, is an error.

### When aliases are checked

Every alias is structurally checked when its body is compiled: references must
resolve, operators must type-check, and dependency cycles are rejected even when
the alias is unused. Exact evaluation still happens only when an actual scalar
use reaches the alias. Unused division by zero, mixed timeline roots,
out-of-bounds coordinates, and destination parameter failures therefore remain
inert until use. Timeline selectors in aliases may capture lexical body inputs,
but do not borrow a contextual timeline root from a later invocation.

See [Timeline selectors and ranges](timeline-selectors.md) for placement
selectors, timeline coordinates, and marker arithmetic.
