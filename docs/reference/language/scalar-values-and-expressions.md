# Scalar values and expressions

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
[normative grammar](../../language-grammar.md#scalar-expressions) for the
complete syntax.

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

Every alias is structurally checked when its body is compiled: references must
resolve, operators must type-check, and dependency cycles are rejected even when
the alias is unused. Exact evaluation still happens only when an actual scalar
use reaches the alias. Unused division by zero, mixed timeline roots,
out-of-bounds coordinates, and destination parameter failures therefore remain
inert until use. Timeline selectors in aliases may capture lexical body inputs,
but do not borrow a contextual timeline root from a later invocation.

See [Timeline selectors and ranges](timeline-selectors.md) for placement
selectors, timeline coordinates, and marker arithmetic.
