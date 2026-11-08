# Wiki

The repository's [GitHub wiki](https://github.com/TheLunchTrae/appy-tappy/wiki)
mirrors `docs/`. Write and change pages in `docs/` only: the wiki holds no
page of its own, and each sync replaces whatever it held before.

## Publishing

The **Wiki** workflow, `.github/workflows/wiki.yml`, runs on each push to
`main` that changes `docs/` or the files that build the wiki. It builds the
pages with `mise run md:wiki` and pushes them to the wiki repository with the
workflow's own token, so it needs no secret. A page removed from `docs/`
leaves the wiki on the next run. When the wiki repository does not exist, the
run fails and says how to create it: turn on **Wikis** in the repository's
settings and save any first page.

A manual run of the workflow on `main` publishes too. On any other branch it
is a dry run: it builds the wiki and keeps it in the run's artifacts for 3
days, without publishing.

## Pages

`scripts/wiki.py` turns each page in `docs/user/` and `docs/developer/` into
one wiki page:

- A page's level 1 heading names its wiki page, so retitling a page renames
  it in the wiki. The user guide's `README.md` is **Home**. Two pages with
  one title fail the build.
- A relative link to another page in `docs/` links its wiki page, anchor
  included. A link to another file in `docs/`, such as an image, links the
  copy that the wiki holds at the same path below `docs/`. A link to any other
  file in the repository opens it on GitHub, on `main`. A link to a file that
  does not exist fails the build, and so does a link the build cannot read,
  such as one with a title in single quotes. Links in code stay as written.
- The sidebar lists the user guide, then the developer docs. Each section
  starts with its `README.md` and follows the order of that page's **Pages**
  list. Pages that list leaves out follow by title.
- Each page ends with a link to the file in `docs/` to edit.

Write links between docs as relative links to the `.md` files, as the rest of
`docs/` does, so they work in the repository and in the wiki.

## Checks

`mise run md:wiki` builds the wiki into `artifacts/wiki/` to read before
publishing. `mise run md:wiki:check`, part of `mise run standards:check`,
tests `scripts/wiki.py` and builds the wiki from the current `docs/`, so a
page that would break the wiki fails the gate before it reaches `main`.
