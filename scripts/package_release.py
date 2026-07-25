#!/usr/bin/env python3
"""Validate release tags and build deterministic ClipAsm binary archives."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
from pathlib import Path
import subprocess
import sys
import tarfile
import tomllib
import zipfile


def repository_root() -> Path:
    return Path(__file__).resolve().parent.parent


def package_version(root: Path) -> str:
    with (root / "Cargo.toml").open("rb") as manifest:
        document = tomllib.load(manifest)
    return str(document["package"]["version"])


def validate_tag(tag: str, version: str) -> None:
    expected = f"v{version}"
    if tag != expected:
        raise ValueError(f"release tag {tag!r} must exactly match {expected!r}")


def rust_host() -> str:
    output = subprocess.run(
        ["rustc", "-vV"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    for line in output.splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ")
    raise RuntimeError("rustc did not report a host target")


def archive_entries(root: Path, binary: Path, archive_root: str) -> list[tuple[str, bytes, int]]:
    binary_name = "clipasm.exe" if binary.suffix.casefold() == ".exe" else "clipasm"
    return [
        (f"{archive_root}/{binary_name}", binary.read_bytes(), 0o755),
        (f"{archive_root}/README.md", (root / "README.md").read_bytes(), 0o644),
        (f"{archive_root}/LICENSE", (root / "LICENSE").read_bytes(), 0o644),
    ]


def write_tar_gz(path: Path, entries: list[tuple[str, bytes, int]]) -> None:
    with path.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w") as archive:
                for name, contents, mode in entries:
                    info = tarfile.TarInfo(name)
                    info.size = len(contents)
                    info.mode = mode
                    info.mtime = 0
                    info.uid = 0
                    info.gid = 0
                    info.uname = ""
                    info.gname = ""
                    archive.addfile(info, io.BytesIO(contents))


def write_zip(path: Path, entries: list[tuple[str, bytes, int]]) -> None:
    with zipfile.ZipFile(
        path,
        mode="w",
        compression=zipfile.ZIP_DEFLATED,
        compresslevel=9,
    ) as archive:
        for name, contents, mode in entries:
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = mode << 16
            archive.writestr(info, contents)


def package_binary(
    root: Path,
    binary: Path,
    target: str,
    version: str,
    output_directory: Path,
) -> tuple[Path, Path]:
    if not binary.is_file():
        raise ValueError(f"release binary does not exist: {binary}")
    host = rust_host()
    if host != target:
        raise ValueError(f"release target {target!r} does not match rustc host {host!r}")

    archive_root = f"clipasm-{version}-{target}"
    output_directory.mkdir(parents=True, exist_ok=True)
    entries = archive_entries(root, binary, archive_root)
    if "windows" in target:
        archive = output_directory / f"{archive_root}.zip"
        write_zip(archive, entries)
    else:
        archive = output_directory / f"{archive_root}.tar.gz"
        write_tar_gz(archive, entries)

    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    checksum = archive.with_name(f"{archive.name}.sha256")
    checksum.write_text(f"{digest}  {archive.name}\n", encoding="ascii")
    return archive, checksum


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__)
    commands = root.add_subparsers(dest="command", required=True)

    verify = commands.add_parser("verify", help="verify a tag against Cargo.toml")
    verify.add_argument("--tag", required=True)

    package = commands.add_parser("package", help="package one native release binary")
    package.add_argument("--tag", required=True)
    package.add_argument("--target", required=True)
    package.add_argument("--binary", type=Path, required=True)
    package.add_argument("--output-dir", type=Path, default=Path("dist"))
    return root


def main() -> int:
    arguments = parser().parse_args()
    root = repository_root()
    version = package_version(root)
    try:
        validate_tag(arguments.tag, version)
        if arguments.command == "verify":
            print(f"release tag {arguments.tag} matches package version {version}")
            return 0
        archive, checksum = package_binary(
            root,
            arguments.binary,
            arguments.target,
            version,
            arguments.output_dir,
        )
    except (OSError, RuntimeError, subprocess.CalledProcessError, ValueError) as error:
        print(f"release packaging failed: {error}", file=sys.stderr)
        return 1
    print(archive)
    print(checksum)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
