# Installer

AppyTappy ships as one Windows installer that holds both programs. Tauri's
bundler builds it as an NSIS installer from the setup app, with the runtime
added as a sidecar. Issue
[#19](https://github.com/TheLunchTrae/appy-tappy/issues/19) tracks packaging.

## What it does

- **Installs for the current user** into `%LOCALAPPDATA%\AppyTappy`, without
  administrator rights. That is also where the runtime keeps its `logs`
  folder, wherever the programs are installed.
- **Adds AppyTappy to the Start menu**, which opens the setup app. The
  finish page can open it too.
- **Starts the runtime** and turns on its
  [autostart](architecture.md#lifecycle) with
  `appytappy-runtime.exe --autostart on`, so it runs at every sign-in. The
  runtime it starts is not elevated, even when the installer is.
- **Stops a running runtime first** when it installs over an earlier version,
  so the new runtime replaces the old one. It asks the runtime's tray window to
  close, which the runtime treats like the end of the session, so it undoes
  what its automations changed. A runtime that is still running after 10
  seconds is killed, and its changes stay as they are.

The uninstaller stops the runtime the same way, turns autostart off, and
removes both programs. Canceling it when it asks to close a running setup app
leaves the runtime stopped until the next sign-in; installing has the same
step. Its **Delete the application data** box also removes the
saved applications, values, and automations in `%APPDATA%\AppyTappy` and the
runtime's logs.

The hooks in `apps/setup/src-tauri/windows/hooks.nsh` add the runtime's steps
to Tauri's installer. `apps/setup/src-tauri/tauri.installer.conf.json` turns
bundling on and adds the sidecar for installer builds only, so the other Rust
tasks build the setup app without a built runtime.

The installer and the programs are not signed, so Windows SmartScreen warns
the first time the installer runs. Select **More info**, then **Run anyway**.

## Builds

`mise run installer:build` builds the installer on Windows into
`target/dist/bundle/nsis/`. It builds the runtime and the setup app with the
`dist` profile from `Cargo.toml`, which optimizes across crates and strips
symbols, so it takes longer than a release build. The bundler downloads NSIS
the first time.

Every build has a version:

- **A release build** has the workspace version from `Cargo.toml`, such as
  `0.1.0`. The release workflow makes one only for a tag that names that
  version, such as `v0.1.0`.
- **A test build** has the workspace version marked as a development version
  of the commit it was built from, such as `0.1.0-dev+27585e8`, with `.dirty`
  after the commit when the tree had uncommitted or untracked changes. A
  release of the same version counts as newer. Two test builds of one version
  are ordered by their commit text, so installing over another one can ask
  whether to downgrade; either answer installs it.

The release workflow, `.github/workflows/release.yml`, builds the installer on
a Windows runner:

- **A pushed `v*` tag** publishes a GitHub release of that tag with the
  installer attached and generated notes. The build fails when the tag does not
  name the workspace version.
- **A manual run** on any branch or tag, from the workflow's **Run workflow**
  button, makes a test build.
- **A pull request** that changes the installer or its build makes a test
  build.

A test build's installer is in its run's artifacts for 14 days.
