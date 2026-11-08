# Wiki

The repository's [GitHub wiki](https://github.com/TheLunchTrae/appy-tappy/wiki)
mirrors the `wiki/` folder. Write and change pages in `wiki/` only: the wiki
holds no page of its own, and each sync replaces whatever it held before.

## Publishing

The **Wiki** workflow, `.github/workflows/wiki.yml`, runs on each push to
`main` that changes `wiki/` or the files that build the wiki. It builds the
pages with `mise run md:wiki` and pushes them to the wiki repository with the
workflow's own token, so it needs no secret. A page removed from `wiki/`
leaves the wiki on the next run. When the wiki repository does not exist, the
run fails and says how to create it: turn on **Wikis** in the repository's
settings and save any first page.

A manual run of the workflow on `main` publishes too. On any other branch it
is a dry run: it builds the wiki and keeps it in the run's artifacts for 3
days, without publishing.

## Pages

`scripts/wiki.py` turns each Markdown file in `wiki/` into one wiki page:

- A file's name is its wiki page's name, and the wiki shows that name, with
  its hyphens as spaces, as the page's title. So `wiki/user/The-tray-icon.md`
  is the page **The tray icon**, and renaming the file renames the page. A
  page's level 1 heading is dropped when it matches that title.
- `wiki/Home.md` is the wiki's first page, and `wiki/_Sidebar.md` is the
  sidebar on every page. An optional `wiki/_Footer.md` shows under every page.
  These stay at the top of `wiki/`, and no other file name starts with `_`.
  The pages sit in `wiki/user/` and `wiki/developer/`, but the wiki itself has
  no folders, so two files with one name, in any folder or case, fail the
  build. A file name with a space or any of `\ / : * ? " < > | # %` fails it
  too, and so does a page in a format other than `.md`.
- Every page other than **Home** needs a link in `wiki/_Sidebar.md` that a
  reader sees, or the build fails, so no page is left where readers cannot
  find it. A link in code or in an HTML comment does not count.
- A relative link to another page in `wiki/` links its wiki page, anchor
  included. A link to another file in `wiki/`, such as an image, links the
  copy that the wiki holds at the same path below `wiki/`. A link to any other
  file in the repository opens it on GitHub, on `main`. A link to a file that
  does not exist, or that matches a file only in another case, fails the
  build, and so does a link the build cannot read, such as one with a title in
  single quotes. Links in code stay as written.
- Each page other than the sidebar ends with a link to the file in `wiki/` to
  edit.

Write links between pages as relative links to the `.md` files, as the rest of
`wiki/` does, so they work in the repository and in the wiki.

## Checks

`mise run md:wiki` builds the wiki into `artifacts/wiki/` to read before
publishing. `mise run md:wiki:check`, part of `mise run standards:check`,
tests `scripts/wiki.py` and builds the wiki from the current `wiki/`, so a
page that would break the wiki fails the gate before it reaches `main`.
