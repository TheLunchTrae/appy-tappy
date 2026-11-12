# Setup app

The setup app is where the user tells AppyTappy what to read from an
application's window and what to do about it. It is a Tauri v2 program:
the UI is React and TypeScript in `apps/setup/src`, and the Rust side in
`apps/setup/src-tauri` holds the capture and the checks.
[Architecture](Architecture.md#the-setup-app) explains why it is built this
way.

The user guide owns the pages and flows as the user sees them:
[Applications](Applications.md), [Values](Values.md),
[Automations](Automations.md), and [Overlay](Applications.md#overlay).
This page covers what the user guide leaves out.

## Behavior the guide leaves out

- **Picker.** The picker of a value or reference chooses a saved application
  and then resolves that application's window with its own match
  (`lib/picker.ts`); [Picker](State-machines.md#picker) lists its states. One
  look (`lib/useAppWindows.ts`) is one `list_app_windows` command: the owner
  reads the saved applications and lists the desktop once, and answers the
  applications, the open windows, and the windows each application's match
  takes, so the three agree. The picker shows the applications from that
  answer, not from the pages' list, so **Refresh** reads them again and a
  failed read shows with its reason. `window_list::matching` marks the window
  a new session would read as `best`, and every offered window that fits as
  well as it as `tied`. `tied` covers only offered windows: one the list
  filters out, untitled, without a size, or of a desktop class, ties with
  none even where the runtime could read it, and when the runtime reads such
  a window no offered window is tied. Capture allows only a tied window,
  since the runtime reads one of those: the only one without asking, or the
  one the user picks among several. A window the match takes with a worse fit
  is never offered. The chosen application's ID travels with the captured
  window as the `Choice`, and every save names it, so nothing reads the owner
  back from the window. **Add application** and **Change how it's found**
  open the application flow for the picker's flow, as
  [Pages and flows](State-machines.md#pages-and-flows) describes.
- **One flow from an automation.** The automation editor is where a first
  automation starts: its condition chooses the application, then one of that
  application's values (`lib/watch.ts`), and says which step is missing while
  there is no value to compare. **Add application** and **New value** open
  those flows for the automation, which waits under them with its draft
  (`lib/flow.ts`). `save_value` answers the saved value's application and ID,
  which the editor picks once the pages' lists show it, so nothing is looked
  up by name. Until then the draft has no value and cannot be saved. The
  pages' lists are read every three seconds and after each save; the reads
  are numbered, so an answer that arrives after a newer one is dropped
  (`lib/reads.ts`), and the editor can tell what is still on its way from
  what a newer read shows to be gone. When a flow shows because another
  ended, the focus lands on the control the next step is taken with
  (`lib/landing.ts`). [Pages and flows](State-machines.md#pages-and-flows)
  and
  [What an automation watches](State-machines.md#what-an-automation-watches)
  list the transitions.
- **Application editor.** A new application's match starts from the picked
  window's title, class, and executable and requires only the executable, with
  no fallback. An application that an automation uses cannot be removed.
- **Screenshots.** The owner thread keeps the session's screenshots until
  its capture stops: on **Change window**, when another window is picked, or
  when the flow closes ([Pictures kept](#pictures-kept)). The draft keeps
  the screenshot its region was drawn on, and a value saves that screenshot's
  window size. The owner refuses a check or a save that cuts the region from
  a labeled screenshot, a recording, or a compared screenshot of another
  window size, and names which (`check_spec::drawn_size`). The screenshot
  shortcut is a global shortcut from `tauri-plugin-global-shortcut`; a letter,
  digit, or Space needs a modifier so the shortcut does not take that key from
  every other program.
- **Capture failures.** A frame error shows in the preview until the next good
  frame of the same capture; only a closed window ends the capture. While a
  failure stands, a screenshot is refused, and a recording stops with the
  failure as its problem. Once the window has closed, no recording starts or goes
  on until another window is picked.
- **Live reading.** `read_value` answers what the check being set up reads
  on the newest frame, or why that frame cannot be read, in the check's own
  words (`CheckError`), which the box under the check shows in place of a
  reading. A reference being taught is read by the reader its labels teach
  (`CheckSpec::Characters`), never by the text reader, and the runtime
  builds the same reader from the saved file
  (`check_spec/round_trip.rs`). The region is read where it was drawn,
  whatever size the live window has, so a window resized since its
  screenshot shows another spot there.
- **Leaving a flow.** Each flow reports what leaving it would lose
  (`lib/unsaved.ts`): nothing while what it shows has the same content as
  what it started with, unsaved work otherwise, or a save under way. A
  capture counts its screenshots and recording too, since only the session
  has them. A fix and an edit open on a screenshot the capture takes
  itself, and start from what they show once the region is placed on it
  (`withFixRegion`, `placedOn`), so leaving one that nobody touched asks
  nothing. The page tabs, the page already shown included, ask before they
  drop unsaved work, and **Change window** asks the same. While a save runs,
  the tabs, **Change window**, the questions' answers, and **Discard** or
  **Cancel** wait. A save closes its flow only while that flow is still open,
  so one that finishes after the user moved on leaves the next flow alone.
  Leaving a capture, as **Change window** also does, stops it, which drops
  its screenshots and recording.
  A flow opened for another unmounts that one, so what a waiting flow would
  lose has to travel in its `Flow` value. Only the automation editor opens a
  flow while it holds work, so it hands over its draft, and the window counts
  that draft with what the open flow reports (`heldInAll`). A tab then asks
  even when the open flow holds nothing, and the question names both losses
  (`lostText`). **Cancel** in a flow opened for another shows that one again
  and drops only its own work.
- **Selection.** Each state of a state check has its own selection, which travels as the steps the user
  took: snap boxes and clicks, each replacing, adding to, or taking from the
  selection, with one color match for all of them (`selection.rs`). The owner
  replays a state's steps on the screenshot the state is taken from, both to
  answer the UI's `select` command and to build and save the check, so moving
  the slider remakes every selection from the same steps. A new step fits the
  region around what every state selects, and so does the slider while the
  region is as it was last fitted; a region changed by hand stays. A step on a
  screenshot with no state adds a state for it, and taking away from nothing
  is ignored. The UI gets the
  selection's box and its runs of selected pixels, and shades the rest of the
  region itself.
- **Matching pixels.** The preview's tint comes from the owner, never from
  the page: `match_pixels` takes a screenshot's index, a rectangle of it, and
  the rule, and answers the pixels the rule takes (`match_preview.rs`). Each
  rule asks the function its reader asks, so the tint and the reading agree
  at the tolerance's edge: `DigitReader::is_ink` for a reference's
  characters, `BarFill::classify` for a bar, and `ColorMatch::near` for a
  color check, which the check itself asks about the region's average color.
  A color check's answer carries that average, its distance from the color,
  and what the check reads on the rectangle. The answer holds each kind of
  pixel as a count and as runs in the rectangle, spelled as a selection's
  are. The runs cross to the page as JSON, so the answer caps them: a kind
  with more than 500,000 runs carries only its count, and the page says that
  it tints none of them. The page draws the runs
  (`lib/matchPreview.ts`) on a canvas of the rectangle's own pixels inside
  the frame that holds the screenshot, placed in fractions of the frame, so
  the tint stays on its pixels at any size the frame shows at, and under the
  surface that takes the pointer. The page asks for a reference's characters
  on the whole screenshot and counts those in the region itself, so moving
  the region asks nothing. It awaits one request at a time and sends the
  newest settings when that one is answered, so the tint follows a dragged
  slider as fast as the owner answers. A request the page stopped awaiting,
  as when the switch is turned off, may still be with the owner, and its
  answer is dropped.
  [State machines](State-machines.md#matching-pixels-preview) maps its
  requests and answers. **Show selected pixels** asks the owner nothing of
  its own: it draws the runs the `select` answer for the screenshot shown
  already carries, on the same canvas. That selection is remade once
  **Color match** rests (`SELECT_DELAY_MS`), so until then the tint is the
  last selection's and its count says that it is updating; a `select` that
  fails drops the tint. The tint is the selection, not what the state check
  compares: that is the selected pixels that lie in the region while
  **Ignore the background** is on, and the whole region otherwise, and the
  switch's help says which.
- **Live reading.** The value editor and the automation editor read the
  window the runtime would, and capture only the value's region. The
  automation editor reads about twice a second and shows numbers as read, to
  four decimals unless more are needed to compare them with the target.
- **Window sizes.** A new value saves the side of the window it stays at
  as `pin`. The pin follows where the region is, the same guess the runtime
  makes for a value saved without one, until the user picks a side by hand
  ([Pin choice](State-machines.md#pin-choice)); either way the file names
  it. Guide lines over the screenshot run from the region to the edges or
  the middle of the window that the pin measures from (`PinGuides.tsx`).
  **This content may move** asks for a reading area where the check can
  read one, and the value saves it as `readingArea`
  ([Content that moves](Screen-regions.md#content-that-moves)). A fix of
  such a value asks for a reading area at its size too, and has no flag of
  its own.
  The Values page reads each running application's values about
  once a second at the window's size now, with a capture of its own like an
  overlay's, whether the overlay is on or not, and stops when the page
  closes (`live_values`, `stop_live_values`). The automation editor's
  reading follows its window to each size the same way and keeps its watch's
  number, so the editor's poll goes on. **Fix at this size** opens the
  capture locked to the value's application. Its first screenshot sets the
  size, and its region starts where the value reads at that size: its fix
  there, or its first drawing scaled (`place_value`). The owner refuses a
  fix at the size of the first drawing, of another kind of check, or with
  other states than the value's, and saves it over any fix at the same size
  (`save_fix`, `remove_fix`). A fix of a number names its own reference only
  when it differs from the value's.
- **Editing a value.** **Edit** opens the capture locked to the value's
  application, on a first screenshot like a fix's. The draft starts as the
  saved value (`lib/edit.ts`), with its region on no screenshot. Once a
  screenshot of the window size the value was drawn at exists, the region
  sits on the newest one (`useValueEdit`); on a screenshot of any other
  size the region does not show and cannot be drawn. The page sends an
  `EditSpec`: the value's ID, its name, its pin, whether its region is a
  reading area, and a drawing. The pin and the reading area are picked as
  for a new value, on the capture's draft and with **This content may
  move**, and open as the saved value has them: the pin by hand only when
  it is saved and is not the side the region is nearest (`pickedPin`). The
  owner refuses a reading area on a check that does not read anywhere in
  its region, tried or saved, so an edit that makes a number a color has to
  turn it off; the page sends it only for a check that reads one. The flag
  is setup guidance, so the value's fixes stay whatever it says. A `kept`
  drawing keeps the saved region and spells the check as the file does, so
  it needs no screenshot and the window may have any size; a state check in
  it names the value's saved pictures in their order, under the names the
  states have now. A `drawn` drawing is a check on a screenshot, which must
  have the window size the value was drawn at, and a state check in it
  takes every state's picture again. The owner builds the value as edited
  once (`owner/edits.rs`) for both commands. `try_edit` reads it back with
  `saved::build` and makes it the live check where `SavedValue::at` places
  it at the newest frame's size, so the editor's reading is the runtime's at
  the window's size now, and answers an `EditPreview`: why the value is not
  read at that size, the fixes the edit removes, and the automations that
  refuse it. An edit that cannot be tried clears the live check, so nothing
  reads with the edit before it. The page asks the owner for the window's
  size twice a second (`window_size`, the newest frame's own size, not the
  scaled live view's) and tries the edit again when it changes, and after a
  save the owner refused. `edit_value` saves the same value over the saved
  one, under its ID, and leaves the application's `checked` build alone. It
  refuses an edit when an automation's condition suits the saved value but
  not the edited one (`TargetDef::suits`). The page sends the window sizes
  of the fixes the user agreed to lose as `dropFixes`, and the owner
  refuses the save, listing the fixes it would remove, unless those are
  exactly the ones the edit removes. An edit removes a value's fixes when
  the check's kind changes or a state check's states no longer line up with
  the fixes' pictures: each state has to keep its picture or its name, in
  the same place. A fix that stays keeps the reference it reads with: when a number value's
  reference changes, a fix that read with the old one names it, and a fix
  that named the new one names none, so a fix still names a reference only
  when it differs from the value's. A fix has no `whole` of its own and
  follows the value's. A value saved without a `pin` keeps none while the
  edit names the side guessed from its region, so an edit that changes
  nothing writes nothing. A value keeps its ID when renamed, and a new
  value named like a renamed one's old name gets that ID with a number.
- **References.** A number value names a reference of its own application
  by ID, and saving refuses one of another application. The reference editor
  sends the characters it teaches as a `characters` check, which names the
  saved reference it adds to, if any; the owner learns that reference's saved
  pictures first, so trying an edit reads as the saved reference will. A
  reference belongs to a saved application. It keeps its ID when renamed, and
  a new one named like a renamed one's old name gets that ID with a number.
  One that a value reads with cannot be deleted.
- **Key recording.** **Record** takes the next keys pressed, Tab included,
  and records a letter as the keyboard layout types it.
- **Saving an automation** checks that its value is still saved and that the
  target is the kind the condition compares. A key press action also needs the
  application's anti-cheat class to be `none`
  ([Key presses](Actions.md#key-presses)).
- **Click it now.** The click target screen sends a click only from that
  button's press: `click_now` in the owner checks the anti-cheat classes,
  reads the click's picture from its file, and has the host place and send it
  as the runtime does ([Trying a click](Actions.md#trying-a-click)). The
  owner thread waits for it, up to a second for a picture's frame and half a
  second more for the answers of the windows a click sent to the window asks,
  so other commands wait meanwhile. It answers the window the click went to, and the
  screen says when that is not the window it picked. While anti-cheat blocks
  the click, the button stays in the tab order with `aria-disabled`, is
  described by the note that says why, and sends nothing. The click's return
  of the focus follows the same
  rule as **Use this target**: what the action had while its button is the
  same, and otherwise on unless it is a right click (`returnsFocus` in
  `lib/click.ts`). [Click target tries](State-machines.md#click-target-tries)
  maps the screen's states.
- **Reload.** Saving or removing an application, saving, editing or deleting
  a value, saving or deleting a reference or an automation, switching an automation on or off, changing
  an anti-cheat class, and changing a setting set the reload event, so the
  runtime reads the files again at once. Overlay changes do not, since
  the runtime does not read them. [The runtime](Architecture.md#the-runtime)
  describes how it reads them.

## Recording

[Recording readings](References.md#record-readings) samples the frames
the reference editor's preview capture already takes, about 30 a second, in
the owner thread (`recording.rs`). Each frame is cut to the region and reduced
to the shapes a reference reads: `DigitReader::shapes` finds the ink, splits it
into characters, and scales each to the grid the check compares. A crop whose
shapes equal a kept reading's is another sighting of that reading, because it
would teach the check nothing new. A blended edge seldom changes a grid
cell, and when it does the reading is kept twice, which costs a tile in review
and teaches no wrong shape. One stroke of a different character changes a
shape however long the number. The
shapes ignore size, so a counter drawn larger is the same reading. Every kept
reading is compared, not only the latest, so a value that comes back is not
kept twice. A crop with no ink is skipped, and so
is a frame of another size than the screenshot the region was drawn on. One
recording keeps at most 200 readings.

Kept readings are named by index in the spec the UI sends, so their pixels
never cross into the UI except as thumbnails. A reference learns its saved
pictures first, then the labeled screenshots, then the kept readings, and
saves the new ones as samples after its old ones. Review sends the characters
it would save and asks which kept readings it could not learn. The owner
learns them as saving would, in that order, with the reference's own
characters' color and tolerance, and leaves out each reading that fails, so a
conflict such as one shape typed as `l` in one reading and `1` in another is
flagged on the reading that causes it. **Record more** goes on with the stopped
recording only when the region, color, tolerance, and window size are the
ones it was recorded with, and otherwise says so rather than start over. The
UI then offers **Use its settings**, which puts the recording's region, color,
and tolerance back on the draft. The region stays on the screenshot it was
drawn on when that one has the recording's window size, and otherwise moves to
the newest screenshot of that size. Kept readings are lost only on a discard,
on **Change window**, or when the flow is left; a closed window keeps them
for review. **Back** and Escape leave
review without keeping, and review's edits stay for the next one.

### The text reader

The text reader guesses each new reading's text so review only needs corrections
(`recording/ocr.rs`). It is [`ocrs`](https://github.com/robertknight/ocrs), a
pure-Rust neural text reader, so it reads the same on every system without the
operating system's OCR. Only its recognition model loads: the region holds one
line, so there is no text to find first. It runs on a thread of its own, because
a read takes tens of milliseconds and the owner answers the UI in microseconds,
and only for new readings, a few times a recording. It gets each reading's ink
as black text on white with a margin, whatever the application's colors
(`recording/ink.rs`). Text for a recording the user discarded or replaced is
dropped by the recording's number. Only the setup app uses it; the runtime reads
numbers with the shapes the user's labels and kept readings taught it ([Screen
regions](Screen-regions.md#checks)).

Its model, `text-recognition.onnx`, is in a `models` folder beside the setup
app's program, where [the installer](Installer.md) puts it. It loads on the
first reading, so a session that never records never loads it. When it cannot
load, recording still works and each reading says why, for the user to type
its text. For a development build, `mise run ocr:model` downloads it, checks
it against the hash pinned in `.config/mise/conf.d/installer.toml`, and copies
it beside the debug build.

## Window matching

[How AppyTappy finds the window](Applications.md#how-appytappy-finds-the-window)
gives the patterns, the must-match fields, and the fallback. The file stores
them as `mustMatch`, with a flag for the title and the class, and `fallback`:
`none` or `executable`. The executable always has to match, because the
runtime attaches to an application by its process's executable. A window
qualifies when its executable and every required field match.
`WindowDef::check` refuses an empty executable pattern, one that matches
nearly every program, such as `*` or `*exe`, and an empty pattern for a field
that must match. The setup app saves no such match, and the runtime does not
load a file that has one. Fallback
windows count only when no window qualifies. Windows rank by fewer missed
fields, then by the title matching. On a tie the runtime keeps a window of
the process its session already follows, and the window it reads while that
shows, and otherwise takes the first that Windows lists, usually the
front-most, that is not minimized
([Lifecycle](Architecture.md#lifecycle)). The setup app marks as the best
match the window a new session would read, and watches a value, shows an
overlay, and finds a click's target in that window.
The setup app uses the match to show which applications are running, and the
runtime finds each application's window with the same rules. A value's region
is drawn at one window size, and the runtime reads it at another by
scaling it, or only at a size it was drawn at, as its application's
`sizeChanges` says ([Window sizes](Screen-regions.md#window-sizes)). When
the window closes or changes size, the runtime forgets the values it read.
Each automation stays as it was until the values read next show whether its
condition still holds, so it neither runs again nor undoes what it changed
while the window is out of sight.
A value that does not read at the window's size is not read. Once the
window has settled at that size, the runtime logs why and that the window
should be resized or the value fixed at that size, each reason once until
the value is read again.

## Saved files

Each application is a folder in `%APPDATA%\AppyTappy\apps\`, named by an ID
made from its executable, such as `example`. The folder holds `app.json`
and the pictures it needs: each reference's labeled screenshots, cropped to
the region, then its kept recorded readings, named `ref-{reference}-{n}.png`;
and each state check's pictures, named after its value and the state's
place, such as `status-1.png`, and transparent where the state's selection
leaves the background out
([Saved pictures](Screen-regions.md#saved-pictures)). A fix's pictures add
a number, such as `status-2-1.png`, and so do the pictures of a drawing
whose states an edit took again. Each new set takes the smallest number no
saved picture of the value has, so a save never overwrites a picture the
saved file still names.

```json
{
  "format": 1,
  "name": "Example App",
  "window": {
    "title": "Example App*",
    "class": "ExampleWindow",
    "executable": "Example.exe",
    "mustMatch": { "title": false, "class": false },
    "fallback": "none"
  },
  "sizeChanges": "often",
  "antiCheat": "none",
  "overlay": { "enabled": true, "mode": "pictureInPicture" },
  "checked": {
    "build": { "version": "1.4.2.0", "size": 48234496, "modified": 1762500000 },
    "at": 1762600000
  },
  "values": [
    {
      "id": "progress",
      "name": "Progress",
      "screen": {
        "windowSize": { "width": 1920, "height": 1080 },
        "region": { "x": 1146, "y": 14, "width": 74, "height": 38 },
        "check": {
          "kind": "number",
          "reference": "percent_font",
          "whole": true
        },
        "pin": "topRight",
        "readingArea": true,
        "fixes": [
          {
            "windowSize": { "width": 3440, "height": 1440 },
            "region": { "x": 2990, "y": 18, "width": 99, "height": 51 },
            "images": []
          }
        ]
      }
    }
  ],
  "references": [
    {
      "id": "percent_font",
      "name": "Percent font",
      "ink": { "red": 255, "green": 255, "blue": 255 },
      "tolerance": 40,
      "samples": [{ "image": "ref-percent_font-1.png", "text": "87.5%" }]
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

An application's `checked` is the build of its executable that its values were
checked against, and `at` is when, in seconds since 1970. A build is the
executable's file version, which is left out when the file has none, its size
in bytes, and when it was last written, in seconds since 1970; two builds are
the same only when all three match. The setup app records the running build
when it saves an application's first value, and when the user selects **Mark
as checked**. It leaves `checked` out again when
the last value is deleted or the window match's executable changes, since the
build was of the values or the executable that are gone. The Values page shows
`checked`'s version and day, and warns while a different build runs, as does
the runtime's log, as [Build detection](Architecture.md#build-detection)
describes.

The settings live in `%APPDATA%\AppyTappy\settings.json`. A setting the
file leaves out takes its default:

```json
{
  "format": 1,
  "readInterval": { "fastestMs": 16, "slowestMs": 500 },
  "screenshotShortcut": "F9"
}
```

An application's `sizeChanges` is `often` or `rarely`, and `often` when the
file leaves it out. A value's `pin` is one of `topLeft`, `topCenter`,
`topRight`, `middleLeft`, `center`, `middleRight`, `bottomLeft`,
`bottomCenter`, and `bottomRight`, and is guessed from where the region is
when the file leaves it out. Its `readingArea` is `true` when the region was
drawn as a reading area and left out otherwise; only a number or bar fill
check may have it, and it changes nothing the value reads. Each of its `fixes` has its own window size and
region; a state check's fix lists `images`, a picture for each state in the
check's order, and a number check's fix may name its own `reference`.

An application's `overlay` is left out until the user first picks one. Its
`mode` is `overlay` or `pictureInPicture`, and once the panel has been moved
its `panel` holds its place and size in screen pixels, as `x`, `y`, `width`,
and `height`.

A yes-or-no condition has `"is": "equals"` or `"notEquals"` and a `true` or
`false` target. A state condition has the same comparisons and a state's name,
or `"none"`, as its target. A state check is saved with its states in order:

```json
{
  "kind": "state",
  "states": [
    { "name": "Paused", "image": "status-1.png" },
    { "name": "Live", "image": "status-2.png" }
  ],
  "tolerance": 16
}
```

An action's `app` is `null` for whatever is playing, or for the whole system's
volume.

A click that looks for a picture names it by file name, and the picture is in
the `clicks` folder beside `automations.json`, named `click-{n}.png`.
[Pictures kept](#pictures-kept) says what each picture holds and when it is
removed.

`appytappy-config` owns the formats (`app_file.rs`, `automation_file.rs`,
`settings_file.rs`) and the files (`store.rs`). Each save writes a temporary
file and renames it over the old one, so a crash never leaves half a file; a
later save in that folder removes a temporary file that a crash left between the
two steps, once it is ten minutes old. A file that cannot be read is listed with
the reason and left alone; an automations file that cannot be read as a whole,
such as one with another format version, is not replaced: until it is fixed, the
setup app saves no automations, removes no application, and edits or deletes
no value, because it checks that file to see whether an automation still uses
the application or value. One automation that breaks a rule on its own, such as
after a hand edit, is kept in the file and listed with its problem, and nothing
runs it; the user can turn it off or delete it, and the others keep working.
Saving an automation, turning one on, and listing them all ask
`Runnable::new` in `appytappy-sources`, which runs `AutomationDef::compile`,
builds the value it watches with `saved::build`, and fits each click's picture,
the same check the runtime runs. An automation's ID is the setup app's own key:
the runtime does not read it, so an ID that a hand edit made invalid, or gave to
more than one automation, stops nothing. When the setup app reads the file, it
gives a new ID, made from the name, to each automation whose ID is invalid or
held by one before it, the same way each time, so every row can be edited or
deleted on its own; the next save writes the new IDs. Saving a value reads it
back with `saved::build`, from its new pictures and those already saved, as the
runtime will, and refuses it if the runtime could not read it. A new
application's anti-cheat class starts as `unknown`, which blocks key press
actions until the user sets it
([#11](https://github.com/TheLunchTrae/appy-tappy/issues/11)).

## Pictures kept

AppyTappy keeps a picture only where reading or clicking needs one, cut to
the region the user drew, and never the rest of the window. A window's whole
frame exists only in memory while its capture is open. A file holds a region
alone, which is as much of the window as the user drew. The user guide says
so in [What AppyTappy keeps](Getting-started.md#what-appytappy-keeps).

### In memory

The owner thread and the page hold these for the picked window. All of them
go when its capture stops (`Owner::stop`): the user selects **Change window**,
another window is picked, or the flow closes: on **Discard**, **Cancel**, a
page tab, or the save of what the flow makes. A reference saved from inside a
value flow leaves the flow, and so its capture, open. Closing AppyTappy Setup
ends the process.

| What                               | Held by            | How much of the window                                    |
| ---------------------------------- | ------------------ | --------------------------------------------------------- |
| The newest frame                   | Owner              | All of it; each frame replaces the one before             |
| The live view                      | Page               | All of it, scaled down to `LIVE_MAX_WIDTH` at most        |
| Each screenshot                    | Owner and page     | All of it, and a thumbnail `THUMB_WIDTH` wide on the page |
| Each recorded reading              | Owner and page     | The region; a recording keeps at most `MAX_READINGS`      |
| The check being tried              | Owner              | The region, once for each state                           |
| A new reading, for the text reader | Text reader thread | The region's ink, until it is read                        |

A capture holds as many screenshots as the user takes. Setting up a click's
target is a capture like a value's, and its screen stops the capture when it
closes and each time it looks for the window again. A recording's readings
also go when it is discarded or another replaces it.

The automation editor's live reading, the overlays, and the Values page
capture only the regions of the values they read, as the runtime does. The
overlays and the Values page keep the readings alone, and the live reading
keeps its value's region of the newest frame.

### On disk

| File                                 | What it holds                                                                             |
| ------------------------------------ | ----------------------------------------------------------------------------------------- |
| `apps\{app}\ref-{reference}-{n}.png` | A reference's sample: the region of a labeled screenshot, or of a kept recorded reading   |
| `apps\{app}\{value}-{n}.png`         | A state's picture: the region, transparent where its selection leaves the background out  |
| `apps\{app}\{value}-{fix}-{n}.png`   | A state's picture in a fix: the fix's region, the same way                                |
| `clicks\click-{n}.png`               | A click's picture: the region the user marked, transparent where its selection leaves out |

A number, bar fill, or color value saves no picture of its own. Nothing else
AppyTappy writes holds pixels: the runtime's log and its [trace](Trace.md)
hold names, readings, and timings as text, and the
[MCP server](Development.md#tools)'s `screen_sample` answers the colors of
single points of the screen, at most `SCREEN_POINTS_LIMIT` a call.

A sample keeps the whole region, not only the box around its characters. A
reference that is edited learns its saved samples again with the color and
tolerance the editor has then, and another color or a higher tolerance counts
pixels as ink that a box cut for the saved settings would have dropped. A
state's picture is the whole region because that is what the check compares.

### Removal

- After each save of an application's file, `Owner::write_app` removes the
  pictures in its folder that the file as saved does not name
  (`AppStore::prune`). So the pictures of a deleted value, fix, or reference
  go with it; a save that fails takes back the pictures it wrote, since the
  file still saved does not name them; and pictures left by a save that the
  program ended in the middle of go with the application's next save.
- A click's picture is written when its target is set up, before its
  automation is saved. Each save of the automations removes the click
  pictures none of them names, and so does the end of the automation's flow
  (`forget_unsaved_click_pictures`), which covers a target set up for an
  automation the user then did not save. While the editor waits under a
  value or application flow it opened, its draft still names its pictures,
  so they stay until the automation is saved or left (`holdsAutomation`).
- Both remove only files named as the table above names them. Any other file
  in those folders stays, such as one the user put there, and so does a
  picture that a file edited by hand names. Removing an application removes
  its folder with everything in it.
- A temporary file that a crash left goes at the next save in its folder, as
  [Saved files](#saved-files) describes, and one in `clicks` goes with the
  click pictures.
- Nothing is removed when AppyTappy Setup starts. An application whose file
  cannot be read keeps every picture, and so do the click pictures while the
  automations file cannot be read, since nothing says which they name.

## Page types

The page sends and reads the Rust side's own types. `ts-rs` writes their
TypeScript declarations into `apps/setup/src/lib/bindings.ts` and reads the
serde attributes, so a field or variant that the Rust side renames, adds, or
removes shows on the page as a type error. Only the tests derive them: the
setup app takes `ts-rs`, and turns on `appytappy-config`'s `ts` feature, as
dev-dependencies, so neither program builds with `ts-rs`.

The test `bindings::bindings_match_the_rust_types` renders the file from the
types each command takes and answers, and every type they reach, and fails when
the checked-in file differs. It renders `apps/setup/src/lib/api.ts` the same
way: one function for each row of the command table,
`apps/setup/src-tauri/src/commands/table.rs`, that calls the command by its
name, sends its arguments under the camelCase keys Tauri reads, and types its
answer. The table is the one list of commands: `main.rs` registers each row
with Tauri, the compiler checks each row's argument and answer types against
its command, and
`bindings::the_command_table_names_each_argument_as_its_command_does` checks
its argument names against the command's parameters. After a change to the
table or to one of those types, write the files again and commit them:

```sh
UPDATE_BINDINGS=1 cargo test -p appytappy-setup bindings
```

Every generated type is readonly all the way down. A `u64`, such as a window
handle or a time, reaches the page as a `number`, and the test fails when a
field would become a `bigint`, which JSON does not carry. The field says so
with `#[cfg_attr(test, ts(type = "number"))]` in the setup app, or
`#[cfg_attr(feature = "ts", ts(type = "number"))]` in `appytappy-config`,
because only those builds derive the types. A command's own `u64` argument or
answer is `U64AsNumber` in the table, or `OptionalU64AsNumber` when it can be
missing, and a picture it answers as raw bytes is `Pixels`, which `api.ts`
decodes. The page imports the types from
`bindings.ts` and calls the commands through `api.ts`.

The file also holds what the page checks a draft against before it saves:
`LIMITS`, such as the longest app name a media or volume action names, the
longest path or link, the most states, the highest tolerance, and the most
pixels a click's search area holds; `KEY_NAMES`, the keys a key press action
can name; `SHORTCUT_KEYS`, the keys the screenshot shortcut can name;
`ACTIONS`, whether each kind of action sends input and whether undo can put
it back; `TRACE_CATEGORIES`; `NO_STATE`; and `DEFAULT_SETTINGS`. The test
reads them from `appytappy-config`, so the page and the save agree, and the
save still checks everything again. The hints that list keys or ranges are
built from them. `OVERLAY_LABELS` holds how an overlay window's label starts,
and `PICTURES` how a picture a command answers starts, with a sample picture
that `apps/setup/tests/pixels.test.ts` decodes. The UI's formatting and lint
checks skip both generated files, and its unused-type check skips
`bindings.ts`.

The page reads a typed path, link, key press, screenshot shortcut, or app name
itself, so the user sees what to fix as they type, and places a click's spot
in the window itself. `PARSE_CASES` holds what the Rust side makes of a fixed
list of samples, one for each way a parser accepts or refuses, and where
sample spots land in sample windows, and
`apps/setup/tests/parse-cases.test.ts` fails when the page reads one
differently. A parser that gains a rule needs a sample for it in
`bindings.rs`.

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
