# Repository history

The chart below shows physical lines in tracked Rust files across the complete
`main` history. One line includes all Rust; the other excludes test-only code.
It is regenerated whenever `main` is pushed and is published with this book.

![Rust lines over main history, with and without tests](loc-history.svg)

The non-test series excludes every Rust file under a `tests/` directory and
items annotated with `#[cfg(test)]` or `#[test]` in other Rust files. This is a
repository-size trend, not a quality metric. Generated files, dependencies,
non-Rust source, documentation, and untracked local files are not counted.
