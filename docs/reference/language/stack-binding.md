# Stack binding

A call can receive Video or Audio values explicitly through arguments or
implicitly from the accessible stack.

## Scalar arguments

Positional scalar values bind parameters in declaration order. Named scalar
arguments use `=`:

```clipasm
image("title.png", duration=2s, fit=contain)
```

## Positional graph expressions

A graph-producing positional expression behaves like a preceding statement in
the current stack frame:

```clipasm
flash_cut(
    image("before.png", 2s),
    image("after.png", 2s),
    160ms,
)
```

is equivalent to:

```clipasm
image("before.png", 2s)
image("after.png", 2s)
flash_cut(160ms)
```

ClipAsm evaluates the expressions in source order.

## Implicit stack inputs

If a call omits graph inputs, ClipAsm selects accessible values by exact type.
For fixed inputs, it works from the program's last input to its first and takes
the nearest matching occurrence for each.

A variadic program such as `concat` consumes every accessible value of the
selected Video or Audio type in physical stack order. Values of another type
stay where they are.

Use `<Video>` or `<Audio>` when both generic choices are possible.

## Named graph inputs

ClipAsm evaluates a named graph input in an isolated input body:

```clipasm
set_audio(
    video=video("picture.mp4"),
    audio=audio("sound.wav"),
)
```

It supplies that input directly and does not consume a value from the caller's
stack. A named graph input must produce exactly one value of the required type.

Do not mix positional graph expressions and named graph inputs in one call.
Named scalar arguments may still accompany positional graph expressions.

## Ownership and visibility

`@owned` allows a call to consume only occurrences created by the current body.
`@visible` may also reach occurrences created by enclosing bodies, stopping at
the nearest owned boundary.

Most direct built-ins and imported programs default to owned access. `join` and
`during` default to visible access. The setting applies to one invocation and
does not automatically propagate into its body.

```clipasm
@owned {
    image("inside.png", 1s)
    @visible concat
}
```

The owned block prevents the inner visible call from reaching values outside the
block.

See [Stack values, ownership, and visibility](../../concepts/stack-values.md) for
an example-led explanation.
