# Developer docs

These pages explain how AppyTappy works and why it is built this way. The
[user guide](../Home.md) explains how to use it and holds no
implementation detail. [`AGENTS.md`](../../AGENTS.md) holds the workflow and
the rules, and [`CONTEXT.md`](../../CONTEXT.md) the domain terms.

## Get set up

Install [mise](https://mise.jdx.dev/), then run:

```sh
mise install
mise run standards:check
```

[Platform](../../AGENTS.md#platform) says which tasks run on Windows and which
need Linux, and how to set up mise in Git Bash.

## Test builds

To get an installer of a branch without a release, use one of these:

- On GitHub, open the **Release** workflow under **Actions**, select
  **Run workflow**, and pick the branch. The installer is in the run's
  artifacts for 14 days.
- On Windows, run `mise run installer:build`. The installer lands in
  `target/dist/bundle/nsis/`.

[Builds](Installer.md#builds) explains test versions and releases.

## Pages

- [Architecture](Architecture.md): the two programs, the crates, the runtime's
  threads and lifecycle, the tray, files, anti-cheat enforcement, why the
  setup app uses Tauri, and unsafe code.
- [Setup app](Setup-app.md): window matching, saved file formats, and running
  the setup app from source.
- [Screen regions](Screen-regions.md): the checks, read intervals, capture, and
  why AppyTappy does not use OCR.
- [Rules](Rules.md): conditions, triggers, re-arming, restoring, and how the
  engine evaluates them.
- [Actions](Actions.md): media, volume, key press, open, and sound actions,
  and how restoring works.
- [Trace](Trace.md): the runtime trace, its file format, and what recording
  costs.
- [Installer](Installer.md): what the installer does, builds, the release
  workflow, updates and the update key, and winget.
- [MCP investigation server](MCP-investigation-server.md): the local server an
  AI agent uses to find how an application shows a value.
- [State machines](State-machines.md): the implicit state machines in the
  runtime and the setup app, and where each can fail.
- [Rust standards](Rust-standards.md): how the Rust checks work, mutation
  testing, and their limits.
- [Wiki](Wiki.md): how `wiki/` publishes to the GitHub wiki, and what that
  means for writing pages.
