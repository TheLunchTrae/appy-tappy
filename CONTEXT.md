# Context

Domain vocabulary for AppyTappy. Use these terms in code, docs, and issues.
[`docs/architecture.md`](docs/architecture.md) shows how the parts fit.

- **Action.** A side effect from the runtime's fixed set, such as pausing media
  or setting an app's volume. A paired action restores the state it changed.
- **Anti-cheat class.** How a game protects itself: `none`, `user-mode`,
  `kernel-level`, or `unknown`. Only `none` permits memory sources.
- **App definition.** Data, never code, that says how to recognize one game,
  detect its version, and read its signals.
- **Authoring files.** The app definitions and rules the setup app reads and
  writes. The runtime never reads them.
- **Binding.** The concrete way to read one signal for one game version: a
  source primitive and its parameters, such as a pointer chain.
- **Compiled config.** The one file the setup app compiles from authoring
  files and the runtime loads.
- **Game.** Any application AppyTappy watches. Most are games, but nothing
  requires it.
- **Rule.** A trigger, optional conditions, and the actions to run when it
  fires.
- **Runtime.** The always-on background program.
- **Sample.** One value a source read for a signal.
- **Setup app.** The program the user opens to discover signals, build rules,
  and compile the config.
- **Signal.** A named, typed value a game exposes, such as `level_percent`.
- **Source primitive.** One kind of state reader the runtime ships: memory
  pointer chain, screen region, log file, local API, or process and window
  events.
- **Trigger.** The condition on signals or game events that makes a rule fire.
