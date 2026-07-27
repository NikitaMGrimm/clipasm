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
of exact numbers. Every program body defines one lexical scalar-alias scope.
Aliases in that scope are predeclared for forward references, may capture body
inputs, inherit visible aliases from enclosing bodies, and do not escape to a
parent or sibling body. Shadowing a visible alias is rejected.

Checked-source construction eagerly resolves every alias reference, infers its
scalar kind, validates operator types, and diagnoses dependency cycles. The
result is a dense table of fully checked expressions addressed by compact alias
identities. Invocation evaluation reduces an alias exactly only when a scalar
parameter reaches it, then applies the destination parameter constraint. Scalar
aliases have no stack effect. Value-dependent failures such as division by zero,
mixed timeline roots, bounds, alignment, and destination constraints therefore
remain use-time checks rather than declaration-time checks.

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
- Scalar aliases may refer forward within one body and may capture lexical body
  inputs. Parent aliases are visible in descendants, nested aliases do not
  escape, and sibling bodies may reuse a name.
- Alias references, scalar kinds, operator types, and dependency cycles are
  checked eagerly, while exact value evaluation remains demand-driven and uses
  the same exact evaluator and parameter ABI.
- Timeline coordinates normalize to linear exact expressions. Media-dependent
  Video or Audio extents may remain symbolic through compilation and are
  substituted by preflight before final native-grid alignment and bounds
  validation.
- Adding arithmetic for another scalar quantity requires explicit typed
  operator definitions rather than treating every scalar as Number.
