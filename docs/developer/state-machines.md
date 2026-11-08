# State machines

AppyTappy has few `enum State` types, but much of its behavior is still a set of
state machines. This page names each one, draws it, and says which events move
it. It also says where a machine can reach a state it never leaves, or a state
that does the wrong thing. Each of those has an issue, linked under
**Open problems** at the end of its section.

Read [Architecture](architecture.md) first: it describes the threads that these
machines run on.

## How to read this page

An _implicit_ state machine is state that is spread over a few fields, flags,
or `Option`s, plus the code that changes them. No type lists its states, so
this page does. For example, the runtime has no type for an application's run
state; whether it runs follows from three maps and a flag on the owner thread.

The diagrams are [Mermaid](https://mermaid.js.org/) state diagrams. GitHub
draws them. A box is a state, and an arrow is a transition labeled with the
event that causes it. `[*]` is where the machine starts or ends.

Many messages travel between threads, so a message can arrive after the state
it was about has gone. Four guards drop such stale messages:

- **Session IDs.** The runtime gives each reader session a new `SessionId`. The
  owner drops a reading whose session no longer runs.
- **Generations.** The setup app's owner thread adds 1 to `generation` each
  time it starts or stops a capture. A frame or a failure from an older
  generation is dropped.
- **Recording IDs.** Each recording gets a new `id`, so text that the text
  reader sends back for a discarded recording is dropped.
- **Overlay numbers.** Each overlay capture gets its own number from
  `Overlays::next`, and a reading from an older capture is dropped.

Source files are named as code text rather than linked, so a file that moves
leaves no broken link.

## Runtime

### Owner thread start and stop

`owner::watch` in `apps/runtime/src/owner.rs` starts every thread, runs the
event loop, and stops every thread in a fixed order.

```mermaid
stateDiagram-v2
    [*] --> Starting
    Starting --> Running: threads started, first plan carried out
    Starting --> [*]: the window events hook, detection, or the action thread cannot start
    Running --> Running: Carry orders, Reload
    Running --> Undoing: Quit, CloseRequested, DetectionEnded
    Undoing --> Stopping: undo orders queued
    Stopping --> [*]: every thread joined
```

- **Starting.** The window events hook starts first, and the runtime exits if
  it cannot. Then the tray and the reload watch start. Each of those two is
  optional: the runtime logs why it cannot make one and runs without it. Then
  the saved files are read, and the lifecycle and action threads start. If
  either cannot start, the runtime stops what it started and exits with the
  error.
- **Running.** Each event becomes a `Step`: `Carry` some orders, `Reload` the
  saved files, or `Stop`. After each step the tray gets a fresh menu if it
  changed.
- **Undoing.** `model.set_enabled(false)` stops every running application and
  queues the undo of what its automations changed.
- **Stopping.** The tray stops first, so its icon goes first. Then the reload
  watch and the window watch stop. Dropping the inputs and the action jobs ends
  detection and the action thread, which runs the queued undos before it ends.
  Each reader is stopped, and every thread is joined.

Open problems:

- [#88](https://github.com/TheLunchTrae/appy-tappy/issues/88): when Windows ends
  the session, the runtime can be ended before the undo runs.
- [#92](https://github.com/TheLunchTrae/appy-tappy/issues/92): a reload that cannot
  read the saved files replaces the running plan with an empty one.

### Runtime state

`model::State` is the runtime's one named state, which the log and the tray
show. It is derived each time from the model rather than stored:

```mermaid
stateDiagram-v2
    [*] --> Idle
    Idle --> Attached: a watched application starts
    Attached --> Idle: the last watched application exits
    Idle --> Degraded: a process snapshot fails
    Attached --> Degraded: a process snapshot fails
    Degraded --> Idle: a snapshot works, nothing watched runs
    Degraded --> Attached: a snapshot works, a watched application runs
```

Degraded wins over the other two while `problem` is set. [Lifecycle](architecture.md#lifecycle)
explains each state.

### Detection timers

The lifecycle thread (`apps/runtime/src/lifecycle.rs`) decides when to take a
process snapshot. Its state is two optional deadlines, `scan_at` and
`rescan_at`, plus whether any watched application runs and whether the last
snapshot failed.

```mermaid
stateDiagram-v2
    [*] --> Due: start with a non-empty watchlist
    [*] --> Quiet: start with nothing to watch
    Quiet --> Settling: a relevant window event
    Settling --> Due: 250 ms after the first event
    Due --> Quiet: snapshot, nothing runs and it worked
    Due --> Backstop: snapshot, an application runs or it failed
    Backstop --> Settling: a relevant window event
    Backstop --> Due: 5 seconds pass
    Quiet --> Due: the watchlist changes and is not empty
    Settling --> Due: the watchlist changes and is not empty
    Backstop --> Due: the watchlist changes and is not empty
    Backstop --> Quiet: the watchlist becomes empty while snapshots work
```

A created top-level window is relevant while anything is watched. A destroyed
window is relevant only while a watched application runs. More events while
**Settling** do not move the deadline, so a burst becomes one snapshot. The
two delays are `SETTLE` and `RESCAN`.

Each snapshot ends the session of an application whose process is gone, and
starts one for an application that runs without one. A new watchlist ends the
session of each application it drops or whose executable pattern it changes.

### Per-application run state

The model (`apps/runtime/src/model.rs`) has no run state type. An application's
automations run while all of these hold:

- It is in `attached`: detection found its process.
- `enabled` is true: automations are on in the tray.
- The plan has automations for it.

When they all hold, the application has an entry in `running`, with its engine
session, its reader's `SessionId`, and the plan it started with.

```mermaid
stateDiagram-v2
    [*] --> Detached
    Detached --> Running: Started, while enabled and planned
    Detached --> Attached: Started, while disabled or not planned
    Attached --> Running: enabled, or a reload plans it
    Running --> Attached: disabled, or a reload drops it
    Running --> Running: a reload changes its plan, so it stops and starts again
    Running --> Detached: Exited
    Attached --> Detached: Exited
```

- **Running to Attached or Detached.** The engine session is stopped or ended,
  which releases every rule that restores. The reader is stopped.
- **A reload that changes the plan.** The application stops, which undoes what
  its automations changed, and starts again with `Start::Resume`. A resumed
  session fires no `Started` rules, because those already ran.
- **Unseen.** When the reader says the values can no longer be seen, the owner
  calls `forget_values`. The rules keep their state, so a rule that fired stays
  fired until new readings decide its condition.

Open problems:

- [#92](https://github.com/TheLunchTrae/appy-tappy/issues/92): a store-level read
  error, or one invalid automation, makes the next plan empty.
- [#96](https://github.com/TheLunchTrae/appy-tappy/issues/96): the session follows
  one process while the reader can read another copy's window.

### Reader thread

Each running application has a reader thread (`read` in
`apps/runtime/src/workers/reader.rs`). The thread's position in its loop is its
state.

```mermaid
stateDiagram-v2
    [*] --> Finding
    Finding --> Waiting: no window fits, or windows cannot be listed
    Waiting --> Finding: 1 second passes, or Lost or Resized
    Finding --> Waiting: the capture cannot start
    Finding --> Capturing: the capture starts
    Capturing --> Finding: Lost, the window closed
    Capturing --> Finding: Resized, the window took another drawn size
    Capturing --> [*]: Stop
    Waiting --> [*]: Stop
```

- **Finding.** `find_window` lists the processes and windows and picks the best
  window with `best_window`. It looks across every process whose executable
  fits the pattern.
- **Capturing.** The thread blocks on `control.recv()`. The capture's frame
  handler runs on another thread and sends `Lost` or `Resized` back to this
  one. Only `Lost`, `Resized`, or `Stop` wakes it.
- Each problem, such as no window fitting yet, is sent once until a different
  problem or a working capture replaces it.

Open problems:

- [#90](https://github.com/TheLunchTrae/appy-tappy/issues/90): a capture that keeps
  running but stops giving usable frames never wakes the thread.
- [#96](https://github.com/TheLunchTrae/appy-tappy/issues/96): `Job` carries no
  process ID, so the reader can pick another copy's window.

### Frame handler

`FrameReader::handle` runs once per frame and decides what each frame means.
Its state is `size` (the window size whose values it reads), `last` (each
value's last reading), and `seen` (whether it sent a reading since it last sent
`Unseen`).

```mermaid
stateDiagram-v2
    [*] --> Reading
    Reading --> Reading: frame read, a Sample for each value that changed
    Reading --> Reading: frame error, reported once
    Reading --> Unseen: the window closed
    Reading --> Unseen: the window took another size
    Unseen --> [*]: Lost or Resized sent to the reader thread
    Unseen --> Reading: another size with no values drawn at it
```

- A `Sample` goes to the owner only when a value reads differently from its
  last reading.
- A frame error other than a closed window is logged once and the capture goes
  on. The values read before stay in the engine.
- A window size at which no value was drawn sends `Unseen` and keeps the
  capture, because a restart would find the same size.

Open problems:

- [#90](https://github.com/TheLunchTrae/appy-tappy/issues/90): frame errors that
  never stop, such as a lost graphics device, keep stale values in the engine.

### Read schedule

`ReadSchedule` in `crates/sources/src/schedule.rs` sets the wait before the next
frame is read. Its only state is the current wait.

```mermaid
stateDiagram-v2
    [*] --> Fast
    Fast --> Backing: a steady read doubles the wait
    Backing --> Backing: a steady read doubles the wait
    Backing --> Slow: the wait reaches the maximum
    Backing --> Fast: a value changed
    Slow --> Fast: a value changed
```

The reader always uses `ReadInterval::DEFAULT`, so **Fast** is 16 ms and
**Slow** is 500 ms.
`Activity::NearThreshold` also resets the wait to the minimum, but the reader
never reports it, so a value near a rule's threshold backs off like any other.

### Number text

The number check reads characters left to right, and `NumberText::push` in
`crates/sources/src/screen/digits.rs` keeps which part of a number it is in.

```mermaid
stateDiagram-v2
    [*] --> Empty
    Empty --> Sign: minus
    Empty --> Whole: digit
    Empty --> Empty: any other character
    Sign --> Whole: digit
    Sign --> Empty: any other character, the sign is dropped
    Whole --> Whole: digit
    Whole --> Fraction: the first point
    Fraction --> Fraction: digit
    Whole --> Ended: any other character
    Fraction --> Ended: any other character
    Ended --> Ended: anything
```

Before a digit, a character that does not fit starts the number over. After a
digit, it ends the number, and the rest of the region is ignored, so `12/50`
reads as 12 once `/` is learned. A region with more than 24 characters (`MAX_CHARACTERS`) is a read
error.

Open problems:

- [#99](https://github.com/TheLunchTrae/appy-tappy/issues/99): `1,234` reads as 1,
  and `.5` reads as 5, both without an error.

### Rule state

Each rule in an engine session (`crates/engine/src/lib.rs`) has a `RuleState`
of three fields: `armed`, `active`, and `last_fired`. Saved automations make
`Becomes` rules that re-arm when their condition is false, so this is the
machine that saved automations use.

```mermaid
stateDiagram-v2
    [*] --> Armed
    Armed --> Fired: the condition holds and the cooldown passed
    Armed --> Armed: the condition holds during the cooldown
    Fired --> Armed: the condition is false, and a restoring rule is released
    Fired --> Fired: the condition is unknown, such as after Unseen
```

- **Fired** means `armed` is false. A rule that restores is also `active` until
  it is released.
- A rule whose re-arm is `Never` is released when its condition turns false but
  stays disarmed for the rest of the session.
- In one sample, releases run before fires, so a restore never undoes a change
  another rule makes on the same value.
- A session that ends or stops releases every active rule. One that ends
  because the application exited then fires the `Exited` rules.
- The engine runs only when a sample of any of the session's values arrives.
  It has no timer.

Open problems:

- [#91](https://github.com/TheLunchTrae/appy-tappy/issues/91): a rule held back by
  its cooldown is checked again only on the next sample, so it never fires
  while the value holds still.

### Action thread and restore journal

The action thread (`apps/runtime/src/workers/act.rs`) makes the media and audio
controls once, then runs each job in order.

```mermaid
stateDiagram-v2
    [*] --> Making
    Making --> Ready: the controls are made
    Making --> Draining: the controls cannot be made
    Ready --> Ready: Run or Undo job
    Ready --> [*]: the job channel closes
    Draining --> [*]: the job channel closes
```

In **Draining** every job is dropped.

The journal in `crates/actions/src/journal.rs` holds one entry per change that
a restoring rule made, in the order they were made. For each target, such as
one audio session's volume, the entries form a stack:

- **Perform, restoring.** The change is pushed with the state before it.
- **Perform, not restoring.** Every entry for that target is removed, so no
  release undoes the permanent change.
- **Release.** The rule's entries are taken newest first. If a later entry on
  the same target exists, it inherits the older state. Otherwise the change is
  undone. Each entry leaves the journal before its undo runs.

The journal lives only in memory.

Open problems:

- [#93](https://github.com/TheLunchTrae/appy-tappy/issues/93): one failure making
  the controls leaves the thread in **Draining** for the whole run.
- [#94](https://github.com/TheLunchTrae/appy-tappy/issues/94): a failed undo is
  dropped with its entry and never tried again.
- [#88](https://github.com/TheLunchTrae/appy-tappy/issues/88): a forced end loses the
  in-memory journal.

### Tray thread

The tray thread (`crates/platform/src/win/tray.rs`) runs a message loop. Its
state is whether the menu is open.

```mermaid
stateDiagram-v2
    [*] --> Shown
    Shown --> MenuOpen: right-click or the keyboard
    MenuOpen --> Shown: a pick or a dismiss, then the icon is refreshed
    Shown --> Shown: a close request is forwarded
    MenuOpen --> MenuOpen: a close request is dropped
    Shown --> [*]: stop
    MenuOpen --> [*]: stop closes the menu first
```

[Tray](architecture.md#tray) explains why a post made while the menu is open
is dropped.

Open problems:

- [#88](https://github.com/TheLunchTrae/appy-tappy/issues/88): the end-of-session
  notice is acknowledged before the undo runs, and is dropped while the menu is
  open.

### Reload watch

The reload watch waits on the auto-reset named event `Local\AppyTappy.Reload`
and sends `Reload` each time the setup app sets it. Several saves before the
reload watch wakes become one reload. The owner then reads the saved files,
sends detection the new watchlist, and calls `replan`.

Open problems:

- [#92](https://github.com/TheLunchTrae/appy-tappy/issues/92): a read that fails
  stops every automation until the next save.

### Log rotation

`Log` in `apps/runtime/src/log.rs` appends to `runtime.log`. A line that would
take it past 512 KiB (`LIMIT`) first renames it to `runtime.old.log` and opens a new
file.

```mermaid
stateDiagram-v2
    [*] --> Appending
    Appending --> Rotating: the next line passes 512 KiB
    Rotating --> Appending: renamed and reopened
    Rotating --> Rotating: the rename fails, the line is lost and the next line tries again
    Rotating --> Broken: renamed, but the new file cannot open
    Broken --> Broken: every line tries to rename a missing file
```

Open problems:

- [#101](https://github.com/TheLunchTrae/appy-tappy/issues/101): **Broken** lasts for
  the rest of the run.

## Setup app

### Pages and flows

The React side (`apps/setup/src/App.tsx`) has two pieces of state: `page`, the
tab shown, and `flow`, the value, automation, or application being made or
edited. A flow covers the page while it is open.

```mermaid
stateDiagram-v2
    [*] --> Page
    Page --> Flow: New or Edit
    Flow --> Page: Cancel
    Flow --> Saving: Save
    Saving --> Page: saved, the page that lists it opens
    Saving --> Flow: the save fails, the flow shows why
    Flow --> Page: a tab is clicked
    Saving --> Page: a tab is clicked, and the save goes on
    Flow --> Flow: Add application in the value flow opens the application flow
```

The unsaved work of a flow, such as its screenshots and recording, lives in the
flow's components and in the owner thread. Leaving the value flow, by a tab or
by **Add application**, unmounts it and stops its capture, which clears its
screenshots and recording. **Change window** unmounts only the capture view: the
capture and the recording keep running until the next pick, or until the value
flow closes.

Open problems:

- [#89](https://github.com/TheLunchTrae/appy-tappy/issues/89): a tab click or
  **Change window** throws the flow's work away without asking, and a save that
  finishes late can move the user to another page.

### Capture slot

The setup app's owner thread (`apps/setup/src-tauri/src/owner.rs`) has one
capture slot, `session`, shared by the value preview and the automation
editor's watch. Starting one stops the other. Its other state is `generation`,
`latest` (the newest frame), `failure`, `screenshots`, `check`, `recording`,
and `watched`.

```mermaid
stateDiagram-v2
    [*] --> Closed
    Closed --> Previewing: a window is picked
    Closed --> Watching: the automation editor watches a value
    Previewing --> Failed: the capture reports an error
    Previewing --> Closed: stop
    Watching --> Closed: stop
    Failed --> Closed: stop
    Previewing --> Previewing: another window is picked, which stops and starts again
    Failed --> Previewing: another window is picked
    Previewing --> Watching: the automation editor watches a value
    Watching --> Previewing: a window is picked
    Watching --> WatchFailed: the watch reports an error
    WatchFailed --> Closed: stop
    WatchFailed --> Watching: the editor watches again
    WatchFailed --> Previewing: a window is picked
```

- **Stop** closes the capture, adds 1 to `generation`, and clears every field
  above.
- **Failed** sets `failure`. Preview answers with it, but frames keep updating
  `latest`, and only a stop clears it.
- **WatchFailed** closes the capture and keeps the watch's error for the editor
  to show.
- The screenshot shortcut is on while the capture view shows, except while the
  user is choosing a new shortcut.

Open problems:

- [#98](https://github.com/TheLunchTrae/appy-tappy/issues/98): one frame error stays
  shown until the window is picked again, and a screenshot can be taken after
  the window closed.
- [#103](https://github.com/TheLunchTrae/appy-tappy/issues/103): choosing a new
  shortcut and then leaving the button keeps the shortcut off.

### Recording

A recording (`apps/setup/src-tauri/src/recording.rs`) keeps each reading a
region shows once, for the user to review. It is `None` or a `Recording` whose
`stopped` field says whether it still records.

```mermaid
stateDiagram-v2
    [*] --> None
    None --> Recording: Record
    Recording --> Stopped: Stop
    Stopped --> Recording: Record more, with the same settings
    Stopped --> Recording: Record with other settings starts a new recording
    Recording --> None: Discard, a window is picked, or the value flow closes
    Stopped --> None: Discard, a window is picked, or the value flow closes
```

- Each frame is cropped to the region. A crop of another window size sets the
  recording's `problem` and is skipped. A crop with no ink is skipped.
- A crop whose characters are new is kept and sent to the text reader. A
  recording keeps at most 200 readings (`MAX_READINGS`).
- **Record more** needs the same region, ink, tolerance, and window size.
- Saving a value takes the window size from the screenshot it names, and builds
  the check from the labeled screenshots and the recording.

Open problems:

- [#98](https://github.com/TheLunchTrae/appy-tappy/issues/98): a capture failure
  leaves the recording in **Recording**, and the UI keeps saying so.
- [#95](https://github.com/TheLunchTrae/appy-tappy/issues/95): a value can be saved
  with a window size that its region, labels, and recording were not drawn at.

### Text reader

The text reader (`apps/setup/src-tauri/src/ocr.rs`) runs on its own thread. It
loads its model on the first reading it gets, then reads each reading and sends
the text back with the recording's ID and the reading's index. The owner drops
text for a recording that is gone. If the thread has stopped, the owner marks
the reading as failed and asks the user to type it.

### Overlays

Each application whose overlay is on has an entry on the owner thread
(`apps/setup/src-tauri/src/overlay.rs`) and a window of the setup app's own
(`apps/setup/src-tauri/src/overlay/window.rs`). The window shows as an overlay
over the application or as a panel.

```mermaid
stateDiagram-v2
    [*] --> Off
    Off --> Opening: the overlay is turned on
    Opening --> Reading: the file and the window are found
    Opening --> Retrying: no window fits, or the file cannot be read
    Retrying --> Opening: 5 polls pass (RETRY_POLLS)
    Reading --> Retrying: the capture reports an error, such as a closed window
    Reading --> Reading: the window takes another size
    Reading --> Opening: a save changes values or applications
    Retrying --> Opening: a save changes values or applications
    Reading --> Off: the overlay is turned off
    Retrying --> Off: the overlay is turned off
```

`overlay::window::sync` opens a window for each overlay that is on and closes
each that is not. It runs when the main window mounts and after each overlay
change.

Open problems:

- [#97](https://github.com/TheLunchTrae/appy-tappy/issues/97): removing an
  application does not run `sync`, so its window stays open, and a panel cannot
  be closed.

### Saved files and copies

Every write in `crates/config/src/store.rs` goes to a temporary file, is synced,
and is renamed over the saved file, so a reader sees the old file or the new
one. Each save reads the file, changes it, and writes it back. After a request
that saves values, automations, or applications, the setup app tells the
runtime to read the saved files again. Turning an overlay on or off and moving a
panel write the application's file without telling the runtime.

Open problems:

- [#100](https://github.com/TheLunchTrae/appy-tappy/issues/100): two copies of the
  setup app can overwrite each other's saves, and a crash leaves the temporary
  file behind.

## MCP investigation server

The [MCP investigation server](mcp.md) answers one request at a time. Its
`Session` in `apps/mcp/src/tools.rs` keeps the executables the user refused,
and at most one attached process with its memory scan.

```mermaid
stateDiagram-v2
    [*] --> Detached
    Detached --> Attached: process_attach, the user allows it
    Attached --> Attached: process_attach for another process
    Attached --> Scanning: memory_scan_first
    Scanning --> Scanning: memory_scan_next
    Scanning --> Scanning: memory_scan_first starts over
    Attached --> Detached: process_detach
    Scanning --> Detached: process_detach
    Scanning --> Attached: process_attach for another process
    Attached --> Detached: process_attach fails after the user allows it
    Scanning --> Detached: process_attach fails after the user allows it
    Scanning --> Attached: memory_scan_first fails
```

A program the user refuses is refused again without asking for the rest of the
session, whatever its process ID.

Open problems:

- [#102](https://github.com/TheLunchTrae/appy-tappy/issues/102): an attach that fails
  after the user allows it drops the process already attached, and a first scan
  that fails drops the old scan.
