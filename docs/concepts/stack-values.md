# Stack ownership and visibility

The learning chapters show the everyday rule: a call with omitted Video or
Audio inputs consumes matching values produced nearby. This page explains what
happens when bodies and nested compositions create more than one stack frame.

## Values and occurrences are different

A Video or Audio value is an immutable graph result. A statement places an
occurrence of that value on a stack. A program consumes occurrences and returns
new values.

Referencing a name creates another usable occurrence without copying or moving
the underlying graph:

```clipasm
image("title.png", 1s) as title
$title
$title
concat
```

`concat` can consume both references. The original named value
continues to identify the same immutable result.

## Bodies create ownership boundaries

Each source-program invocation and program body owns the occurrences created
directly in its stack frame. Ownership prevents an inner operation from
accidentally consuming unrelated values created by a caller or enclosing body.

Most direct built-ins and imported programs use **owned** access: omitted inputs
may come only from the current owned frame. `join` and `during` use **visible**
access because their bodies commonly need the values those programs provide.

## Explicit access is local

`@owned` restricts one block or call to the current ownership frame. `@visible`
allows one call to search enclosing visible frames until it reaches an owned
boundary:

```clipasm
@owned {
    image("inside.png", 1s)
    @visible concat
}
```

The owned block stops the visible `concat` from reaching Videos outside the
block. Access annotations apply only to the form that they prefix. They do not
change every nested operation.

Use explicit access only when a nested composition genuinely needs different
visibility. Ordinary linear compositions should rely on each program's
documented default.

## Names do not create lexical graph scope

Stack ownership and name visibility are separate. A graph name created in a
nested body remains available throughout the containing source-program
invocation. Temporary body-input names such as `$before`, `$after`, and
`$timeline` exist only while that body is active.

A bare `{ ... }` stack block groups work. It returns every child-stack value
left inside it. The block is not a lexical name scope.

`clip { ... }` combines one timeline type and removes its temporary outer
occurrence. An optional name remains available for later references.

See [Stack binding](../reference/language/stack-binding.md) for exact selection
rules and [Composition forms](../reference/language/composition-forms.md) for
`clip`, stack blocks, names, and references.
