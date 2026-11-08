#!/usr/bin/env python3
"""Build the GitHub wiki from wiki/.

wiki/ is the only source of the pages; the GitHub wiki is a generated mirror
that .github/workflows/wiki.yml publishes. wiki/developer/Wiki.md describes
the mapping. This script only writes files into an output directory and never
touches the network or the wiki repository.

Usage: wiki.py OUTPUT_DIRECTORY --repo OWNER/NAME [--ref BRANCH]
"""

import argparse
import os
import posixpath
import re
import shutil
import sys
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from urllib.parse import quote, unquote

WIKI = PurePosixPath("wiki")
HOME = WIKI / "Home.md"
SIDEBAR = WIKI / "_Sidebar.md"
# The files GitHub's wiki shows around every page: no page lists or footers them.
SPECIAL = {"_Sidebar", "_Footer"}
# Other formats GitHub's wiki renders as pages, which the build does not read.
OTHER_PAGE_SUFFIXES = {
    ".adoc", ".asciidoc", ".creole", ".markdown", ".mdown", ".mediawiki", ".mkdn",
    ".org", ".pod", ".rdoc", ".rst", ".textile", ".wiki",
}
# Characters GitHub refuses in a wiki file name, or that break a page's URL.
UNSAFE_NAME = re.compile(r'[\\/:*?"<>|#%]')
FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})")
H1 = re.compile(r"^# +(.+?)(?: +#+)? *$")
PARAGRAPH_BREAK = re.compile(r"(\n[ \t]*\n)")
CODE_SPAN = re.compile(r"(?<!`)(`+)(?!`)[\s\S]*?(?<!`)\1(?!`)")
# An image, then an inline link: the bracketed text, then the destination, in
# angle brackets or bare, and an optional title. Images go first, so an image
# inside a link's text is rewritten too.
DESTINATION = r"\(\s*(<[^<>\n]*>|[^\s()]+(?:\([^\s()]*\)[^\s()]*)*)(?:\s+\"[^\"\n]*\")?\s*\)"
IMAGE = re.compile(r"!\[[^\[\]]*\]" + DESTINATION)
INLINE_LINK = re.compile(r"(?<!!)\[(?:[^\[\]]|\[[^\[\]]*\])*\]" + DESTINATION)
# A link reference definition; a footnote's label starts with ^.
REFERENCE_DEFINITION = re.compile(r"^ {0,3}\[(?!\^)[^\]\n]+\]:[ \t]*(<[^<>\n]*>|\S+)", re.M)
# What is left of a link that the patterns above do not read, including an
# HTML link to a relative address, which the build does not rewrite.
UNREAD_LINK = re.compile(r"\]\(|^ {0,3}\[(?!\^)[^\]\n]+\]:|href=[\"']?(?![\"']?(?:#|[A-Za-z][A-Za-z0-9+.-]*:))", re.M)
HTML_COMMENT = re.compile(r"<!--[\s\S]*?-->")
INDENTED_CODE = re.compile(r"^(?: {4}|\t)(?![ \t]*(?:[-*+]|\d+[.)])[ \t]).*$", re.M)
SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")


class BuildError(Exception):
    """A page in wiki/ that cannot become a wiki page as written."""


@dataclass(frozen=True)
class Page:
    source: PurePosixPath
    name: str
    title: str | None


def build(root: Path, output: Path, repo: str, ref: str) -> None:
    """Write the wiki for the pages under `root`'s wiki/ into the empty or absent `output`."""
    if output.exists() and any(output.iterdir()):
        raise BuildError(f"{output} is not empty")
    pages, assets = collect(root)
    by_source = {page.source: page for page in pages}
    check_sidebar(root, pages, by_source, repo, ref)
    output.mkdir(parents=True, exist_ok=True)
    for page in pages:
        text = (root / page.source).read_text(encoding="utf-8")
        body = render(
            page, text, lambda target, image, page=page: resolve(root, page, target, image, by_source, repo, ref)
        )
        if page.name not in SPECIAL:
            body += footer(page, repo, ref)
        # GitHub's wiki names a page by its file name alone, so pages land at
        # the top of the wiki repository whatever folder holds them here.
        (output / f"{page.name}.md").write_text(body, encoding="utf-8")
    for asset in assets:
        destination = output / asset.relative_to(WIKI)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(root / asset, destination)


def collect(root: Path) -> tuple[list[Page], list[PurePosixPath]]:
    """The wiki pages and every other file under wiki/."""
    pages: list[Page] = []
    assets: list[PurePosixPath] = []
    seen: dict[str, PurePosixPath] = {}
    for path in sorted((root / WIKI).rglob("*")):
        if not path.is_file():
            continue
        source = PurePosixPath(path.relative_to(root).as_posix())
        suffix = source.suffix.lower()
        if suffix in OTHER_PAGE_SUFFIXES or (suffix == ".md" and source.suffix != ".md"):
            raise BuildError(f"{source}: write wiki pages as .md files")
        if suffix != ".md":
            assets.append(source)
            continue
        name = source.stem
        if UNSAFE_NAME.search(name) or " " in name:
            raise BuildError(f"{source}: a wiki file name holds no spaces or any of \\ / : * ? \" < > | # %")
        if name.startswith("_") and name not in SPECIAL:
            raise BuildError(f"{source}: only {' and '.join(sorted(SPECIAL))} start with _ in the wiki")
        top = HOME if name.casefold() == "home" else WIKI / source.name if name in SPECIAL else None
        if top is not None and source != top:
            raise BuildError(f"{source}: this file belongs at {top}")
        key = name.casefold()
        if key in seen:
            raise BuildError(f"{source} and {seen[key]} both make the wiki page {name}; rename one")
        seen[key] = source
        pages.append(Page(source, name, heading(path.read_text(encoding="utf-8"))))
    if HOME not in {page.source for page in pages}:
        raise BuildError(f"{WIKI}/ has no Home.md, the wiki's first page")
    return pages, assets


def heading(text: str) -> str | None:
    for line in outside_fences(text):
        if match := H1.match(line):
            return match.group(1)
    return None


def check_sidebar(root: Path, pages: list[Page], by_source: dict, repo: str, ref: str) -> None:
    """Fail when the sidebar is missing or leaves out a page other than Home."""
    sidebar = by_source.get(SIDEBAR)
    if sidebar is None:
        raise BuildError(f"{SIDEBAR} is missing; it lists every page")
    linked = set()
    for match in shown_links(root / SIDEBAR):
        destination = match.group(1).strip("<>")
        linked.add(unquote(resolve(root, sidebar, destination, False, by_source, repo, ref).partition("#")[0]))
    missing = [page.source for page in pages if page.source != HOME and page.name not in SPECIAL]
    missing = [str(source) for source in missing if by_source[source].name not in linked]
    if missing:
        raise BuildError(f"{SIDEBAR} does not link {', '.join(missing)}")


def shown_links(path: Path) -> list[re.Match]:
    """The inline links and link definitions in a file that a reader sees.

    Links in code, in an indented code block, or in an HTML comment are not
    shown, so they do not count. A line indented four spaces is taken as code
    unless it is a list item, so nested lists still count.
    """
    text = "".join(chunk for code, chunk in chunks(path.read_text(encoding="utf-8")) if not code)
    text = HTML_COMMENT.sub("", text)
    text = INDENTED_CODE.sub("", text)
    matches: list[re.Match] = []
    for paragraph in PARAGRAPH_BREAK.split(text):
        paragraph = CODE_SPAN.sub(lambda match: " " * len(match.group(0)), paragraph)
        matches += INLINE_LINK.finditer(paragraph)
        matches += REFERENCE_DEFINITION.finditer(paragraph)
    return matches


def outside_fences(text: str) -> list[str]:
    lines = []
    for code, chunk in chunks(text):
        if not code:
            lines += chunk.splitlines()
    return lines


def chunks(text: str) -> list[tuple[bool, str]]:
    """Split Markdown into fenced code blocks (True) and the text between them (False)."""
    result: list[tuple[bool, str]] = []
    current: list[str] = []
    fence = None
    for line in text.splitlines(keepends=True):
        match = FENCE.match(line)
        if fence is None and match:
            result.append((False, "".join(current)))
            current, fence = [line], match.group(1)
        elif fence is not None and match and match.group(1)[0] == fence[0] and len(match.group(1)) >= len(fence):
            current.append(line)
            result.append((True, "".join(current)))
            current, fence = [], None
        else:
            current.append(line)
    result.append((fence is not None, "".join(current)))
    return [(code, chunk) for code, chunk in result if chunk]


def render(page: Page, text: str, rewrite) -> str:
    """The page's Markdown with its links rewritten for the wiki.

    GitHub shows a wiki page's name, with its hyphens as spaces, as the page's
    title. The page's own level 1 heading is dropped when that title shows it
    unchanged, and kept otherwise, as on Home.
    """
    output = []
    drop_heading = page.name.replace("-", " ") == page.title
    for code, chunk in chunks(text):
        if code:
            output.append(chunk)
            continue
        chunk = "".join(rewrite_links(page, part, rewrite) for part in PARAGRAPH_BREAK.split(chunk))
        if drop_heading:
            chunk, count = re.subn(r"^# .*\n+", "", chunk, count=1, flags=re.M)
            drop_heading = count == 0
        output.append(chunk)
    notice = f"<!-- Generated from {page.source}. Edit that file in the repository, not this page. -->\n\n"
    return notice + "".join(output).lstrip("\n")


def rewrite_links(page: Page, text: str, rewrite) -> str:
    """Rewrite one paragraph's link, image, and link definition destinations outside code spans.

    A paragraph is the unit, because a code span cannot cross a blank line, so
    a stray backtick hides no link past its own paragraph. A link that the
    patterns cannot read fails the build rather than reach the wiki unchanged.
    """

    def substitute(pattern: re.Pattern, text: str, image: bool) -> str:
        spans = [match.span() for match in CODE_SPAN.finditer(text)]

        def replace(match: re.Match) -> str:
            if any(start <= match.start() < end or start <= match.start(1) < end for start, end in spans):
                return match.group(0)
            destination = match.group(1)
            bracketed = destination.startswith("<")
            target = rewrite(destination[1:-1] if bracketed else destination, image)
            target = f"<{target}>" if bracketed else target
            offset = match.start()
            start, end = match.start(1) - offset, match.end(1) - offset
            return match.group(0)[:start] + target + match.group(0)[end:]

        return pattern.sub(replace, text)

    text = substitute(IMAGE, text, True)
    text = substitute(INLINE_LINK, text, False)
    text = substitute(REFERENCE_DEFINITION, text, False)
    rest = text
    for pattern in (CODE_SPAN, IMAGE, INLINE_LINK, REFERENCE_DEFINITION):
        rest = pattern.sub("", rest)
    if match := UNREAD_LINK.search(rest):
        line = rest[: match.start()].count("\n")
        near = rest.splitlines()[line].strip()
        raise BuildError(f"{page.source}: the link near {near!r} is in a form the wiki build cannot read")
    return text


def resolve(root: Path, page: Page, target: str, image: bool, by_source: dict, repo: str, ref: str) -> str:
    """The wiki destination for a link or image written in `page`."""
    if target.startswith(("#", "//")) or SCHEME.match(target):
        return target
    path, hash_mark, fragment = target.partition("#")
    if not path:
        return target
    # A path that starts with / is relative to the repository root, as on GitHub.
    if path.startswith("/"):
        joined = unquote(path).lstrip("/")
    else:
        joined = posixpath.join(str(page.source.parent), unquote(path))
    resolved = posixpath.normpath(joined)
    anchor = hash_mark + fragment
    if resolved == ".." or resolved.startswith("../"):
        raise BuildError(f"{page.source}: the link {target} leaves the repository")
    resolved_path = PurePosixPath(resolved)
    on_disk = root / resolved_path
    if resolved_path in by_source:
        return page_link(by_source[resolved_path].name) + anchor
    if not exists_as_written(root, resolved_path):
        near = [str(source) for source in by_source if str(source).casefold() == resolved.casefold()]
        hint = f"; did you mean {near[0]}?" if near else ""
        raise BuildError(f"{page.source}: the link {target} names no file in the repository{hint}")
    # Other files under wiki/ are copied to the same place in the wiki.
    if resolved_path.is_relative_to(WIKI) and on_disk.is_file():
        return quote(resolved_path.relative_to(WIKI).as_posix()) + anchor
    # GitHub shows an image only from its raw file.
    kind = "tree" if on_disk.is_dir() else "raw" if image else "blob"
    location = "" if resolved == "." else "/" + quote(resolved)
    return f"https://github.com/{repo}/{kind}/{ref}{location}{anchor}"


def exists_as_written(root: Path, path: PurePosixPath) -> bool:
    """Whether `path` exists with each part in the case it is written.

    Windows and macOS find a file whatever the case, but GitHub and the wiki do
    not, so a link that matches only in another case would break there.
    """
    current = root
    for part in path.parts:
        if not current.is_dir() or part not in os.listdir(current):
            return False
        current = current / part
    return True


def page_link(name: str) -> str:
    return quote(name, safe="-,'")


def footer(page: Page, repo: str, ref: str) -> str:
    link = f"https://github.com/{repo}/blob/{ref}/{quote(str(page.source))}"
    return (
        f"\n---\n\n_This page is generated from [{page.source}]({link}). "
        "Edit that file in the repository: the next sync replaces changes made in the wiki._\n"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="Build the GitHub wiki from wiki/ into a directory.")
    parser.add_argument("output", type=Path, help="an empty or absent directory to write the wiki into")
    parser.add_argument("--repo", required=True, help="the GitHub repository, OWNER/NAME, for links to its files")
    parser.add_argument("--ref", default="main", help="the branch that links to repository files name")
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    try:
        build(root, arguments.output.resolve(), arguments.repo, arguments.ref)
    except BuildError as error:
        print(f"wiki: {error}", file=sys.stderr)
        return 1
    print(f"wiki: wrote {arguments.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
