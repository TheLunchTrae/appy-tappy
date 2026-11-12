# Debug page

The **Debug** page in AppyTappy Setup shows what AppyTappy is doing and gathers
what a developer needs when something goes wrong. It is hidden at first. To
show it, turn on **Show debug details** under **Debug** on the
[Settings](Settings.md#debug) page; a **Debug** tab then appears after
**Settings**. Turning the setting off hides the tab again.

While debug details are on, AppyTappy Setup also writes its own log,
`setup.log`, beside the background part's `runtime.log`. It names each thing
the page asked for that failed or took a while, so a slow or failing page can
be told apart from a slow or failing runtime.

## Runtime

The top line says whether the background part is running, and whether a
[runtime trace](Settings.md#runtime-trace) is recording and until when.

Under it are timings from the newest trace file, over its latest 10,000
actions and changes:

- **Frame to action, median** and **95th percentile.** How long it took from
  the application drawing a frame to the action it led to starting. Half the
  actions started sooner than the median, and 1 in 20 took longer than the
  95th percentile.
- **Rule handling, median.** How long the background part took to handle each
  change it read.
- **Rows dropped.** Rows the trace left out because it could not write them as
  fast as they came. Any number above 0 means it records more than it keeps up
  with; record less, such as without **Each read**.

A dash means the trace did not record what that number needs. Frame to action
needs **Changes** and **Actions** recorded, and rule handling needs **Rules**.
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
