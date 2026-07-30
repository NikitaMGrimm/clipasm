# Statements and calls

A statement calls a program, reads a named value, creates a scalar alias, or
runs a structural block. This page covers program-call syntax.

## Call shape

The complete statement form is:

```text
@access name<Type>(arguments) { body } as output
```

Each part is optional only when the target allows it:

```clipasm
image("title.png", 2s, contain)
concat<Audio>
@visible repeat(2)
during(1s..3s) {
    zoom_in(2%)
}
operation as result
operation as (first, second)
operation as (first, _, third)
```

- `@owned` or `@visible` chooses stack access.
- `<Video>` or `<Audio>` selects a generic type.
- `(arguments)` supplies graph or scalar inputs.
- `{ body }` supplies a body to a body program or language form.
- `as output` binds one or several output positions.

## Output bindings

An output name creates a graph reference and a timeline placement label for its
position:

```clipasm
operation as result
operation as (first, second)
```

Use `_` to leave a position unnamed:

```clipasm
operation as (first, _, third)
```

The wildcard still occupies an output position, so binding arity remains exact.
It creates no `$_` reference and no `_` timeline placement. It also does not
discard the value: the unnamed output remains on the stack for later calls.
Multiple `_` slots are allowed. For a single-output statement, `as _` is the
explicit wildcard form and has the same stack effect as omitting `as`.

## Omitting empty syntax

At statement position, a zero-argument call may omit `()`:

```clipasm
concat
repeat(2)
```

Inside an argument expression, a program call must keep its parentheses:

```clipasm
set_audio(video=video("picture.mp4"), audio=audio("sound.wav"))
```

An unparenthesized identifier inside an argument is a scalar atom, not a call.

For a construct that accepts a body, omitted braces mean an empty body. These
forms are equivalent:

```clipasm
join
join()
join {}
join() {}
```

Normal input and body-output requirements still apply. Bare `join` still needs
two matching timelines. A direct program that does not accept a body rejects
braces.

## Generic type selection

The compiler usually infers whether a generic call uses Video or Audio. Write an
explicit type when both are accessible or when deliberate selection improves
clarity:

```clipasm
concat<Video>
drop<Audio>
```

See [Stack binding](stack-binding.md) for arguments and access modifiers. See
[Composition forms](composition-forms.md) for `clip`, blocks, and names. See
[Built-in programs](../programs/index.md) for exact call shapes.
