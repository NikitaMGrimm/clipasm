# Stack values, ownership, and visibility

Most ClipAsm calls can take their Video or Audio inputs from values produced by
preceding statements. This is the stack model.

## Values stay immutable

A Video or Audio value is an immutable result. Placing the same named value on
the stack more than once creates several usable occurrences; it does not copy or
change the underlying result.

```clipasm
image("title.png", 1s) as title
$title
$title
concat
```

The two references can both be consumed by `concat`.

## Omitted inputs come from the stack

```clipasm
image("before.png", 2s)
image("after.png", 2s)
crossfade(500ms)
```

`crossfade` needs `before` and `after`. It binds the nearest matching Video to
its last input first, so the second image becomes `after` and the first becomes
`before`.

Values of another type remain in place. A generic variadic program such as
`concat` consumes all accessible values of the selected type in their physical
order.

Explicit arguments do not consume occurrences from the caller's stack:

```clipasm
set_audio(
    video=video("picture.mp4"),
    audio=audio("sound.wav"),
)
```

## Bodies own the values they create

`@owned` restricts a call to values created by the current body. `@visible`
allows it to also reach values from enclosing bodies until an owned boundary is
encountered.

Most direct programs and imported programs default to owned access. `join` and
`during` default to visible access because their bodies commonly work with the
inputs those programs provide.

A plain `{ ... }` stack block can expose enclosing values to explicitly visible
calls inside it. `@owned { ... }` creates a boundary.

## Names read values; they do not consume them

`as name` attaches an immutable name to an output. `$name` places a readable
occurrence of that value where the reference appears. Naming and referencing do
not remove another stack occurrence.

Graph names are unique within one source-program invocation. Names created in a
nested body remain available in that invocation. Body-input names such as
`$before`, `$after`, and `$timeline` are temporary exceptions scoped to the
active body.

Scalar aliases are separate from graph names and do not affect the media stack.

See [Stack binding](../reference/language/stack-binding.md) for exact input and
visibility rules and [Composition forms](../reference/language/composition-forms.md)
for blocks, `clip`, names, and references.
