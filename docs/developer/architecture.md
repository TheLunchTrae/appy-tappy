# Architecture

AppyTappy has two programs that share a set of library crates:

- The **setup app** runs when the user wants to change something. It finds
  the application's window, lets the user draw values on it, and saves
  automations that act on those values.
- The **runtime** runs in the background all the time. It reads what the
  setup app saved, watches the applications it names, and runs an
  automation's actions when it fires.

The setup app runs rarely, so it can afford a heavier UI. The runtime is always
on, so it must stay small. Every design choice below follows from that split.
[`CONTEXT.md`](../../CONTEXT.md) defines the domain terms.

## Components and data flow

```text
                setup app (Tauri, runs on demand)
  ┌─────────────────────────────────────────────────────────┐
  │ React UI ── Tauri commands ── owner thread              │
  │             windows, capture, values, automations       │
  └───────────────┬──────────────────────────┬──────────────┘
    reads, writes │                          │ sets after each save
     saved files  ▼                          ▼
          applications, automations    Reload event
                  │ read at start          │
                  │ and on each reload     │
                  ▼                        ▼
                runtime (plain Rust, always on)
  ┌─────────────────────────────────────────────────────────┐
  │ reload watch ─ reload ──────────┐                       │
  │ lifecycle ─ started, exited ────┤                       │
  │ readers ─ samples, problems ────┼─► owner thread:       │
  │ action thread ─ failures ───────┤   model + engine      │
  │ tray ─ enable, disable, quit ───┘           │           │
  │ ◄── actions, undos, readers started, stopped┘           │
  └─────────────────────────────────────────────────────────┘
```

1. The setup app saves each application, with the values drawn on its
   window, and each automation as files. [Setup app](setup-app.md#saved-files)
   describes them.
2. After each save it sets the named event `Local\AppyTappy.Reload`.
3. The runtime reads the saved files when it starts and again on each reload,
   checks them, and logs any automation that cannot run. The rest run.
4. The runtime writes its log to `%LOCALAPPDATA%\AppyTappy\logs\`.

Apart from that one event, the two programs share only files: no sockets,
pipes, or shared memory. A compiled config with a
status file, described under [Files and handoff](#files-and-handoff), stays
planned in [#8](https://github.com/TheLunchTrae/appy-tappy/issues/8).

## Workspace layout

The repository is one Cargo workspace. `apps/` holds the two programs and
`crates/` holds the libraries they share.

```text
Cargo.toml       workspace root; hosts the standards policy tests
apps/
  mcp/           appytappy-mcp: the local MCP investigation server
  runtime/       appytappy-runtime: the always-on binary
  setup/         the Tauri setup app (#16); its Rust side is in src-tauri/
crates/
  config/        appytappy-config: formats, validation, compile step, paths
  engine/        appytappy-engine: the rule evaluator
  sources/       appytappy-sources: Windows source primitives
  actions/       appytappy-actions: Windows actions
  platform/      appytappy-platform: the runtime's Windows host services
```

| Crate                | Platform | Depends on                                           | Used by        |
| -------------------- | -------- | ---------------------------------------------------- | -------------- |
| `appytappy-config`   | any      | `serde`, `serde_json`                                | setup, runtime |
| `appytappy-engine`   | any      | `config`                                             | setup, runtime |
| `appytappy-sources`  | Windows  | `config`, `windows`                                  | setup, runtime |
| `appytappy-actions`  | Windows  | `config`, `windows`                                  | runtime        |
| `appytappy-platform` | Windows  | `windows`                                            | runtime        |
| `appytappy-mcp`      | Windows  | `config`, `sources`, `serde`, `serde_json`           | —              |
| `appytappy-runtime`  | Windows  | `config`, `engine`, `sources`, `actions`, `platform` | —              |
| setup app            | Windows  | `config`, `sources`, `tauri`, `tokio`, `serde`       | —              |

Each crate's owning issue adds its code and its dependencies. Where issues #6, #8,
and #11 say "the core crate", they mean `appytappy-config` for formats, validation,
and policy, and `appytappy-engine` for rule evaluation. `config` owns what a
condition means, so the setup app and the runtime agree on it; `engine` owns
the state of each game session. `apps/setup/src-tauri`
is a workspace member like the other programs; the setup app takes `engine`
once it previews automations.

The design uses the [`windows`](https://github.com/microsoft/windows-rs) crate
rather than `windows-sys`. Media sessions and Core Audio are WinRT and COM
APIs, which only `windows` covers; `windows-sys` covers plain Win32 functions.
[#12](https://github.com/TheLunchTrae/appy-tappy/issues/12) measures what this
costs.

### Portable core, thin Windows adapters

`config` and `engine` contain no Windows calls, so the Linux `quality` job
lints, tests, and mutation-tests them in full. `sources`, `actions`,
`platform`, and the runtime wrap Windows APIs. Their Windows code sits behind
`#[cfg(windows)]` and their Windows-only dependencies under
`[target.'cfg(windows)'.dependencies]`, so those crates still build on Linux,
where they are nearly empty. The `windows` CI job lints and documents them on
Windows. Their Windows code can run only there, so that job also runs the
member crates' tests.

Logic that can be wrong belongs in `config` or `engine`. A Windows adapter
turns one API call into one typed value or error and holds no decisions of its
own. Mutation testing cannot check adapter code: the Linux gate compiles it
out, and cargo-mutants reports its mutants as missed. So each crate keeps its
Windows code in `win.rs` modules (not `windows.rs`, whose name would clash with
the `windows` crate), and `.cargo/mutants.toml` excludes those files with the
owner's approval.
[`AGENTS.md`](../../AGENTS.md#tests-and-mutation) requires that approval for any
configuration exclusion.

## The runtime

The runtime is one process for all applications. It uses no async runtime and no
webview, and it keeps its dependencies few. It is built as a Windows GUI
program (`#![windows_subsystem = "windows"]`), so no console window opens. It
has no console output, so it logs only to `%LOCALAPPDATA%\AppyTappy\logs\`.
Footprint targets belong to
[#12](https://github.com/TheLunchTrae/appy-tappy/issues/12).

It follows the standards' ownership rule: one thread owns all state, and other
threads send it events over channels instead of sharing state behind locks.
Each thread below is a plain `std::thread`, and each channel is a
`std::sync::mpsc` channel.

- **Owner thread.** Holds the root model: the saved automations it runs, which
  applications are attached, and one engine session per attached application
  that has automations, which holds its signal values and rule state. It applies each event as a
  `(state, event) -> (state, effects)` transition. Effects are action requests
  and orders to start or stop a worker. It also wakes when a cooldown that
  held back a rule ends, as [Evaluation](rules.md#evaluation) describes.
- **Lifecycle.** Reports when a watched application starts and exits. It matches
  processes to applications by the executable name in a process snapshot
  (`CreateToolhelp32Snapshot`), which does not open any process, so it is safe
  for every anti-cheat class. It never waits on a process handle.
  [Lifecycle](#lifecycle) describes when it takes a snapshot.
- **Readers.** One per attached application that has automations. Each finds
  the application's window with its [window match](setup-app.md#window-matching),
  captures the smallest rectangle that covers the values drawn at the window's
  current size, and reads each value from its own region of each frame. It
  sends a `Sample` only when a value changes, and reads less often while
  nothing changes. When the window closes or changes size, it sends `Unseen`,
  and the owner forgets the application's values, so no rule acts on a value
  nobody can see. Each rule stays as it was, run or not, until the values read
  next decide its condition. When the window
  closes it looks for one again, once a second, until the application exits.
  Other kinds of source, such as a log tail or a memory read, join them later
  ([#3](https://github.com/TheLunchTrae/appy-tappy/issues/3)).
- **Action worker.** Runs action requests and releases in order and sends
  back what failed. It makes the media and audio controls on its own thread,
  because Windows ties them to the thread that made them. It keeps the restore
  journal: the state each restoring rule
  changed, such as the volume before a change, so a release can put it back
  even when several rules changed the same thing.
- **Reload watch.** Waits on the named event `Local\AppyTappy.Reload`, which
  the setup app sets after each save, and sends `Reload`. The owner reads the
  saved applications and automations again. An application whose automations
  did not change keeps running untouched; one whose automations changed starts
  over, undoing what they changed first. An automation that cannot run, such
  as one whose value lost a picture or one that breaks a rule after a hand
  edit, is logged, and the rest run. When the applications' folder or the
  automations' file cannot be read as a whole, such as while another program
  holds the file, the runtime logs it, keeps running the plan it has, and
  reads them again every 2 seconds, up to 5 times, before it waits for the
  next save. The runtime
  reads the setup app's files directly; the compiled config under
  [Files and handoff](#files-and-handoff) stays planned in
  [#8](https://github.com/TheLunchTrae/appy-tappy/issues/8).
- **Tray thread.** Owns the hidden window behind the tray icon, runs its
  Windows message loop, and sends the user's menu picks, Windows' requests
  to close, and its own early end to the owner.
  [Tray](#tray) describes what it shows.

Shared flags are banned, so a worker learns to stop from its channel. Each
worker owns a control `Receiver` and waits on it with `recv_timeout`, using the
poll interval as its tick. A stop message ends the worker, and the owner sends
one to each worker it started when it stops. A worker that holds no sender of
its own channel also ends when the owner drops the `Sender`; a reader holds one
to restart itself when its window closes, so only a stop message ends it. A
push source must also be able to cancel its blocking read, for example with
`CancelIoEx`, so that it does not hang on a quiet file.

Some Windows callbacks, such as the hook that reports window events, carry no
pointer back to the code that registered them. The usual way out is a static
or a thread-local, and the standards ban both. Instead, the callback posts a
message to its own thread with `PostThreadMessageW`. That thread's message loop
owns the `Sender` and forwards the event over the channel. The window events
hook runs on its own thread, separate from the tray's message thread. A modal
loop, such as the one an open tray menu runs, drops thread messages, so a
thread that receives them must never show UI.

### Lifecycle

The runtime is in one of three states:

- **Idle.** No watched application runs. The window event hook is installed, and
  no timer runs. A new top-level window anywhere on the desktop, a tooltip or a
  menu included, still leads to a snapshot; what that costs is measured in
  [#12](https://github.com/TheLunchTrae/appy-tappy/issues/12).
- **Attached.** At least one watched application runs, and each has a session
  that follows one of its processes.
- **Degraded.** A problem stops the runtime from watching applications: process
  snapshots fail. It overrides the other two states until a snapshot works
  again. A value that cannot be read is logged instead, because the other
  applications still run.

```text
          app starts              last app exits
  idle ─────────────────► attached ───────────────► idle

  idle or attached ── snapshot fails ──► degraded
  degraded ── snapshot works ──► idle or attached, from what it finds
```

Detection runs on two threads. The window events thread, in
`appytappy-platform`, holds an out-of-context `SetWinEventHook` for window
creation and destruction across the desktop. Its callback keeps a created
window only when it is top-level; Windows cannot say whether a destroyed
window was, so it keeps every destroyed window. The lifecycle thread, in the
runtime, turns those events into snapshots:

- While no watched application runs, only a new top-level window makes a
  snapshot due. While one runs, any destroyed window does too, child windows
  included, so a busy desktop can mean a few snapshots a second while an
  application runs.
- A burst of events becomes one snapshot 250 ms after its first event.
- While an application runs or snapshots fail, a backstop snapshot runs
  every 5 seconds. It finds an application that exits without an event, such as a
  process that never made a window, and an application that takes longer than
  250 ms to exit after closing its window. So an exit can take up to 5 seconds
  to notice.
- The first snapshot runs at startup, so an application that already runs is
  found.

Each snapshot ends the session of an application whose process is gone, then
starts one for an application that runs without one. A session follows the
running process with the lowest ID, so a second copy of an application starts no
second session, and it takes over when the first exits. A process ID that
Windows reused for another executable ends the session. An application
relaunched within 5 seconds that gets its old process ID back keeps its session,
because a snapshot cannot tell the two processes apart. A crashed application is
the same as one that exits: its windows are destroyed and its process leaves the
snapshot. Nothing holds a handle to an application, so detaching leaves nothing
to clean up.

The runtime watches the saved applications that have at least one automation
that can run, and matches each by its window match's executable pattern, such
as `Example.exe`, against the file names in the snapshot. A reload changes the
list at once: an application that is no longer watched detaches, and one that
is newly watched is found by the next snapshot.

A named mutex, `Local\AppyTappy.Runtime`, is the single-instance guard. A
second copy that finds it logs that and exits at once, and the setup app can
open it to learn whether the runtime runs.

Autostart is the `AppyTappy` value under the user's
`HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run` key,
which starts the runtime at sign-in without elevation. A scheduled task can
also do that, but it takes more setup for no gain here.
`appytappy-runtime.exe --autostart on` writes the value as the runtime's own
quoted path, and `--autostart off` removes it. The runtime takes no other
options. Either command exits without
starting the runtime and logs its result. The exit code is 0 on success, 1 on
failure, and 2 for a command line it does not understand, but the runtime is
a windowed program, so a console does not wait for it: the code shows only to
an installer or to `start /wait`. The [installer](installer.md) runs
`--autostart on`, and its uninstaller removes the value itself; the runtime
never turns autostart on by itself.

The log is `runtime.log` in `%LOCALAPPDATA%\AppyTappy\logs\`, one
timestamped line per start, exit, state change, and problem. A line that
would take it past 512 KiB first moves it to `runtime.old.log`, replacing the
older one, so the log never takes more than about 1 MiB. Only the copy that
holds the single-instance guard rotates it; a second copy or an `--autostart`
command only adds its line. When `%LOCALAPPDATA%` is unknown or the log
cannot be opened, nothing is logged, and running exits with code 1.

### Tray

[The tray icon](../user/tray.md) in the user guide describes the tray as the
user sees it. The runtime shows an icon in the taskbar's notification area:
the setup app's icon, which the runtime's build script also links into its
program file from `apps/runtime/runtime.rc`. Pointing at it
shows a tooltip with the state, such as `AppyTappy: idle`, or
`AppyTappy: disabled` while automations are off. Right-clicking it opens a
small menu:

- **Enabled.** A check mark that turns automations on and off. Off, rules do not
  fire, the readers stop, and what the automations changed is undone, but
  detection keeps running, so turning it back on finds the applications
  already attached. The runtime starts enabled, and the setting is
  not kept across restarts.
- **Active automations.** A submenu that lists, by name, the automations that
  run now: those of the attached applications, while automations are on. With
  none, it shows a grayed `None`.
- **Quit.** Stops the runtime until the next sign-in or start.

When Windows asks the program to close, as when the session ends or a tool
sends it a close message, the runtime logs it and quits the same way. Either
way it undoes what the running automations changed before it exits: the
owner queues the undo, waits for the action thread to finish it, and only
then stops the tray.

`appytappy-platform` draws the icon and the menu from a description the
runtime gives it, and the runtime's `tray` module decides what the
description says. The tray thread makes a hidden top-level window, which,
unlike a message-only window, hears when Explorer restarts, so it adds the
icon again. At sign-in the taskbar may not exist yet; the icon then appears
when it starts, and until then the window still hears that the session
ends. The window's procedure posts a right-click and a taskbar restart back
to its own window, and the tray thread's loop, which holds the menu, handles
them. While the menu is open, Windows runs its own loop, which drops those
posts, so the tray loop refreshes the icon each time the menu closes, which
covers a taskbar restart in that time. Stopping the tray closes an open menu
first.

A close request and the end of the session cannot wait for the tray loop, so
the procedure sends `CloseRequested` to the owner itself, through a value the
tray thread attaches to the window (`GWLP_USERDATA`). The menu's loop still
calls the procedure, so neither is lost while the menu is open. A close
request never destroys the window, so the icon cannot outlive its menu.

Windows may end the process as soon as every program returns from
`WM_ENDSESSION`. So when the session ends, the procedure does not return
until `Tray::stop` drops the sender it waits on, or 10 seconds pass. It sets
a shutdown block reason with `ShutdownBlockReasonCreate`, which Windows shows
if the wait passes its own 5 seconds, and the user can end the session
anyway. `WM_QUERYENDSESSION` always agrees, because the undo runs only once
the session really ends. The restore journal lives only in memory: a forced
end, a crash, or a wait that runs out loses what it had not undone.
Without the tray window, such as when it cannot be made or someone destroys
it, nothing hears the end of the session, and Windows ends the runtime
without an undo.

The Enabled item names what a pick does, enable or disable, rather than
flipping the current setting, so a pick from a menu that was out of date
does what the menu showed. The menu opens where the shell says, which is the
pointer for a right-click and the icon for the keyboard. After each event the
owner gives the tray a fresh description. When the runtime cannot make the
tray, such as without a desktop, it logs that and runs without one; when the
tray stops on its own, the runtime logs why and keeps running without it.

## Sources, rules, and actions

These three parts stay separate so that each one can grow without the others
changing.

- A **source primitive** reads one kind of state: a memory pointer chain, a
  screen region, a log file, a local API, or process and window events. It knows
  nothing about rules. The runtime ships a fixed set; an application is data,
  never code. The v1 set and its Windows APIs belong to
  [#3](https://github.com/TheLunchTrae/appy-tappy/issues/3).
  [Screen regions](screen.md) describes the screen region source: its checks
  are portable Rust, and only capture calls Windows.
- A **signal** is a named, typed value an application exposes, such as
  `progress`. An app definition binds each signal to a source primitive
  per application version. The format belongs to
  [#6](https://github.com/TheLunchTrae/appy-tappy/issues/6), and version
  changes to [#7](https://github.com/TheLunchTrae/appy-tappy/issues/7).
- A **rule** says when to act: a trigger on signals or application events, with
  conditions, a cooldown, and re-arming. The engine evaluates rules as a pure
  function with no I/O, so its tests need no application and no Windows.
  [Rules](rules.md) describes the model.
- An **action** is a side effect from a fixed set, such as pausing media or
  setting an app's volume. The engine emits action requests; only the action
  worker performs them. [Actions](actions.md) describes the v1 set.

## Files and handoff

- **Locations.** One function in `config` resolves every path, and both
  programs call it. The setup app does not use Tauri's path resolver, which
  names folders after the app's bundle identifier.
  - `%APPDATA%\AppyTappy\` holds the authoring files, which are small and can
    roam with the user's profile. Each application has a folder in `apps\`,
    named by its ID, that holds its `app.json` and the pictures its values
    learned from; [Setup app](setup-app.md#saved-files) describes the file.
  - `%LOCALAPPDATA%\AppyTappy\` holds what belongs to this machine: the
    compiled config in `compiled\`, the status file, scan caches, and logs in
    `logs\`. The compiled config names install paths and memory layouts that
    can differ between machines.
- **What is resolved at compile time.** Each signal resolves to the concrete
  binding for each known application version, and every rule and action is
  validated. The runtime only picks the binding for the version it detects.
  It detects the version from the application's executable file on disk, at the
  install path the compiled config records, such as its file version. Reading
  that file is allowed for every anti-cheat class.
- **Writing.** The setup app writes the compiled config to a uniquely named
  temporary file in `compiled\`, flushes it to disk with `sync_all`, and
  renames it over the old file. A rename within one folder replaces the file
  in one step, so the runtime never reads a half-written config. Antivirus or
  backup tools can hold the old file open for a moment, so the setup app
  retries a failed rename a few times and then tells the user that it failed.
- **Reading.** The runtime validates everything again before use, because the
  file is outside its control.
- **Status file.** The runtime writes it the same way, temporary file then
  rename, so the setup app never reads it half-written.
- **Format.** The file carries a format version. A runtime that meets a version
  it does not know keeps its current config and records the problem in the
  status file, so the setup app can tell the user to update the runtime.
  Encoding, integrity checks, and version policy belong to
  [#8](https://github.com/TheLunchTrae/appy-tappy/issues/8).

## Anti-cheat enforcement

AppyTappy must never get a user banned. The policy and its classes belong to
[#11](https://github.com/TheLunchTrae/appy-tappy/issues/11). For any application
whose anti-cheat class is not `none`, three things are blocked: memory sources,
input-injection actions, and anything else that opens the application's process.
Screen, log, local API, and process and window event sources stay allowed. The
design enforces the block in four places:

1. The compile step in `config` refuses a blocked binding or action.
2. The runtime applies the same check when it loads the compiled config.
3. Parsing and validation produce different types. Reading a file gives a raw
   type that nothing else accepts; only `config` validation turns it into the
   validated type that sources and actions take. The validated type has
   private fields and no `Deserialize` or `Default`, so code that skips
   validation does not compile.
4. Every path that opens an application's process needs a process-access permit,
   including the guided memory scan
   ([#14](https://github.com/TheLunchTrae/appy-tappy/issues/14)) and any version
   detection that reads the running process. Only `config` issues one, and only
   for an application of class `none`. The scan runs before any binding exists,
   so this point, not the first three, is what blocks it.

## The setup app

The setup app is a Tauri v2 app with a React and TypeScript frontend, in
`apps/setup/`. The owner chose Tauri, which settles the framework question in
[#16](https://github.com/TheLunchTrae/appy-tappy/issues/16); the shell and its
screens stay with that issue.

Its Rust side exposes Tauri commands and holds no logic of its own: discovery,
scanning, preview, and compiling come from the shared crates. It follows the
same ownership rule as the runtime. One owner thread holds the setup app's
state, and Tauri keeps only that thread's `Sender`, never state behind a
`Mutex`. Each command is `async`: it sends a request with a
`tokio::sync::oneshot` reply channel and awaits the reply, so a slow scan never
freezes the window. The guided memory scan
([#14](https://github.com/TheLunchTrae/appy-tappy/issues/14)) lives with the
memory source. The AI-assisted discovery flow belongs to
[#15](https://github.com/TheLunchTrae/appy-tappy/issues/15).

How Tauri compares with the alternatives:

- **Tauri** uses the WebView2 runtime that Windows 11 ships, so the app is a
  few megabytes on disk and far lighter than Electron. While it is open,
  WebView2 runs its own browser, renderer, and GPU processes, which typically
  use 100 MB or more of memory.
- **egui** is an immediate-mode Rust toolkit: code redraws the UI on each
  repaint, and its eframe shell repaints only on input, so idle CPU is near
  zero. It uses tens of megabytes and no webview, but it looks less like a
  Windows app, and complex forms such as the rule editor take more work.
- **Slint** uses its own declarative UI language and native-feeling widgets
  with a small footprint. Its GPLv3 license option fits this project's
  license. It would mean learning that language instead of using React.
- **WinUI** through `windows-rs` is the native Windows UI, but its Rust
  support is immature and its tooling assumes C# or C++.

The webview's cost lands only while the user has the setup app open, and the
always-on runtime has no UI framework at all. In return, Tauri gets React,
which is the quickest of these to build a rich rule editor in.

Tauri brought gate changes, each with the owner's approval. The Linux CI job
installs the webview's development packages, so the workspace still lints and
tests there. `deny.toml` checks only the `x86_64-pc-windows-msvc` graph,
because a Linux-only GTK dependency is unmaintained and never ships. The
setup app's entry point and commands, which only forward to the owner thread,
are left out of mutation testing. The UI has its own Bun lockfile and `ui:`
mise tasks, which the gate runs first because the Rust side embeds the built
UI. The [user guide](../user/README.md) describes the screens, and
[Setup app](setup-app.md) how they work.

The overlay ([#36](https://github.com/TheLunchTrae/appy-tappy/issues/36))
is more windows of the setup app, one for each application whose overlay is
on, so nothing is hooked or injected into the application. Each window loads
the same UI, which renders only the readings when its label names an
application. The owner thread keeps one capture for each overlay, of the
smallest region that holds the values drawn at the window's current size, and
the capture thread reads them from each frame, so only the readings cross to
the owner. The window over the application is transparent, lets clicks
through, and cannot take the focus. Tauri's window library activates a window
it shows, so `platform` shows it once, off the screen, without activating it.
From then on a thread of its own asks the owner over the channel where it goes
about twenty times a second: the corner of the application's window while that
window is in front, at the size its page measured for the card, and off the
screen otherwise. Moving it never shows it again, so a page that the webview
throttles cannot hold its place back; the page's own polls still open the
overlay, start its capture and restart it after a change. `sources` answers where
one window is with a few calls about that window alone, so following it costs
no listing of every window. The picture-in-picture panel reports where it was
moved, and the owner saves that once the panel stops moving.

## Unsafe code

The standards deny `unsafe` code. Win32 and COM calls through the `windows`
crate are `unsafe`, because Rust cannot check what Windows does with the
pointers it receives. That covers reading another process's memory, Core
Audio, window hooks, the tray icon, directory watching, and the Direct3D
interop that screen capture needs. WinRT calls, such as media sessions and the
screen capture API itself, are safe Rust methods. A `win.rs` module's
submodules, such as screen capture in `sources/src/win/capture.rs` and the
tray in `platform/src/win/tray.rs`, follow the same rules.

The owner approved changing the workspace lint from `forbid` to `deny` with
the first `unsafe` Windows call, in `sources`, as
[`AGENTS.md`](../../AGENTS.md#exceptions-and-workspace-policy) describes.
`unsafe` may appear only in the `win.rs` modules of `sources`, `actions`, and
`platform`, and each block carries a reasoned expectation and a `SAFETY`
comment. Every crate root without such a module, including `config`,
`engine`, the runtime, the MCP server, and the setup app, keeps
`#![forbid(unsafe_code)]`.

## Crate glossary

Terms:

- **Crate.** A unit of Rust code that compiles on its own: a library or a
  binary. Each test file in a `tests/` folder is its own small crate too.
- **Package.** A folder with a `Cargo.toml` and the crates it builds. Each
  package here builds one library or one binary, plus its tests.
- **Workspace.** Packages that share one `Cargo.lock` and one `target/` build
  folder. Each package opts in to the lint rules in the root `Cargo.toml` with
  `[lints] workspace = true`.
- **Dependency.** A crate from [crates.io](https://crates.io/), Rust's package
  registry, or another crate in the workspace. Code dependencies go in
  `[dependencies]`; test-only ones go in `[dev-dependencies]`. `Cargo.lock`
  pins the exact version of each.
- **`cfg(windows)`.** Marks code or dependencies that exist only when
  compiling for Windows.
- **Feature.** An optional part of a crate that a dependent turns on in
  `Cargo.toml`, so it compiles only what it uses.
- **Benchmarks.** `cargo bench` runs the files in a crate's `benches/`
  folder. AppyTappy's benchmarks use a small timing loop of their own rather
  than a benchmark crate, so they add no dependency.

Dependencies the design expects. Each is added with the issue that needs it,
and `AGENTS.md` asks every addition to state what complexity it removes.

- **[`serde`](https://serde.rs/).** Converts Rust types to and from data
  formats. Every format crate builds on it.
- **[`serde_json`](https://docs.rs/serde_json).** JSON support for `serde`;
  the likely first encoding for the compiled config
  ([#8](https://github.com/TheLunchTrae/appy-tappy/issues/8)).
- **[`windows`](https://github.com/microsoft/windows-rs).** Microsoft's
  official Rust bindings for Windows APIs, generated from Windows metadata.
  Its features select which APIs compile, which mainly keeps build times down.
  The runtime's tray icon, message loops, and directory watching use it
  directly, with no extra crates. Screen capture does too, rather than through
  the `windows-capture` crate, which adds its own thread pool and locks.
- **[`png`](https://docs.rs/png).** Reads and writes the PNG files that hold
  a screen check's reference picture.
- **[`tauri`](https://v2.tauri.app/).** The setup app framework: a Rust
  backend with a web frontend shown in WebView2.
- **[`tokio`](https://tokio.rs/).** The async runtime Tauri runs on. The setup
  app lists it directly, with only its `sync` feature, for the `oneshot`
  channel its commands await; Tauri does not re-export that channel.
- **[`tauri-plugin-global-shortcut`](https://v2.tauri.app/plugin/global-shortcut/).**
  Tauri's plugin for a key press that works while another window has focus,
  which the setup app uses for its screenshot shortcut.
- **[`proptest`](https://docs.rs/proptest).** A dev-dependency that generates
  many random inputs to check a property, such as "a compiled config reads back
  to the same value". The standards require it at trust boundaries such as file
  formats.
