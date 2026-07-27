# Statements and calls

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

`<Video>` or `<Audio>` constrains a generic invocation. Usually the compiler
infers the type; an explicit argument is useful for ambiguity or deliberate
selection.

See [Stack binding](stack-binding.md) for access modifiers and argument binding,
and the [built-in program reference](../programs/index.md) for exact call shapes
and body contracts.
