# Context

Domain vocabulary for AppyTappy. Use these terms in code, docs, and issues.
[`wiki/Architecture.md`](wiki/Architecture.md) shows how the
parts fit.

- **Action.** A side effect from the runtime's fixed set, such as pausing media
  or setting an app's volume. A paired action restores the state it changed.
- **Anti-cheat class.** How an application protects itself from other
  programs: `none`, `user-mode`, `kernel-level`, or `unknown`. A protected
  application is one whose class is not `none`, such as one with anti-cheat.
  Only `none` permits input-injection actions.
- **App definition.** Data, never code, that says how to recognize one
  application, detect its version, and read its signals.
- **Application.** Any program AppyTappy watches: a game, a media player, a
  browser, or anything else with a window. Code and short UI labels call it
  an app.
- **Binding.** The concrete way to read one signal for one application
  version: a source primitive and its parameters, such as a screen region and
  its check.
- **Check.** How a screen region binding turns a region's pixels into a value:
  a number read with a reference, a bar's fill, a color match, or a match
  with a saved image.
- **Click delivery.** How a click action reaches its window: with the
  cursor, which comes to the window and presses, or as mouse messages sent
  to the window, which move neither the cursor nor the focus.
- **Cooldown.** How long after firing a rule cannot fire again. It is not a
  hold time: a rule fires on the first value that makes its condition hold.
- **Edit.** A saved value changed as it was first drawn, under the ID it
  has, so the automations that watch it stay linked.
- **Fix.** A screen region value drawn again at one more window size, which
  it reads there exactly instead of scaled from its first drawing.
- **Look.** What the whole of a value's region shows at one moment. Two looks
  are the same when the region's pixels differ by no more than a tolerance on
  average, the way a state check compares a region with a state's whole
  picture. A look takes no account of what a state selects in the region.
- **Pin.** The side of the window, a corner, an edge's middle, or the center,
  that a region keeps its scaled distance from as the window changes size.
  The UI calls it **Stays at**.
- **Read interval.** The minimum and maximum time between a source's reads.
  The source reads at the minimum while its value moves and backs off toward
  the maximum while it holds still.
- **Reading area.** A screen region drawn larger than what it reads, to
  cover every place content that moves inside the window can show. A number
  or bar fill check reads its content wherever it is in the area; nothing
  follows the content. A value saves that its region is one, as guidance
  for the setup app: it reads the same either way.
- **Re-arm.** Let a rule that fired fire again, once its condition stops
  holding.
- **Release.** End a fired rule's effect: it re-armed, or the session ended. A
  rule that restores undoes its paired actions then.
- **Recording.** A setup-app capture of one region over time, while the user
  uses the application. A recording of readings keeps each different reading
  a number region shows once, for the user to review and label; kept readings
  teach a reference like labeled screenshots. A recording of looks keeps a
  screenshot each time a value's region shows a look that no screenshot shows
  yet. A capture records one of the two at a time.
- **Reference.** The characters of one font in one application, learned from
  labeled pictures of its text. A number check reads with one, at any size.
- **Rule.** A condition on signals, the actions to run when it starts to hold,
  a cooldown, and whether those actions restore.
- **Runtime.** The always-on background program.
- **Sample.** One value a source read for a signal.
- **Saved files.** The files the setup app saves: each application's
  `app.json` with its pictures, `automations.json`, and `settings.json`. The
  runtime reads them when it starts and again each time the setup app saves an
  application, a value, a reference, an anti-cheat class, an automation, or the
  settings; saving only an overlay's settings does not tell it.
  `appytappy-config` calls each one an authoring file.
- **Session.** One run of an application's automations: from when they start
  (the application starts, automations are turned on, or a reload changes what
  it runs) until they stop. Rule state resets with each session.
- **Setup app.** The program the user opens to discover signals, build rules,
  and save its files.
- **Signal.** A named, typed value an application exposes, such as `progress`
  or `icon_visible`.
- **Size changes.** How often an application's window changes size: often,
  so its values scale with the window, or rarely, so each is read only at
  the sizes it was drawn at.
- **Source primitive.** One kind of state reader the runtime ships: screen
  region, log file, local API, or process and window events.
