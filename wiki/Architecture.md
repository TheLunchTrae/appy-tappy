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
[`CONTEXT.md`](https://github.com/TheLunchTrae/appy-tappy/blob/main/CONTEXT.md)
defines the domain terms.

## Components and data flow

```text
                setup app (Tauri, runs on demand)
  ┌─────────────────────────────────────────────────────────┐
  │ React UI ── Tauri commands ── owner thread              │
  │             windows, capture, values, automations       │
  └───────────────┬──────────────────────────┬──────────────┘
    reads, writes │                          │ sets after saves the runtime uses
     saved files  ▼                          ▼
   applications, automations, settings Reload event
                  │ read at start          │
                  │ and on each reload     │
                  ▼                        ▼
                runtime (plain Rust, always on)
  ┌─────────────────────────────────────────────────────────┐
  │ reload watch ─ reload ──────────┐                       │
  │ lifecycle ─ started, exited ────┤                       │
  │ readers ─ samples, problems ────┼─► owner thread:       │
  │ action thread ─ notes, can run ─┤   model + engine      │
  │ tray ─ enable, disable, quit ───┘           │           │
  │ ◄── actions, undos, readers started, stopped┘           │
  └─────────────────────────────────────────────────────────┘
```

1. The setup app saves each application, with the values drawn on its
   window, each automation, and the settings as files.
   [Setup app](Setup-app.md#saved-files) describes them.
2. After each save that changes what the runtime runs, it sets the named event
   `Local\AppyTappy.Reload`, as [Files and handoff](#files-and-handoff) lists.
3. The runtime reads the saved files when it starts and again on each reload,
   checks them, and logs any automation that cannot run. The rest run.
4. The runtime writes its log to `%LOCALAPPDATA%\AppyTappy\logs\`, and
   keeps the tray's last Enabled choice in
   `%LOCALAPPDATA%\AppyTappy\enabled.txt`.

Apart from that one event, the two programs share only files: no sockets,
pipes, or shared memory. The runtime reads the same files the setup app saves;
[Files and handoff](#files-and-handoff) describes how each side writes and
reads them.

## Workspace layout

The repository is one Cargo workspace. `apps/` holds the two programs and
`crates/` holds the libraries they share.

```text
Cargo.toml       workspace root; hosts the standards policy tests
apps/
  mcp/           appytappy-mcp: the local MCP investigation server
  runtime/       appytappy-runtime: the always-on binary
  setup/         the Tauri setup app; its Rust side is in src-tauri/
crates/
  config/        appytappy-config: formats, validation, saved files, paths
  engine/        appytappy-engine: the rule evaluator
  sources/       appytappy-sources: Windows source primitives
  actions/       appytappy-actions: Windows actions
  platform/      appytappy-platform: the runtime's Windows host services
```

| Crate                | Platform | Depends on                                           | Used by        |
| -------------------- | -------- | ---------------------------------------------------- | -------------- |
| `appytappy-config`   | any      | `serde`, `serde_json`, `ts-rs` (`ts` feature)        | setup, runtime |
| `appytappy-engine`   | any      | `config`                                             | setup, runtime |
| `appytappy-sources`  | Windows  | `config`, `windows`                                  | setup, runtime |
| `appytappy-actions`  | Windows  | `config`, `windows`                                  | runtime        |
| `appytappy-platform` | Windows  | `windows`                                            | runtime        |
| `appytappy-mcp`      | Windows  | `config`, `sources`, `serde`, `serde_json`           | —              |
| `appytappy-runtime`  | Windows  | `config`, `engine`, `sources`, `actions`, `platform` | —              |
| setup app            | Windows  | `config`, `sources`, `tauri`, `tokio`, `serde`       | —              |

The table leaves out dev-dependencies, such as `sources` for the actions
crate's manual media player test. Each crate's owning issue adds its code and
its dependencies. Where issues #6 and #11 say "the core crate", they mean
`appytappy-config` for formats, validation, and policy, and `appytappy-engine`
for rule evaluation. `config` owns what a condition means, so the setup app and the runtime agree on it; `engine` owns
the state of each game session. `apps/setup/src-tauri`
is a workspace member like the other programs; the setup app takes `engine`
once it previews automations.

The design uses the [`windows`](https://github.com/microsoft/windows-rs) crate
rather than `windows-sys`. Media sessions and Core Audio are WinRT and COM
APIs, which only `windows` covers; `windows-sys` covers plain Win32 functions.
Link-time optimization drops the bindings the runtime does not call, so the
crate's cost shows in the runtime's binary size, which [Footprint](#footprint)
bounds.

### Portable core, thin Windows adapters

`config` and `engine` contain no Windows calls, so the Linux `quality` job
lints, tests, and mutation-tests them in full. `sources`, `actions`,
`platform`, and the runtime wrap Windows APIs. Their Windows code sits behind
`#[cfg(windows)]` and their Windows-only dependencies under
`[target.'cfg(windows)'.dependencies]`, so those crates still build on Linux,
where they are nearly empty. The `windows` CI job lints and documents them on
Windows. Their Windows code can run only there, so that job also runs the
member crates' tests.

Logic that can be wrong belongs in `config` or `engine`. A Windows adapter turns
one API call into one typed value or error and holds no decisions of its own.
Mutation testing cannot check adapter code: the Linux gate compiles it out, and
cargo-mutants reports its mutants as missed. So each crate keeps its Windows
code in `win.rs` modules (not `windows.rs`, whose name would clash with the
`windows` crate), and `.cargo/mutants.toml` excludes those files with the
owner's approval.
[`AGENTS.md`](https://github.com/TheLunchTrae/appy-tappy/blob/main/AGENTS.md#tests-and-mutation)
requires that approval for any configuration exclusion.

## The runtime

The runtime is one process for all applications. It uses no async runtime and no
webview, and it keeps its dependencies few. It is built as a Windows GUI
program (`#![windows_subsystem = "windows"]`), so no console window opens. It
has no console output, so it logs only to `%LOCALAPPDATA%\AppyTappy\logs\`.
[Footprint](#footprint) states how small and quiet it has to stay.

It follows the standards' ownership rule: one thread owns all state, and other
threads send it events over channels instead of sharing state behind locks.
Each thread below is a plain `std::thread`, and each channel is a
`std::sync::mpsc` channel.

- **Owner thread.** Holds the root model: the saved automations it runs, which
  applications are attached, and one engine session per attached application
  that has automations, which holds its signal values and rule state. It applies
  each event as a `(state, event) -> (state, effects)` transition. Effects are
  action requests and orders to start or stop a worker. It also wakes when a
  cooldown that held back a rule ends, as [Evaluation](Rules.md#evaluation)
  describes.
- **Lifecycle.** Reports when a watched application starts and exits. It matches
  processes to applications by the executable name in a process snapshot
  (`CreateToolhelp32Snapshot`), which does not open any process, so it is safe
  for every anti-cheat class. It never waits on a process handle.
  [Lifecycle](#lifecycle) describes when it takes a snapshot.
- **Readers.** One per attached application that has automations. Each finds the
  application's window with its [window match](Setup-app.md#window-matching),
  copies only the regions of the values that read at the window's current
  size, packed one under another, and reads each value from its place in
  that packed image. Where each value reads at a size, drawn there, fixed
  there, or scaled from its first drawing, is resolved once each time the
  window settles at the size, after it showed that size alone for 250 ms
  ([Window sizes](Screen-regions.md#window-sizes)). A `Layout` in
  `appytappy-sources::screen` says where each region goes, and copies the one
  rectangle around them instead when that is smaller, such as for values
  that overlap. It sends a `Sample` only when a value changes, and reads less
  often while nothing changes. When the window closes or changes size, it
  sends `Unseen`, and the owner forgets the application's values, so no rule
  acts on a value nobody can see. Each rule stays as it was, run or not,
  until the values read next decide its condition. When the window closes,
  when the capture cannot go on, such as after the graphics device was
  lost, and after `RESTART_AFTER` frames in a row fail, it sends `Unseen`
  and looks for the window again. After a capture ends that way it waits
  first, from `FIND_AGAIN`, doubling up to `RESTART_WAIT_MAX` while each
  new capture ends before reading a frame, and it logs why once until the
  reason changes.
  Every `CHECK_AGAIN` it checks that the window still shows and that no other
  window fits better, such as the main window opening after a launcher's,
  and looks again if not. It reads only a window of the process the session
  follows, picked among the windows that fit equally well as
  [Lifecycle](#lifecycle) describes: one that shows over a minimized one.
  While no window fits, the window it would read is minimized, or the window
  that fits best is another process's, it looks every `FIND_AGAIN`, until
  the application exits. The constants are in
  `apps/runtime/src/workers/reader.rs`.
  Other kinds of source, such as a log tail, join them later
  ([#3](https://github.com/TheLunchTrae/appy-tappy/issues/3)).
- **Action worker.** Runs action requests and releases in order and sends
  back what failed. It makes the media and audio controls on its own thread,
  because Windows ties them to the thread that made them. It keeps the restore
  journal: the state each restoring rule
  changed, such as the volume before a change, so a release can put it back
  even when several rules changed the same thing. A restore that fails is
  tried again, and controls that cannot be made are made again when a later
  automation fires (see [Actions](Actions.md#when-the-controls-cannot-start)).
  It places each click just before it runs, finding the target's window and,
  for a picture, grabbing one frame of its area (see
  [Actions](Actions.md#clicks)).
- **Reload watch.** Waits on the named event `Local\AppyTappy.Reload`, which
  the setup app sets after each save that changes what the runtime runs, and
  sends `Reload`. The owner reads the settings, then the saved applications
  and automations, again. A new read interval or trace restarts each reader
  the new plan keeps, in its session, so automations keep their state and nothing is
  undone; settings that cannot be read are logged, and the runtime keeps the
  ones it has. An application starts over, undoing what its automations
  changed first, only when what it runs changed: its window match, its values'
  checks or its automations' rules. Renaming the application, a value or an
  automation, or marking its values as checked, restarts nothing, and the log,
  the tray and the [trace](Trace.md) show the new names at once. An automation that cannot run, such
  as one whose value lost a picture or one that breaks a rule after a hand
  edit, is logged, and the rest run. When the applications' folder or the
  automations' file cannot be read as a whole, such as while another program
  holds the file, the runtime logs it, keeps running the plan it has, and
  reads them again a few times, as `RETRY_WAIT` and `RETRIES` in
  `apps/runtime/src/owner.rs` set, before it waits for the next save.
  [Files and handoff](#files-and-handoff) describes what it reads.
- **Trace writer.** Only while the [runtime trace](Trace.md) records. Owns
  the trace files and turns the records the other threads send it into CSV
  rows, at the lowest thread priority. The owner starts and stops it as the
  settings say.
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
  menu included, still leads to a snapshot. [Footprint](#footprint) measures
  idle CPU on a CI desktop, where few windows open; a busy desktop's snapshots
  are not measured.
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
starts one for an application that runs without one. When several running
processes fit the application's executable pattern, such as two copies or a
helper beside the application, the snapshot also lists the windows and
checks which show, and the session follows the process that owns the window
the reader reads. Both pick it the same way (`read_window` in
`crates/sources/src/system.rs`) among the windows the application's
[window match](Setup-app.md#window-matching) fits best: one of the session's
own process while it owns one, and otherwise the first that is not
minimized, then the first. So the values, the session, and the check that a
key press goes to the application in front all agree on one process. When
the session's process owns no such window and another does, such as the
application opening its window after a helper started first, the session
ends and starts again on that process. A process that owned such a window
keeps the session for `OWNER_GRACE` (`apps/runtime/src/lifecycle.rs`) after
it owns none, because an application that switches to full screen replaces
its window, and the snapshot after the old one closed may not list the new
one yet; only after that does another copy that owns one take over. While no
window fits, a session keeps its process, and a new one follows the process
with the lowest ID. So a second copy of an application starts no second
session, and it takes over when the first exits or has shown no window
that fits for that long. A process ID that
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

#### Build detection

When a session starts, the lifecycle thread sends the `Started` change, then
reads which build of the application's executable runs and sends it as an
`Identified` change, so reading the file never holds back the session. A
build is the executable's file version, its size, and when it was written
(`appytappy_sources::build`). The Windows adapter
(`crates/sources/src/win/build.rs`) asks `NtQuerySystemInformation` for the
process's executable path by its process ID, which needs no process handle, so
it is allowed for every anti-cheat class, and turns the device path it gives
into a drive or network path with `QueryDosDeviceW`. Then it reads the file's
version resource, size, and write time from disk. A file that cannot be read,
such as one another program holds locked, makes the build unreadable rather
than one without a version.

The owner logs a line when the build differs from the one the application's
values were checked against, the `checked` that its file records, or when it
cannot be read. Either way the values are still read: the line tells the user to
check them in the setup app, whose Values page shows the same warning from the
build of the process behind the best window its match finds. The setup app
reads the build the same way, once for each process it sees, and records it as
described in [Saved files](Setup-app.md#saved-files).

The path is the one the process started from, so an updater that renames the
running executable and writes the new build in its place makes both programs
read the new file until the application restarts. When two copies of different
builds run, the setup app reads the copy whose window fits best and the runtime
the one its session follows, which can differ.

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
an installer or to `start /wait`. The [installer](Installer.md) runs
`--autostart on`, and its uninstaller removes the value itself; the runtime
never turns autostart on by itself. The setup app's Settings page turns the
same value on and off, with the quoted path of the runtime beside its own
program, and reads whether it is there. Autostart is not in the settings file,
because Windows reads the registry and the uninstaller cleans it up.

The log is `runtime.log` in `%LOCALAPPDATA%\AppyTappy\logs\`, one
timestamped line per start, exit, state change, and problem. A line that
would take it past 512 KiB first moves it to `runtime.old.log`, replacing the
older one, so the log normally takes no more than about 1 MiB. When moving
it or opening the new file fails, such as while another program holds the log open, the
runtime keeps writing to the file it has and tries again with the next line,
because the log is its only output. Only the copy that
holds the single-instance guard rotates it; a second copy or an `--autostart`
command only adds its line. When `%LOCALAPPDATA%` is unknown or the log
cannot be opened, nothing is logged, and running exits with code 1.

### Tray

[The tray icon](The-tray-icon.md) in the user guide describes the tray
as the user sees it. The runtime shows an icon in the taskbar's notification
area: the setup app's icon, which the runtime's build script also links into its
program file from `apps/runtime/runtime.rc`. Pointing at it shows a tooltip with
the state, such as `AppyTappy: idle`, or `AppyTappy: disabled` while automations
are off. While the action thread cannot make the media and audio controls, the
tooltip adds `actions cannot run yet`, as in
`AppyTappy: attached, actions cannot run yet`, and the menu starts with a grayed
`Actions cannot run yet` line; see
[When the controls cannot start](Actions.md#when-the-controls-cannot-start).
Right-clicking it opens a small menu:

- **Enabled.** A check mark that turns automations on and off. Off, rules do not
  fire, the readers stop, and what the automations changed is undone, but
  detection keeps running, so turning it back on finds the applications
  already attached. The runtime starts enabled, unless the settings'
  `rememberEnabled` is true; then it starts as the tray last left it, as
  [Files and handoff](#files-and-handoff) describes.
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
until `Tray::stop` drops the sender it waits on, or 10 seconds pass. Windows
ends a program with no visible window that takes more than 5 seconds there,
unless the program gave a shutdown block reason. The procedure therefore
calls `ShutdownBlockReasonCreate` on `WM_QUERYENDSESSION` and agrees at once,
since the undo runs only once the session really ends; Windows then treats
the hidden window like a visible one, which after 5 seconds can show the
reason and let the user end the session anyway. `WM_ENDSESSION` takes the
reason back with `ShutdownBlockReasonDestroy`, also when Windows calls the
session's end off, so a later end that something else holds does not name the
runtime. A critical shutdown, or a user who ends the session anyway, can still
end the runtime before the wait is over. The restore journal lives only in memory: a forced
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

### Footprint

The runtime runs all day beside games, so it has to stay small and use next to
no CPU. `mise run rust:footprint` builds the runtime the installer ships, with
the `dist` profile, and
[`scripts/footprint.ps1`](https://github.com/TheLunchTrae/appy-tappy/blob/main/scripts/footprint.ps1)
runs it on Windows and fails when a measurement exceeds its target. The
`footprint` CI job runs it. The targets, and how long each phase settles and
samples, are the constants at the top of the script.

The script gives the runtime scratch app data folders that hold one
application with three values, two color checks and a bar fill, and an
automation on each. It runs the runtime through five phases:

1. **Idle.** The application is not running.
2. **Active.** A probe program the script compiles shows the application's
   window and repaints it every 16 ms, the way a game does. Two of the values
   change on every frame, so the runtime reads at its fastest. One automation
   fires once, which shows the reads work; its action targets a program that
   does not exist, so it changes nothing. The phase fails if any value cannot
   be read.
3. **Active, trace on.** The same load with the
   [runtime trace](Trace.md) recording every category, which costs the most.
   The script saves the settings and sets the reload event, as the setup app
   does, and turns the trace off again at the end of the phase. This phase has
   no targets: it reports what recording costs, as
   [Overhead](Trace.md#overhead) describes.
4. **Steady.** The probe starts again and keeps repainting every 16 ms, but
   only a strip below the values, so the window changes while its values hold
   still. The runtime's reads back off to their slowest, and its capture asks
   Windows for frames only that often. Most of what that saves is the
   compositor's work, which this script does not measure; the runtime's own
   CPU and memory share the active targets.
5. **Idle after.** The trace has stopped, the probe has exited, and the
   runtime has detached. The trace's buffer is freed once its last handle
   and its writer end, so a handle left behind shows here as memory over the
   target.

Before the last phase, a settle check measures nothing but has to pass. The
probe starts once more with its values dim gray and repaints the strip for
eight seconds after its window shows, so the capture asks for a frame only every
half second. Then it turns the values silver in one last repaint and draws
nothing more. The check fails if the runtime never reads that change, as
happens if Windows drops a frame drawn inside the interval. It also fails if
the change comes less than two seconds after the runtime attached, since it
would then test nothing.

After the last phase, a handle check also measures nothing but has to pass.
The probe opens and closes three more times, the runtime attaching and
detaching each time, and 30 seconds after the last detach the runtime may hold
at most 12 more handles than it did at the end of the idle after phase, a
minute after the third detach, when that capture has finished closing. The
first attaches load libraries and set up thread pools once, which is why the
check starts there. A capture that left its handles behind added
about 10 a cycle, while a clean one adds one or two, mostly idle thread pool
workers that Windows ends later; the script's comment gives the numbers. When
the check fails, the details count the runtime's handles by type, such as
Event or Thread, at both points, to show what grew.

It checks these against their targets:

- **Binary size.** The size of the program the installer ships.
- **Visual C++ Redistributable.** The program must not import the Visual C++
  runtime DLLs. The runtime links the C startup files and the Visual C++
  runtime into the program, as the setup app's Tauri build does, and leaves
  the Universal C runtime to the copy in Windows.
- **Memory.** The private working set at the end of each phase, which Task
  Manager's Details tab shows as Memory. It leaves out the pages of Windows
  DLLs that every program shares. Idle has its own target; active, steady and
  idle after share a larger one.
- **CPU.** The share of one core over each phase's sample. Task Manager shows
  the share of all cores, which is smaller by the number of cores. Both idle
  phases share one target. Windows charges CPU time in ticks of about 16 ms,
  so an idle sample runs long enough that its target spans about four ticks.
  A shared CI machine can still charge a stray tick or two, so a phase whose
  sample is over its CPU target samples again, up to three samples in all,
  and the lowest counts. A runtime over the target the whole time fails every
  sample, though one that is over it only now and then can pass, and the
  details list each sample.

The script also prints each phase's full working set, commit size, threads,
and handles, and the CI job shows the table in its run summary. For the trace
phase it also reports the trace files' size, their rows and rows a second, the
records the trace dropped, and the memory and CPU the trace added to the
active phase. The CPU it added compares the two phases' first samples.

## Sources, rules, and actions

These three parts stay separate so that each one can grow without the others
changing.

- A **source primitive** reads one kind of state: a screen region, a log
  file, a local API, or process and window events. It knows
  nothing about rules. The runtime ships a fixed set; an application is data,
  never code. The v1 set and its Windows APIs belong to
  [#3](https://github.com/TheLunchTrae/appy-tappy/issues/3). [Screen
  regions](Screen-regions.md) describes the screen region source: its checks are
  portable Rust, and only capture calls Windows.
- A **signal** is a named, typed value an application exposes, such as
  `progress`. An app definition binds each signal to a source primitive
  per application version. The format belongs to
  [#6](https://github.com/TheLunchTrae/appy-tappy/issues/6). The runtime and
  the setup app tell which build of an application runs, as [Build
  detection](#build-detection) describes.
- A **rule** says when to act: a condition on signals that fires it when it
  starts to hold, actions, a cooldown, and whether to restore. The engine
  evaluates rules as a pure function with no I/O, so its tests need no
  application and no Windows.
  [Rules](Rules.md) describes the model.
- An **action** is a side effect from a fixed set, such as pausing media or
  setting an app's volume. The engine emits action requests; only the action
  worker performs them. [Actions](Actions.md) describes the v1 set.

## Files and handoff

- **Locations.** `Locations` in `crates/config/src/paths.rs` resolves every
  path, and both programs use it. The setup app does not use Tauri's path
  resolver, which names folders after the app's bundle identifier.
  - `%APPDATA%\AppyTappy\` holds the saved files, which are small and can
    roam with the user's profile. Each application has a folder in `apps\`,
    named by its ID, that holds its `app.json` and the pictures its references
    learned from and its state checks compare against. Every automation is in
    one file, `automations.json`, with the pictures its clicks look for in
    `clicks\` beside it, and the settings in `settings.json`.
    [Setup app](Setup-app.md#saved-files) describes each file.
  - `%LOCALAPPDATA%\AppyTappy\` holds what belongs to this machine, such as
    the installed programs, as [Installer](Installer.md#what-it-does)
    describes, the runtime's log in `logs\`, the
    [runtime trace](Trace.md)'s files in `traces\`, and `enabled.txt`.
- **Writing.** Only the setup app writes the saved files, through
  `crates/config/src/store.rs`. Each write goes to a uniquely named temporary
  file beside the old one, is flushed to disk with `sync_all`, and is renamed
  over the old file. A rename within one folder replaces the file in one step,
  so the runtime never reads half a file. In an application's folder the
  pictures are written before `app.json`, so `app.json` never names a picture
  that is not there yet. Antivirus or backup tools can hold the old file open
  for a moment, so a failed rename is tried again a few times, as
  `RENAME_ATTEMPTS` and `RENAME_WAIT` set, and then the save fails and the
  setup app shows the error.
- **The enabled file.** The runtime writes one file of its own,
  `enabled.txt`, through `EnabledStore` in the same module and in the same
  way. It holds one line, `on` or `off`: what the user last picked from the
  tray's Enabled item. The owner writes it on each pick, whatever the
  settings say, so turning `rememberEnabled` on while the runtime runs
  remembers the picks that follow. A file that cannot be written is logged,
  and the pick still applies. The owner reads it once, when the settings
  first read, even on a retry, and only when they say `rememberEnabled` and
  the user has not picked from the tray in this run; settings read later
  leave the state to the tray. Only `off` means disabled: a file that is
  missing, cannot be read, or holds anything else means enabled. The tray
  starts before that first read and gets a fresh menu after it, so its check
  mark shows the state the runtime starts in. Quitting disables automations
  to undo them, but that is not a pick and writes nothing.
- **Reloading.** After each save of an application, a value, a reference, an
  anti-cheat class, a checked build, an automation, or the settings that
  works, the setup app sets the named event `Local\AppyTappy.Reload`, and the
  runtime reads `settings.json`, every `app.json`, and `automations.json`
  again, as the [reload watch](#the-runtime) describes. Each save passes its
  result and what it changed to `Owner::after` in
  `apps/setup/src-tauri/src/owner.rs`, which sets the event only when the save
  worked. Turning an overlay on or off, changing its mode, and moving its
  panel also write `app.json`, but they do not set the event, because the
  runtime does not use the overlay. A runtime that is not running reads the
  files when it starts, so the setup app ignores a failure to set the event.
- **Reading.** The runtime reads the files with the same `config` code the setup
  app uses, and checks everything again before use, because the files are
  outside its control and a person can edit them by hand. Each `app.json` must
  parse and pass `AppFile::parse`'s checks, which cover the file as a whole,
  each automation must pass `AutomationDef::compile` against the saved
  applications, and each value it watches must pass `saved::build` with its
  pictures. Saving a value in the setup app calls `saved::build`, and saving an
  automation calls `compile` and builds the value it watches, so what saves is
  what runs. `apps/runtime/src/plan.rs` turns what passes into the plan the
  runtime runs, in memory; nothing it builds is written to disk. An automation
  or a value that fails a check is logged and left out, and the rest run.
- **Format.** `app.json`, `automations.json`, and `settings.json` each carry a
  `format` number, and the `FORMAT` constant in `app_file.rs`,
  `automation_file.rs`, and `settings_file.rs` sets the one version this build
  reads and writes. A setting that `settings.json` leaves out takes its
  default; one it does not know makes the file unreadable, and then the runtime
  keeps the settings it has, or the defaults at start, and the Settings page
  shows the defaults and says why. Neither program converts another
  version. An `app.json` with another version counts as unreadable, so its
  application's automations do not run, and the log names its folder. An
  `automations.json` with another version cannot be read as a whole: the
  runtime keeps running what it ran, or runs nothing if it has not read the
  files yet. Until the file is fixed, the setup app saves no automations, and
  it cannot remove an application or delete a value, because it checks the
  file to see whether an automation still uses them.
- **Application versions.** A saved value has one binding for every version of
  its application. `app.json` records the build its values were checked
  against, and both programs warn when another build runs, as [Build
  detection](#build-detection) describes.

## Anti-cheat enforcement

AppyTappy must never get a user banned. The policy and its classes belong to
[#11](https://github.com/TheLunchTrae/appy-tappy/issues/11). For any application
whose anti-cheat class is not `none`, input-injection actions are blocked;
screen, log, local API, and process and window event sources stay allowed. The
design enforces the block in three places:

1. The setup app refuses to save or turn on an automation whose actions the
   application's class blocks. The one blocked item the saved files can hold
   is a key press action, which `AutomationDef::compile` refuses unless the
   class is `none`.
2. The runtime runs the same check each time it reads the saved files, so an
   automation that a hand edit or a later change to the application's class
   made blocked does not run.
3. For rules and actions, parsing and validation produce different types.
   Reading `automations.json` gives `AutomationDef` and `ActionDef`, which the
   engine and the action worker do not accept; only `AutomationDef::compile`
   and `ActionDef::to_action` in `config` turn them into a `Rule` and its
   `Action`s, and `compile` reads the rule's actions to find a key press.
   `Rule` has private
   fields, an action's values such as a key chord or a volume level are types
   that only their checked constructors make, and neither has `Deserialize` or
   `Default`, so code that skips validation does not compile. The screen
   source has no such split: `saved::build` in `appytappy-sources` takes the
   raw `ScreenDef` and `ReferenceDef` from `app.json` and checks them as it
   builds the check it reads with. It is the one owner of the rules for a
   readable value, such as a state check's states, so one value that breaks
   them stops only itself.

Nothing in AppyTappy opens another application's process. [Build
detection](#build-detection) reads only the executable file, and process and
window discovery read system-wide snapshots.

## The setup app

The setup app is a Tauri v2 app with a React and TypeScript frontend, in
`apps/setup/`. The owner chose Tauri, which settles the framework question in
[#16](https://github.com/TheLunchTrae/appy-tappy/issues/16).

Its Rust side exposes Tauri commands and holds no logic of its own: discovery,
capture, preview, and saving come from the shared crates. It follows the
same ownership rule as the runtime. One owner thread holds the setup app's
state, and Tauri keeps only that thread's `Sender`, never state behind a
`Mutex`. Each command is `async`: it sends the owner thread a `Run`, a
closure that calls one of the owner's methods, with a `tokio::sync::oneshot`
reply channel, and awaits the reply, so a slow capture or save never freezes
the window. A new command is a method on the owner, a command in `commands.rs`
that calls it, its name in `generate_handler!` in `main.rs`, and its call in
the UI's `api.ts`, whose types the setup app generates
([Page types](Setup-app.md#page-types)). The AI-assisted discovery flow
belongs to [#15](https://github.com/TheLunchTrae/appy-tappy/issues/15).

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
UI. The [user guide](Home.md#user-guide) describes the screens, and
[Setup app](Setup-app.md) how they work.

The overlay ([#36](https://github.com/TheLunchTrae/appy-tappy/issues/36))
is more windows of the setup app, one for each application whose overlay is
on, so nothing is hooked or injected into the application. Each window loads
the same UI, which renders only the readings when its label names an
application. The owner thread keeps one capture for each overlay, which
copies the values that read at the window's current size with the same
`Layout` and the same `SavedValue::at` as a reader, and the capture thread
reads them from each frame, so only the readings cross to the owner. The
window over the application is transparent, lets clicks through, and
cannot take the focus. Tauri's window library activates a window it shows,
so `platform` shows it once, off the screen, without activating it.
From then on a thread of its own asks the owner over the channel where it goes
about twenty times a second: the corner of the application's window while that
window is in front, at the size its page measured for the card, and off the
screen otherwise. Moving it never shows it again, so a page that the webview
throttles cannot hold its place back; the page's own polls still open the
overlay, start its capture and restart it after a change. `sources` answers where
one window is with a few calls about that window alone, so following it costs
no listing of every window. The windows are matched to the overlays that are on
when the main window opens, after an overlay changes and after an application
is removed; a window that cannot open or close is reported and does not stop
the others, and a removal or overlay change that worked is not undone by such
a report. With no monitors listed, a panel opens where it was left. The
picture-in-picture panel reports where it was moved, and the owner saves that
once the panel stops moving.

## Unsafe code

The standards deny `unsafe` code. Win32 and COM calls through the `windows`
crate are `unsafe`, because Rust cannot check what Windows does with the
pointers it receives. That covers Core Audio, window hooks, the tray icon,
directory watching, and the Direct3D interop that screen capture needs. WinRT
calls, such as media sessions and the screen capture API itself, are safe Rust
methods. A `win.rs` module's
submodules, such as screen capture in `sources/src/win/capture.rs` and the
tray in `platform/src/win/tray.rs`, follow the same rules.

The owner approved changing the workspace lint from `forbid` to `deny` with the
first `unsafe` Windows call, in `sources`, as
[`AGENTS.md`](https://github.com/TheLunchTrae/appy-tappy/blob/main/AGENTS.md#exceptions-and-workspace-policy)
describes. `unsafe` may appear only in the `win.rs` modules of `sources`,
`actions`, and `platform`, and each block carries a reasoned expectation and a
`SAFETY` comment. Every crate root without such a module, including `config`,
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
- **[`ts-rs`](https://docs.rs/ts-rs).** Writes TypeScript declarations of
  Rust types from their serde attributes, so the setup app's page cannot
  drift from the JSON the Rust side sends and reads, which hand-written types
  did. `appytappy-config` takes it only behind its `ts` feature, which only
  the setup app's tests turn on, so neither program ships it.
- **[`serde_json`](https://docs.rs/serde_json).** JSON support for `serde`;
  the encoding of the saved files.
- **[`windows`](https://github.com/microsoft/windows-rs).** Microsoft's
  official Rust bindings for Windows APIs, generated from Windows metadata.
  Its features select which APIs compile, which mainly keeps build times down.
  The runtime's tray icon, message loops, and directory watching use it
  directly, with no extra crates. Screen capture does too, rather than through
  the `windows-capture` crate, which adds its own thread pool and locks.
- **[`png`](https://docs.rs/png).** Reads and writes the PNG files that hold
  a reference's samples and a state check's pictures.
- **[`tauri`](https://v2.tauri.app/).** The setup app framework: a Rust
  backend with a web frontend shown in WebView2.
- **[`tokio`](https://tokio.rs/).** The async runtime Tauri runs on. The setup
  app lists it directly, with only its `sync` feature, for the `oneshot`
  channel its commands await; Tauri does not re-export that channel.
- **[`tauri-plugin-global-shortcut`](https://v2.tauri.app/plugin/global-shortcut/).**
  Tauri's plugin for a key press that works while another window has focus,
  which the setup app uses for its screenshot shortcut.
- **[`tauri-plugin-single-instance`](https://v2.tauri.app/plugin/single-instance/).**
  Tauri's plugin that keeps one copy of the setup app running, so two copies
  never save over each other or open the same overlays. A second copy tells
  the first to bring its window forward and exits before it opens a window. A
  named mutex like the runtime's would find the running copy too, but bringing
  its window forward would also need a message between the two processes.
- **[`proptest`](https://docs.rs/proptest).** A dev-dependency that generates
  many random inputs to check a property. `appytappy-sources` uses it to check
  that a picture encoded as PNG decodes back the same. The standards
  require it at trust boundaries such as file formats.
