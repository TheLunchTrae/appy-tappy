#!/usr/bin/env python3
"""Build the GitHub wiki from docs/.

docs/ is the only source of the pages; the wiki is a generated mirror that
.github/workflows/wiki.yml publishes. docs/developer/wiki.md describes the
mapping. This script only writes files into an output directory and never
touches the network or the wiki repository.

Usage: wiki.py OUTPUT_DIRECTORY --repo OWNER/NAME [--ref BRANCH]
"""

import argparse
import posixpath
import re
import shutil
import sys
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from urllib.parse import quote, unquote

DOCS = PurePosixPath("docs")
# Each section is a folder of docs/ and a block of the sidebar, in this order.
# The user guide's README is the wiki's Home page.
SECTIONS = ("user", "developer")
HOME = DOCS / "user" / "README.md"
INDEX = "README.md"
# Characters GitHub refuses in a wiki file name, or that break a page's URL.
UNSAFE_NAME = re.compile(r'[\\/:*?"<>|#%]')
RESERVED_NAMES = {"_sidebar", "_footer"}

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
PAGES_SECTION = re.compile(r"^## Pages\n([\s\S]*?)(?=^## |\Z)", re.M)
SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")


class BuildError(Exception):
    """A docs page that cannot become a wiki page as written."""


@dataclass(frozen=True)
class Page:
    source: PurePosixPath
    section: str
    title: str
    name: str


def build(root: Path, output: Path, repo: str, ref: str) -> None:
    """Write the wiki for the docs under `root` into the empty or absent `output`."""
    if output.exists() and any(output.iterdir()):
        raise BuildError(f"{output} is not empty")
    pages, assets = collect(root)
    by_source = {page.source: page for page in pages}
    output.mkdir(parents=True, exist_ok=True)
    for page in pages:
        text = (root / page.source).read_text(encoding="utf-8")
        body = render(
            page, text, lambda target, image, page=page: resolve(root, page, target, image, by_source, repo, ref)
        )
        body += footer(page, repo, ref)
        (output / f"{page.name}.md").write_text(body, encoding="utf-8")
    for asset in assets:
        destination = output / asset.relative_to(DOCS)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(root / asset, destination)
    (output / "_Sidebar.md").write_text(sidebar(root, pages, by_source, repo, ref), encoding="utf-8")


def collect(root: Path) -> tuple[list[Page], list[PurePosixPath]]:
    """The docs pages, in section and file order, and every other file under docs/."""
    pages: list[Page] = []
    assets: list[PurePosixPath] = []
    for path in sorted((root / DOCS).rglob("*")):
        if not path.is_file():
            continue
        source = PurePosixPath(path.relative_to(root).as_posix())
        if source.suffix != ".md":
            assets.append(source)
            continue
        section = source.parts[1] if len(source.parts) > 2 else None
        if section not in SECTIONS:
            raise BuildError(f"{source}: a docs page belongs in one of docs/{', docs/'.join(SECTIONS)}")
        title = heading(source, path.read_text(encoding="utf-8"))
        pages.append(Page(source, section, title, page_name(source, title)))
    pages.sort(key=lambda page: SECTIONS.index(page.section))
    seen: dict[str, PurePosixPath] = {}
    for page in pages:
        key = page.name.casefold()
        if key in RESERVED_NAMES or (key == "home" and page.source != HOME):
            raise BuildError(f"{page.source}: its title makes the reserved wiki page name {page.name}")
        if key in seen:
            raise BuildError(f"{page.source} and {seen[key]} both make the wiki page {page.name}; retitle one")
        seen[key] = page.source
    return pages, assets


def heading(source: PurePosixPath, text: str) -> str:
    for line in outside_fences(text):
        if match := H1.match(line):
            return match.group(1)
    raise BuildError(f"{source}: no level 1 heading to name its wiki page")


def page_name(source: PurePosixPath, title: str) -> str:
    """Home for the user guide's README; otherwise the title, as GitHub names wiki files."""
    if source == HOME:
        return "Home"
    return UNSAFE_NAME.sub("", title).strip().replace(" ", "-")


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
    if on_disk.is_dir() and resolved_path / INDEX in by_source:
        resolved_path = resolved_path / INDEX
    if resolved_path in by_source:
        return page_link(by_source[resolved_path].name) + anchor
    if not on_disk.exists():
        raise BuildError(f"{page.source}: the link {target} names no file in the repository")
    if resolved_path.is_relative_to(DOCS) and on_disk.is_file():
        return quote(resolved_path.relative_to(DOCS).as_posix()) + anchor
    # GitHub shows an image only from its raw file.
    kind = "tree" if on_disk.is_dir() else "raw" if image else "blob"
    location = "" if resolved == "." else "/" + quote(resolved)
    return f"https://github.com/{repo}/{kind}/{ref}{location}{anchor}"


def page_link(name: str) -> str:
    return quote(name, safe="-,'")


def footer(page: Page, repo: str, ref: str) -> str:
    link = f"https://github.com/{repo}/blob/{ref}/{quote(str(page.source))}"
    return (
        f"\n---\n\n_This page is generated from [{page.source}]({link}). "
        "Edit that file in the repository: the next sync replaces changes made in the wiki._\n"
    )


def sidebar(root: Path, pages: list[Page], by_source: dict, repo: str, ref: str) -> str:
    """Each section's index page, then its other pages in the order its Pages list links them.

    Pages the index's `## Pages` section, or the whole index when it has none,
    does not link follow in title order.
    """
    blocks = []
    for section in SECTIONS:
        members = [page for page in pages if page.section == section]
        index = next((page for page in members if page.source.name == INDEX), None)
        if index is None:
            raise BuildError(f"docs/{section} has no {INDEX} to head its sidebar section")
        linked: list[Page] = []
        text = "".join(chunk for code, chunk in chunks((root / index.source).read_text(encoding="utf-8")) if not code)
        if listed := PAGES_SECTION.search(text):
            text = listed.group(1)
        names = {page.name: page for page in members}
        for match in INLINE_LINK.finditer(text):
            destination = match.group(1).strip("<>")
            name = unquote(resolve(root, index, destination, False, by_source, repo, ref).partition("#")[0])
            if (member := names.get(name)) and member is not index and member not in linked:
                linked.append(member)
        rest = sorted((page for page in members if page is not index and page not in linked), key=lambda p: p.title)
        lines = [f"**[{index.title}]({page_link(index.name)})**", ""]
        lines += [f"- [{page.title}]({page_link(page.name)})" for page in linked + rest]
        blocks.append("\n".join(lines) + "\n")
    notice = "<!-- Generated from docs/ by scripts/wiki.py. Edit docs/ in the repository, not this page. -->\n\n"
    return notice + "\n".join(blocks)


def main() -> int:
    parser = argparse.ArgumentParser(description="Build the GitHub wiki from docs/ into a directory.")
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
