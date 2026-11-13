//! Adding, changing, and removing applications.
//!
//! The Applications page is the one place an application is made, renamed,
//! told how to find its window and how often that window changes size, or
//! removed. A new application starts with no
//! values; the Values page adds them to an application saved here.

use appytappy_config::app_file::{AppFile, FORMAT, SizeChangesDef, WindowDef, WindowDefError};

use appytappy_config::anti_cheat::AntiCheatClass;
use appytappy_config::store::StoredApp;
use serde::Deserialize;

use crate::automations;
use crate::stores::Stores;

/// An application's name, how to find its window, and how often that window
/// changes size, as the editor sends them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AppSpec {
    /// The name the user sees, such as `Example App`.
    pub(crate) name: String,
    /// How to recognize the application's window.
    pub(crate) window: WindowDef,
    /// How often its window changes size, which decides whether its values
    /// scale to sizes they were not drawn at.
    pub(crate) size_changes: SizeChangesDef,
}

/// The file `spec` makes of the application `existing`, or of a new
/// application with no values, for the owner to save.
pub(crate) fn apply(
    stores: &Stores,
    existing: Option<&str>,
    spec: AppSpec,
) -> Result<AppFile, String> {
    let name = spec.name.trim();
    if name.is_empty() {
        return Err("Name the application first.".to_owned());
    }
    spec.window.check().map_err(|error| {
        match error {
            WindowDefError::NoExecutable => "Fill in the executable.",
            WindowDefError::BroadExecutable => {
                "Name more of the executable, such as Example.exe. This pattern would take \
                 nearly every program."
            },
            WindowDefError::NoTitle => "Fill in the window title, or untick its Must match.",
            WindowDefError::NoClass => "Fill in the window class, or untick its Must match.",
        }
        .to_owned()
    })?;
    let loaded = stores.apps.load().map_err(|error| error.to_string())?;
    check_name_free(&loaded, existing, name)?;
    let mut file = if let Some(id) = existing {
        loaded
            .into_iter()
            .find(|app| app.id == id)
            .ok_or_else(|| "That application is no longer saved.".to_owned())?
            .file?
    } else {
        AppFile {
            format: FORMAT,
            name: String::new(),
            window: spec.window.clone(),
            size_changes: spec.size_changes,
            anti_cheat: AntiCheatClass::Unknown,
            overlay: None,
            checked: None,
            values: Vec::new(),
            references: Vec::new(),
        }
    };
    name.clone_into(&mut file.name);
    // The build its values were checked against was of the old executable.
    if !file
        .window
        .executable
        .trim()
        .eq_ignore_ascii_case(spec.window.executable.trim())
    {
        file.checked = None;
    }
    file.window = spec.window;
    file.size_changes = spec.size_changes;
    Ok(file)
}

/// Fails when a saved application other than `own` is already called `name`,
/// whatever its capitals: two applications with one name could not be told
/// apart where a value's application is picked.
pub(crate) fn check_name_free(
    saved: &[StoredApp],
    own: Option<&str>,
    name: &str,
) -> Result<(), String> {
    match named(saved, own, name) {
        Some(other) => Err(format!(
            "Another application is already called {name}. Pick another name or edit {other}."
        )),
        None => Ok(()),
    }
}

/// The name of the saved application other than `own` that is called `name`,
/// if one is.
fn named<'saved>(saved: &'saved [StoredApp], own: Option<&str>, name: &str) -> Option<&'saved str> {
    saved
        .iter()
        .filter(|app| Some(app.id.as_str()) != own)
        .filter_map(|app| app.file.as_ref().ok())
        .map(|file| file.name.as_str())
        .find(|other| other.trim().eq_ignore_ascii_case(name))
}

/// `name`, or `name` with ` 2`, ` 3` and so on, whichever comes first that
/// no saved application is called.
pub(crate) fn free_name(saved: &[StoredApp], name: &str) -> String {
    let name = name.trim();
    if named(saved, None, name).is_none() {
        return name.to_owned();
    }
    // `saved` is finite, so one of its length plus one numbers is free.
    (2_usize..=saved.len().saturating_add(2))
        .map(|number| format!("{name} {number}"))
        .find(|candidate| named(saved, None, candidate).is_none())
        .unwrap_or_else(|| name.to_owned())
}

/// Checks that the application `app` is saved and that no automation
/// watches one of its values, so it can be removed. An application whose
/// file cannot be read can be removed too, so the user can clear it away.
pub(crate) fn check_removable(stores: &Stores, app: &str) -> Result<(), String> {
    let found = stores
        .apps
        .load()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|each| each.id == app)
        .ok_or_else(|| "That application is no longer saved.".to_owned())?;
    let name = found
        .file
        .map_or_else(|_broken| app.to_owned(), |file| file.name);
    automations::check_app_unused(stores, app, &name)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use appytappy_config::app_file::FallbackDef;

    use appytappy_config::action::MediaCommand;
    use appytappy_config::automation_file::{
        ActionDef, AutomationDef, AutomationsFile, TargetDef, WhenDef,
    };
    use appytappy_config::rule::Comparison;
    use appytappy_config::window_match::MustMatch;
    use appytappy_config::window_match::WindowFacts;

    use super::*;

    fn stores(name: &str) -> (Stores, PathBuf) {
        crate::stores::scratch(&format!("applications-{name}"))
    }

    /// Saves what [`apply`] makes of `spec`, as the owner does: over
    /// `existing`, or as a new application.
    fn save(stores: &Stores, existing: Option<&str>, spec: AppSpec) -> Result<String, String> {
        let file = apply(stores, existing, spec)?;
        match existing {
            Some(id) => stores.apps.save(id, &file, &[]).map(|()| id.to_owned()),
            None => stores.apps.create(&file, &[]),
        }
        .map_err(|error| error.to_string())
    }

    fn spec(name: &str, executable: &str) -> AppSpec {
        AppSpec {
            name: name.to_owned(),
            window: WindowDef::exact(WindowFacts {
                title: name,
                class: "ExampleWindow",
                executable,
            }),
            size_changes: SizeChangesDef::Often,
        }
    }

    #[test]
    fn a_new_application_is_saved_with_no_values_and_can_be_changed() {
        let (stores, folder) = stores("save");
        let id = save(&stores, None, spec(" Example App ", "Example.exe")).expect("saves");
        assert_eq!(id, "example");
        let mut changed = spec("Renamed", "Other.exe");
        changed.window.must_match = MustMatch {
            title: true,
            class: true,
        };
        changed.window.fallback = FallbackDef::Executable;
        changed.size_changes = SizeChangesDef::Rarely;
        assert_eq!(save(&stores, Some(&id), changed.clone()), Ok(id.clone()));
        let apps = stores.apps.load().expect("loads");
        assert_eq!(apps.len(), 1);
        let file = apps[0].file.as_ref().expect("readable");
        assert_eq!(file.name, "Renamed");
        assert_eq!(file.window, changed.window);
        assert_eq!(file.size_changes, SizeChangesDef::Rarely);
        assert_eq!(file.anti_cheat, AntiCheatClass::Unknown);
        assert_eq!(file.values, Vec::new());
        assert_eq!(
            save(&stores, None, spec("Second", "Example.exe")),
            Ok("example-2".to_owned())
        );
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn saving_says_what_to_fix() {
        let (stores, folder) = stores("refuse");
        save(&stores, None, spec("Example App", "Example.exe")).expect("saves");
        assert_eq!(
            save(&stores, None, spec("  ", "Example.exe")),
            Err("Name the application first.".to_owned())
        );
        assert_eq!(
            save(&stores, None, spec("Other", " ")),
            Err("Fill in the executable.".to_owned())
        );
        assert_eq!(
            save(&stores, None, spec("Other", "*.exe")),
            Err(
                "Name more of the executable, such as Example.exe. This pattern would take \
                 nearly every program."
                    .to_owned()
            )
        );
        let mut untitled = spec("Other", "Other.exe");
        untitled.window.title = String::new();
        assert_eq!(
            save(&stores, None, untitled.clone()),
            Ok("other".to_owned())
        );
        untitled.window.class = String::new();
        untitled.window.must_match.class = true;
        assert_eq!(
            save(&stores, Some("other"), untitled.clone()),
            Err("Fill in the window class, or untick its Must match.".to_owned())
        );
        untitled.window.must_match.title = true;
        assert_eq!(
            save(&stores, Some("other"), untitled),
            Err("Fill in the window title, or untick its Must match.".to_owned())
        );
        assert_eq!(
            save(&stores, None, spec("example app", "Other.exe")),
            Err(
                "Another application is already called example app. Pick another name or edit \
                 Example App."
                    .to_owned()
            )
        );
        assert_eq!(
            save(&stores, Some("example"), spec("Example App", "Example.exe")),
            Ok("example".to_owned())
        );
        assert_eq!(
            save(&stores, Some("gone"), spec("Gone", "Gone.exe")),
            Err("That application is no longer saved.".to_owned())
        );
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn removing_waits_until_no_automation_watches_the_application() {
        let (stores, folder) = stores("remove");
        let id = save(&stores, None, spec("Example App", "Example.exe")).expect("saves");
        let mut file = AutomationsFile::default();
        file.automations.push(AutomationDef {
            id: "pause_music".to_owned(),
            name: "Pause music".to_owned(),
            enabled: true,
            when: WhenDef {
                app: id.clone(),
                value: "progress".to_owned(),
                is: Comparison::AtLeast,
                target: TargetDef::Number(80.0),
            },
            actions: vec![ActionDef::Media {
                command: MediaCommand::Pause,
                app: None,
            }],
            undo: false,
            cooldown_ms: 0,
        });
        stores.automations.save(&file).expect("saves automations");
        assert_eq!(
            check_removable(&stores, &id),
            Err("Pause music uses a value of Example App. Change or delete it first.".to_owned())
        );
        stores
            .automations
            .save(&AutomationsFile::default())
            .expect("clears automations");
        assert_eq!(check_removable(&stores, &id), Ok(()));
        stores.apps.remove(&id).expect("removes the application");
        assert_eq!(
            check_removable(&stores, &id),
            Err("That application is no longer saved.".to_owned())
        );
        fs::create_dir_all(stores.apps.folder("broken")).expect("broken folder");
        assert_eq!(check_removable(&stores, "broken"), Ok(()));
        fs::remove_dir_all(folder).expect("clean up");
    }
}
