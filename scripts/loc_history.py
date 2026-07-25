#!/usr/bin/env python3
"""Generate a dependency-free SVG chart of Rust source lines over Git history."""

from __future__ import annotations

import argparse
import html
import io
import subprocess
import tarfile
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Point:
    commit: str
    subject: str
    timestamp: str
    rust_lines: int


def git(*args: str, binary: bool = False) -> bytes | str:
    result = subprocess.run(
        ["git", *args],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return result.stdout if binary else result.stdout.decode("utf-8")


def rust_lines_at(commit: str) -> int:
    archive = git("archive", "--format=tar", commit, binary=True)
    assert isinstance(archive, bytes)

    total = 0
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as entries:
        for entry in entries:
            if not entry.isfile() or not entry.name.endswith(".rs"):
                continue
            extracted = entries.extractfile(entry)
            if extracted is None:
                continue
            total += len(extracted.read().splitlines())
    return total


def history(ref: str) -> list[Point]:
    commits = str(git("rev-list", "--reverse", ref)).splitlines()
    points: list[Point] = []
    for commit in commits:
        metadata = str(
            git("show", "-s", "--format=%h%x00%cs%x00%s", commit)
        ).rstrip("\n")
        short, timestamp, subject = metadata.split("\0", 2)
        points.append(
            Point(
                commit=short,
                subject=subject,
                timestamp=timestamp,
                rust_lines=rust_lines_at(commit),
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
    height = 520
    left = 92
    right = 38
    top = 92
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

    path = " ".join(
        f"{'M' if index == 0 else 'L'} {x(index):.2f} {y(point.rust_lines):.2f}"
        for index, point in enumerate(points)
    )
    area = (
        f"M {x(0):.2f} {top + plot_height:.2f} "
        + " ".join(
            f"L {x(index):.2f} {y(point.rust_lines):.2f}"
            for index, point in enumerate(points)
        )
        + f" L {x(len(points) - 1):.2f} {top + plot_height:.2f} Z"
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
    description = html.escape(
        f"Commit {len(points)} ({latest.commit}, {latest.timestamp}) has "
        f"{latest.rust_lines:,} tracked Rust source lines."
    )
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-labelledby="title description">
  <title id="title">Rust source lines over main history</title>
  <desc id="description">{description}</desc>
  <style>
    :root {{ color-scheme: light dark; }}
    .background {{ fill: #ffffff; }}
    .grid {{ stroke: #d8dee4; stroke-width: 1; }}
    .axis {{ stroke: #57606a; stroke-width: 1.5; }}
    .axis-label {{ fill: #57606a; font: 14px system-ui, sans-serif; }}
    .title {{ fill: #24292f; font: 700 25px system-ui, sans-serif; }}
    .subtitle {{ fill: #57606a; font: 14px system-ui, sans-serif; }}
    .area {{ fill: #54aeff; opacity: 0.18; }}
    .line {{ fill: none; stroke: #0969da; stroke-width: 3; stroke-linejoin: round; stroke-linecap: round; }}
    .latest {{ fill: #0969da; stroke: #ffffff; stroke-width: 3; }}
    @media (prefers-color-scheme: dark) {{
      .background {{ fill: #0d1117; }}
      .grid {{ stroke: #30363d; }}
      .axis {{ stroke: #8c959f; }}
      .axis-label, .subtitle {{ fill: #8c959f; }}
      .title {{ fill: #f0f6fc; }}
      .area {{ fill: #2f81f7; opacity: 0.2; }}
      .line {{ stroke: #58a6ff; }}
      .latest {{ fill: #58a6ff; stroke: #0d1117; }}
    }}
  </style>
  <rect width="{width}" height="{height}" class="background" rx="12" />
  <text x="{left}" y="42" class="title">Rust source lines over main history</text>
  <text x="{left}" y="68" class="subtitle">Physical lines in tracked *.rs files · {len(points)} commits · latest {latest.rust_lines:,} lines</text>
  {''.join(grid)}
  {''.join(labels)}
  <line x1="{left}" y1="{top + plot_height}" x2="{left + plot_width}" y2="{top + plot_height}" class="axis" />
  <line x1="{left}" y1="{top}" x2="{left}" y2="{top + plot_height}" class="axis" />
  {''.join(x_labels)}
  <text x="{left + plot_width / 2:.2f}" y="{height - 18}" text-anchor="middle" class="axis-label">Commit number on main</text>
  <text x="24" y="{top + plot_height / 2:.2f}" text-anchor="middle" class="axis-label" transform="rotate(-90 24 {top + plot_height / 2:.2f})">Rust source lines</text>
  <path d="{area}" class="area" />
  <path d="{path}" class="line" />
  <circle cx="{x(len(points) - 1):.2f}" cy="{y(latest.rust_lines):.2f}" r="6" class="latest">
    <title>{html.escape(latest.commit)} · {html.escape(latest.timestamp)} · {latest.rust_lines:,} lines · {html.escape(latest.subject)}</title>
  </circle>
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
  <p>Physical lines in tracked Rust source files, regenerated from the complete <code>main</code> history after every push.</p>
  <img src="loc-history.svg" alt="Rust source lines over main history">
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
