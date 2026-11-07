# Setup app

The setup app is where the user tells AppyTappy what to read from an
application's window and what to do about it. It is a Tauri v2 program:
the UI is React and TypeScript in `apps/setup/src`, and the Rust side in
`apps/setup/src-tauri` holds the capture and the checks.
[Architecture](architecture.md#the-setup-app) explains why it is built this
way. Issues [#16](https://github.com/TheLunchTrae/appy-tappy/issues/16) and
[#17](https://github.com/TheLunchTrae/appy-tappy/issues/17) track it.

## Pages

- **Values** lists each saved value under its application: its check, its
  region, and the automations that use it. A green dot marks an application
  whose window is open now. **New value** starts the flow below.
- **Automations** lists each automation: a switch that turns it on or off, its
  condition, its actions in order, and the application that must be running.
  An automation that can no longer run, such as one whose value was deleted,
  says why under its name. **New automation** and **Edit** open the editor
  below.
- **Applications** lists how each saved application's window is recognized,
  its anti-cheat class, and how many of its windows are open. The anti-cheat
  class is picked from a menu: not set, none, user-mode, or kernel-level.
  Only none lets automations press keys into the application, so picking it
  asks first and warns that pressing keys into an application with
  anti-cheat can get the user's account banned.

## Making a value

1. **Pick the window.** The picker lists the open windows. Picking one fills
   in its application: a name, and a window match taken from the window's
   title, class, and executable. A saved application whose match fits the
   window is chosen instead, so a second value joins the first. The picker
   counts the open windows the match fits as it is edited.
2. **Take screenshots.** **Take screenshot** or the screenshot shortcut, `F9`
   at first, keeps the window's newest frame. The shortcut works while the
   other application has the keyboard, so the user can carry on in it until
   it shows the value and press it then. **Change shortcut** records a new one;
   a letter, digit, or Space needs a modifier so it does not stop that key
   working everywhere else. Screenshots last until the window changes or the
   user leaves the new value.
3. **Draw the region.** On a screenshot, drag to draw a box, drag inside it to
   move it, or drag a corner to resize it. The X, Y, width, and height fields
   set it to the pixel. The region is measured in the window's own pixels.
4. **Choose the check.** The four checks are those in
   [Screen regions](screen.md#checks). Colors are picked by clicking the
   screenshot. A number check learns from every screenshot the user labels
   with what the region shows, such as `87%`; label enough screenshots to show
   every digit it should know.
5. **Watch the reading.** Once the check is complete, the box under it shows
   what the check reads on the live window, a few times a second. A check that
   cannot work, such as a bar whose two colors are the same, says why there.
6. **Save.** The value is saved with its application; a new application is
   saved with its first value.

## Making an automation

An automation is its own thing, apart from the values: it watches one saved
value, and any number of automations can watch the same one.

1. **Name it**, such as `Pause music near the end`.
2. **When.** Pick the value, then how it compares: a value that reads a number
   is at least, above, at most, below, is, or is not a number; a value that
   reads yes or no is or is not yes. Conditions on several values come later
   ([#34](https://github.com/TheLunchTrae/appy-tappy/issues/34)).
   While the value's application is open, the editor reads the value on its
   window about twice a second and says what it reads now and whether the
   condition is true, so a target can be tried before it is saved. It says so
   when the application is not open, or when the window is not the size the
   value was drawn at, since a value reads only at that size. It reads the
   window the runtime would and captures only the value's region. Numbers
   show as read, to four decimals unless more are needed to compare them with
   the target.
3. **Do, in order.** Add any number of actions and move them up or down. Each
   one pauses, plays, or skips media, or sets a volume, for whatever is
   playing or the whole system, or for one application named by its
   executable, such as `Player.exe`. The open windows' executables are
   offered as suggestions. A key press action presses keys such as
   `Ctrl+Shift+M`, typed or recorded with **Record**, which takes the next
   keys pressed, Tab and Escape included. Keys go to the window that has the
   keyboard focus, and only into an application whose anti-cheat is set to
   none on the Applications page ([Key presses](actions.md#key-presses)).
4. **Afterwards.** With **Undo these when the condition stops being true**,
   paused media plays again, started media pauses again, and each volume goes
   back. A skipped track cannot be unskipped and pressed keys cannot be
   unpressed, so an automation that only skips or presses keys cannot undo.
   Either way, it can run again once the condition has stopped
   being true. An optional wait, in seconds, keeps it from running again
   sooner.
5. **Save.** The editor says what is still missing until the automation is
   complete. Saving checks the value is still saved and reads the kind of
   target the condition compares.

A value that an automation watches cannot be deleted; the Values page names
the automations to change or delete first.

The runtime runs every enabled automation that can run, and each save,
delete, or switch on this page or the Values page tells it to read them
again at once. An automation that stops running because of the change undoes
what it changed first. [The runtime](architecture.md#the-runtime) describes
how it reads them.

## Window matching

An application's window is found the way OBS finds one: patterns on the
title, the class, and the executable, where `*` matches any run of characters
and `?` matches one, ignoring case. `Example App*` matches a title that
gains a document name. When no open window fits all three patterns, the
application falls back as its setting says:

- **Only a window that matches all three.**
- **Otherwise one with the same class and executable.**
- **Otherwise any window of the executable**, the default, for the many
  applications whose title changes as they run.

The executable always has to match. The setup app uses the match to show
which applications are running, and the runtime finds each application's
window with the same rules. A value's region means something only while the
window has the size it had when the region was drawn. Values can be drawn at
more than one size, and the runtime reads the ones drawn at the window's
current size. When the window closes or changes size, the runtime forgets the
values it read. Each automation stays as it was until the values read next
show whether its condition still holds, so it neither runs again nor undoes
what it changed while the window is out of sight.
When the window has a size no value was drawn at, the runtime reads nothing
and logs that the window should be resized or the value drawn again.

## Saved files

Each application is a folder in `%APPDATA%\AppyTappy\apps\`, named by an ID
made from its executable, such as `example`. The folder holds `app.json`
and the pictures its values need: a number check's labeled screenshots,
cropped to the region, and a picture check's reference.

```json
{
  "format": 1,
  "name": "Example App",
  "window": {
    "title": "Example App*",
    "class": "ExampleWindow",
    "executable": "Example.exe",
    "priority": "titleElseExecutable"
  },
  "antiCheat": "unknown",
  "values": [
    {
      "id": "progress",
      "name": "Progress",
      "screen": {
        "windowSize": { "width": 1920, "height": 1080 },
        "region": { "x": 1146, "y": 14, "width": 74, "height": 38 },
        "check": {
          "kind": "number",
          "ink": { "red": 255, "green": 255, "blue": 255 },
          "tolerance": 40,
          "samples": [{ "image": "progress-1.png", "text": "87%" }]
        }
      }
    }
  ]
}
```

The automations live in one file, `%APPDATA%\AppyTappy\automations.json`.
Each names its value by application ID and value ID:

```json
{
  "format": 1,
  "automations": [
    {
      "id": "pause_music_near_the_end",
      "name": "Pause music near the end",
      "enabled": true,
      "when": { "app": "example", "value": "progress", "is": "atLeast", "target": 90.0 },
      "actions": [
        { "kind": "media", "command": "pause", "app": "Player.exe" },
        { "kind": "volume", "percent": 20, "app": null },
        { "kind": "keys", "keys": "Ctrl+Shift+M" }
      ],
      "undo": true,
      "cooldownMs": 0
    }
  ]
}
```

A yes-or-no condition has `"is": "equals"` or `"notEquals"` and a `true` or
`false` target. An action's `app` is `null` for whatever is playing, or for
the whole system's volume.

`appytappy-config` owns the formats (`app_file.rs`, `automation_file.rs`) and
the files (`store.rs`). Each save writes a temporary file and renames it over the old
one, so a crash never leaves half a file. A file that cannot be read is listed
with the reason and left alone; an automations file that cannot be read stops
automations from being saved until it is fixed, rather than being replaced. A new application's anti-cheat class starts
as `unknown`, which blocks key press actions until the user sets it
([#11](https://github.com/TheLunchTrae/appy-tappy/issues/11)).

## Running it

The program embeds the UI built into `apps/setup/dist`, so build the UI first:

```sh
mise run ui:build
cargo run -p appytappy-setup
```

To work on the UI with live reloading, run the Vite dev server and the program
without its embedded UI:

```sh
cd apps/setup && bun run dev
cargo run -p appytappy-setup --no-default-features
```

`mise run ui:standards:check` runs the UI's gate: formatting, type-aware lint,
type check, tests, unused-code check, and the build.
