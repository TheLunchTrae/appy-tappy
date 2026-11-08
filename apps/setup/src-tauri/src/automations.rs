//! Saving, listing, and removing automations.
//!
//! Automations live in their own file, apart from the applications' files,
//! and name a value by its application's ID and the value's ID. Saving one
//! checks it against the saved applications, so it never names a value that
//! is not there or compares a value with the wrong kind of target. A value
//! that later goes away shows as a problem on the automation that used it.

use appytappy_config::action::KeyChord;
use appytappy_config::app_file::AppFile;
use appytappy_config::automation_file::{
    ActionDef, AutomationDef, AutomationsFile, new_automation_id,
};
use appytappy_config::store::StoredApp;
use serde::Serialize;

use crate::stores::Stores;

/// A saved automation, as the Automations page lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationSummary {
    /// The automation.
    pub(crate) automation: AutomationDef,
    /// Why the runtime cannot run it as saved, such as a value that was
    /// removed since.
    pub(crate) problem: Option<String>,
}

/// `problem` as a sentence: a capital letter first and a full stop last.
fn sentence(problem: &str) -> String {
    let mut characters = problem.chars();
    let mut text: String = characters
        .next()
        .map(|first| first.to_uppercase().chain(characters).collect())
        .unwrap_or_default();
    if !text.ends_with('.') {
        text.push('.');
    }
    text
}

/// Loads the automations, saying what went wrong for the user.
fn load(stores: &Stores) -> Result<AutomationsFile, String> {
    stores
        .automations
        .load()
        .map_err(|error| sentence(&format!("AppyTappy cannot read your automations: {error}")))
}

/// Loads the applications, saying what went wrong for the user.
fn apps(stores: &Stores) -> Result<Vec<StoredApp>, String> {
    stores.apps.load().map_err(|error| error.to_string())
}

/// What stops `automation` from running with the applications in `apps`, if
/// anything.
fn problem(automation: &AutomationDef, apps: &[StoredApp]) -> Option<String> {
    if let Some(broken) = apps
        .iter()
        .find(|app| app.id == automation.when.app && app.file.is_err())
    {
        return Some(format!(
            "Its application's app.json, in the {} folder, cannot be read.",
            broken.id
        ));
    }
    automation
        .check_against(good(apps))
        .err()
        .map(|problem| sentence(&problem))
}

/// The applications whose files read fine, by ID.
fn good(apps: &[StoredApp]) -> impl Iterator<Item = (&str, &AppFile)> {
    apps.iter()
        .filter_map(|app| app.file.as_ref().ok().map(|file| (app.id.as_str(), file)))
}

/// Every automation, with what stops each one from running.
pub(crate) fn list(stores: &Stores) -> Result<Vec<AutomationSummary>, String> {
    let file = load(stores)?;
    let apps = apps(stores)?;
    Ok(file
        .automations
        .into_iter()
        .map(|automation| {
            let problem = problem(&automation, &apps);
            AutomationSummary {
                automation,
                problem,
            }
        })
        .collect())
}

/// Saves `draft` over the automation `existing`, keeping its ID, or as a new
/// automation with an ID made from its name. Answers the ID.
pub(crate) fn save(
    stores: &Stores,
    existing: Option<&str>,
    mut draft: AutomationDef,
) -> Result<String, String> {
    let name = draft.name.trim().to_owned();
    draft.name = name;
    if draft.name.is_empty() {
        return Err("Name the automation first.".to_owned());
    }
    for action in &mut draft.actions {
        match action {
            ActionDef::Media { app, .. } | ActionDef::Volume { app, .. } => {
                if let Some(name) = app {
                    *name = name.trim().to_owned();
                }
            },
            // The file keeps the keys in one spelling, such as Ctrl+Shift+M.
            ActionDef::Keys { keys } => {
                if let Ok(chord) = KeyChord::parse(keys) {
                    *keys = chord.to_string();
                }
            },
        }
    }
    draft.validate().map_err(|problem| sentence(&problem))?;
    let apps = apps(stores)?;
    if let Some(problem) = problem(&draft, &apps) {
        return Err(problem);
    }
    let mut file = load(stores)?;
    if file.automations.iter().any(|automation| {
        Some(automation.id.as_str()) != existing
            && automation.name.eq_ignore_ascii_case(&draft.name)
    }) {
        return Err(format!(
            "Another automation is already named \"{}\". Pick another name.",
            draft.name
        ));
    }
    let id = if let Some(id) = existing {
        let slot = file
            .automations
            .iter_mut()
            .find(|automation| automation.id == id)
            .ok_or_else(|| "That automation is no longer saved.".to_owned())?;
        id.clone_into(&mut draft.id);
        *slot = draft;
        id.to_owned()
    } else {
        let taken: Vec<&str> = file
            .automations
            .iter()
            .map(|automation| automation.id.as_str())
            .collect();
        draft.id = new_automation_id(&draft.name, &taken);
        let id = draft.id.clone();
        file.automations.push(draft);
        id
    };
    stores
        .automations
        .save(&file)
        .map_err(|error| error.to_string())?;
    Ok(id)
}

/// Turns the automation `id` on or off. One that cannot run as saved stays
/// off until it is fixed.
pub(crate) fn set_enabled(stores: &Stores, id: &str, enabled: bool) -> Result<(), String> {
    let mut file = load(stores)?;
    let automation = file
        .automations
        .iter_mut()
        .find(|automation| automation.id == id)
        .ok_or_else(|| "That automation is no longer saved.".to_owned())?;
    if enabled && let Some(problem) = problem(automation, &apps(stores)?) {
        return Err(format!(
            "{} cannot run until it is fixed. {problem}",
            automation.name
        ));
    }
    automation.enabled = enabled;
    stores
        .automations
        .save(&file)
        .map_err(|error| error.to_string())
}

/// Removes the automation `id`. One that is already gone is fine.
pub(crate) fn delete(stores: &Stores, id: &str) -> Result<(), String> {
    let mut file = load(stores)?;
    file.automations.retain(|automation| automation.id != id);
    stores
        .automations
        .save(&file)
        .map_err(|error| error.to_string())
}

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

/// Fails, naming the automations that watch any value of the application
/// `app`, unless none does. `name` is the application's name.
pub(crate) fn check_app_unused(stores: &Stores, app: &str, name: &str) -> Result<(), String> {
    let file = load(stores)?;
    unused(
        file.automations
            .iter()
            .filter(|automation| automation.when.app == app),
        &format!("a value of {name}"),
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::{env, fs, process};

    use appytappy_config::app_file::{
        AntiCheatDef, CheckDef, ColorDef, FORMAT, RegionDef, ScreenDef, SizeDef, ValueDef,
        WindowDef,
    };
    use appytappy_config::automation_file::{
        ActionDef, ComparisonDef, MediaCommandDef, TargetDef, WhenDef,
    };
    use appytappy_config::store::{AppStore, AutomationStore};
    use appytappy_config::window_match::WindowFacts;

    use super::*;

    /// Fresh stores for one test, holding the application `example` with a
    /// yes-or-no value `icon_visible` and a number value `health`.
    fn stores(name: &str) -> (Stores, PathBuf) {
        let folder =
            env::temp_dir().join(format!("appytappy-automations-{name}-{}", process::id()));
        if folder.exists() {
            fs::remove_dir_all(&folder).expect("remove leftovers");
        }
        let stores = Stores {
            apps: AppStore::new(folder.join("apps")),
            automations: AutomationStore::new(folder.join("automations.json")),
        };
        let white = ColorDef {
            red: 255,
            green: 255,
            blue: 255,
        };
        let value = |id: &str, name: &str, check| ValueDef {
            id: id.to_owned(),
            name: name.to_owned(),
            screen: ScreenDef {
                window_size: SizeDef {
                    width: 100,
                    height: 50,
                },
                region: RegionDef {
                    x: 0,
                    y: 0,
                    width: 10,
                    height: 10,
                },
                check,
            },
        };
        let app = AppFile {
            format: FORMAT,
            name: "Example App".to_owned(),
            window: WindowDef::exact(WindowFacts {
                title: "Example App",
                class: "ExampleWindow",
                executable: "Example.exe",
            }),
            anti_cheat: AntiCheatDef::Unknown,
            overlay: None,
            values: vec![
                value(
                    "icon_visible",
                    "Icon visible",
                    CheckDef::Color {
                        color: white,
                        tolerance: 10,
                    },
                ),
                value(
                    "health",
                    "Health",
                    CheckDef::BarFill {
                        filled: white,
                        empty: white,
                    },
                ),
            ],
        };
        stores.apps.save("example", &app, &[]).expect("saves");
        (stores, folder)
    }

    fn draft(name: &str) -> AutomationDef {
        AutomationDef {
            id: String::new(),
            name: name.to_owned(),
            enabled: true,
            when: WhenDef {
                app: "example".to_owned(),
                value: "icon_visible".to_owned(),
                is: ComparisonDef::Equals,
                target: TargetDef::Flag(true),
            },
            actions: vec![ActionDef::Media {
                command: MediaCommandDef::Pause,
                app: Some("Player.exe".to_owned()),
            }],
            undo: true,
            cooldown_ms: 0,
        }
    }

    fn ids(stores: &Stores) -> Vec<String> {
        list(stores)
            .expect("lists")
            .into_iter()
            .map(|summary| summary.automation.id)
            .collect()
    }

    #[test]
    fn problems_read_as_sentences() {
        assert_eq!(sentence("add an action"), "Add an action.");
        assert_eq!(sentence("Done."), "Done.");
        assert_eq!(sentence(""), ".");
    }

    #[test]
    fn saving_adds_new_automations_and_replaces_edited_ones() {
        let (stores, folder) = stores("save");
        assert_eq!(list(&stores), Ok(Vec::new()));
        assert_eq!(
            save(&stores, None, draft("  Pause music ")),
            Ok("pause_music".to_owned())
        );
        assert_eq!(
            save(&stores, None, draft("Pause music!")),
            Ok("pause_music_2".to_owned())
        );
        assert_eq!(
            save(&stores, None, draft("pause MUSIC")),
            Err(
                "Another automation is already named \"pause MUSIC\". Pick another name."
                    .to_owned()
            )
        );
        assert_eq!(ids(&stores), ["pause_music", "pause_music_2"]);

        let mut edited = draft("Pause music");
        edited.undo = false;
        edited.id = "ignored".to_owned();
        assert_eq!(
            save(&stores, Some("pause_music"), edited),
            Ok("pause_music".to_owned())
        );
        let listed = list(&stores).expect("lists");
        assert_eq!(listed[0].automation.name, "Pause music");
        assert!(!listed[0].automation.undo);
        assert_eq!(listed[0].problem, None);
        assert_eq!(
            save(&stores, Some("gone"), draft("Gone")),
            Err("That automation is no longer saved.".to_owned())
        );
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn saving_refuses_what_the_runtime_could_not_run() {
        let (stores, folder) = stores("refuse");
        assert_eq!(
            save(&stores, None, draft(" ")),
            Err("Name the automation first.".to_owned())
        );
        let mut no_actions = draft("Nothing");
        no_actions.actions.clear();
        assert_eq!(
            save(&stores, None, no_actions),
            Err("Add an action.".to_owned())
        );
        let mut wrong_kind = draft("Wrong kind");
        wrong_kind.when.value = "health".to_owned();
        assert_eq!(
            save(&stores, None, wrong_kind),
            Err("Health reads a number, so compare it with a number.".to_owned())
        );
        let mut elsewhere = draft("Elsewhere");
        elsewhere.when.app = "other".to_owned();
        assert_eq!(
            save(&stores, None, elsewhere),
            Err("Its application is no longer saved.".to_owned())
        );
        assert_eq!(ids(&stores), Vec::<String>::new());
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn listing_explains_automations_whose_value_went_away() {
        let (stores, folder) = stores("problems");
        save(&stores, None, draft("Pause music")).expect("saves");
        let mut app = stores.apps.load().expect("loads")[0]
            .file
            .clone()
            .expect("good");
        app.values.retain(|value| value.id != "icon_visible");
        stores.apps.save("example", &app, &[]).expect("saves");
        assert_eq!(
            list(&stores).expect("lists")[0].problem.as_deref(),
            Some("Example App no longer has the value it watches.")
        );
        set_enabled(&stores, "pause_music", false).expect("turns off");
        assert_eq!(
            set_enabled(&stores, "pause_music", true),
            Err(
                "Pause music cannot run until it is fixed. Example App no longer has the value \
                 it watches."
                    .to_owned()
            )
        );
        assert!(!list(&stores).expect("lists")[0].automation.enabled);
        fs::write(folder.join("apps").join("example").join("app.json"), b"{").expect("breaks");
        assert_eq!(
            list(&stores).expect("lists")[0].problem.as_deref(),
            Some("Its application's app.json, in the example folder, cannot be read.")
        );
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn automations_turn_off_and_go_away() {
        let (stores, folder) = stores("toggle");
        save(&stores, None, draft("Pause music")).expect("saves");
        save(&stores, None, draft("Lower volume")).expect("saves");
        set_enabled(&stores, "pause_music", false).expect("turns off");
        let listed = list(&stores).expect("lists");
        assert!(!listed[0].automation.enabled);
        assert!(listed[1].automation.enabled);
        assert_eq!(
            set_enabled(&stores, "gone", true),
            Err("That automation is no longer saved.".to_owned())
        );
        delete(&stores, "pause_music").expect("removes");
        delete(&stores, "pause_music").expect("already gone");
        assert_eq!(ids(&stores), ["lower_volume"]);
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn a_value_in_use_names_its_automations() {
        let (stores, folder) = stores("in-use");
        let unused =
            |stores: &Stores| check_unused(stores, "example", "icon_visible", "Icon visible");
        assert_eq!(unused(&stores), Ok(()));
        save(&stores, None, draft("Pause music")).expect("saves");
        assert_eq!(
            unused(&stores),
            Err("Pause music uses Icon visible. Change or delete it first.".to_owned())
        );
        save(&stores, None, draft("Lower volume")).expect("saves");
        save(&stores, None, draft("Skip ahead")).expect("saves");
        assert_eq!(
            unused(&stores),
            Err(
                "Pause music, Lower volume and Skip ahead use Icon visible. Change or delete them \
                 first."
                    .to_owned()
            )
        );
        assert_eq!(check_unused(&stores, "example", "health", "Health"), Ok(()));
        fs::write(folder.join("automations.json"), b"{").expect("breaks");
        assert!(
            unused(&stores)
                .is_err_and(|error| error.starts_with("AppyTappy cannot read your automations"))
        );
        assert!(list(&stores).is_err());
        assert!(save(&stores, None, draft("New")).is_err());
        fs::remove_dir_all(folder).expect("clean up");
    }
}
