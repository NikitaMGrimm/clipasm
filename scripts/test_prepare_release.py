#!/usr/bin/env python3

import importlib.util
from pathlib import Path
import sys
import unittest

MODULE_PATH = Path(__file__).with_name("prepare_release.py")
SPEC = importlib.util.spec_from_file_location("prepare_release", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
prepare_release = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = prepare_release
SPEC.loader.exec_module(prepare_release)


class PrepareReleaseTests(unittest.TestCase):
    def test_accepts_exact_semantic_versions(self) -> None:
        self.assertEqual(prepare_release.parse_version("0.3.1"), (0, 3, 1))
        self.assertEqual(prepare_release.parse_version("12.0.4"), (12, 0, 4))

    def test_rejects_noncanonical_versions(self) -> None:
        for value in ("v0.3.1", "0.3", "01.2.3", "0.3.1-rc.1", " 0.3.1"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                prepare_release.parse_version(value)

    def test_replaces_only_the_package_version(self) -> None:
        manifest = '[package]\nname = "clipasm"\nversion = "0.3.0"\n'
        self.assertEqual(
            prepare_release.replace_manifest_version(manifest, "0.3.0", "0.3.1"),
            '[package]\nname = "clipasm"\nversion = "0.3.1"\n',
        )

    def test_requires_one_matching_version(self) -> None:
        with self.assertRaises(ValueError):
            prepare_release.replace_manifest_version('[package]\nname = "clipasm"\n', "0.3.0", "0.3.1")


if __name__ == "__main__":
    unittest.main()
