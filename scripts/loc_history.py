#!/usr/bin/env python3
"""Generate a dependency-free SVG chart of Rust lines over Git history."""

from __future__ import annotations

import argparse
import html
import io
import re
import subprocess
import tarfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath


@dataclass(frozen=True)
class Point:
    commit: str
    subject: str
    timestamp: str
    rust_lines: int
    non_test_rust_lines: int


def git(*args: str, binary: bool = False) -> bytes | str:
    result = subprocess.run(
        ["git", *args],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return result.stdout if binary else result.stdout.decode("utf-8")


def sanitized_rust(source: str) -> str:
    """Replace comments and literals with spaces while preserving newlines."""

    output = list(source)
    index = 0
    state = "code"
    block_depth = 0
    raw_hashes = 0
    while index < len(source):
        char = source[index]
        following = source[index + 1] if index + 1 < len(source) else ""

        if state == "code":
            if char == "/" and following == "/":
                output[index] = output[index + 1] = " "
                index += 2
                state = "line-comment"
                continue
            if char == "/" and following == "*":
                output[index] = output[index + 1] = " "
                index += 2
                state = "block-comment"
                block_depth = 1
                continue
            if char == '"':
                output[index] = " "
                index += 1
                state = "string"
                continue
            if char == "'" and following and following not in "_abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ":
                output[index] = " "
                index += 1
                state = "char"
                continue
            if char == "r":
                cursor = index + 1
                while cursor < len(source) and source[cursor] == "#":
                    cursor += 1
                if cursor < len(source) and source[cursor] == '"':
                    raw_hashes = cursor - index - 1
                    for replaced in range(index, cursor + 1):
                        output[replaced] = " "
                    index = cursor + 1
                    state = "raw-string"
                    continue
            index += 1
            continue

        if char != "\n":
            output[index] = " "

        if state == "line-comment":
            if char == "\n":
                state = "code"
            index += 1
            continue
        if state == "block-comment":
            if char == "/" and following == "*":
                output[index] = output[index + 1] = " "
                block_depth += 1
                index += 2
                continue
            if char == "*" and following == "/":
                output[index] = output[index + 1] = " "
                block_depth -= 1
                index += 2
                if block_depth == 0:
                    state = "code"
                continue
            index += 1
            continue
        if state in {"string", "char"}:
            if char == "\\" and index + 1 < len(source):
                if source[index + 1] != "\n":
                    output[index + 1] = " "
                index += 2
                continue
            terminator = '"' if state == "string" else "'"
            if char == terminator:
                state = "code"
            index += 1
            continue
        if state == "raw-string":
            terminator = '"' + "#" * raw_hashes
            if source.startswith(terminator, index):
                for replaced in range(index, index + len(terminator)):
                    output[replaced] = " "
                index += len(terminator)
                state = "code"
                continue
            index += 1

    return "".join(output)


TEST_ATTRIBUTE = re.compile(
    r"#\s*\[\s*(?:test|cfg\s*\([^\]]*\btest\b[^\]]*\))\s*\]",
    re.DOTALL,
)


def matching_delimiter(source: str, start: int, opening: str, closing: str) -> int:
    depth = 0
    for index in range(start, len(source)):
        if source[index] == opening:
            depth += 1
        elif source[index] == closing:
            depth -= 1
            if depth == 0:
                return index + 1
    return len(source)


def attributed_item_end(source: str, after_attribute: int) -> int:
    cursor = after_attribute
    while True:
        while cursor < len(source) and source[cursor].isspace():
            cursor += 1
        if cursor < len(source) and source[cursor] == "#":
            bracket = source.find("[", cursor + 1)
            if bracket < 0:
                return len(source)
            cursor = matching_delimiter(source, bracket, "[", "]")
            continue
        break

    parentheses = 0
    brackets = 0
    for index in range(cursor, len(source)):
        char = source[index]
        if char == "(":
            parentheses += 1
        elif char == ")":
            parentheses = max(0, parentheses - 1)
        elif char == "[":
            brackets += 1
        elif char == "]":
            brackets = max(0, brackets - 1)
        elif parentheses == 0 and brackets == 0:
            if char == "{":
                return matching_delimiter(source, index, "{", "}")
            if char in ";,":
                return index + 1
    return len(source)


def non_test_line_count(source: str) -> int:
    lines = source.splitlines()
    if not lines:
        return 0
    sanitized = sanitized_rust(source)
    excluded = [False] * len(lines)
    for attribute in TEST_ATTRIBUTE.finditer(sanitized):
        end = attributed_item_end(sanitized, attribute.end())
        start_line = sanitized.count("\n", 0, attribute.start())
        end_line = sanitized.count("\n", 0, max(attribute.start(), end - 1))
        for line in range(start_line, min(end_line + 1, len(excluded))):
            excluded[line] = True
    return sum(not is_excluded for is_excluded in excluded)


def rust_line_counts(path: str, contents: bytes) -> tuple[int, int]:
    total = len(contents.splitlines())
    parts = PurePosixPath(path).parts
    if "tests" in parts:
        return total, 0
    source = contents.decode("utf-8", errors="replace")
    return total, non_test_line_count(source)


def rust_lines_at(commit: str) -> tuple[int, int]:
    archive = git("archive", "--format=tar", commit, binary=True)
    assert isinstance(archive, bytes)

    total = 0
    non_test = 0
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as entries:
        for entry in entries:
            if not entry.isfile() or not entry.name.endswith(".rs"):
                continue
            extracted = entries.extractfile(entry)
            if extracted is None:
                continue
            file_total, file_non_test = rust_line_counts(entry.name, extracted.read())
            total += file_total
            non_test += file_non_test
    return total, non_test


def history(ref: str) -> list[Point]:
    commits = str(git("rev-list", "--reverse", ref)).splitlines()
    points: list[Point] = []
    for commit in commits:
        metadata = str(git("show", "-s", "--format=%h%x00%cs%x00%s", commit)).rstrip("\n")
        short, timestamp, subject = metadata.split("\0", 2)
        total, non_test = rust_lines_at(commit)
        points.append(
            Point(
                commit=short,
                subject=subject,
                timestamp=timestamp,
                rust_lines=total,
                non_test_rust_lines=non_test,
            )
        )
    if not points:
        raise RuntimeError(f"no commits found for {ref!r}")
    return points


def nice_ceiling(value: int) -> int:
    if value <= 10:
        return 10
    magnitude = 10 ** (len(str(value)) - 1)
    for multiplier in (1, 2, 5, 10):
        candidate = multiplier * magnitude
        if candidate >= value:
            return candidate
    raise AssertionError("unreachable")


def svg(points: list[Point]) -> str:
    width = 1200
    height = 540
    left = 92
    right = 38
    top = 112
    bottom = 72
    plot_width = width - left - right
    plot_height = height - top - bottom
    y_max = nice_ceiling(max(point.rust_lines for point in points))

    def x(index: int) -> float:
        if len(points) == 1:
            return left + plot_width / 2
        return left + plot_width * index / (len(points) - 1)

    def y(lines: int) -> float:
        return top + plot_height * (1 - lines / y_max)

    def path_for(attribute: str) -> str:
        return " ".join(
            f"{'M' if index == 0 else 'L'} {x(index):.2f} {y(getattr(point, attribute)):.2f}"
            for index, point in enumerate(points)
        )

    grid: list[str] = []
    labels: list[str] = []
    for step in range(6):
        value = y_max * step // 5
        py = y(value)
        grid.append(
            f'<line x1="{left}" y1="{py:.2f}" x2="{left + plot_width}" '
            f'y2="{py:.2f}" class="grid" />'
        )
        labels.append(
            f'<text x="{left - 14}" y="{py + 5:.2f}" text-anchor="end" '
            f'class="axis-label">{value:,}</text>'
        )

    x_labels: list[str] = []
    label_count = min(6, len(points))
    for step in range(label_count):
        index = round((len(points) - 1) * step / max(1, label_count - 1))
        x_labels.append(
            f'<text x="{x(index):.2f}" y="{top + plot_height + 34}" '
            f'text-anchor="middle" class="axis-label">{index + 1}</text>'
        )

    latest = points[-1]
    test_lines = latest.rust_lines - latest.non_test_rust_lines
    description = html.escape(
        f"Commit {len(points)} ({latest.commit}, {latest.timestamp}) has "
        f"{latest.rust_lines:,} tracked Rust lines: {latest.non_test_rust_lines:,} "
        f"non-test lines and {test_lines:,} test lines."
    )
    latest_title = html.escape(
        f"{latest.commit} · {latest.timestamp} · {latest.non_test_rust_lines:,} non-test / "
        f"{latest.rust_lines:,} total lines · {latest.subject}"
    )
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-labelledby="title description">
  <title id="title">Rust lines over main history, with and without tests</title>
  <desc id="description">{description}</desc>
  <style>
    :root {{ color-scheme: light dark; }}
    .background {{ fill: #ffffff; }}
    .grid {{ stroke: #d8dee4; stroke-width: 1; }}
    .axis {{ stroke: #57606a; stroke-width: 1.5; }}
    .axis-label {{ fill: #57606a; font: 14px system-ui, sans-serif; }}
    .title {{ fill: #24292f; font: 700 25px system-ui, sans-serif; }}
    .subtitle, .legend {{ fill: #57606a; font: 14px system-ui, sans-serif; }}
    .line-total {{ fill: none; stroke: #8250df; stroke-width: 3; stroke-linejoin: round; stroke-linecap: round; }}
    .line-non-test {{ fill: none; stroke: #0969da; stroke-width: 3; stroke-linejoin: round; stroke-linecap: round; }}
    .latest-total {{ fill: #8250df; stroke: #ffffff; stroke-width: 3; }}
    .latest-non-test {{ fill: #0969da; stroke: #ffffff; stroke-width: 3; }}
    @media (prefers-color-scheme: dark) {{
      .background {{ fill: #0d1117; }}
      .grid {{ stroke: #30363d; }}
      .axis {{ stroke: #8c959f; }}
      .axis-label, .subtitle, .legend {{ fill: #8c959f; }}
      .title {{ fill: #f0f6fc; }}
      .line-total {{ stroke: #d2a8ff; }}
      .line-non-test {{ stroke: #58a6ff; }}
      .latest-total {{ fill: #d2a8ff; stroke: #0d1117; }}
      .latest-non-test {{ fill: #58a6ff; stroke: #0d1117; }}
    }}
  </style>
  <rect width="{width}" height="{height}" class="background" rx="12" />
  <text x="{left}" y="42" class="title">Rust lines over main history</text>
  <text x="{left}" y="68" class="subtitle">Physical lines in tracked *.rs files · {len(points)} commits</text>
  <line x1="{left}" y1="91" x2="{left + 26}" y2="91" class="line-non-test" />
  <text x="{left + 34}" y="96" class="legend">Rust excluding tests · latest {latest.non_test_rust_lines:,}</text>
  <line x1="{left + 290}" y1="91" x2="{left + 316}" y2="91" class="line-total" />
  <text x="{left + 324}" y="96" class="legend">All Rust · latest {latest.rust_lines:,}</text>
  {''.join(grid)}
  {''.join(labels)}
  <line x1="{left}" y1="{top + plot_height}" x2="{left + plot_width}" y2="{top + plot_height}" class="axis" />
  <line x1="{left}" y1="{top}" x2="{left}" y2="{top + plot_height}" class="axis" />
  {''.join(x_labels)}
  <text x="{left + plot_width / 2:.2f}" y="{height - 18}" text-anchor="middle" class="axis-label">Commit number on main</text>
  <text x="24" y="{top + plot_height / 2:.2f}" text-anchor="middle" class="axis-label" transform="rotate(-90 24 {top + plot_height / 2:.2f})">Rust lines</text>
  <path d="{path_for('rust_lines')}" class="line-total" />
  <path d="{path_for('non_test_rust_lines')}" class="line-non-test" />
  <circle cx="{x(len(points) - 1):.2f}" cy="{y(latest.rust_lines):.2f}" r="6" class="latest-total"><title>{latest_title}</title></circle>
  <circle cx="{x(len(points) - 1):.2f}" cy="{y(latest.non_test_rust_lines):.2f}" r="6" class="latest-non-test"><title>{latest_title}</title></circle>
</svg>
"""


def page() -> str:
    return """<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>ClipAsm repository history</title>
  <style>
    :root { color-scheme: light dark; font-family: system-ui, sans-serif; }
    body { margin: 0 auto; max-width: 1240px; padding: 2rem; }
    img { display: block; width: 100%; height: auto; }
    p { color: #6e7781; }
  </style>
</head>
<body>
  <h1>ClipAsm repository history</h1>
  <p>Physical lines in tracked Rust files, with a second series excluding test-only code.</p>
  <img src="loc-history.svg" alt="Rust lines over main history, with and without tests">
</body>
</html>
"""


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ref", default="main")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--html-output", type=Path)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    points = history(args.ref)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(svg(points), encoding="utf-8")
    if args.html_output is not None:
        args.html_output.parent.mkdir(parents=True, exist_ok=True)
        args.html_output.write_text(page(), encoding="utf-8")


if __name__ == "__main__":
    main()
