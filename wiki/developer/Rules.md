# Rules

A rule says when AppyTappy acts on an application and which
[actions](Actions.md) it runs. The types live in
`appytappy-config` (`crates/config/src/rule.rs` and
`crates/config/src/signal.rs`), and the evaluator lives in `appytappy-engine`.
The setup app's automations file holds a subset of this model, and
[Saved files](Setup-app.md#saved-files) describes it. Each enabled automation
that passes the setup app's and the runtime's checks becomes one rule: a
Becomes trigger on one comparison that re-arms when false, a cooldown, and
restore on release or never. The file has no all or any groups, no Changes or
On triggers, no guards, and no other re-arm. A format that holds the whole
model belongs to [#6](https://github.com/TheLunchTrae/appy-tappy/issues/6).

## Signals and conditions

A signal value is a finite number, a flag (yes or no), or a state: one of the
names a state check gives its looks, or `none`. Sources produce the values; a
non-finite reading, such as NaN, is not a value.

A condition tests the application's current signal values:

- A **comparison** relates one signal to a value: below, at most, equals, not
  equals, at least, or above. Flags and states support only equals and not
  equals, and states compare by exact name.
- An **all** group holds when every condition in it holds, and an **any**
  group holds when at least one does. Conditions nest up to eight levels,
  counting the comparison.

A comparison on a signal with no value yet, or with a value of the other
type, does not hold. A signal with no value is also unknown: an all group with
an unknown condition and no false one is unknown, and so is an any group with
an unknown condition and no true one. The runtime forgets an application's
values when it can no longer see them, and a rule that fired is released only
once its condition is known to be false, so values that are out of sight
neither release nor fire it. Rules have no scripting: conditions are the only
logic.

## Triggers

Each rule has one trigger:

- **Becomes** fires when its condition starts to hold. A threshold crossing,
  such as "a bar's fill reaches 90%", is a condition that becomes true. If
  the first values a session reads already make the condition hold, the rule
  fires then, so attaching after the value got there still fires.
- **Changes** fires when a new value of one signal differs from its last value.
  The first value of a session is not a change.
- **On** fires on an application event: the application started, exited,
  gained the keyboard focus, or lost it.

Changes and On triggers take an optional guard: a condition that must also
hold when the trigger fires. A start trigger takes no guard, because no signal
has a value when a session starts.

## Firing again

A Becomes rule fires once and then waits to be re-armed:

- **When false** re-arms it once its condition stops holding. When a
  progress bar empties and starts over, the drop re-arms the rule for the
  next time it fills.
- **When** re-arms it once its condition stops holding and a second
  condition holds. It keeps a value that wavers around the threshold from
  firing the rule again: a rule at 90% that re-arms below 5% ignores a dip
  to 89%.
- **Never** fires once per application session.

A rule never re-arms while its own condition holds, so it fires once for each
stretch of holding.

Every rule also has a **cooldown**: after it fires, it cannot fire again until
the cooldown has passed. A Becomes rule held back by its cooldown stays armed,
and if its condition still holds when the cooldown ends, it fires then, even
when no new value arrives. An event that arrives during an On rule's cooldown
is dropped. The cooldown only limits firing again: there is no hold time, so a
rule fires on the first value that makes its condition hold.

## Restoring

A rule can restore on release, which undoes what its paired actions changed,
such as setting the volume back. A rule is released:

- for a Becomes trigger, when it re-arms, or for a **Never** rule, when its
  condition stops holding. A rule that re-arms on a second condition stays
  active through a dip that does not re-arm it;
- for a focus trigger, on the opposite focus event;
- for every rule that has not been released yet, when the session ends.

Changes rules and exit rules are never released, so they cannot restore. A
focus rule that restores takes no cooldown, because a focus event dropped
during the cooldown would leave the rule released while the application has
the focus.

## Evaluation

The engine evaluates rules on events, and on a timer only for the end of a
cooldown. The runtime's owner thread keeps one session per running application.
A session starts when the application starts, with every rule armed and no
signal values, and ends when the application exits. It takes three kinds of
events, each with the time it happened: a signal's new value, a focus change,
and the application's exit. For each event it returns effects: release a rule,
fire it, or report that its cooldown held it back. Releases come first, then
fires, each in rule order, so a restore never undoes a change that another rule
makes for the same event. A held rule runs nothing; the runtime trace records
it ([Trace](Trace.md)).

A value that holds still sends no new sample, so a Becomes rule that its
cooldown held back could wait forever. A session therefore reports when the
next such rule is due (`Session::next_due`): the time its cooldown ends, for a
rule that is armed and whose condition holds on the values recorded. The owner
thread waits for its next event until then at most, and if none comes, checks
those rules again at that time (`Session::recheck`).

The first focus report of a session fires the matching focus rules, whether the
application has the focus or not. A report that repeats the current focus does
nothing.

At exit, the session releases every active rule first and then fires the exit
rules, so restores run before anything the exit rules do. The runtime can also
stop a session while the application keeps running, for a pause or a config
change: that releases every active rule and fires nothing. The session that
replaces it resumes without firing the start rules again; like any new session,
it starts with every rule armed and no signal values.

## Examples

These use plain words rather than the saved file's format, and only the first
can be written in the automations file yet. `Player.exe` stands for whichever
app plays the audio.

A number in a region of the window reaches a threshold: pause the music while
it is at 90 or more.

```text
trigger:  becomes progress at least 90
actions:  pause Player.exe
re-arm:   when false
cooldown: none
restore:  on release (resume when the number drops or the application exits)
```

Any application: turn the music down while it has the focus.

```text
trigger:  on focused
actions:  set Player.exe volume to 20%
cooldown: none
restore:  on release (put the volume back when it loses the focus or exits)
```

A counter on screen changes while an icon is visible: skip to the next track
each time.

```text
trigger:  changes counter, guard icon_visible equals yes
actions:  next track on Player.exe
cooldown: 10 seconds
restore:  never
```
