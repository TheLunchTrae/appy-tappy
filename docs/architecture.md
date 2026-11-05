# Architecture

AppyTappy has two programs that share a set of library crates:

- The **setup app** runs when the user wants to change something. It finds out
  how to read a game's state, lets the user build rules, and compiles them into
  one file.
- The **runtime** runs in the background all the time. It loads that file,
  watches the games it names, and runs actions when a rule fires.

The setup app may be large because it runs rarely. The runtime must stay
small, because it is always on. Every design choice below follows from that
split. [`CONTEXT.md`](../CONTEXT.md) defines the domain terms.

## Components and data flow

```text
                 setup app (Tauri, runs on demand)
  ┌──────────────────────────────────────────────────────────┐
  │ React UI ── Tauri commands ── discovery, scan, compile    │
  │                                (config, engine, sources) │
  └──────────────────────────────┬───────────────────────────┘
       reads and writes          │ writes atomically
  app definitions + rules        ▼
  (authoring files)        compiled config
                                 │ watched and loaded
                                 ▼
                 runtime (plain Rust, always on)
  ┌──────────────────────────────────────────────────────────┐
  │ lifecycle ─ game started/exited ─┐                       │
  │ source workers ─ samples ────────┼─► owner thread:       │
  │ action worker ─ results ─────────┘   root model + engine │
  │                ◄──── action requests ────┘               │
  │ tray icon                                                │
  └──────────────────────────────────────────────────────────┘
```

1. The setup app reads and writes **authoring files**: app definitions (how
   to read a game) and the user's rules.
2. Its compile step validates them, applies the anti-cheat policy, and writes
   one **compiled config** that holds only what the runtime needs.
3. The runtime watches that file, validates it again, and swaps it in. It
   never reads authoring files.

The flow goes one way. The runtime does not talk to the setup app, and the
setup app does not talk to the runtime. The setup app previews and tests rules
itself, because it links the same `engine` and `sources` crates the runtime
uses.

## Workspace layout

The repository is one Cargo workspace. `apps/` holds the two programs and
`crates/` holds the libraries they share.

```text
Cargo.toml               workspace root; hosts the standards policy tests
apps/
  runtime/               appytappy-runtime: the always-on binary
  setup/                 the Tauri setup app (added by #16)
crates/
  config/                appytappy-config: formats, validation, compile step
  engine/                appytappy-engine: the rule evaluator
  sources/               appytappy-sources: Windows source primitives
  actions/               appytappy-actions: Windows actions
```

| Crate               | Platform | Depends on                               | Used by        |
| ------------------- | -------- | ---------------------------------------- | -------------- |
| `appytappy-config`  | any      | `serde`, `serde_json`                    | setup, runtime |
| `appytappy-engine`  | any      | `config`                                 | setup, runtime |
| `appytappy-sources` | Windows  | `config`, `windows`                      | setup, runtime |
| `appytappy-actions` | Windows  | `config`, `windows`                      | runtime        |
| `appytappy-runtime` | Windows  | `config`, `engine`, `sources`, `actions` | —              |
| setup app           | Windows  | all four libraries, `tauri`              | —              |

The skeleton crates exist now and are empty. The issues that own each part add
the code and the dependencies.

### Portable core, thin Windows adapters

`config` and `engine` contain no Windows calls, so the Linux `quality` job
lints, tests, and mutation-tests them in full. `sources`, `actions`, and the
runtime wrap Windows APIs. Their Windows code sits behind `#[cfg(windows)]`
and Windows-only dependencies under `[target.'cfg(windows)'.dependencies]`, so
those crates still build on Linux, where they are nearly empty. The `windows`
CI job lints and documents them on Windows. Their Windows code can run only
there, so the first change that gives these crates tests adds a test step to
that job.

Mutation testing runs only in the Linux gate, so logic that can be wrong
belongs in `config` or `engine`. A Windows adapter turns one API call into one
typed value or error and holds no decisions of its own.

## The runtime

The runtime is one process for all games. A single-instance guard stops a
second copy from starting. It shows a tray icon and no window, uses no async
runtime and no webview, and keeps its dependencies few. Footprint targets
belong to [#12](https://github.com/TheLunchTrae/appy-tappy/issues/12).

It follows the standards' ownership rule: one thread owns all state, and other
threads send it events over channels instead of sharing state behind locks.

- **Owner thread.** Holds the root model: the loaded compiled config, which
  games are attached, current signal values, and the engine's rule state. It
  applies each event as a `(state, event) -> (state, action requests)`
  transition.
- **Lifecycle.** Reports `GameStarted` and `GameExited`. While no configured
  game runs, nothing polls. Detection and autostart belong to
  [#13](https://github.com/TheLunchTrae/appy-tappy/issues/13).
- **Source workers.** One per attached game. Each runs the bindings the
  compiled config chose for that game's version and sends `Sample` events.
  Push sources, such as a log tail, send when data arrives; polled sources,
  such as a memory read, run at the interval the binding sets. A worker stops
  when its game exits.
- **Action worker.** Runs action requests in order and sends back results,
  including any prior state a paired action needs to restore, such as the
  volume before a change.
- **Config watcher.** Sends `ConfigChanged` when the compiled config file
  changes. The owner validates the new file and swaps it in only if it is
  valid; otherwise it keeps the old one and reports the problem.

## Sources, rules, and actions

These three parts stay separate so that each one can grow without the others
changing.

- A **source primitive** reads one kind of state: a memory pointer chain, a
  screen region, a log file, a local API, or process and window events. It
  knows nothing about rules. The runtime ships a fixed set; a game is data,
  never code. The v1 set and its Windows APIs belong to
  [#3](https://github.com/TheLunchTrae/appy-tappy/issues/3).
- A **signal** is a named, typed value a game exposes, such as
  `level_percent`. An app definition binds each signal to a source primitive
  per game version. The format belongs to
  [#6](https://github.com/TheLunchTrae/appy-tappy/issues/6), and version
  changes to [#7](https://github.com/TheLunchTrae/appy-tappy/issues/7).
- A **rule** says when to act: a trigger on signals or game events, with
  conditions, debounce, and re-arming. The engine evaluates rules as a pure
  function with no I/O, so its tests need no game and no Windows. The model
  belongs to [#9](https://github.com/TheLunchTrae/appy-tappy/issues/9).
- An **action** is a side effect from a fixed set, such as pausing media or
  setting an app's volume. The engine emits action requests; only the action
  worker performs them. The v1 set belongs to
  [#10](https://github.com/TheLunchTrae/appy-tappy/issues/10).

## Compiled config handoff

- **Location.** Authoring files and the compiled config live in
  `%APPDATA%\AppyTappy\`. Logs live in `%LOCALAPPDATA%\AppyTappy\logs\`.
- **What is resolved at compile time.** Each signal resolves to the concrete
  binding for each known game version, and every rule and action is
  validated. The runtime only picks the binding for the version it detects.
- **Writing.** The setup app writes a temporary file in the same directory
  and renames it over the old one, so the runtime never reads a half-written
  file.
- **Reading.** The runtime watches the directory and reloads on change. It
  validates everything again before use, because the file is outside its
  control.
- **Format.** The file carries a format version. The runtime rejects a version
  it does not know and keeps running with its current config. Encoding,
  integrity checks, and version policy belong to
  [#8](https://github.com/TheLunchTrae/appy-tappy/issues/8).

## Anti-cheat enforcement

AppyTappy must never get a user banned. The policy belongs to
[#11](https://github.com/TheLunchTrae/appy-tappy/issues/11); the architecture
enforces it in three places:

1. The compile step in `config` refuses a memory binding for any game whose
   anti-cheat class is not `none`.
2. The runtime applies the same check when it loads the compiled config.
3. The memory source accepts only a binding type that `config` validation
   produces, so code that skips validation does not compile.

## The setup app

The setup app is a Tauri v2 app with a React and TypeScript frontend, in
`apps/setup/`. Its Rust side exposes Tauri commands and holds no logic of its
own: discovery, scanning, preview, and compiling come from the shared crates.
It follows the same ownership rule as the runtime: commands send requests to
one owner thread over a channel, instead of keeping state in Tauri's managed
`State<Mutex<T>>`, which the standards ban.
The guided memory scan ([#14](https://github.com/TheLunchTrae/appy-tappy/issues/14))
reads process memory, so it lives with the memory source. The AI-assisted
discovery flow belongs to
[#15](https://github.com/TheLunchTrae/appy-tappy/issues/15), and the shell to
[#16](https://github.com/TheLunchTrae/appy-tappy/issues/16).

Tauri uses the WebView2 runtime that Windows 11 already ships, so the app
stays a few megabytes on disk. A webview costs more memory than a native UI
while the app is open, but the app runs rarely and the always-on runtime has
no UI framework at all. Two native Rust toolkits would be lighter:

- **egui** draws everything itself each frame. It has a small binary and no
  webview, but it looks less like a Windows app and complex forms such as the
  rule editor take more work.
- **Slint** uses its own declarative UI language and native-feeling widgets
  with a small footprint. It would mean learning that language instead of
  using React.

Tauri wins because the setup app's cost matters little and React lets the UI
move fastest. The setup app's CI needs (Tauri's Linux build dependencies, or
running its checks only on Windows) are settled with #16, together with the
TypeScript standards profile.

## Unsafe code

The standards forbid `unsafe` code. Reading another process's memory, Core
Audio, and media controls go through the `windows` crate, whose calls are
`unsafe` because Rust cannot check what Windows does with the pointers it is
given. The first change that adds a Windows call
([#3](https://github.com/TheLunchTrae/appy-tappy/issues/3) or
[#4](https://github.com/TheLunchTrae/appy-tappy/issues/4)) needs the owner's
explicit approval to:

- change `unsafe_code` from `forbid` to `deny` in `[workspace.lints.rust]`,
  and add `undocumented_unsafe_blocks = "deny"` to `[workspace.lints.clippy]`;
- keep `#![forbid(unsafe_code)]` at the root of `config`, `engine`, the
  runtime, and the setup app, so `unsafe` can appear only in `sources` and
  `actions`;
- give each `unsafe` block a reasoned `#[expect(unsafe_code, reason = "...")]`
  and a `// SAFETY:` comment that states why the call is sound.

## Crate glossary

Terms:

- **Crate.** A unit of Rust code that compiles on its own: a library or a
  binary. Each crate here lives in its own folder with a `Cargo.toml`.
- **Package.** A `Cargo.toml` and the crates it builds. Here each package
  holds one crate, so the words mean the same thing.
- **Workspace.** Packages that share one `Cargo.lock`, one `target/` build
  folder, and the lint rules in the root `Cargo.toml`.
- **Dependency.** A crate from [crates.io](https://crates.io/), Rust's package
  registry, or another crate in the workspace, listed in `[dependencies]`.
  `Cargo.lock` pins the exact version of each.
- **`cfg(windows)`.** Marks code or dependencies that exist only when
  compiling for Windows.
- **Feature.** An optional part of a crate that a dependent turns on in
  `Cargo.toml`, so it compiles only what it uses.

Dependencies the design expects. Each is added with the issue that needs it,
and `AGENTS.md` asks every addition to state what complexity it removes.

- **[`serde`](https://serde.rs/).** Converts Rust types to and from data
  formats. Every format crate builds on it.
- **[`serde_json`](https://docs.rs/serde_json).** JSON support for `serde`;
  the likely first encoding for the compiled config (#8).
- **[`windows`](https://github.com/microsoft/windows-rs).** Microsoft's
  official Rust bindings for Windows APIs, generated from Windows metadata.
  Its features select which APIs compile, which keeps builds and binaries
  small.
- **[`tauri`](https://v2.tauri.app/).** The setup app framework: a Rust
  backend with a web frontend shown in WebView2.
- **[`proptest`](https://docs.rs/proptest).** Test-only. Generates many random
  inputs to check a property, such as "a compiled config reads back to the
  same value". The standards require it at trust boundaries such as file
  formats.
