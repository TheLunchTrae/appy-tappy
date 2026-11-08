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
  Select **Change** and press the new keys, or Esc to keep the old ones. A
  letter, a digit, or Space needs Ctrl, Alt, Shift, or Win with it, so the key
  keeps working everywhere else. **Change shortcut** on the screenshot page
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
