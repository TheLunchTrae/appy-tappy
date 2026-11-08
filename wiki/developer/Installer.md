# Installer

AppyTappy ships as one Windows installer that holds both programs.
[Install](../user/Install,-update,-and-uninstall.md) in the user guide describes
it from the user's side. Tauri's bundler builds it as an NSIS installer from the
setup app, with the runtime added as a sidecar. Issue
[#19](https://github.com/TheLunchTrae/appy-tappy/issues/19) tracks packaging.

## What it does

- **Installs for the current user** into `%LOCALAPPDATA%\AppyTappy`, without
  administrator rights. That is also where the runtime keeps its `logs`
  folder, wherever the programs are installed.
- **Adds AppyTappy to the Start menu**, which opens the setup app. The
  finish page can open it too.
- **Starts the runtime** and turns on its
  [autostart](Architecture.md#lifecycle) with
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

The installer also holds the setup app's text reader model,
`models\text-recognition.onnx` beside the setup app, which
[recording readings](../user/References.md#record-readings) uses
([The text reader](Setup-app.md#the-text-reader)). `mise run
installer:build` downloads it first with `mise run ocr:model`, which fails when
the download does not match the hash pinned in
`.config/mise/conf.d/installer.toml`.

The hooks in `apps/setup/src-tauri/windows/hooks.nsh` add the runtime's steps
to Tauri's installer. `apps/setup/src-tauri/tauri.installer.conf.json` turns
bundling on and adds the sidecar for installer builds only, so the other Rust
tasks build the setup app without a built runtime.

The installer and the programs are not code-signed, so Windows SmartScreen
warns the first time the installer runs;
[Install](../user/Install,-update,-and-uninstall.md) tells the user what to
select.

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
  name the workspace version. Before publishing, the release job attests the
  installer's build provenance.
- **A manual run** on any branch or tag, from the workflow's **Run workflow**
  button, makes a test build.

Nothing else starts it, so pull requests and pushes to branches build no
installer.

A test build's installer is in its run's artifacts for 14 days.

## Attestation

A release's installer has a
[build provenance attestation](https://docs.github.com/en/actions/security-guides/using-artifact-attestations-to-establish-provenance-for-builds):
a signed record of the workflow run, commit, and tag that built it, stored on
the repository. Because the installer is not signed, this is the way to check
that a download came from this repository's release workflow. It does not
change the SmartScreen warning.

Check a downloaded installer with the [GitHub CLI](https://cli.github.com/):

```sh
gh attestation verify AppyTappy_0.1.0_x64-setup.exe -R TheLunchTrae/appy-tappy
```

Only the release job attests, and only for a pushed `v*` tag. Test builds have
no attestation, so a passing check means the installer came from a release
tag. Add `--source-ref refs/tags/v0.1.0` to check that it came from one
release in particular, and
`--signer-workflow TheLunchTrae/appy-tappy/.github/workflows/release.yml` to
reject an attestation that any other workflow might add later. The attesting
job alone gets the `id-token` and `attestations` permissions, so the build
steps cannot sign anything.

## Updates

AppyTappy Setup checks for a newer release each time it opens, with Tauri's
updater (`apps/setup/src-tauri/src/updates.rs`). It downloads one when the
user selects **Update**, and installs it only when they agree, with nothing
left unsaved. [Update](../user/Install,-update,-and-uninstall.md#update) shows
what the user sees, and the
[update notice](State-machines.md#update-notice) maps its states. The runtime
has no updater, so it stays as small and idle as it is. One installer holds
both programs, so the setup app's update updates the runtime too, and the two
always come from the same release and read the same file formats.

- **Check.** The updater reads `latest.json` from the newest GitHub release.
  It names the release's version, its installer's address, and the installer's
  signature. A test build of a version counts as older than its release.
- **Download.** The updater checks again, downloads the newest release's
  installer, and checks its signature against the update key's public half,
  which the setup app carries. It also requires the version inside the
  signature to be the one `latest.json` names (`requireSignedVersion`), so an
  altered `latest.json` cannot offer an older release as a newer one. The
  owner thread keeps the installer and the release's `latest.json` until the
  install. A connection that sends nothing for 30 seconds fails the check or
  the download.
- **Install.** The updater rebuilds the release from the kept `latest.json`,
  checks the installer's signature again, and has the owner save where each
  panel is and stop the capture. Then it runs the installer in passive mode,
  which shows only its progress, and exits. The installer stops the runtime as
  [What it does](#what-it-does) describes, installs, starts the runtime, and
  opens the setup app again.
- **Builds.** `apps/setup/src-tauri/tauri.updater.conf.json` holds the
  updater's settings. `mise run installer:build` adds them, with the public
  key from the `UPDATER_PUBLIC_KEY` environment variable, only when that
  variable is set. A build without it has no updater and never checks.

### The update key

The update key is a key pair for Tauri's updater only. It is not a Windows
code-signing certificate and does not change the SmartScreen warning. The
release workflow reads three repository settings:

- **`UPDATER_PUBLIC_KEY`**, a repository variable: the key's public half. The
  installer job builds it into the setup app.
- **`TAURI_SIGNING_PRIVATE_KEY`** and **`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`**,
  repository secrets: the key's private half and its password. Only the sign
  job gets them. It runs `mise run update:sign`, which signs the installer,
  fails if the signature does not match `UPDATER_PUBLIC_KEY`, and writes
  `latest.json`. The release job publishes it beside the installer.

Without `UPDATER_PUBLIC_KEY`, the sign job is skipped, the release has no
`latest.json`, and the release run warns about it. That release becomes the
newest, so every installed copy, including copies from earlier signed
releases, finds no `latest.json` and offers nothing until the next signed
release. A copy installed from an unsigned release has no updater at all, so
its user installs a later release by hand once.

To set the key up, generate it once on your own PC from `apps/setup`, with a
password, and keep both files and the password somewhere safe:

```sh
bun run tauri signer generate -w ~/.tauri/appytappy-update.key
```

Then, in the repository's **Settings**, under **Secrets and variables** and
**Actions**, add the contents of `appytappy-update.key` as the
`TAURI_SIGNING_PRIVATE_KEY` secret, the password as the
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secret, and the contents of
`appytappy-update.key.pub` as the `UPDATER_PUBLIC_KEY` variable. Losing the
private half or its password means installed copies cannot take another
update; a new key needs every user to install one release by hand.

## winget

`apps/setup/src-tauri/winget/` holds templates of the
[winget](https://learn.microsoft.com/windows/package-manager/) manifests for the
package `TheLunchTrae.AppyTappy`. Once a release is published, write its
manifests with its version:

```sh
WINGET_VERSION=0.1.0 mise run winget:manifest
```

The task downloads the release's installer for its SHA-256 and writes the
manifests to `artifacts/winget/0.1.0/`. To submit them, check them with
`winget validate --manifest artifacts/winget/0.1.0` on Windows, then open a
pull request that adds them to
[winget-pkgs](https://github.com/microsoft/winget-pkgs) under
`manifests/t/TheLunchTrae/AppyTappy/0.1.0/`, for example with
`wingetcreate submit artifacts/winget/0.1.0`. winget's moderators review each
pull request, so a release reaches winget some time after GitHub.

The release workflow does not submit to winget. Submitting from it would need
a personal token with write access to a fork of winget-pkgs and a submission
tool that the workflow downloads, for a step that runs once per release.

AppyTappy is not in the Microsoft Store: the Store takes either a code-signed
installer, which AppyTappy skips, or an MSIX package, which Tauri does not
build and whose sandbox would change how the runtime starts at sign-in.
