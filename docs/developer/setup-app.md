# Setup app

The setup app is where the user tells AppyTappy what to read from an
application's window and what to do about it. It is a Tauri v2 program:
the UI is React and TypeScript in `apps/setup/src`, and the Rust side in
`apps/setup/src-tauri` holds the capture and the checks.
[Architecture](architecture.md#the-setup-app) explains why it is built this
way. Issues [#16](https://github.com/TheLunchTrae/appy-tappy/issues/16) and
[#17](https://github.com/TheLunchTrae/appy-tappy/issues/17) track it.

The user guide owns the pages and flows as the user sees them:
[Applications](../user/applications.md), [Values](../user/values.md),
[Automations](../user/automations.md), and [Overlay](../user/overlay.md).
This page covers what the user guide leaves out.

## Behavior the guide leaves out

- **Picker.** A saved application whose window match fits the picked window
  is chosen, so a second value joins the first. A new application's match
  starts from the window's exact title, class, and executable.
- **Screenshots.** The owner thread keeps the session's screenshots until
  another window is picked or the user leaves the new value. The screenshot
  shortcut is a global shortcut from `tauri-plugin-global-shortcut`; a letter,
  digit, or Space needs a modifier so the shortcut does not take that key from
  every other program.
- **Live reading.** The value editor and the automation editor read the
  window the runtime would, and capture only the value's region. The
  automation editor reads about twice a second and shows numbers as read, to
  four decimals unless more are needed to compare them with the target.
- **Key recording.** **Record** takes the next keys pressed, Tab included,
  and records a letter as the keyboard layout types it.
- **Saving an automation** checks that its value is still saved and that the
  target is the kind the condition compares. A key press action also needs the
  application's anti-cheat class to be `none`
  ([Key presses](actions.md#key-presses)).
- **Reload.** Saving or deleting a value or an automation, switching an
  automation on or off, and changing an anti-cheat class set the reload event,
  so the runtime reads the files again at once. Overlay changes do not, since
  the runtime does not read them. [The runtime](architecture.md#the-runtime) describes how it
  reads them.

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
  "antiCheat": "none",
  "overlay": { "enabled": true, "mode": "pictureInPicture" },
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

An application's `overlay` is left out until the user first picks one. Its
`mode` is `overlay` or `pictureInPicture`, and once the panel has been moved
its `panel` holds its place and size in screen pixels, as `x`, `y`, `width`,
and `height`.

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
