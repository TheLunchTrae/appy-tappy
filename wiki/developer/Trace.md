# Trace

The runtime trace records what the runtime reads and does, with timings, to a
CSV file. It is for benchmarking: how long a check takes, how long a changed
value takes to reach the rules, and how long an automation's actions take to
run. It is off unless the user turns it on under **Runtime trace** on the
Settings page, which [Settings](../user/Settings.md#runtime-trace) describes
for users.

## Settings

The trace's settings are `trace` in `settings.json`, as `TraceDef` in
`crates/config/src/settings_file.rs` defines them:

- `on`: whether the runtime records.
- `until`: when recording ends, as Unix time in milliseconds, or `null` for
  no end. The setup app sets it when the user turns the trace on, so a trace
  the user forgets stops by itself, and the runtime never has to write the
  settings file to turn it off.
- `record`: the categories to record: `readings`, `changes`, `rules`,
  `actions`, and `lifecycle`. Every category but `readings` is on at first,
  because a row for every read of every value makes large files quickly.
- `fileLimitMb`: the size at which a file ends and the trace continues in a
  new one, 50 MB at first, from 1 to 1000.
- `keepFiles`: how many trace files to keep, 5 at first, from 1 to 100. The
  oldest beyond that are deleted.

## The files

Trace files are in `%LOCALAPPDATA%\AppyTappy\traces\`, as `Locations::traces`
says. Each trace's first file is named for the local time it started, as
`trace-2026-11-08-142530.csv`, with `-2` and so on added should that name be
taken. A file that reaches the size limit ends, and the trace continues in
`-part2.csv`, `-part3.csv`, and so on.

Every file starts with this header:

```text
time_ns,thread,event,app,item,value,took_ns,cause,detail
```

- `time_ns`: when it happened, in nanoseconds since the trace started.
- `thread`: the thread it happened on: `reader`, `owner`, `action`,
  `lifecycle`, or `trace` for the trace's own rows.
- `event`: what happened, from the table below.
- `app`: the application's name.
- `item`: the value's name, for readings and changes, or the automation's
  name, for rules and actions.
- `value`: the value read.
- `took_ns`: how long it took, in nanoseconds, where the event says.
- `cause`: when the frame arrived whose reading led to this row, in
  nanoseconds since the trace started. Every row one reading causes has the
  same `cause`, so a filter on it follows one change from the frame to the
  action it ran.
- `detail`: a word or a count, where the event says.

A field the event leaves out is empty. A field with a comma or a quote is
quoted, with its quotes doubled.

| Category  | Event                       | Thread    | `took_ns`               | `detail`          |
| --------- | --------------------------- | --------- | ----------------------- | ----------------- |
| Readings  | `read`                      | reader    | the check               |                   |
| Changes   | `changed`                   | reader    |                         |                   |
| Changes   | `received`                  | owner     | since the frame arrived |                   |
| Rules     | `handled`                   | owner     | handling the event      | the kind of event |
| Rules     | `fired`, `held`, `released` | owner     |                         |                   |
| Actions   | `queued`                    | owner     |                         |                   |
| Actions   | `started`                   | action    |                         |                   |
| Actions   | `done`                      | action    | running the actions     | `ok` or `failed`  |
| Lifecycle | `snapshot`                  | lifecycle | the process snapshot    | processes listed  |
| Lifecycle | `reading`, `stopped`        | owner     |                         |                   |
| Lifecycle | `reloaded`                  | owner     | reading the saved files |                   |

`held` is a rule that would have fired, but its cooldown held it back. The
trace's own rows are `start`, the first row, whose detail is the local time
and the runtime's version; `part`, the first row of each later part, whose
`value` is the part's number; and `dropped`, whose `value` counts the records
dropped before the next row, as [Overhead](#overhead) describes.

## How it works

The code is in `apps/runtime/src/trace.rs` and `apps/runtime/src/trace/`.

- **Handles.** Each thread that records holds a `Trace`. Off, it holds
  nothing, and each place that could record checks that first and does no
  more: it reads no clock, builds no record, and sends nothing. On, it knows
  which categories to record, and builds a `Record` only for those. A record
  holds no text of its own, only the session, the value's or rule's position
  in the plan, and fixed words, so building one allocates nothing.
- **Writer thread.** The writer, in `trace/writer.rs`, owns the files. Each
  handle sends its records into one bounded buffer of `BUFFER` records, which
  is allocated when the trace starts. A send never waits: when the buffer is
  full, the record is dropped and counted, and the next record from the same
  handle that gets through carries the count, so the file says how many were
  lost. The writer runs at the lowest thread priority, so it takes only time
  that nothing else wants, and flushes what it wrote about once a second. It
  turns each session and position into names from the plans the owner gives
  it, so names are looked up only there.
- **Switch.** The owner thread holds the `Switch`, in `trace/switch.rs`. Each
  time it reads the settings, the switch starts a trace, keeps the one that
  runs, or stops it. A new end time keeps the trace that runs. Other changes,
  such as another category, the file size limit, or the number of files to
  keep, stop it and start a new one in a new file, because a file's rows
  should all record the same things.
- **Handing out a new trace.** When the trace changes, the owner gives the new
  handle to the action thread and to detection over their channels, and the
  readers start over with it, as a new read interval restarts them. Settings
  are read before the saved applications and automations, so a reader the new
  plan starts gets the new trace, and no reader starts twice.
- **Stopping.** The owner wakes when the trace's end time comes and stops it.
  When a file cannot be written, as when the disk is full, the writer logs why,
  tells the owner, and stops; every handle turns itself off at its next record.
  The runtime stops the trace last when it quits, so the file holds what the
  other threads did up to their end.

## Overhead

Off, the trace costs one check of an empty handle at each place that could
record. On, each recorded row costs a clock read and a send into the buffer;
the formatting and the disk writes happen on the writer thread, at the lowest
priority. `readings` adds a row and a clock read for every read of every
value, so it costs the most, and the files grow fastest. A trace measures a
runtime that also records, so compare timings between traces that record the
same categories.
