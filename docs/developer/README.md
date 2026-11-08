# Developer docs

These pages explain how AppyTappy works and why it is built this way. The
[user guide](../user/README.md) explains how to use it and holds no
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
- Open a pull request that changes the installer or its build; its Release run
  keeps a test build the same way.
- On Windows, run `mise run installer:build`. The installer lands in
  `target/dist/bundle/nsis/`.

[Builds](installer.md#builds) explains test versions and releases.

## Pages

- [Architecture](architecture.md): the two programs, the crates, the runtime's
  threads and lifecycle, the tray, files, anti-cheat enforcement, why the
  setup app uses Tauri, and unsafe code.
- [Setup app](setup-app.md): window matching, saved file formats, and running
  the setup app from source.
- [Screen regions](screen.md): the checks, read intervals, capture, and why
  AppyTappy does not use OCR.
- [Rules](rules.md): conditions, triggers, re-arming, restoring, and how the
  engine evaluates them.
- [Actions](actions.md): media, volume, and key press actions, and how
  restoring works.
- [Installer](installer.md): what the installer does, builds, and the release
  workflow.
- [MCP investigation server](mcp.md): the local server an AI agent uses to
  find how an application exposes a value.
- [State machines](state-machines.md): the implicit state machines in the
  runtime, the setup app, and the MCP server, and where each can fail.
- [Rust standards](rust.md): how the Rust checks work, mutation testing, and
  their limits.
