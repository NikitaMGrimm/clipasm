# Composition forms

ClipAsm lets you compose work in three related ways:

- Callable programs create or transform values.
- A stack block groups statements and returns the values left by that group.
- `clip` is shorthand for building one reusable timeline value.

From an author's perspective, all three are composition tools. Only callable
programs have registered names and call signatures. The `clipasm programs`
command lists built-ins such as `image`, `concat`, and `during`. It does not
list `clip` or a bare `{ ... }` block.

## `clip`

Use `clip` when a group of statements should become one named Video or Audio:

```clipasm
clip {
    image("title.png", 2s)
    zoom_in(8%)
} as opening

$opening
```

The body must leave one or more values of one timeline type. ClipAsm concatenates
them in order, assigns the optional name to the result, then removes the temporary
outer-stack occurrence. The name remains available through `$opening`.

The equivalent explicit form is:

```clipasm
@owned {
    image("title.png", 2s)
    zoom_in(8%)
    @owned concat
} as opening
@owned drop
```

The compiler performs that expansion in memory. Diagnostics still refer to the
authored `clip`, not the generated `concat` or `drop`.

`clip` accepts no scalar arguments. Use a type argument such as `clip<Audio>`
when the compiler cannot infer the result type.

## Stack blocks

A stack block groups statements and returns every value produced by the block
that remains on its child stack, in order:

```clipasm
{
    video("picture.mp4")
    audio("sound.wav")
} as (picture, sound)
```

Unlike `clip`, a stack block does not combine the returned values and does not
remove them. It can therefore return zero, one, or several values.

A plain block permits explicitly visible operations inside it to reach outward.
Use `@owned { ... }` when the block must create an ownership boundary. Programs
inside a block still use their own default access rules.

A stack block is structural. It is not a callable program and does not create a
lexical scope for graph names.

## Names and references

```clipasm
image("title.png", 2s) as title
$title
```

`as name` requires exactly one output. `as (first, second)` names an exact
ordered multi-output result. Naming does not consume or move a value.

Graph names are immutable and unique within one source-program invocation.
Names created in nested bodies or blocks remain available in the containing
source program. The compiler permits forward references when it can resolve
their dependencies. Cycles are errors.

Body-input names such as `$before`, `$after`, and `$timeline` exist only while
that body is active. They temporarily shadow an outer graph name with the same
name. Scalar aliases follow separate lexical-scope rules.

## Choosing a form

Use a normal program call for one known operation. Use `clip` for a reusable
single timeline assembled from several statements. Use a stack block when you
need explicit grouping, multiple outputs, or precise ownership behavior.
