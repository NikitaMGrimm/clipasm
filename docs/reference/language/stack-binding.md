# Stack binding

`@owned` and `@visible` control stack access. Direct programs default to owned
access. `join` and `during` default to visible access. An enclosing owned
boundary cannot be pierced by an inner visible invocation.

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
