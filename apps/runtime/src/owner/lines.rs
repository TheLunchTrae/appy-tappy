//! The owner's log lines: what it writes for a change detection found, an
//! order it carries out, a read of the saved files, and a new read interval.
//! They change with the log, not with the owner's loop.

use appytappy_config::app_file::BuildDef;
use appytappy_config::settings_file::ReadInterval;

use crate::lifecycle::Change;
use crate::model::{Model, Order};
use crate::plan;

/// The log line for `change`, if it needs one.
pub(super) fn describe(model: &Model, change: &Change) -> Option<String> {
    let line = match change {
        Change::Started {
            app,
            process,
            executable,
        } => format!("{app} started as {executable} (process {process}): attached"),
        Change::Exited { app, process } => format!("{app} exited (process {process}): detached"),
        Change::Identified { app, build } => {
            let plan = model.plan(app)?;
            return build_change(&plan.labels.name, plan.labels.checked.as_ref(), build);
        },
        Change::ScanFailed(problem) => format!("process snapshots fail: {problem}"),
        Change::ScanRestored => "process snapshots work again".to_owned(),
    };
    Some(line)
}

/// The log line for an app named `name` that runs `running` while its
/// values were checked against `checked`, or `None` when that needs no
/// line: the builds match, or no build was checked.
fn build_change(
    name: &str,
    checked: Option<&BuildDef>,
    running: &Result<BuildDef, String>,
) -> Option<String> {
    let checked = checked?;
    match running {
        Ok(running) if running == checked => None,
        Ok(running) => Some(format!(
            "{name}: its executable changed since its values were checked ({}). Its values are \
             still read; check them in AppyTappy Setup",
            build_difference(checked, running)
        )),
        Err(error) => Some(format!(
            "{name}: which build of its executable runs cannot be read, so a change since its \
             values were checked goes unnoticed: {error}"
        )),
    }
}

/// How `running` differs from the `checked` build, in a few words: both
/// versions when they differ, and otherwise that the file does.
fn build_difference(checked: &BuildDef, running: &BuildDef) -> String {
    let version = |build: &BuildDef| build.version.as_deref().unwrap_or("no version").to_owned();
    match (&checked.version, &running.version) {
        (then, now) if then != now => {
            format!("{} then, {} now", version(checked), version(running))
        },
        (Some(both), _) => format!("still {both}, but another file"),
        (None, _) => "another file, with no version either time".to_owned(),
    }
}

/// The log line that says what the runtime runs.
pub(super) fn loaded_line(plan: &plan::Plan) -> String {
    let automations: usize = plan.apps.values().map(|app| app.runs.rules.len()).sum();
    let names: Vec<&str> = plan
        .apps
        .values()
        .map(|app| app.labels.name.as_str())
        .collect();
    match automations {
        0 => "no saved automation can run".to_owned(),
        1 => format!("1 automation runs for {}", names.join(", ")),
        _ => format!("{automations} automations run for {}", names.join(", ")),
    }
}

/// The log line for a new read interval.
pub(super) fn interval_line(interval: ReadInterval) -> String {
    format!(
        "values are read every {} to {} ms",
        interval.min().as_millis(),
        interval.max().as_millis()
    )
}

/// The log line for an order, if it gets one.
pub(super) fn order_line(order: &Order) -> Option<String> {
    match order {
        Order::Read { plan, .. } => Some(format!("{}: reading its values", plan.labels.name)),
        Order::StopReading(..) | Order::Held(..) => None,
        Order::Run { automation, .. } => Some(format!("{automation}: runs")),
        Order::Undo { automation, .. } => Some(format!("{automation}: undone")),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use appytappy_config::process::ProcessId;

    use super::*;
    use crate::model::{ReaderNumber, SessionId};
    use crate::owner::tests::app_plan;

    #[test]
    fn log_lines_name_the_app_and_process() {
        let process = ProcessId::new(42);
        let lines = [
            Change::Started {
                app: "example".to_owned(),
                process,
                executable: "Example.exe".to_owned(),
            },
            Change::Exited {
                app: "example".to_owned(),
                process,
            },
            Change::ScanFailed("denied".to_owned()),
            Change::ScanRestored,
        ]
        .map(|change| describe(&Model::default(), &change));
        assert_eq!(
            lines,
            [
                "example started as Example.exe (process 42): attached",
                "example exited (process 42): detached",
                "process snapshots fail: denied",
                "process snapshots work again",
            ]
            .map(|line| Some(line.to_owned()))
        );
    }

    #[test]
    fn a_build_difference_names_both_versions_or_the_file() {
        let build = |version: Option<&str>, size| BuildDef {
            version: version.map(str::to_owned),
            size,
            modified: 1_762_588_800,
        };
        assert_eq!(
            build_difference(&build(Some("1.4.2.0"), 1), &build(Some("1.5.0.0"), 1)),
            "1.4.2.0 then, 1.5.0.0 now"
        );
        assert_eq!(
            build_difference(&build(None, 1), &build(Some("1.5.0.0"), 1)),
            "no version then, 1.5.0.0 now"
        );
        assert_eq!(
            build_difference(&build(Some("1.4.2.0"), 1), &build(Some("1.4.2.0"), 2)),
            "still 1.4.2.0, but another file"
        );
        assert_eq!(
            build_difference(&build(None, 1), &build(None, 2)),
            "another file, with no version either time"
        );
    }

    #[test]
    fn a_build_other_than_the_checked_one_gets_a_log_line() {
        let build = |version: Option<&str>, size| BuildDef {
            version: version.map(str::to_owned),
            size,
            modified: 1_762_588_800,
        };
        let checked = build(Some("1.4.2.0"), 1_024);
        let mut model = Model::default();
        let mut plan = app_plan("Example App", &["Pause music"]);
        Arc::get_mut(&mut plan).expect("unshared").labels.checked = Some(checked.clone());
        model.replan(BTreeMap::from([
            ("example".to_owned(), plan),
            ("unchecked".to_owned(), app_plan("Unchecked", &["Mute"])),
        ]));
        let line = |app: &str, running: Result<BuildDef, String>| {
            describe(
                &model,
                &Change::Identified {
                    app: app.to_owned(),
                    build: running,
                },
            )
        };
        assert_eq!(line("example", Ok(checked.clone())), None);
        assert_eq!(
            line("example", Ok(build(Some("1.5.0.0"), 1_024))).as_deref(),
            Some(
                "Example App: its executable changed since its values were checked (1.4.2.0 \
                 then, 1.5.0.0 now). Its values are still read; check them in AppyTappy Setup"
            )
        );
        // The same version in a file of another size is another build.
        assert_eq!(
            line("example", Ok(build(Some("1.4.2.0"), 2_048))).as_deref(),
            Some(
                "Example App: its executable changed since its values were checked (still \
                 1.4.2.0, but another file). Its values are still read; check them in \
                 AppyTappy Setup"
            )
        );
        assert_eq!(
            line("example", Ok(build(None, 2_048))).as_deref(),
            Some(
                "Example App: its executable changed since its values were checked (1.4.2.0 \
                 then, no version now). Its values are still read; check them in AppyTappy Setup"
            )
        );
        assert_eq!(
            line("example", Err("access denied".to_owned())).as_deref(),
            Some(
                "Example App: which build of its executable runs cannot be read, so a change \
                 since its values were checked goes unnoticed: access denied"
            )
        );
        assert_eq!(line("unchecked", Ok(checked.clone())), None);
        assert_eq!(line("gone", Ok(checked)), None);
    }

    #[test]
    fn the_log_counts_the_automations_and_names_their_apps() {
        let mut plan = plan::Plan::default();
        plan.apps
            .insert("a".to_owned(), app_plan("Example App", &["Mute"]));
        assert_eq!(loaded_line(&plan), "1 automation runs for Example App");
        plan.apps
            .insert("b".to_owned(), app_plan("Other App", &["Pause", "Play"]));
        assert_eq!(
            loaded_line(&plan),
            "3 automations run for Example App, Other App"
        );
    }

    #[test]
    fn orders_that_change_something_get_a_log_line() {
        let plan = app_plan("Example App", &[]);
        let owner = (SessionId(1), appytappy_engine::RuleIndex(0));
        let watched = appytappy_actions::Watched {
            process: ProcessId::new(1),
            executable: "Example.exe".to_owned(),
        };
        let orders = [
            Order::Read {
                session: SessionId(1),
                reader: ReaderNumber(0),
                plan,
                watched: watched.clone(),
                interval: ReadInterval::DEFAULT,
            },
            Order::StopReading(SessionId(1), "Example App".to_owned()),
            Order::Run {
                owner,
                automation: "Mute".to_owned(),
                watched,
                actions: Box::new([]),
                clicks: Arc::default(),
                keeps: appytappy_actions::Keeps::ForRelease,
                cause: None,
            },
            Order::Undo {
                owner,
                automation: "Mute".to_owned(),
                cause: None,
            },
            Order::Held(owner, None),
        ];
        assert_eq!(
            orders.each_ref().map(order_line),
            [
                Some("Example App: reading its values".to_owned()),
                None,
                Some("Mute: runs".to_owned()),
                Some("Mute: undone".to_owned()),
                None,
            ]
        );
    }
}
