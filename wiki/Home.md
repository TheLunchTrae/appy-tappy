# AppyTappy wiki

AppyTappy watches the windows of other programs on your Windows PC and acts
when something in them changes, such as pausing your music when a progress bar
fills up. New to it? Start with [Getting started](Getting-started.md).

## User guide

- [Getting started](Getting-started.md): install AppyTappy, make your first
  automation, what AppyTappy keeps, update, and uninstall.
- [Applications](Applications.md): how AppyTappy recognizes a program's
  window, the anti-cheat setting, and the overlay that shows your values as
  AppyTappy reads them.
- [Values](Values.md): what AppyTappy reads from a window, and the four ways it
  can read it.
- [References](References.md): how AppyTappy knows an application's
  characters, which its number values read with.
- [Automations](Automations.md): what to do, and when.
- [Settings](Settings.md): starting with Windows, the screenshot shortcut, and
  how often values are read.
- [Debug page](Debug-page.md): what AppyTappy is doing, its logs, and a debug
  bundle to share when something goes wrong.
- [The tray icon](The-tray-icon.md): turn automations off and on, and see
  which ones run.
- [Troubleshooting](Troubleshooting.md)

## Developer docs

These pages explain how AppyTappy works and why it is built this way.

- [Architecture](Architecture.md): the two programs, the crates, the runtime's
  threads and lifecycle, the tray, files, anti-cheat enforcement, why the
  setup app uses Tauri, and unsafe code.
- [Setup app](Setup-app.md): window matching, saved file formats, the
  pictures it keeps, the page's generated types, and running the setup app
  from source.
- [Screen regions](Screen-regions.md): the checks, read intervals, capture, and
  why AppyTappy does not use OCR.
- [Rules](Rules.md): conditions, firing and re-arming, cooldowns, restoring,
  and how the engine evaluates them.
- [Actions](Actions.md): media, volume, key press, open, and sound actions,
  and how restoring works.
- [Trace](Trace.md): the runtime trace, its file format, and what recording
  costs.
- [Installer](Installer.md): what the installer does, builds, the release
  workflow, updates and the update key, and winget.
- [State machines](State-machines.md): the implicit state machines in the
  runtime and the setup app, and where each can fail.

## Contributing

- [Development](Development.md): getting set up, test builds, the MCP
  investigation server, and how `wiki/` publishes to this wiki.
- [Rust standards](Rust-standards.md): how the Rust checks work, mutation
  testing, and their limits.
