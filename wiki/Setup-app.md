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
- **Screenshots.** The owner thread keeps the session's screenshots, taken or
  [recorded](#recording-looks), in memory until the user removes them or
  their capture stops: on **Change window**, when another window is picked, or
  when the flow closes ([Pictures kept](#pictures-kept)). None is written to
  disk unless the user saves a [draft](#drafts), which copies them to its
  folder. A recording, with the looks or readings it holds, goes with them.
  Each screenshot is named by the index it got when it was kept
  (`screenshots.rs`).
  Removing one releases its pixels and leaves its index empty, so the indexes
  the draft holds stay right, and the strip numbers a screenshot by its index
  so its number never changes. The UI refuses to remove a screenshot the draft
  uses: the one its region is drawn on, one a state is taken from, a labeled
  one, and the value's own while the reference editor is open over it
  (`lib/looks.ts`). A check that names a removed screenshot is refused like
  one that names a screenshot of another session. The draft keeps
  the screenshot its region was drawn on, and a value saves that screenshot's
  window size. The owner refuses a check or a save that cuts the region from
  a labeled screenshot, a recording, or a compared screenshot of another
  window size, and names which (`check_spec::drawn_size`). The screenshot
  shortcut is a global shortcut from `tauri-plugin-global-shortcut`; a letter,
  digit, or Space needs a modifier so the shortcut does not take that key from
  every other program.
- **Region.** The region is kept in the pixels of the screenshot it is drawn
  on, whatever size the screenshot shows at. `ZoomView` lays the picture out:
  the whole picture with room around it at first, and larger by the zoom,
  which belongs to screenshots of one size. `RegionSurface` reads the pointer
  against the picture's box on the page at each pointer event, and again when
  a scroll or a zoom moves the picture under a pointer that stays still, so
  the zoom, the scroll, and the window's size are no state of the drag. The
  region's outline, its label, and the dim around it are drawn by `Marquee`,
  outside the box. A corner takes presses 12 pixels out from the box and up
  to 4 into it, less into a small box, which keeps 8 pixels each way, or all
  of a smaller side, to be moved by (`.marquee .corner` in `styles.css`). The
  zoom controls stay focusable at their limits and say so with
  `aria-disabled`, and the percentage is a status that screen readers hear
  when it changes. The surface reaches over the
  room around the picture, which is how a drag starts outside it, and only
  while a tool drags: a click that picks a color or selects a pixel lands on
  the picture alone. `onRegion` is called once, when a drag is let go
  ([Region drag](State-machines.md#region-drag)), so a check runs for the
  finished region and not for each one on the way. A selection that fails
  says why above the view, on either step.
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
- **Bar detection.** **Detect the bar** sends the region and the screenshot
  it was drawn on (`detect_bar`). The owner cuts the region from that
  screenshot and from the newest others of its window size that the session
  still keeps, taken or recorded, at most `MAX_OTHERS` of them
  (`check_spec/bar.rs`), since the region means another
  spot at another size and each screenshot costs the owner thread a copy of
  the region and a few passes over it. It hands them to [the
  detector](Screen-regions.md#bar-detection), the drawn-on one first, and
  answers the two colors, the screenshot they were found on, the ones that
  show the bar and the ones that show neither color, whether it went by only
  part of the region, and how many older ones it left out; or a sentence
  that says why none are proposed and names the screenshots concerned.
  `measure_bar` reads the same screenshots with two colors through the bar
  reader itself, and answers what each one reads. The UI measures the
  draft's colors, or the detected ones while it asks whether they replace
  colors picked by hand, so the list describes hand-picked colors as well.
  Which pixels count is the matching pixels preview's to show. Colors found
  go into a draft with no color at once. An edit's region sits on the
  newest screenshot of the size its value was drawn at, so both commands
  look at the screenshots of that size whichever one shows, and the UI asks
  for one while there is none. A fix keeps its value's colors and offers
  neither. [Bar detection](State-machines.md#bar-detection) covers the
  request and its stale answers.
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
  when it differs from the value's. A fix has no `whole` and no
  `characters` of its own and follows the value's. A value saved without a `pin` keeps none while the
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
- **Expected characters.** A reference may list the characters the user says
  it needs, as [Choose what it needs](References.md#choose-what-it-needs)
  describes. The list guides and never decides. `appytappy-sources` builds a
  reference's reader from its color, tolerance, and samples alone, so the
  list adds no glyph, labels no shape, and leaves number parsing as it is.
  No rule of the file covers it either: the runtime and the setup app load
  an application whatever its list holds. The save is the one place that
  shapes it. `expected_characters` in `check_spec.rs` writes each character a
  label teaches that is no control character, once, the digits `0` to `9`
  first and the rest by code point, and refuses more than
  `MAX_EXPECTED_CHARACTERS`, which the page reads as
  `LIMITS.maxExpectedCharacters` to say so before the save. The page keeps
  the list as one set of characters. `lib/expected.ts` reads a saved list as
  that same set, so one written by hand shows as its characters and the next
  save writes it in order; it reads each preset's state back from the set;
  and its `coverage` splits the set and the learned characters into known,
  missing, and extra, for the editor, the References page, and the reference
  picker. The learned characters are those of the saved samples, the labeled
  screenshots, and the kept readings. The save sends the set beside the
  `characters` check, not in it, and it replaces what the saved reference
  expected. A character is counted as a label's is: `is_label_character` in
  `appytappy-config` serves the learner and the save, and the samples in
  `PARSE_CASES` hold the page's reading of labels and lists to the Rust
  side's.
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

A capture records one thing at a time (`Recorder` in `recording.rs`): the
readings a number's region shows, for a reference to learn, or
[the looks a value's region shows](#recording-looks), each kept as a
screenshot. Both sample the frames the preview capture already takes, about 30
a second, in the owner thread, and share when they record and why they stop
(`recording/lifecycle.rs`): a frame of another size than the screenshot the
region was drawn on is skipped, a capture failure stops the recording with the
failure as its problem, and a closed window ends it. Starting one kind
replaces a recording of the other. Recordings of both kinds are numbered from
one counter.

[Recording readings](References.md#record-readings) runs in the reference
editor. A number value's **Record readings** opens that editor on the value's
region, for the reference picked or a new one. A saved reference has its
characters' color, so its recording starts at once; for a new one the editor
opens on the screenshot the region is drawn on, with the color next to pick. Each frame is cut to the region and reduced
to the shapes a reference reads: `DigitReader::shapes` finds the ink, splits it
into characters, and scales each to the grid the check compares. A crop with
no ink is skipped, and so is a frame of another size than the screenshot the
region was drawn on.

A recording collects shapes, not readings (`recording/sightings.rs`). A crop
is kept when it shows a shape no kept reading has, and the first frame that
shows a new shape is the one kept. Any other crop would teach the check
nothing, so it is only counted, with one exception below. A digit, a `%`,
and a `.` are shapes alike: whatever `DigitReader::shapes` makes one
character is one shape.

- **What counts as the same shape.** Two shapes are the same only when every
  grid cell is, which is how `DigitReader::learn` tells glyphs apart. Shapes
  that differ in a cell are not merged, because nothing in a grid tells a
  blended edge from a second character, and merging would hide a glyph the
  check needs. So one character can be several shapes: a blended edge that
  changes a cell is one, and so is a digit beside a `,`, because the grid
  scales by the line's height and the `,` makes the line taller. Each costs a
  kept reading and teaches no wrong shape.
- **Clean carriers.** A reading is learned only when its text names every
  shape in it, so a shape whose one kept reading also holds something no
  character names, such as a neighbor caught half drawn, could never be
  taught. A kept reading carries a shape cleanly when every other shape in
  it has shown in another reading too, which a passing shape seldom does. A
  crop of known shapes is kept after all when it would carry cleanly a shape
  that no kept reading carries so (`Sightings::worth_keeping`). The counts
  only grow, so a shape stays cleanly carried once it is, and each shape
  costs at most two kept readings: one that first shows it and one that
  first carries it cleanly. Since **Record more** goes on with the same
  shapes and counts, recording on fixes a shape that was first caught
  badly. A shape that shows in two different readings counts as a
  character, so something that stays beside a value through several
  readings is not told from one.
- **Evidence.** Each shape holds how many frames showed it and how many
  different readings did. A reading is counted as different by a 64-bit
  fingerprint of its shapes, so no frame is kept to count it. A kept reading
  also holds how many frames showed exactly it. These are counts of what the
  recording saw. They are not a match score and say nothing of whether a
  label is right, so the UI shows them as counts and never as a percentage.
- **Bounds.** Recording runs until the user stops it. It holds at most
  `MAX_SHAPES` different shapes, the 256 glyphs a reference learns, and
  counts at most `MAX_READINGS` different readings, 1,048,576 fingerprints
  or about 16 MB, which a timer that changes every frame reaches in about
  ten hours. A frame that does not fit, with more new shapes than there is
  room for or with one reading too many, is not counted and makes the
  recording full: it keeps and counts no more, and its view says why in
  words. The shapes bound the kept readings at two each, 512. What those
  hold is not bounded by a number: a kept reading holds its crop at four
  bytes a pixel, so it grows with the region the user draws, 72 KB a reading
  for a region 300 by 60 pixels and 8 MB for a whole 1920 by 1080 window.
- **Sizes.** A frame of another window size is skipped because a recording
  has a region and no [pin](Screen-regions.md#window-sizes): only a value
  says which side of the window its region keeps its distance from, and a
  reference's region is drawn only to teach. Placing it at another size
  would mean guessing the pin, and a wrong guess records other pixels. A
  reference learned at one size reads at others, since shapes scale to the
  grid, so the limit is where the recording looks, not what it learns.
- **Color and tolerance.** The shapes are the ones the recording's own
  characters' color and tolerance show, and "every shape is in a kept
  reading" holds for those. A reference learns the kept crops again with the
  color and tolerance it has at the save, as it does its labeled
  screenshots. If the user changes either before saving, a crop can split
  into other shapes: its text may have another count of characters than the
  crop then shows, which `review_problems` and the save refuse by name, and
  a shape the recording counted as known may be in no kept crop. The page's
  own lists still follow the recording's shapes, so they can be off until
  **Use its settings** puts the recording's color and tolerance back.
- **Signs in several marks.** The reader splits characters at columns with no
  ink. A sign whose marks share no column, such as a `%` drawn as two dots
  and a stroke apart, is one shape for each mark, each time alike, and no
  one-character label can teach it.

The page works out what the shapes mean, because the labels live there
(`lib/review.ts`, `lib/discovery.ts`). A reading's text names its shapes
when it has one character for each. From that, while recording and in
review, the page shows the characters the reference needs that nothing
shows, using the [expected characters](#behavior-the-guide-leaves-out) and
`coverage`; the ones only the text reader's unchecked guesses show; and the
ones the reference knows or a reading the user checked shows. A reading is
checked once the user edits it, which **Keep all** does to every reading, or
keeps it with the **Keep** button. A guess is never counted as learned: a
character is learned only when the user keeps its reading and the reference
learns it as before.

In review, a reading adds the shapes no other kept reading teaches
(`additions`). Going through the kept readings in order, one whose every
shape another still shows is dropped, and each shape counts for the first of
the rest that shows it. A reading that adds nothing starts left out. Three
rules keep that default from hiding anything:

- **A start is decided once** (`decide`). Review decides a reading's start
  when it first shows the reading with its text read, against the readings
  the user edited and the ones decided before, which stay as they are. After
  that only the user changes its **Keep**. A text that arrives later decides
  its own reading alone.
- **Texts are compared on every shape** (`disagreements`). When two
  readings' texts give one shape two characters, both are flagged, kept or
  left out, and neither starts left out. With both kept, the owner's
  `review_problems` flags the later one as it always did, and **Keep**
  waits. The owner stays the authority for what is kept; the page's
  comparison is what reaches the readings that are left out, and it does not
  hold **Keep** back by itself.
- **What is left untaught is named** (`untaught`). When the only readings
  whose text could teach a shape are left out, by default or by the user, a
  notice above the readings names the characters and offers the readings to
  keep for them, worked out against the current edits. It describes the
  **Keep** button too. It does not stop the user: leaving a character out
  can be what they want.

Review lists the readings that add shapes first, in an order that follows
the shapes alone, so tiles stay put while the user types.

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

### Finding more like it

[Find more like it](References.md#find-more-like-it) searches the screenshot
the user labels for more text like the region shows (`recording/find.rs`). It
marks each pixel as ink with the reference's color and tolerance, as
`DigitReader` does, and takes the band of inked rows with the most ink in the
region, as the reader segments it. It joins touching ink, corners included,
into blobs, leaving out blobs over twice that band's height, such as a panel in
the same color. The sample's height is its tallest blob centered in the region,
and a run's is its tallest blob, so a comma or a mark below the characters
measures neither. Blobs whose rows meet and that start no further than the
sample's height past each other join into a run of text; runs are looked up by
bands of rows and dropped from the lookup once the next blobs start too far
right of them, so a screenshot full of ink takes well under a second. A run
within 15% of the sample's height (`SIZE_PERCENT`) that touches neither the
region nor a find added already is a find; a shorter or taller run, at least
half the sample's height, counts as another size, and smaller specks are
ignored. Finds go top to bottom, then left to right; the first 12
(`MAX_FOUND`) are kept with a margin of a quarter
of the sample's height, and the rest are counted.

The owner keeps the search (`owner/found.rs`) with each find's pixels and
sends each to the text reader. A new search replaces it, and dismissing,
adding, **Change window**, picking another window, or leaving the flow
forgets it; reads already
queued for a forgotten search still run, and their text is dropped. Pictures
of finds are asked for by the search's number, so an answer for a replaced
search is empty. The UI lists
the finds, polls while text is still being read, and asks which kept finds the
reference cannot learn (`found_problems`), learning them after the draft's own
labels the way review learns recorded readings. Added finds become labels with
a `region` of their own on their screenshot, which the reference cuts instead
of the spec's region, so they save as samples like any labeled screenshot.

### Recording looks

[Recording screenshots](Values.md#record-screenshots) is for the checks that
are set up on screenshots: a state, a bar fill, and a color
(`recording/looks.rs`, `owner/looks.rs`). A frame is cut to the region and
compared with every look the recording knows. Each look is an `ImageMatch` of
the whole region, the comparison a state check makes with a state's whole
picture: two looks are the same when the average difference of their red,
green, and blue is at most the tolerance. A frame that shows a look it does
not know is kept whole in the screenshot store, like a screenshot the user
takes, so every tool that works on a screenshot works on it. Only the crop of
a frame is compared; the window's whole content is copied only when it is
kept.

- **The whole region.** A state check with **Ignore the background** compares
  only the pixels each state selects, and a recording does not: it has no
  selection to compare within, since a state gets its selection on a
  screenshot the recording has yet to keep. So a changing background behind
  the thing makes most frames new looks, which stops the recording at its
  limit, and a small change in a large region can stay within the tolerance
  and never be kept. Comparing within the draft's state selections is left
  out.
- **Sensitivity.** The UI sends the tolerance when the recording starts
  (`lookTolerance`). For a state check it is the check's own **Tolerance**,
  so one control adjusts both. A bar's and a color's own tolerance compare
  colors, not looks, so they record with 16, the tolerance a state check
  starts with.
- **What it knows.** A recording starts knowing the region on every screenshot
  of the same window size that the session holds, taken or recorded. So the
  look of the screenshot the region was drawn on is not kept again, and
  recording again keeps only what is new. A screenshot removed since is not
  known, so its look is kept again if it shows.
- **Bounds.** A recording keeps at most 16 screenshots (`MAX_LOOKS`, the most
  states a check holds), then stops with the limit as its problem. A screenshot
  is a whole window at four bytes a pixel, about 8 MB at 1920 by 1080, so one
  recording holds at most about 130 MB there. Each new recording may keep 16
  more, and the screenshots the user takes have no limit; removing a
  screenshot releases it at once, and the capture's end releases all of them.
- **Cost.** A comparison is one pass over the region for each look known,
  until one matches, on the owner thread, which also answers the UI. After
  each comparison the recording rests four times as long as that frame took
  from arriving to compared (`REST`), and a frame that comes meanwhile only
  becomes the preview. So comparing takes at most a fifth of the owner
  thread's time, whatever the region's size and the number of looks, and less
  while the thread is behind, since a frame that waited took longer. A frame's
  wait behind a slow command counts too, so a recording rests longer after
  the owner was busy, while the panel still says it records. Frames
  cannot pile up behind it, and a command waits for one comparison at most:
  one pass over the region for each look known. A small region has every frame
  compared. A large one that shows many looks has fewer frames compared, so
  it can miss a look that shows only briefly.
- **Review.** There is no review sheet: the UI lists each kept screenshot in
  the strip as the recording reports it (`useLookRecording`), marked as
  recorded, and the user keeps or removes them there. The owner refuses to
  remove a screenshot while a recording of looks records, and the UI says so:
  the recording knows each look it kept and would not keep a removed one
  again. Stopping shows the first kept screenshot that the strip lists, and
  the panel counts what the recording kept against the strip. A stopped recording only says what it kept and why it stopped;
  discarding it, or starting readings in the reference editor, leaves its
  screenshots in the strip.

### The text reader

The text reader guesses each new reading's text, and each find's, so review only needs corrections
(`recording/ocr.rs`). It is [`ocrs`](https://github.com/robertknight/ocrs), a
pure-Rust neural text reader, so it reads the same on every system without the
operating system's OCR. Only its recognition model loads: the region holds one
line, so there is no text to find first. It runs on a thread of its own, because
a read takes tens of milliseconds and the owner answers the UI in microseconds,
and only for new readings, a few times a recording. It gets each reading's ink
as black text on white with a margin, whatever the application's colors
(`recording/ink.rs`). Text for a recording or a search the user discarded or
replaced is dropped by its number, and a queued job of a recording or search
that a newer one of the same kind replaced is skipped without reading it. Only the setup app uses it; the runtime reads
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

Drafts are kept apart from these files, in `%LOCALAPPDATA%\AppyTappy\drafts\`,
so the runtime never reads them ([Drafts](#drafts)).

Each application is a folder in `%APPDATA%\AppyTappy\apps\`, named by an ID
made from its executable, such as `example`. The folder holds `app.json`
and the pictures it needs: each reference's labeled screenshots, cropped to
the region or to an added find's own region, then its kept recorded readings, named `ref-{reference}-{n}.png`;
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
      "samples": [{ "image": "ref-percent_font-1.png", "text": "87.5%" }],
      "expects": "0123456789.%"
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

A number check's `characters` is the characters the value reads of its
reference, such as `"0123456789%"`, and is left out when it reads every
character the reference knows. Whitespace in it does not count, and neither
does a character the reference has yet to learn. A check that names
characters names at least one, or the file does not load; a value whose
reference, or whose fix's reference, knows none of them is not read. The
setup app writes each character once, the digits first, then the rest by
code point. Its live check builds the same glyphs the runtime reads with
(`check_spec::build`).

A reference's `expects` is the characters the user says it needs to know,
and is left out when the user chose none. Only the setup app reads it, to
show which characters a reference has yet to learn, so no rule covers it and
a file loads whatever it holds. The setup app writes each character once,
exactly as labels spell them, so `A` and `a` are two characters: the digits
first, then the rest by code point.

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

## Drafts

A draft is an item the user saved before it was finished
([Drafts](Drafts.md) in the user guide). It is what its flow holds, written
down: the same state the flow would carry while it waits under another flow
or shows on screen ([Pages and flows](State-machines.md#pages-and-flows)).
The setup app keeps each in its own folder under
`%LOCALAPPDATA%\AppyTappy\drafts\`, named by an ID made from the time it was
first saved (`apps/setup/src-tauri/src/drafts.rs`). The folder holds
`draft.json` and, for a value, fix, or reference, the capture's screenshots,
each as `shot-{index}.png` under the index it has in the capture, so the
indexes of removed screenshots stay empty. The runtime never reads this
folder, so a draft never runs and the authoring files never hold an
unfinished item.

`draft.json` names the kind of flow that finishes it, the application it
belongs to, the saved item it changes (null for a new one), the name and the
sentences the lists show, the click pictures it names, and `state`: what the
flow had set up. Only the page reads `state`, marked with its own format and
kind (`apps/setup/src/lib/drafts.ts`). A draft whose `state` has another
format was saved by another version and can only be deleted.

What each kind keeps in `state`:

| Kind                     | What it keeps                                                                                                                                                                                                                                                                                                                                                                                                         | What it does not keep                                                                                                                  |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Automation               | `AutomationSetup` (`lib/flow.ts`): the editor's whole draft, with each click's application, button, way of clicking, focus choice, and target, and the application chosen for the condition                                                                                                                                                                                                                           | A value or application flow the editor had opened, and what such a flow would hand back                                                |
| Application              | `AppSetup` (`lib/matching.ts`): the name, how the window is found, and how often it changes size                                                                                                                                                                                                                                                                                                                      | The window picked in the list                                                                                                          |
| Value, fix, or reference | `CaptureSetup` (`lib/captureDraft.ts`): the name, the step, whether the content may move, the check's whole draft (region, the screenshot it is drawn on, **Stays at**, tolerances, colors, states and their selections, labels, added found text, the reference, the characters it reads and **Whole number only**), the reference being edited with the characters it needs, and which screenshots a recording kept | A recording that runs or waits for review, the readings kept from one, an open **Find more like it** list, and the zoom of the picture |

Screenshots follow one rule, however they were kept: every screenshot the
capture holds when the draft is saved goes into the draft, those a
[recording of looks](#recording-looks) kept as well as those the user took,
and the strip marks the recorded ones again when the draft is finished. A
recording itself lives on the owner thread with the capture and is not
written: its readings and the looks it knows go when the flow closes. An
[edit of a saved value](State-machines.md#value-edit) is never a draft. It starts
from the value as it is saved and is checked against the automations that use
it when it is saved, so a copy kept for later would be a second, stale state
of a value that runs meanwhile. Its leave question says so and offers no
**Save as draft**. A fix adds a drawing at one more size and changes nothing
saved until it is saved, so it can be a draft.

A save writes the draft into a folder of its own, ending in `.saving`, with
each file flushed to the disk. It then moves the old draft aside, to a folder
ending in `.old`, and moves the new one into place. Listing the drafts tidies
up after a save that stopped: it puts an old draft back when that save's own
`.saving` folder shows the new one never took its place, keeping that folder
until the old one is back, and removes the rest. Deleting a draft first
removes what a stopped save of it left, so tidying never brings a deleted
draft back. A draft of changes to an
automation, application, or reference replaces any other draft of the same
saved item, and a save that cannot remove the older one still worked: the
page takes the newest. Fix drafts never replace each other and are not keyed
by window size, so a value can have several, each finished from its own row.
A fix draft copies how its value is read (`fixDraft`); once the value's
check, colors, or states are edited, the draft no longer fits it, and its row
says so and offers only **Delete** (`fixOutdated` in `lib/captureDraft.ts`).
A value draft that kept the reference editor open takes that reference again
from the application's file when it is finished (`refreshedEditing`).
Removing an application deletes the drafts that belong to it, and deleting a
value its fix drafts (`DraftStore::delete_where`), since nothing could finish
them; that is best effort, and one that stays says it can only be deleted. Finishing a draft opens its flow; a value, fix,
or reference asks for the window again, and once it is picked, the draft's
screenshots replace the session's, each under the index the draft's state
refers to. Saving the item deletes the draft.
[Pictures kept](#pictures-kept) says how long a draft's pictures stay.

## Pictures kept

AppyTappy keeps a picture only while something saved needs it. A saved value,
reference, or click needs one to read or to click, cut to the region the user
drew or to the box around text it found that the user added, and never the
rest of the window. A saved [draft](#drafts) needs the pictures its flow was
working on, so that the flow can go on: a capture's screenshots, each the
window's whole content, and an automation's click pictures. Nothing else outlives its flow: a
window's whole content otherwise exists only in memory while its capture is
open. The user guide says so in
[What AppyTappy keeps](Getting-started.md#what-appytappy-keeps).

### In memory

The owner thread and the page hold these for the picked window. All of them
go when its capture stops (`Owner::stop`): the user selects **Change window**,
another window is picked, or the flow closes: on **Discard**, **Cancel**, a
page tab, or the save of what the flow makes or of its draft. A reference
saved from inside a value flow leaves the flow, and so its capture, open.
Closing AppyTappy Setup ends the process. **Save as draft** copies the
screenshots to the draft's folder before the flow closes; the copies in
memory go with the capture all the same.

| What                               | Held by            | How much of the window                                    |
| ---------------------------------- | ------------------ | --------------------------------------------------------- |
| The newest frame                   | Owner              | All of it; each frame replaces the one before             |
| The live view                      | Page               | All of it, scaled down to `LIVE_MAX_WIDTH` at most        |
| Each screenshot                    | Owner and page     | All of it, and a thumbnail `THUMB_WIDTH` wide on the page |
| Each recorded reading              | Owner and page     | The region; a recording keeps at most `MAX_READINGS`      |
| Each look a recording knows        | Owner              | The region, once for each screenshot it knew or kept      |
| The check being tried              | Owner              | The region, once for each state                           |
| A new reading, for the text reader | Text reader thread | The region's ink, until it is read                        |
| Each find of a search              | Owner and page     | Its box; the page's is `CROP_WIDTH` wide at most          |
| A find, for the text reader        | Text reader thread | The find's ink, until it is read                          |

A capture holds as many screenshots as the user takes, and at most
`MAX_LOOKS` more for each [recording of looks](#recording-looks). A screenshot
the user removes goes at once. Setting up a click's
target is a capture like a value's, and its screen stops the capture when it
closes and each time it looks for the window again. A recording's readings
or looks also go when it is discarded or another replaces it; the screenshots
a recording of looks kept stay with the capture's others.

The automation editor's live reading, the overlays, and the Values page
capture only the regions of the values they read, as the runtime does. The
overlays and the Values page keep the readings alone, and the live reading
keeps its value's region of the newest frame.

### On disk

| File                                 | What it holds                                                                                        |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------- |
| `apps\{app}\ref-{reference}-{n}.png` | A reference's sample: the region of a labeled screenshot or kept reading, or a find's box            |
| `apps\{app}\{value}-{n}.png`         | A state's picture: the region, transparent where its selection leaves the background out             |
| `apps\{app}\{value}-{fix}-{n}.png`   | A state's picture in a fix: the fix's region, the same way                                           |
| `clicks\click-{n}.png`               | A click's picture: the region the user marked, transparent where its selection leaves out            |
| `drafts\{draft}\shot-{index}.png`    | A screenshot of a value, fix, or reference draft: the window's whole content, as the capture held it |

The first four are in `%APPDATA%\AppyTappy\`, and a draft's screenshots in
`%LOCALAPPDATA%\AppyTappy\`, where the runtime never reads. A draft's
screenshots are the only files that hold more of a window than a region the
user drew. A draft of an automation names its click pictures in `draft.json`
and keeps them in `clicks` with the others.

A number, bar fill, or color value saves no picture of its own. Nothing else
AppyTappy writes holds pixels: the runtime's log and its [trace](Trace.md)
hold names, readings, and timings as text, `setup.log` holds each command's
name, time, and failure as text, never its arguments or its answer, and the
[MCP server](Development.md#tools)'s `screen_sample` answers the colors of
single points of the screen, at most `SCREEN_POINTS_LIMIT` a call. A
[debug bundle](#debug-page) copies the picture files above into the zip file
the user saves, a draft's screenshots among them, and only when the user
includes them. Each draft's `draft.json` goes in either way, as each
application's `app.json` does.

A sample of a labeled screenshot or a recorded reading keeps the whole region,
not only the box around its characters. A
reference that is edited learns its saved samples again with the color and
tolerance the editor has then, and another color or a higher tolerance counts
pixels as ink that a box cut for the saved settings would have dropped. A
state's picture is the whole region because that is what the check compares.
An added find's sample is the box the search cut around its text, with a
margin of a quarter of the text's height, so a color or tolerance that counts
more pixels as ink sees only those within the margin.

### Removal

- After each save of an application's file, `Owner::write_app` removes the
  pictures in its folder that the file as saved does not name
  (`AppStore::prune`). So the pictures of a deleted value, fix, or reference
  go with it; a save that fails takes back the pictures it wrote, since the
  file still saved does not name them; and pictures left by a save that the
  program ended in the middle of go with the application's next save.
- A click's picture is written when its target is set up, before its
  automation is saved. Each save of the automations removes the click
  pictures that neither a saved automation nor a saved draft names, and so
  does the end of the automation's flow (`forget_unsaved_click_pictures`),
  which covers a target set up for an automation the user then neither saved
  nor kept as a draft. While the editor waits under a value or application
  flow it opened, the draft it holds still names its pictures, so they stay
  until the automation is saved, saved as a draft, or left
  (`holdsAutomation`).
- A draft's pictures go with the draft. Deleting a draft, or saving the item
  it was a draft of, removes its folder with its screenshots. The click
  pictures an automation's draft named are then named by the saved
  automation, or by nothing: the page removes those that nothing names when
  the draft is deleted from the Automations page, and at the end of the flow
  that finished or deleted it. Saving a draft again with fewer screenshots
  keeps only those.
- Both remove only files named as the table above names them. Any other file
  in those folders stays, such as one the user put there, and so does a
  picture that a file edited by hand names. Removing an application removes
  its folder with everything in it.
- A temporary file that a crash left goes at the next save in its folder, as
  [Saved files](#saved-files) describes, and one in `clicks` goes with the
  click pictures.
- Nothing is removed when AppyTappy Setup starts. An application whose file
  cannot be read keeps every picture, and so do the click pictures while the
  automations file or the drafts folder cannot be read, since nothing says
  which they name. A draft whose file cannot be read counts the same way:
  nothing says which click pictures it names, so `DraftStore::pictures`
  answers an error and no click picture is removed until that draft reads
  again or the user deletes it, which its row says. It keeps its own
  screenshots until it is deleted.
- The uninstaller's **Delete the application data** removes the drafts folder
  with the saved files and the logs ([Installer](Installer.md)).

## Debug page

The [Debug page](Debug-page.md) shows only while `showDebug` in
`settings.json` is true; the runtime ignores that setting. Its commands live in
`owner/debug.rs` and its file readers in `src-tauri/src/debug/`:

- `debug_overview` asks `InstanceGuard::is_held` whether the runtime runs, and
  summarizes the newest `trace-*.csv` in the trace folder. `TraceTail` keeps
  its place in that file, so each poll reads only the rows added since the last
  one, and starts over when a newer file appears or the file shrinks. A header
  other than the one this build writes is reported as a trace from another
  version, not parsed; the header is `TRACE_HEADER` in `appytappy-config`,
  which the runtime writes too. Frame to action is each `started` row's time
  minus its `cause`, rule handling is each `handled` row's `took_ns`, running
  actions is each `done` row's `took_ns`, and each is summarized as a median
  and 95th percentile of the newest 10,000, so a long trace keeps a fixed
  amount in memory. A poll reads at most 4 MiB, from near the end when more
  was added, and a row that no line break ends within that much is given up
  at the next line break. The summary covers the newest file only, so a trace
  that has moved to its next file starts its numbers again.
- `trace_files` lists the `trace-*.csv` files, newest first, by when each was
  last written. `trace_metrics` and `trace_rows` read the picked file through a
  `TraceView` in `debug/trace_view.rs`, which reads it from its first row, at
  most two 4 MiB windows a call, and keeps its place between calls. The page
  asks again at once while it has more to read. The view keeps each count for
  the whole file and by the second for its last 10 minutes, the newest 20,000
  times of each kind with when each happened, and the newest 100,000 rows,
  each name held once and let go with the last row that uses it, so a long trace
  holds about the same amount, more only for each value and automation it
  names. A range is measured back from the newest row. The view starts over
  when the file shrinks, and `close_trace` lets it go as the page leaves its
  **Metrics** and **Trace rows** tabs. Each read the page starts makes the
  ones before it stale, so an older answer never replaces a newer one.
- `read_log` returns the last 64 KB of `runtime.log` or `setup.log`, as whole
  lines.
- `debug_bundle` hands the owner's file locations to a blocking task, which
  builds the zip in memory away from the owner thread, so the page's other
  commands do not wait for a large trace to be packed. The page saves the
  returned bytes with the WebView's save picker. Pictures are stored,
  everything else is deflated, with the `zip` crate and its pure-Rust deflate
  backend. `setup.log` records the whole time, packing included.

The same setting adds debug details to the editors. The page asks for them
only while `showDebug` is true, so with it off they cost nothing:

- `explain_reading` and `explain_watched` score the region of the newest frame
  that the value editor's check or the automation editor's watch read, with
  `Check::explain` from `appytappy-sources`. A read decides with the same
  scores: a number's read takes the closest glyph by `GlyphScore`, and a
  state's by the `Difference` that `StateScore::within` also checks. The state
  marked as read comes from a read of the same region, so the scores and the
  state marked as read always agree; the runtime never asks for them. The live
  reading above them is its own read, so it can come from a frame before or
  after. The page asks every 500 ms, less often than the reading, as scoring a
  number checks each character against every glyph.
- `capture_outline` builds the saved application's capture as the runtime
  would at the screenshot's window size, in `debug/outline.rs`: the values
  that its enabled automations watch, of those `Runnable::new` says can run,
  as `automations::watched_values` gives them, each placed with
  `SavedValue::at`, then `Layout::new` for the rectangles copied. It answers
  every saved value, with where it reads or why it does not, and the page
  draws them over the screenshot. The page asks again when the window size or
  the saved file changes.
- `window_scores` compares each window that the picker lists with the match
  being edited, field by field, with the same `Pattern::matches` the match
  uses, and gives the `WindowFit` that `match_windows` gives the windows it
  takes. With scores, the window list takes each window's label from its
  score, so the label and the score come from the same read of the windows.

Every command passes its own name to `ask` or `try_ask`, which a test in
`bindings.rs` checks against the command table. `debug_bundle` passes its name
to `record_command` once the bundle is packed instead. The work then records how long
the command took from being sent, and why it failed, in `SetupLog`, which the
owner thread holds and opens only while `showDebug` is true. Loading or saving
the settings opens or closes it.

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
