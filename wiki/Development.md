# Development

The developer docs explain how AppyTappy works and why it is built this way;
[Home](Home.md) lists them. The user guide explains how to use it and holds no
implementation detail. [`AGENTS.md`](https://github.com/TheLunchTrae/appy-tappy/blob/main/AGENTS.md)
holds the workflow and the rules, and
[`CONTEXT.md`](https://github.com/TheLunchTrae/appy-tappy/blob/main/CONTEXT.md)
the domain terms.

## Get set up

Install [mise](https://mise.jdx.dev/), then run:

```sh
mise install
mise run standards:check
```

[Platform](https://github.com/TheLunchTrae/appy-tappy/blob/main/AGENTS.md#platform)
says which tasks run on Windows and which need Linux, and that mise runs from
Git Bash on Windows.

## Test builds

To get an installer of a branch without a release, use one of these:

- On GitHub, open the **Release** workflow under **Actions**, select
  **Run workflow**, and pick the branch. The installer is in the run's
  artifacts for 14 days.
- On Windows, run `mise run installer:build`. The installer lands in
  `target/dist/bundle/nsis/`.

[Builds](Installer.md#builds) explains test versions and releases.

## MCP investigation server

`appytappy-mcp` is a local [MCP](https://modelcontextprotocol.io/) server. An
AI agent such as Claude Code starts it and uses its tools to work out how an
application shows a value, such as a number in one spot of its window.
AppyTappy then reads that value without hardcoding anything per application.

The server talks MCP over standard input and output, so it has no network
port. Every tool only reads: none sends input.

### Tools

| Tool            | What it does                                                       |
| --------------- | ------------------------------------------------------------------ |
| `process_list`  | Lists running processes and names known anti-cheat processes.      |
| `window_list`   | Lists windows with their process, title, class, and screen bounds. |
| `screen_sample` | Reads the color of screen pixels.                                  |

No tool opens an application's process, so they are safe for protected
applications too, such as ones with anti-cheat. Tool names are `area_verb`,
so the tools that come next slot in beside these: `definition_*` and `rule_*`
for app definitions and rules, and `source_test` to try a source.

### Build

On Windows, install Rust with [rustup](https://rustup.rs/) and the Visual
Studio C++ build tools. From the repository root:

```sh
cargo build --release -p appytappy-mcp
```

The server is `target\release\appytappy-mcp.exe`.

### Register it in Claude Code

Add the server once for your user, with the full path to the program:

```sh
claude mcp add --scope user appytappy -- C:/path/to/appy-tappy/target/release/appytappy-mcp.exe
```

Check it with `claude mcp list`, or run `/mcp` inside Claude Code. Claude
Code starts the server when a session starts and stops it when the session
ends. Rebuild with the same command after pulling changes, then start a new
Claude Code session.

## Wiki

The repository's [GitHub wiki](https://github.com/TheLunchTrae/appy-tappy/wiki)
mirrors the `wiki/` folder. Write and change pages in `wiki/` only: the wiki
holds no page of its own, and each sync replaces whatever it held before.

### Publishing

The **Wiki** workflow, `.github/workflows/wiki.yml`, runs on each push to
`main` that changes `wiki/` or the workflow. It copies the pages, drops `.md`
from links between them, drops each page's title heading except on **Home**,
and pushes them to the wiki repository with the workflow's own token, so it
needs no secret. A page removed from `wiki/` leaves the wiki on the next run.
When the wiki repository does not exist, the run fails and says how to create
it: turn on **Wikis** in the repository's settings and save any first page.

A manual run of the workflow on `main` publishes too. On any other branch it
is a dry run: it builds the wiki and keeps it in the run's artifacts for 3
days, without publishing.

### Pages

- `wiki/` is one flat folder, as the wiki is. A file's name is its wiki page's
  name, and the wiki shows that name, with its hyphens as spaces, as the
  page's title. So `wiki/The-tray-icon.md` is the page **The tray icon**, and
  renaming the file renames the page. Each page other than **Home** starts
  with a level 1 heading that matches that title, which the workflow drops,
  since the wiki already shows the title.
- `wiki/Home.md` is the wiki's first page and lists every page.
  `wiki/_Sidebar.md` is the sidebar on every page, and `wiki/_Footer.md` shows
  under every page. Every page other than **Home** needs a link in the
  sidebar.
- Link between pages with bare relative links to the `.md` files, such as
  a page's file name with an anchor, so they work in the repository and in the
  wiki: the workflow drops `.md` from those links. It rewrites such a link
  inside code too, so keep page links out of code examples. An image in
  `wiki/` links by its bare file name. Link to a file outside `wiki/` with its
  full GitHub address on `main`.

### Checks

`mise run md:links`, part of `mise run standards:check`, fails on a relative
link or anchor that points nowhere. `mise run md:wiki:check`, part of the same
gate, fails when `wiki/` has a folder, a page has no link in the sidebar or a
level 1 heading that doesn't match its name, or a link has a form the
workflow doesn't rewrite, such as `../`, `./`, a title, or a reference
definition.
