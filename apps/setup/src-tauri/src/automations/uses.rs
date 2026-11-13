//! Which automations use a value or an application, so one in use is not
//! deleted or changed from under them.

use appytappy_config::app_file::{ValueDef, ValueKind};
use appytappy_config::automation_file::{ActionDef, AutomationDef, TargetDef};

use crate::stores::Stores;

use super::load;

/// Fails, naming the automations that watch the value `value` of the
/// application `app`, unless none does. `name` is the value's name.
pub(crate) fn check_unused(
    stores: &Stores,
    app: &str,
    value: &str,
    name: &str,
) -> Result<(), String> {
    let file = load(stores)?;
    unused(file.watching(app, value), name)
}

/// Fails, naming the automations that watch the value `saved` of the
/// application `app` and whose condition suits it but would not suit it as
/// `edited`: one that compares it with another kind of target than the edit
/// makes it read, or with a state the edit renames or removes. An automation
/// whose condition does not suit `saved` either is already listed with its
/// problem, and the edit is not what breaks it.
pub(crate) fn check_still_suits(
    stores: &Stores,
    app: &str,
    saved: &ValueDef,
    edited: &ValueDef,
) -> Result<(), String> {
    let file = load(stores)?;
    let (was, now) = (saved.screen.check.reads(), edited.screen.check.reads());
    // What each broken automation uses that the edit takes away, with the
    // automations that use the same thing together.
    let mut broken: Vec<(String, Vec<&AutomationDef>)> = Vec::new();
    for automation in file.watching(app, &saved.id) {
        let target = &automation.when.target;
        if target.suits(saved).is_err() || target.suits(edited).is_ok() {
            continue;
        }
        let what = match target {
            TargetDef::State(state) if was == now => format!(
                "the state {state:?} of {}, which this change renames or removes",
                saved.name
            ),
            TargetDef::Number(_) | TargetDef::Flag(_) | TargetDef::State(_) => format!(
                "{} as {}, and this change makes it read {}",
                saved.name,
                kind_text(was),
                kind_text(now)
            ),
        };
        match broken.iter_mut().find(|(used, _)| *used == what) {
            Some((_, users)) => users.push(automation),
            None => broken.push((what, vec![automation])),
        }
    }
    let problems: Vec<String> = broken
        .into_iter()
        .filter_map(|(what, users)| unused(users.into_iter(), &what).err())
        .collect();
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join(" "))
    }
}

/// What a value of `kind` reads, in a sentence.
const fn kind_text(kind: ValueKind) -> &'static str {
    match kind {
        ValueKind::Number => "a number",
        ValueKind::Flag => "yes or no",
        ValueKind::State => "a state",
    }
}

/// Fails, naming the automations that watch any value of the application
/// `app` or click in it, unless none does. `name` is the application's name.
pub(crate) fn check_app_unused(stores: &Stores, app: &str, name: &str) -> Result<(), String> {
    let file = load(stores)?;
    unused(
        file.automations
            .iter()
            .filter(|automation| automation.when.app == app),
        &format!("a value of {name}"),
    )?;
    unused(
        file.automations.iter().filter(|automation| {
            automation.actions.iter().any(
                |action| matches!(action, ActionDef::Click { app: target, .. } if target == app),
            )
        }),
        &format!("{name} as the application it clicks in"),
    )
}

/// Fails, naming `users`, unless there are none. `what` is what they use.
fn unused<'file>(
    users: impl Iterator<Item = &'file AutomationDef>,
    what: &str,
) -> Result<(), String> {
    let users: Vec<&str> = users.map(|automation| automation.name.as_str()).collect();
    let named = match users.as_slice() {
        [] => return Ok(()),
        [only] => format!("{only} uses"),
        [first @ .., last] => format!("{} and {last} use", first.join(", ")),
    };
    Err(format!(
        "{named} {what}. Change or delete {} first.",
        if users.len() == 1 { "it" } else { "them" }
    ))
}
