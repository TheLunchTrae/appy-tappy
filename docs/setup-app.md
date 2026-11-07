# Setup app

The setup app is where the user tells AppyTappy what to read from an
application's window and, next, what to do about it. It is a Tauri v2 program:
the UI is React and TypeScript in `apps/setup/src`, and the Rust side in
`apps/setup/src-tauri` holds the capture and the checks.
[Architecture](architecture.md#the-setup-app) explains why it is built this
way. Issue [#16](https://github.com/TheLunchTrae/appy-tappy/issues/16) tracks
it.

## Pages

- **Values** lists each saved value under its application: its check, its
  region, and the automations that use it. A green dot marks an application
  whose window is open now. **New value** starts the flow below.
- **Automations** will list automations: the values they use, their conditions,
  their actions in order, and the applications that must be running. Issue
  [#17](https://github.com/TheLunchTrae/appy-tappy/issues/17) adds them; the
  page says so until then.
- **Applications** lists how each saved application's window is recognized,
  its anti-cheat class, and how many of its windows are open.

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
   working everywhere else. Screenshots last until the window changes.
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
which applications are running; the runtime will find applications with the
same rules ([#6](https://github.com/TheLunchTrae/appy-tappy/issues/6)).

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

`appytappy-config` owns the format (`app_file.rs`) and the folder
(`store.rs`). Each save writes a temporary file and renames it over the old
one, so a crash never leaves half a file. A file that cannot be read is listed
with the reason and left alone. A new application's anti-cheat class starts
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
