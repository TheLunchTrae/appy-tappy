# Context

Domain vocabulary for AppyTappy. Use these terms in code, docs, and issues.
[`wiki/developer/Architecture.md`](wiki/developer/Architecture.md) shows how the
parts fit.

- **Action.** A side effect from the runtime's fixed set, such as pausing media
  or setting an app's volume. A paired action restores the state it changed.
- **Anti-cheat class.** How an application protects itself from other
  programs: `none`, `user-mode`, `kernel-level`, or `unknown`. A protected
  application is one whose class is not `none`, such as one with anti-cheat.
  Only `none` permits memory sources, input-injection actions, and anything
  else that opens the application's process.
- **App definition.** Data, never code, that says how to recognize one
  application, detect its version, and read its signals.
- **Application.** Any program AppyTappy watches: a game, a media player, a
  browser, or anything else with a window. Code and short UI labels call it
  an app.
- **Authoring files.** The files the setup app saves: each application's
  `app.json` with its pictures, and `automations.json`. The runtime reads them
  when it starts and again each time the setup app saves.
- **Binding.** The concrete way to read one signal for one application
  version: a source primitive and its parameters, such as a pointer chain.
- **Check.** How a screen region binding turns a region's pixels into a value:
  a number read with a reference, a bar's fill, a color match, or a match
  with a saved image.
- **Process-access permit.** The value `config` issues for an application of
  class `none`; every path that opens an application's process requires one.
- **Cooldown.** How long after firing a rule cannot fire again. It is not a
  hold time: a rule fires on the first value that makes its condition hold.
- **Read interval.** The minimum and maximum time between a source's reads.
  The source reads at the minimum while its value moves and backs off toward
  the maximum while it holds still.
- **Re-arm.** Let a rule that fired fire again, once its condition stops
  holding and its re-arm condition holds.
- **Release.** End a fired rule's effect: it re-armed, the opposite focus
  event happened, or the session ended. A rule that restores undoes its
  paired actions then.
- **Recording.** A setup-app capture of one number region over time that
  keeps each different reading once, for the user to review and label. Kept
  readings teach a reference like labeled screenshots.
- **Reference.** The characters of one font in one application, learned from
  labeled pictures of its text. A number check reads with one, at any size.
- **Rule.** A trigger, the actions to run when it fires, a cooldown, and
  whether those actions restore.
- **Runtime.** The always-on background program.
- **Sample.** One value a source read for a signal.
- **Session.** One run of an application, from its start to its exit, or until
  the runtime stops it for a pause or a config change. Rule state resets with
  each session.
- **Setup app.** The program the user opens to discover signals, build rules,
  and save the authoring files.
- **Signal.** A named, typed value an application exposes, such as `progress`
  or `icon_visible`.
- **Source primitive.** One kind of state reader the runtime ships: memory
  pointer chain, screen region, log file, local API, or process and window
  events.
- **Trigger.** The condition on signals or application events that makes a
  rule fire.
