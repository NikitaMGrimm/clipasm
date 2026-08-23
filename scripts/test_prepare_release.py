#!/usr/bin/env python3

import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
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

    def test_requires_clean_release_branch_at_origin_main(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.initialize_repository(root)
            self.git(root, "switch", "-c", "feat/release-0-6-3")

            prepare_release.require_clean_release_branch(root, "0.6.3")

            (root / "README.md").write_text("changed\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "clean worktree"):
                prepare_release.require_clean_release_branch(root, "0.6.3")

    def test_rejects_wrong_or_advanced_release_branch(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.initialize_repository(root)

            with self.assertRaisesRegex(ValueError, "feat/release-0-6-3"):
                prepare_release.require_clean_release_branch(root, "0.6.3")

            self.git(root, "switch", "-c", "feat/release-0-6-3")
            (root / "README.md").write_text("advanced\n", encoding="utf-8")
            self.git(root, "add", "README.md")
            self.git(root, "commit", "-m", "advance release branch")
            with self.assertRaisesRegex(ValueError, "origin/main commit"):
                prepare_release.require_clean_release_branch(root, "0.6.3")

    @classmethod
    def initialize_repository(cls, root: Path) -> None:
        cls.git(root, "init", "--initial-branch=main")
        cls.git(root, "config", "user.email", "test@example.com")
        cls.git(root, "config", "user.name", "ClipAsm Test")
        (root / "README.md").write_text("initial\n", encoding="utf-8")
        cls.git(root, "add", "README.md")
        cls.git(root, "commit", "-m", "initial")
        cls.git(root, "update-ref", "refs/remotes/origin/main", "HEAD")

    @staticmethod
    def git(root: Path, *arguments: str) -> None:
        subprocess.run(
            ("git", *arguments),
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
        )


if __name__ == "__main__":
    unittest.main()
