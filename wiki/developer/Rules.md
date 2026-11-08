# Rules

A rule says when AppyTappy acts on an application and which
[actions](Actions.md) it runs. The types live in
`appytappy-config` (`crates/config/src/rule.rs` and
`crates/config/src/signal.rs`), and the evaluator lives in `appytappy-engine`.
Each enabled automation in the setup app's automations file that passes the
setup app's and the runtime's checks becomes one rule.
[Saved files](Setup-app.md#saved-files) describes the file, which holds one
comparison per automation and no all or any groups.

A rule has a condition, the actions to run in order, a cooldown, and whether
it restores what it changed.

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

## Firing

A rule fires when its condition starts to hold. A threshold crossing, such as
"a bar's fill reaches 90%", is a condition that becomes true. If the first
values a session reads already make the condition hold, the rule fires then,
so attaching after the value got there still fires.

A rule that fired may fire again once its condition stops holding: when a
progress bar empties and starts over, the drop re-arms the rule for the next
time it fills. A rule never re-arms while its own condition holds, so it fires
once for each stretch of holding.

Every rule also has a **cooldown**: after it fires, it cannot fire again until
the cooldown has passed. A rule held back by its cooldown stays armed, and if
its condition still holds when the cooldown ends, it fires then, even when no
new value arrives. The cooldown only limits firing again: there is no hold
time, so a rule fires on the first value that makes its condition hold.

## Restoring

A rule can restore on release, which undoes what its paired actions changed,
such as setting the volume back. A rule is released when its condition stops
holding, and every rule that has not been released yet is released when the
session stops. A rule whose actions cannot be undone, such as one that only
skips tracks, cannot restore.

## Evaluation

The engine evaluates rules on new values, and on a timer only for the end of a
cooldown. The runtime's owner thread keeps one session per application whose
automations run. A session starts with every rule armed and no signal values,
when the application starts, when automations are turned on, or when a reload
changes what the application runs. It takes each new signal value with the
time it was read, and returns effects: release a rule, fire it, or report that
its cooldown held it back. Releases come first, then fires, each in rule
order, so a restore never undoes a change that another rule makes for the same
value. A held rule runs nothing; the runtime trace records it
([Trace](Trace.md)).

A value that holds still sends no new sample, so a rule that its cooldown held
back could wait forever. A session therefore reports when the next such rule
is due (`Session::next_due`): the time its cooldown ends, for a rule that is
armed and whose condition holds on the values recorded. The owner thread waits
for its next event until then at most, and if none comes, checks those rules
again at that time (`Session::recheck`).

A session stops when the application exits, when automations are turned off,
or when a reload changes what the application runs. Stopping releases every
active rule.

## Example

`Player.exe` stands for whichever app plays the audio. A number in a region of
the window reaches a threshold: pause the music while it is at 90 or more.

```text
when:     progress at least 90
actions:  pause Player.exe
cooldown: none
restore:  on release (resume when the number drops or the session stops)
```
