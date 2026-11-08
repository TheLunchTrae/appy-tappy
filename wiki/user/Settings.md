# Settings

The **Settings** tab in AppyTappy Setup holds what applies to AppyTappy as a
whole. Each change takes effect as you make it; there is no Save button. The
background part picks up a change within a moment.

## General

- **Start AppyTappy when I sign in to Windows.** On, the background part
  starts each time you sign in. The installer turns this on. Turn it off to
  start AppyTappy only when you open it yourself.
- **Screenshot shortcut.** The key that takes a screenshot while you
  [make a value](Values.md) or a [reference](References.md), `F9` at first.
  Select **Change** and press the new keys, or Esc to keep the old ones. F1 to
  F24, Print Screen, and Pause work alone. A letter, a digit, Space, an arrow,
  Insert, Delete, Home, End, Page Up, or Page Down needs Ctrl, Alt, Shift, or
  Win with it, so it keeps working everywhere else. Other keys can't be the
  shortcut. **Change shortcut** on the screenshot page
  changes the same setting.

## Reading

AppyTappy reads a value only when its window draws something new. Between
reads it waits:

- **Fastest read.** How long it waits at least while a value changes, 16 ms at
  first, which is about 60 reads a second. A shorter wait notices a change
  sooner.
- **Slowest read.** How long it waits at most while a value holds still,
  500 ms at first. A longer wait uses less CPU while nothing changes.

Each wait is a whole number of milliseconds from 1 to 10000, and the fastest
read cannot wait longer than the slowest. Type a number and press Enter, or
move to another field, to apply it. A change applies to every application
right away and does not undo or rerun any automation.

## Runtime trace

The runtime trace records what the background part reads and does, with
timings, to a file. It is for measuring how quickly AppyTappy responds, such as
how long a changed value takes to run its automation, and for sharing with a
developer. It is off at first.

- **Record a runtime trace.** Turn it on to start recording. The line under it
  says until when it records. Recording makes the background part do a little
  more work, so turn it off when you are done.
- **Record for.** 10 minutes, 1 hour, or until you turn it off. A trace left
  on stops by itself at the end of this time, so a forgotten trace does not
  keep writing. Picking another length while it records counts from then.
- **Record.** What the trace records. **Each read** writes a row for every read
  of every value, which makes large files quickly, so it is off at first. A
  trace records at least one thing.
- **Largest trace file.** A trace continues in a new file once its file
  reaches this size, 50 MB at first.
- **Trace files to keep.** The oldest files are deleted once there are more,
  5 at first.
- **Trace folder.** Where the files go. **Open folder** opens it in File
  Explorer.

Changing what the trace records, or its files' size or number, while it records
starts a new trace in a new file. A trace that cannot write, as when the disk
is full, stops, and the runtime's log says why; the page still says it
records, because it cannot tell. The files are CSV, which a spreadsheet opens;
[Trace](../developer/Trace.md) describes their columns.
