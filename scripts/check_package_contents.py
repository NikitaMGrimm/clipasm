#!/usr/bin/env python3
"""Verify that files embedded by the CLI and its examples ship in the crate."""

from __future__ import annotations

from pathlib import Path
import subprocess
import sys


REQUIRED_PATHS = frozenset(
    {
        "src/cli/init.rs",
        "examples/assets/evening.png",
        "examples/assets/meadow.png",
        "examples/assets/morning.png",
        "examples/scenic-sequence.clipasm",
        "examples/starter/.gitignore",
        "examples/starter/README.md",
        "examples/programs/brighten/brighten.py",
    }
)


def repository_root() -> Path:
    return Path(__file__).resolve().parent.parent


def packaged_paths(root: Path) -> set[str]:
    try:
        result = subprocess.run(
            ["cargo", "package", "--list", "--locked", "--allow-dirty"],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
        )
    except FileNotFoundError:
        print("package contents check failed: cargo is not installed", file=sys.stderr)
        raise SystemExit(1) from None
    except subprocess.CalledProcessError as error:
        print("package contents check failed: cargo package --list failed", file=sys.stderr)
        if error.stderr:
            print(error.stderr, file=sys.stderr, end="" if error.stderr.endswith("\n") else "\n")
        raise SystemExit(error.returncode) from None
    return set(result.stdout.splitlines())


def main() -> int:
    missing = sorted(REQUIRED_PATHS - packaged_paths(repository_root()))
    if missing:
        print("package contents check failed: required paths are missing:", file=sys.stderr)
        for path in missing:
            print(f"- {path}", file=sys.stderr)
        return 1
    print("package contains initializer sources, assets, starter files, and external examples")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
