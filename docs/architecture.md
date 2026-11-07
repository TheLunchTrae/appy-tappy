# Architecture

AppyTappy has two programs that share a set of library crates:

- The **setup app** runs when the user wants to change something. It finds out
  how to read a game's state, lets the user build rules, and compiles them into
  one file.
- The **runtime** runs in the background all the time. It loads that file,
  watches the games it names, and runs actions when a rule fires.

The setup app runs rarely, so it can afford a heavier UI. The runtime is always
on, so it must stay small. Every design choice below follows from that split.
[`CONTEXT.md`](../CONTEXT.md) defines the domain terms.

## Components and data flow

```text
                setup app (Tauri, runs on demand)
  ┌─────────────────────────────────────────────────────────┐
  │ React UI ── Tauri commands ── owner thread              │
  │             discovery, scan, preview, compile           │
  └───────────────┬──────────────────────────▲──────────────┘
    reads, writes │ writes atomically        │ reads
  authoring files ▼                          │
                compiled config          status file
                  │ watched, revalidated     ▲ writes
                  ▼                          │
                runtime (plain Rust, always on)
  ┌─────────────────────────────────────────────────────────┐
  │ config watcher ─ ConfigChanged ─┐                       │
  │ lifecycle ─ started, exited ────┤                       │
  │ source workers ─ samples ───────┼─► owner thread:       │
  │ action worker ─ results ────────┤   root model + engine │
  │ tray ─ pause, open, quit ───────┘           │           │
  │ ◄── action requests, worker start and stop ─┘           │
  └─────────────────────────────────────────────────────────┘
```

1. The setup app reads and writes **authoring files**: app definitions (how
   to read a game) and the user's rules.
2. Its compile step validates them, applies the anti-cheat policy, and writes
   one **compiled config** that holds only what the runtime needs.
3. The runtime watches that file, validates it again, and swaps it in. It
   never reads authoring files.
4. The runtime writes a **status file**: which games are attached and the
   last problem, such as a rejected config. The setup app reads it to show the
   user what the runtime is doing. A file cannot say whether its writer is
   still alive, so the setup app checks that the runtime is running through
   the runtime's single-instance guard instead.

The two programs never talk directly: no sockets, pipes, or shared memory.
Files carry everything between them. The setup app previews and tests rules
itself, because it links the same `engine` and `sources` crates that the
runtime uses.

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
| setup app            | Windows  | `config`, `engine`, `sources`, `tauri`, `tokio`      | —              |

Each crate's owning issue adds its code and its dependencies. Where issues #6, #8,
and #11 say "the core crate", they mean `appytappy-config` for formats, validation,
and policy, and `appytappy-engine` for rule evaluation. `config` owns what a
condition means, so the setup app and the runtime agree on it; `engine` owns
the state of each game session. `apps/setup/src-tauri`
joins the workspace `members` list with
[#16](https://github.com/TheLunchTrae/appy-tappy/issues/16).

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
[`AGENTS.md`](../AGENTS.md#tests-and-mutation) requires that approval for any
configuration exclusion.

## The runtime

The runtime is one process for all games. It uses no async runtime and no
webview, and it keeps its dependencies few. It is built as a Windows GUI
program (`#![windows_subsystem = "windows"]`), so no console window opens. It
has no console output, so it logs only to `%LOCALAPPDATA%\AppyTappy\logs\`.
Footprint targets belong to
[#12](https://github.com/TheLunchTrae/appy-tappy/issues/12).

It follows the standards' ownership rule: one thread owns all state, and other
threads send it events over channels instead of sharing state behind locks.
Each thread below is a plain `std::thread`, and each channel is a
`std::sync::mpsc` channel.

- **Owner thread.** Holds the root model: the loaded compiled config, which
  games are attached, and one engine session per attached game, which holds
  its signal values and rule state. It
  applies each event as a `(state, event) -> (state, effects)` transition.
  Effects are action requests and orders to start or stop a worker.
- **Lifecycle.** Reports `GameStarted` and `GameExited` from the process and
  window events source. It matches a new window's process to a game by the
  executable name in a process snapshot (`CreateToolhelp32Snapshot`), which
  does not open the process, so it is safe for every anti-cheat class. It
  learns that a game exited the same way, from window-destroy events and a
  fresh snapshot, never by waiting on a process handle. While no configured
  game runs, nothing polls. Detection and autostart belong to
  [#13](https://github.com/TheLunchTrae/appy-tappy/issues/13).
- **Source workers.** One per attached game. Each runs the bindings the
  compiled config chose for that game's version and sends `Sample` events.
  Push sources, such as a log tail, send when data arrives; polled sources,
  such as a memory read, run at the interval the binding sets.
- **Action worker.** Runs action requests and releases in order and sends
  back results. It keeps the restore journal: the state each restoring rule
  changed, such as the volume before a change, so a release can put it back
  even when several rules changed the same thing.
- **Config watcher.** Watches the compiled config's folder, ignores every file
  but the compiled config, and turns a burst of change events into one
  `ConfigChanged`. The owner validates the new file and swaps it in only if it
  is valid. Otherwise it keeps the old config, logs the problem, and shows it
  in the status file and the tray.
- **Message thread.** Owns the hidden window behind the tray icon, runs the
  Windows message loop, and sends tray commands to the owner. The tray shows
  idle, attached, or problem and offers pause, open setup, and quit. Whether
  the tray is worth its cost belongs to
  [#18](https://github.com/TheLunchTrae/appy-tappy/issues/18); without it, the
  status file still reports the same states. A single-instance guard stops a
  second copy of the runtime from starting.

Shared flags are banned, so a worker learns to stop from its channel. Each
worker owns a control `Receiver` and waits on it with `recv_timeout`, using the
poll interval as its tick. A stop message, or the owner dropping the `Sender`,
ends the worker. A push source must also be able to cancel its blocking read,
for example with `CancelIoEx`, so that it does not hang on a quiet file.

Some Windows callbacks, such as the hook that reports window events, carry no
pointer back to the code that registered them. The usual way out is a static
or a thread-local, and the standards ban both. Instead, the callback posts a
message to its own thread with `PostThreadMessageW`. That thread's message loop
owns the `Sender` and forwards the event over the channel. The window events
hook runs on its own thread, separate from the tray's message thread. A modal
loop, such as the one an open tray menu runs, drops thread messages, so a
thread that receives them must never show UI.

## Sources, rules, and actions

These three parts stay separate so that each one can grow without the others
changing.

- A **source primitive** reads one kind of state: a memory pointer chain, a
  screen region, a log file, a local API, or process and window events. It
  knows nothing about rules. The runtime ships a fixed set; a game is data,
  never code. The v1 set and its Windows APIs belong to
  [#3](https://github.com/TheLunchTrae/appy-tappy/issues/3).
  [Screen regions](screen.md) describes the screen region source: its checks
  are portable Rust, and only capture calls Windows.
- A **signal** is a named, typed value a game exposes, such as
  `level_percent`. An app definition binds each signal to a source primitive
  per game version. The format belongs to
  [#6](https://github.com/TheLunchTrae/appy-tappy/issues/6), and version
  changes to [#7](https://github.com/TheLunchTrae/appy-tappy/issues/7).
- A **rule** says when to act: a trigger on signals or game events, with
  conditions, a cooldown, and re-arming. The engine evaluates rules as a pure
  function with no I/O, so its tests need no game and no Windows.
  [Rules](rules.md) describes the model.
- An **action** is a side effect from a fixed set, such as pausing media or
  setting an app's volume. The engine emits action requests; only the action
  worker performs them. [Actions](actions.md) describes the v1 set.

## Files and handoff

- **Locations.** One function in `config` resolves every path, and both
  programs call it. The setup app does not use Tauri's path resolver, which
  names folders after the app's bundle identifier.
  - `%APPDATA%\AppyTappy\` holds the authoring files, which are small and can
    roam with the user's profile.
  - `%LOCALAPPDATA%\AppyTappy\` holds what belongs to this machine: the
    compiled config in `compiled\`, the status file, scan caches, and logs in
    `logs\`. The compiled config names install paths and memory layouts that
    can differ between machines.
- **What is resolved at compile time.** Each signal resolves to the concrete
  binding for each known game version, and every rule and action is
  validated. The runtime only picks the binding for the version it detects.
  It detects the version from the game's executable file on disk, at the
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
[#11](https://github.com/TheLunchTrae/appy-tappy/issues/11). For any game whose
anti-cheat class is not `none`, three things are blocked: memory sources,
input-injection actions, and anything else that opens the game's process.
Screen, log, local API, and process and window event sources stay allowed.
The design enforces the block in four places:

1. The compile step in `config` refuses a blocked binding or action.
2. The runtime applies the same check when it loads the compiled config.
3. Parsing and validation produce different types. Reading a file gives a raw
   type that nothing else accepts; only `config` validation turns it into the
   validated type that sources and actions take. The validated type has
   private fields and no `Deserialize` or `Default`, so code that skips
   validation does not compile.
4. Every path that opens a game's process needs a process-access permit,
   including the guided memory scan
   ([#14](https://github.com/TheLunchTrae/appy-tappy/issues/14)) and any
   version detection that reads the running process. Only `config` issues one, and
   only for a game of class `none`. The scan runs before any binding exists,
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

The Tauri work in #16 needs two gate changes, each with the owner's approval.
Tauri's Linux build pulls in GTK, so CI needs either GTK's development
packages or a setup-app check that runs only on Windows. One of those
Linux-only GTK dependencies is also unmaintained, which fails `rust:deny`;
setting `[graph] targets = ["x86_64-pc-windows-msvc"]` in `deny.toml` checks
only what ships on Windows. The same change brings in the TypeScript standards
profile.

## Unsafe code

The standards deny `unsafe` code. Win32 and COM calls through the `windows`
crate are `unsafe`, because Rust cannot check what Windows does with the
pointers it receives. That covers reading another process's memory, Core
Audio, window hooks, the tray icon, directory watching, and the Direct3D
interop that screen capture needs. WinRT calls, such as media sessions and the
screen capture API itself, are safe Rust methods. Screen capture lives in
`sources/src/win/capture.rs`, a submodule of `win.rs`, under the same rules.

The owner approved changing the workspace lint from `forbid` to `deny` with
the first `unsafe` Windows call, in `sources`, as
[`AGENTS.md`](../AGENTS.md#exceptions-and-workspace-policy) describes.
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
  The runtime's tray icon, message loop, and directory watching use it
  directly, with no extra crates. Screen capture does too, rather than through
  the `windows-capture` crate, which adds its own thread pool and locks.
- **[`png`](https://docs.rs/png).** Reads and writes the PNG files that hold
  a screen check's reference picture.
- **[`tauri`](https://v2.tauri.app/).** The setup app framework: a Rust
  backend with a web frontend shown in WebView2.
- **[`tokio`](https://tokio.rs/).** The async runtime Tauri runs on. The setup
  app lists it directly, with only its `sync` feature, for the `oneshot`
  channel its commands await; Tauri does not re-export that channel.
- **[`proptest`](https://docs.rs/proptest).** A dev-dependency that generates
  many random inputs to check a property, such as "a compiled config reads back
  to the same value". The standards require it at trust boundaries such as file
  formats.
