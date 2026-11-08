//! Adding, changing, and removing applications.
//!
//! The Applications page is the one place an application is made, renamed,
//! told how to find its window, or removed. A new application starts with no
//! values; the Values page adds them to an application saved here.

use appytappy_config::app_file::{AntiCheatDef, AppFile, FORMAT, WindowDef};
use appytappy_config::store::new_app_id;
use appytappy_config::window_match::Pattern;
use serde::Deserialize;

use crate::automations;
use crate::stores::Stores;

/// An application's name and how to find its window, as the editor sends
/// them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AppSpec {
    /// The name the user sees, such as `Example App`.
    pub(crate) name: String,
    /// How to recognize the application's window.
    pub(crate) window: WindowDef,
}

/// Saves `spec` over the application `existing`, or as a new application
/// with no values, and returns its ID.
pub(crate) fn save(
    stores: &Stores,
    existing: Option<&str>,
    spec: AppSpec,
) -> Result<String, String> {
    let name = spec.name.trim();
    if name.is_empty() {
        return Err("Name the application first.".to_owned());
    }
    let window = &spec.window;
    if window.executable.trim().is_empty() {
        return Err("Fill in the executable.".to_owned());
    }
    // A pattern with less than two characters of its own beside any `.exe`,
    // such as `*` or `*.*`, or one that takes two unrelated programs, such as
    // `*exe`, matches nearly every program, so the application would always
    // look running.
    let executable = window.executable.trim().to_ascii_lowercase();
    let named = executable
        .strip_suffix(".exe")
        .unwrap_or(&executable)
        .chars()
        .filter(|each| !matches!(each, '*' | '?' | '.'))
        .count();
    let pattern = Pattern::new(&executable);
    if named < 2 || (pattern.matches("a.exe") && pattern.matches("zq.exe")) {
        return Err(
            "Name more of the executable, such as Example.exe. This pattern would take nearly \
             every program."
                .to_owned(),
        );
    }
    for (required, pattern, field) in [
        (window.must_match.title, &window.title, "window title"),
        (window.must_match.class, &window.class, "window class"),
    ] {
        if required && pattern.trim().is_empty() {
            return Err(format!("Fill in the {field}, or untick its Must match."));
        }
    }
    let loaded = stores.apps.load().map_err(|error| error.to_string())?;
    if let Some(other) = loaded.iter().find(|app| {
        Some(app.id.as_str()) != existing
            && app
                .file
                .as_ref()
                .is_ok_and(|file| file.name.trim().eq_ignore_ascii_case(name))
    }) {
        return Err(format!(
            "Another application is already called {name}. Pick another name or edit {}.",
            other.file.as_ref().map_or(name, |file| file.name.as_str())
        ));
    }
    let (id, mut file) = if let Some(id) = existing {
        let file = loaded
            .into_iter()
            .find(|app| app.id == id)
            .ok_or_else(|| "That application is no longer saved.".to_owned())?
            .file?;
        (id.to_owned(), file)
    } else {
        let taken: Vec<&str> = loaded.iter().map(|app| app.id.as_str()).collect();
        let file = AppFile {
            format: FORMAT,
            name: String::new(),
            window: spec.window.clone(),
            anti_cheat: AntiCheatDef::Unknown,
            overlay: None,
            checked: None,
            values: Vec::new(),
            references: Vec::new(),
        };
        (new_app_id(&spec.window.executable, &taken), file)
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
    stores
        .apps
        .save(&id, &file, &[])
        .map_err(|error| error.to_string())?;
    Ok(id)
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
    use std::path::PathBuf;
    use std::{env, fs, process};

    use appytappy_config::app_file::{FallbackDef, MustMatchDef};
    use appytappy_config::automation_file::{
        ActionDef, AutomationDef, AutomationsFile, ComparisonDef, MediaCommandDef, TargetDef,
        WhenDef,
    };
    use appytappy_config::store::{AppStore, AutomationStore, SettingsStore};
    use appytappy_config::window_match::WindowFacts;

    use super::*;

    fn stores(name: &str) -> (Stores, PathBuf) {
        let folder =
            env::temp_dir().join(format!("appytappy-applications-{name}-{}", process::id()));
        if folder.exists() {
            fs::remove_dir_all(&folder).expect("remove leftovers");
        }
        let stores = Stores {
            apps: AppStore::new(folder.join("apps")),
            automations: AutomationStore::new(folder.join("automations.json")),
            settings: SettingsStore::new(folder.join("settings.json")),
        };
        (stores, folder)
    }

    fn spec(name: &str, executable: &str) -> AppSpec {
        AppSpec {
            name: name.to_owned(),
            window: WindowDef::exact(WindowFacts {
                title: name,
                class: "ExampleWindow",
                executable,
            }),
        }
    }

    #[test]
    fn a_new_application_is_saved_with_no_values_and_can_be_changed() {
        let (stores, folder) = stores("save");
        let id = save(&stores, None, spec(" Example App ", "Example.exe")).expect("saves");
        assert_eq!(id, "example");
        let mut changed = spec("Renamed", "Other.exe");
        changed.window.must_match = MustMatchDef {
            title: true,
            class: true,
        };
        changed.window.fallback = FallbackDef::Executable;
        assert_eq!(save(&stores, Some(&id), changed.clone()), Ok(id.clone()));
        let apps = stores.apps.load().expect("loads");
        assert_eq!(apps.len(), 1);
        let file = apps[0].file.as_ref().expect("readable");
        assert_eq!(file.name, "Renamed");
        assert_eq!(file.window, changed.window);
        assert_eq!(file.anti_cheat, AntiCheatDef::Unknown);
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
        for every in [
            "*", " *.EXE ", "?*", ".exe", "*.*", "*.e*", "*e", "*exe", "*ex*", "*e*e",
        ] {
            assert_eq!(
                save(&stores, None, spec("Other", every)),
                Err(
                    "Name more of the executable, such as Example.exe. This pattern would take \
                     nearly every program."
                        .to_owned()
                ),
                "{every}"
            );
        }
        // Two characters of its own are enough when the pattern takes only
        // one of two unrelated programs.
        assert_eq!(
            save(&stores, None, spec("Short", "zq*")),
            Ok("zq".to_owned())
        );
        let mut untitled = spec("Other", "Other.exe");
        untitled.window.title = String::new();
        assert_eq!(
            save(&stores, None, untitled.clone()),
            Ok("other".to_owned())
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
                is: ComparisonDef::AtLeast,
                target: TargetDef::Number(80.0),
            },
            actions: vec![ActionDef::Media {
                command: MediaCommandDef::Pause,
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
