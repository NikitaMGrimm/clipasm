#!/usr/bin/env python3
"""Prepare a ClipAsm release version without committing, tagging, or pushing."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import subprocess
import sys
import tomllib

VERSION_PATTERN = re.compile(r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$")
MANIFESTS = (Path("Cargo.toml"), Path("playground/Cargo.toml"))
EXPECTED_CHANGED_FILES = {"Cargo.toml", "Cargo.lock", "playground/Cargo.toml"}


def run(*command: str, capture: bool = False, cwd: Path | None = None) -> str:
    result = subprocess.run(
        command,
        check=True,
        cwd=cwd,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
    )
    return result.stdout.strip() if capture else ""


def parse_version(value: str) -> tuple[int, int, int]:
    match = VERSION_PATTERN.fullmatch(value)
    if match is None:
        raise ValueError("version must be an exact X.Y.Z semantic version")
    return tuple(int(part) for part in match.groups())  # type: ignore[return-value]


def manifest_version(path: Path) -> str:
    with path.open("rb") as manifest:
        return str(tomllib.load(manifest)["package"]["version"])


def replace_manifest_version(contents: str, old: str, new: str) -> str:
    needle = f'version = "{old}"'
    if contents.count(needle) != 1:
        raise ValueError(f"manifest must contain exactly one package version {old!r}")
    return contents.replace(needle, f'version = "{new}"', 1)


def require_clean_release_branch(root: Path, version: str) -> None:
    expected_branch = f"feat/release-{version.replace('.', '-')}"
    branch = run("git", "branch", "--show-current", capture=True, cwd=root)
    if branch != expected_branch:
        raise ValueError(
            f"release preparation requires branch {expected_branch!r}, found {branch!r}"
        )
    if run("git", "status", "--porcelain", capture=True, cwd=root):
        raise ValueError("release preparation requires a clean worktree")
    main = run("git", "rev-parse", "origin/main", capture=True, cwd=root)
    head = run("git", "rev-parse", "HEAD", capture=True, cwd=root)
    if head != main:
        raise ValueError(
            "release branch must start at the synchronized origin/main commit"
        )


def prepare(root: Path, version: str) -> None:
    target = parse_version(version)
    require_clean_release_branch(root, version)

    current_versions = {path: manifest_version(root / path) for path in MANIFESTS}
    if len(set(current_versions.values())) != 1:
        raise ValueError(f"workspace package versions disagree: {current_versions}")
    current_text = next(iter(current_versions.values()))
    current = parse_version(current_text)
    if target <= current:
        raise ValueError(f"new version {version} must be greater than current version {current_text}")
    if run("git", "tag", "--list", f"v{version}", capture=True, cwd=root):
        raise ValueError(f"tag v{version} already exists locally")

    originals = {path: (root / path).read_bytes() for path in (*MANIFESTS, Path("Cargo.lock"))}
    try:
        for path in MANIFESTS:
            manifest = root / path
            manifest.write_text(
                replace_manifest_version(manifest.read_text(encoding="utf-8"), current_text, version),
                encoding="utf-8",
            )
        run("cargo", "check", "--workspace", "--all-targets", cwd=root)
        run(
            "python3",
            "scripts/package_release.py",
            "verify",
            "--tag",
            f"v{version}",
            cwd=root,
        )
        changed = set(
            run("git", "diff", "--name-only", capture=True, cwd=root).splitlines()
        )
        if changed != EXPECTED_CHANGED_FILES:
            raise ValueError(
                f"release preparation changed unexpected files: {sorted(changed)}; "
                f"expected {sorted(EXPECTED_CHANGED_FILES)}"
            )
    except Exception:
        for path, contents in originals.items():
            (root / path).write_bytes(contents)
        raise

    print(f"prepared ClipAsm {version}")
    print(
        "review the three changed files, run ./scripts/check.sh, "
        "then commit and push the release branch"
    )
    print("create and push the annotated tag only after main CI succeeds")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version", help="new X.Y.Z package version")
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    try:
        prepare(root, arguments.version)
    except (OSError, subprocess.CalledProcessError, ValueError) as error:
        print(f"release preparation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
