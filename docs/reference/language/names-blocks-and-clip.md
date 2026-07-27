# Names, blocks, and `clip`

## References and output names

```clipasm
image("title.png", 2s) as title
$title
```

Output names are immutable, program-wide, and unique. Names declared in nested
bodies remain available throughout the containing source program. Forward
references are allowed when dependencies can be resolved; cycles are diagnosed.
Body-input aliases such as `$before`, `$after`, and `$timeline` temporarily
shadow program-wide names while their body is active.

Scalar aliases are different: they are lexical to one program body, inherit
visible aliases from enclosing bodies, and never escape to a parent or sibling
body. They still may not collide with program inputs, parameters, or output
names.

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
