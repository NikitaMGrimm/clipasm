#!/usr/bin/env python3

import importlib.util
import sys
from pathlib import Path
import unittest

MODULE_PATH = Path(__file__).with_name("loc_history.py")
SPEC = importlib.util.spec_from_file_location("loc_history", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
loc_history = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = loc_history
SPEC.loader.exec_module(loc_history)


class RustLineCountTests(unittest.TestCase):
    def test_excludes_integration_test_files(self) -> None:
        source = b"fn helper() {}\n#[test]\nfn works() {}\n"
        self.assertEqual(loc_history.rust_line_counts("tests/example.rs", source), (3, 0))

    def test_excludes_inline_test_module_and_test_function(self) -> None:
        source = """fn production() {
    println!(\"#[cfg(test)] inside a string\");
}

#[cfg(test)]
mod tests {
    #[test]
    fn works() {
        assert!(true);
    }
}

#[test]
fn standalone_test() {}

fn more_production() {}
"""
        self.assertEqual(loc_history.non_test_line_count(source), 7)

    def test_excludes_test_only_field_without_hiding_neighbors(self) -> None:
        source = """struct State {
    live: bool,
    #[cfg(test)]
    fixture: bool,
    next: bool,
}
"""
        self.assertEqual(loc_history.non_test_line_count(source), 4)

    def test_comments_do_not_create_test_attributes(self) -> None:
        source = """// #[cfg(test)]
fn production() {}
/* #[test] fn fake() {} */
"""
        self.assertEqual(loc_history.non_test_line_count(source), 3)


if __name__ == "__main__":
    unittest.main()
