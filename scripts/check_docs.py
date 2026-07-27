#!/usr/bin/env python3
"""Check ClipAsm documentation navigation and local links without dependencies."""

from __future__ import annotations

import argparse
from html.parser import HTMLParser
from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlsplit


MARKDOWN_LINK = re.compile(r"(?<!!)\[[^\]]*\]\(([^)]+)\)")
SUMMARY_LINK = re.compile(r"\[[^\]]+\]\(([^)#?]+\.md)(?:#[^)]+)?\)")
EXPECTED_ADR_ROUTES = (
    Path("adr/0001-keep-compilation-pure.html"),
    Path("adr/0002-use-one-program-model.html"),
    Path("adr/0003-separate-semantic-and-execution-identities.html"),
    Path("adr/0004-quantize-source-duration-by-coverage.html"),
    Path("adr/0005-treat-source-files-as-programs.html"),
    Path("adr/0007-support-ordered-program-outputs.html"),
    Path("adr/0008-separate-parsing-from-canonical-source.html"),
    Path("adr/0009-call-authored-source-programs.html"),
    Path("adr/0010-add-typed-audio-and-body-input-scopes.html"),
    Path("adr/0011-add-type-preserving-timeline-programs.html"),
    Path("adr/0012-run-external-programs.html"),
    Path("adr/0013-adopt-native-clipasm-language.html"),
    Path("adr/0014-map-frame-and-sample-boundaries.html"),
    Path("adr/0015-keep-native-operations-phase-owned.html"),
    Path("adr/0016-overlap-audiovisual-transitions-exactly.html"),
    Path("adr/0017-run-ffmpeg-recipes-through-host-adapters.html"),
    Path("adr/0018-evaluate-scalar-expressions-exactly.html"),
    Path("adr/0019-model-rooted-timeline-layouts.html"),
)


class HtmlLinks(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.ids: set[str] = set()
        self.links: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        values = dict(attrs)
        if identifier := values.get("id"):
            self.ids.add(identifier)
        if tag == "a" and (href := values.get("href")):
            self.links.append(href)


def repository_root() -> Path:
    return Path(__file__).resolve().parent.parent


def markdown_links(root: Path) -> list[str]:
    problems: list[str] = []
    for source in sorted([*root.glob("*.md"), *root.glob("docs/**/*.md")]):
        text = source.read_text(encoding="utf-8")
        for raw_target in MARKDOWN_LINK.findall(text):
            target = raw_target.strip().split(maxsplit=1)[0].strip("<>")
            parsed = urlsplit(target)
            if parsed.scheme or parsed.netloc or target.startswith("#"):
                continue
            path = unquote(parsed.path)
            if not path:
                continue
            destination = (source.parent / path).resolve()
            try:
                destination.relative_to(root)
            except ValueError:
                problems.append(f"{source.relative_to(root)}: link escapes repository: {target}")
                continue
            if not destination.exists():
                problems.append(f"{source.relative_to(root)}: missing link target: {target}")
    return problems


def book_coverage(root: Path) -> list[str]:
    docs = root / "docs"
    summary = (docs / "SUMMARY.md").read_text(encoding="utf-8")
    listed = {Path(path) for path in SUMMARY_LINK.findall(summary)}
    excluded = {Path("SUMMARY.md"), Path("repository-history.md")}
    excluded.update(path.relative_to(docs) for path in (docs / "agents").glob("*.md"))
    public = {path.relative_to(docs) for path in docs.rglob("*.md")} - excluded
    return [f"docs/{path}: public page is missing from docs/SUMMARY.md" for path in sorted(public - listed)]


def expected_adr_routes(book: Path) -> list[str]:
    return [
        f"{book}: missing generated ADR page: {route}"
        for route in EXPECTED_ADR_ROUTES
        if not (book / route).is_file()
    ]


def generated_html_links(root: Path, book: Path) -> list[str]:
    problems: list[str] = []
    pages: dict[Path, HtmlLinks] = {}
    for path in book.rglob("*.html"):
        parser = HtmlLinks()
        parser.feed(path.read_text(encoding="utf-8"))
        pages[path.resolve()] = parser

    for source, parser in list(pages.items()):
        for href in parser.links:
            parsed = urlsplit(href)
            if parsed.scheme or parsed.netloc or href.startswith(("mailto:", "javascript:")):
                continue
            destination = source if not parsed.path else (source.parent / unquote(parsed.path)).resolve()
            if destination.is_dir():
                destination /= "index.html"
            if not destination.exists():
                problems.append(f"{source.relative_to(root)}: missing generated target: {href}")
                continue
            if parsed.fragment and destination.suffix == ".html":
                target_parser = pages.get(destination.resolve())
                if target_parser is None:
                    target_parser = HtmlLinks()
                    target_parser.feed(destination.read_text(encoding="utf-8"))
                fragment = unquote(parsed.fragment)
                if fragment not in target_parser.ids:
                    problems.append(f"{source.relative_to(root)}: missing generated anchor: {href}")
    return problems


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--book", type=Path, default=Path("target/book"))
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = repository_root()
    book = args.book if args.book.is_absolute() else root / args.book
    problems = markdown_links(root) + book_coverage(root)
    if not book.is_dir():
        problems.append(f"generated book directory does not exist: {book.relative_to(root)}")
    else:
        problems.extend(expected_adr_routes(book))
        problems.extend(generated_html_links(root, book))
    if problems:
        print("documentation checks failed:", file=sys.stderr)
        for problem in problems:
            print(f"- {problem}", file=sys.stderr)
        return 1
    print("documentation links, anchors, and book coverage are valid")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
