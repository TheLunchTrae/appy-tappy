# Debug page

The **Debug** page in AppyTappy Setup shows what AppyTappy is doing and gathers
what a developer needs when something goes wrong. It is hidden at first. To
show it, turn on **Show debug details** under **Debug** on the
[Settings](Settings.md#debug) page; a **Debug** tab then appears after
**Settings**. Turning the setting off hides the tab again.

While debug details are on, AppyTappy Setup also writes its own log,
`setup.log`, beside the background part's `runtime.log`. It names each request
the page made to the rest of AppyTappy Setup, how long it took, and why it
failed, if it did, so a slow or failing page can be told apart from a slow or
failing runtime.

## Debug details in the editors

The same setting adds debug details to the pages that read a window live.

- **Live readings.** Under the live reading in the value editor and in the
  automation editor, a dashed line says how long the check took and how old
  the frame it read was. Under it are the check's scores:
  - **Number:** each character read, how closely it matches the reference,
    and the closest other character. A character reads at 75% or closer, so a
    close second means the reference needs more samples of both.
  - **State:** how far the region is from each state, on average per color
    channel, against the state's tolerance. The state read is marked.
  - **Color:** the region's average color beside the color looked for, and the
    tolerance.
  - **Bar fill:** how many of the pixels near either color are nearer the
    filled one, before it is rounded.
- **Capture outlines.** On a screenshot in the value editor, dashed outlines
  show the rectangles the background part copies out of each frame at that
  window size, for the values watched by enabled automations that can run. A
  solid outline marks each watched value and a faint one each other saved
  value, with its name. A line under the preview counts them, and names each value that does
  not read at that size, with why.
- **Window match.** In the application editor, each open window says which of
  its title, class and executable match, and why the match leaves it out if
  it does, such as a field that must match or, with the fallback on, another
  window that qualifies.

## Runtime

The top line says whether the background part is running, and whether a
[runtime trace](Settings.md#runtime-trace) is recording and until when.

Under it are timings from the newest trace file, over its latest 10,000
actions and changes:

- **Frame to action, median** and **95th percentile.** How long it took from
  the application drawing a frame to the action it led to starting. Half the
  actions started sooner than the median, and 1 in 20 took longer than the
  95th percentile.
- **Rule handling, median** and **95th percentile.** How long the background
  part took to handle each event, such as a changed value or a reload.
- **Running actions, median** and **95th percentile.** How long the background
  part took to run an automation's actions once its rules matched.
- **Rows dropped.** Rows the trace left out because it could not write them as
  fast as they came. Any number above 0 means it records more than it keeps up
  with; record less, such as without **Each read**.

A dash means the trace did not record what that number needs. Frame to action
needs **Value changes** and **Actions** recorded, rule handling needs **Rules**, and
running actions needs **Actions**. When the trace file is too large to read
whole, the numbers cover its newest rows, and the page says so.
Under the numbers are the trace's newest rows, with times counted from when
the trace started. The page reads the trace again every few seconds. A trace
written by another version of AppyTappy shows why it cannot be read instead.

## Logs

The switch picks the background part's log or AppyTappy Setup's own, and the
box shows its end, read again every few seconds. **Open logs folder** opens
the folder that holds both, with the older copy of each.

## Debug bundle

**Export debug bundle** saves one zip file to share with a developer when
something goes wrong. Nothing is sent anywhere; you choose where the file goes
and who gets it. It holds:

- your settings and automations;
- each application's settings, without its pictures unless you include them;
- both logs and their older copies;
- the newest trace file, unless you leave it out;
- the AppyTappy Setup version and when the bundle was made.

**Include pictures** adds the screenshots your values, references, and click
actions learned from. They are off at first, because they can show what was on
your screen. Even without them, the bundle holds your applications' window
titles and executable names.
