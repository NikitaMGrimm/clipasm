---
status: accepted
---

# Evaluate scalar expressions exactly

ClipAsm represents authored Number values as arbitrary-precision reduced
rationals. Integer is a value refinement of Number: its reduced denominator
must be one, and the executable Integer parameter ABI additionally requires the
value to fit its signed integer representation. Decimal input never passes
through binary floating point.

The native parser owns precedence and produces a source-located scalar
expression tree. It does not evaluate arithmetic or inspect the implementation
of exact numbers. Checked-source construction resolves scalar parameter
references and validates operator types. Invocation evaluation then reduces the
checked expression exactly before applying the destination parameter
constraint.

Postfix `%` is ordinary division by 100 and may repeat. Postfix `ms` and `s`
require an Integer-valued Number and construct an exact Duration. Duration is a
distinct scalar type supporting unary signs and addition or subtraction with
another Duration. `..` constructs a TimeRange from two Duration expressions.
The existing media model receives a Duration only after the exact expression
has been checked as nonnegative and exactly representable in its nanosecond
authoring grid.

Semantic and prepared identities store reduced rational values rather than
authored spellings. Equivalent expressions such as `8%`, `0.08`, and `2 / 25`
therefore share identity. The `zoom_in` built-in accepts `by: Number`, defaults
to `8%`, and stores the exact fractional increase.

## Consequences

- Number arithmetic and identity are deterministic across native and browser
  hosts.
- Integer constraints observe evaluated values, so `6 / 2` satisfies Integer
  while `5 / 2` reports `2.5` and the exact value `5/2`.
- Unit suffixes and arithmetic compose through precedence instead of
  program-specific parsing or hardcoded expression patterns.
- The parser, exact evaluator, scalar type checker, and parameter binder remain
  separate phase owners.
- Adding arithmetic for another scalar quantity requires explicit typed
  operator definitions rather than treating every scalar as Number.
