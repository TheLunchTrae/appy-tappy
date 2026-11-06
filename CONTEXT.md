# Context

Domain vocabulary for AppyTappy. Use these terms in code, docs, and issues.
[`docs/architecture.md`](docs/architecture.md) shows how the parts fit.

- **Action.** A side effect from the runtime's fixed set, such as pausing media
  or setting an app's volume. A paired action restores the state it changed.
- **Anti-cheat class.** How a game protects itself: `none`, `user-mode`,
  `kernel-level`, or `unknown`. Only `none` permits memory sources,
  input-injection actions, and anything else that opens the game's process.
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
- **Process-access permit.** The value `config` issues for a game of class
  `none`; every path that opens a game's process requires one.
- **Cooldown.** How long after firing a rule cannot fire again. It is not a
  hold time: a rule fires on the first value that makes its condition hold.
- **Re-arm.** Let a rule that fired fire again, once its condition stops
  holding and its re-arm condition holds.
- **Release.** End a fired rule's effect: it re-armed, the opposite focus
  event happened, or the session ended. A rule that restores undoes its
  paired actions then.
- **Rule.** A trigger, a cooldown, and whether its paired actions restore.
- **Runtime.** The always-on background program.
- **Sample.** One value a source read for a signal.
- **Session.** One run of a game, from its start to its exit, or until the
  runtime stops it for a pause or a config change. Rule state resets with
  each session.
- **Setup app.** The program the user opens to discover signals, build rules,
  and compile the config.
- **Signal.** A named, typed value a game exposes, such as `level_percent`.
- **Source primitive.** One kind of state reader the runtime ships: memory
  pointer chain, screen region, log file, local API, or process and window
  events.
- **Status file.** The file the runtime writes and the setup app reads: which
  games are attached and the last problem.
- **Trigger.** The condition on signals or game events that makes a rule fire.
