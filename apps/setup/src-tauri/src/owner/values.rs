//! Saved values and applications: listing them with their running windows,
//! saving and deleting them, and each application's anti-cheat setting.

use appytappy_config::app_file::{AntiCheatDef, AppFile, ScreenDef, SizeDef, ValueDef, value_id};
use appytappy_sources::screen::Image;

use crate::automations::{self, Stores};
use crate::check_spec::{self, CheckSpec};
use crate::window_list;

use super::Owner;
use super::host::Host;
use super::messages::{AppSummary, SaveSpec, SavedValue};

impl<H: Host> Owner<H> {
    /// The saved application `app`, from its file.
    pub(super) fn app_file(&self, app: &str) -> Result<AppFile, String> {
        self.stores()?
            .apps
            .load()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|stored| stored.id == app)
            .ok_or_else(|| "That application is no longer saved.".to_owned())?
            .file
    }

    pub(super) fn stores(&self) -> Result<&Stores, String> {
        self.stores.as_ref().map_err(Clone::clone)
    }

    pub(super) fn apps(&self) -> Result<Vec<AppSummary>, String> {
        let stored = self
            .stores()?
            .apps
            .load()
            .map_err(|error| error.to_string())?;
        // The saved files read fine without the running windows, so a
        // failed window listing only leaves each one's windows empty.
        let windows = self
            .window_list()
            .map(|list| list.windows)
            .unwrap_or_default();
        Ok(stored
            .into_iter()
            .map(|app| {
                let (file, error) = match app.file {
                    Ok(file) => (Some(file), None),
                    Err(error) => (None, Some(error)),
                };
                let matched = file
                    .as_ref()
                    .map(|file| {
                        window_list::matching(&windows, &file.window)
                            .into_iter()
                            .map(|fit| fit.handle)
                            .collect()
                    })
                    .unwrap_or_default();
                AppSummary {
                    id: app.id,
                    file,
                    error,
                    windows: matched,
                }
            })
            .collect())
    }

    pub(super) fn save(&self, spec: &SaveSpec) -> Result<SavedValue, String> {
        let store = &self.stores()?.apps;
        let name = spec.value_name.trim();
        if name.is_empty() {
            return Err("Name the value first.".to_owned());
        }
        // A picture check's region was drawn on the screenshot it keeps.
        let drawn_on = match &spec.check {
            CheckSpec::Reference { screenshot, .. } => *screenshot,
            _ => spec.screenshot,
        };
        let size = self
            .screenshots
            .get(drawn_on)
            .map(Image::size)
            .ok_or_else(|| "Take a screenshot first.".to_owned())?;
        let loaded = store.load().map_err(|error| error.to_string())?;
        let app = spec.app.clone();
        let mut file = loaded
            .into_iter()
            .find(|each| each.id == app)
            .ok_or_else(|| "That application is no longer saved.".to_owned())?
            .file?;
        let id = value_id(name);
        if file.values.iter().any(|value| value.id == id) {
            return Err(format!(
                "{} already has a value named like \"{name}\". Pick another name.",
                file.name
            ));
        }
        let saved =
            check_spec::to_saved(&spec.check, &self.screenshots, self.recording.as_ref(), &id)
                .map_err(|error| error.to_string())?;
        file.values.push(ValueDef {
            id: id.clone(),
            name: name.to_owned(),
            screen: ScreenDef {
                window_size: SizeDef {
                    width: size.width,
                    height: size.height,
                },
                region: saved.region,
                check: saved.check,
            },
        });
        store
            .save(&app, &file, &saved.pictures)
            .map_err(|error| error.to_string())?;
        Ok(SavedValue { app, value: id })
    }

    pub(super) fn delete(&self, app: &str, value: &str) -> Result<(), String> {
        let stores = self.stores()?;
        let store = &stores.apps;
        let mut file = store
            .load()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|stored| stored.id == app)
            .ok_or_else(|| "That application is no longer saved.".to_owned())?
            .file?;
        if let Some(saved) = file.values.iter().find(|saved| saved.id == value) {
            automations::check_unused(stores, app, value, &saved.name)?;
        }
        file.values.retain(|saved| saved.id != value);
        store
            .save(app, &file, &[])
            .and_then(|()| store.prune(app, &file))
            .map_err(|error| error.to_string())
    }

    /// Saves `class` as the anti-cheat class of the application `app`. A
    /// class other than none stops its automations that press keys, which
    /// the Automations page then shows as unable to run.
    pub(super) fn set_anti_cheat(&self, app: &str, class: AntiCheatDef) -> Result<(), String> {
        let store = &self.stores()?.apps;
        let mut file = store
            .load()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|stored| stored.id == app)
            .ok_or_else(|| "That application is no longer saved.".to_owned())?
            .file?;
        file.anti_cheat = class;
        store
            .save(app, &file, &[])
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::app_file::WindowDef;
    use appytappy_config::automation_file::{ActionDef, AutomationDef, MediaCommandDef};
    use appytappy_sources::screen::Size;

    use crate::owner::messages::Request;
    use crate::owner::test_support::{
        BAR, Harness, REFERENCE, add_example_app, frame_image, harness, harness_with,
        scratch_store, spec,
    };

    use super::*;

    #[test]
    fn saved_values_are_listed_with_their_running_windows_and_can_be_removed() {
        let (stores, root) = scratch_store("save");
        let apps_root = stores.apps.folder("");
        let mut harness = harness_with(false, Ok(stores));
        add_example_app(&mut harness);
        let mut handler = harness.pick();
        let save = |spec| {
            move |reply| Request::SaveValue {
                spec: Box::new(spec),
                reply,
            }
        };
        assert_eq!(
            harness.ask(save(spec("example", "Icon visible", REFERENCE))),
            Err("Take a screenshot first.".to_owned())
        );
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        // The window shrinks after the screenshot, and the value keeps the
        // size of the screenshot its region was drawn on.
        let shrunk = Image::new(
            Size {
                width: 2,
                height: 1,
            },
            vec![0; 8],
        )
        .expect("matching length");
        harness.deliver(&mut handler, Ok(&shrunk));
        assert_eq!(
            harness.ask(save(spec("example", "  ", REFERENCE))),
            Err("Name the value first.".to_owned())
        );
        let mut stale = spec("example", "Icon visible", REFERENCE);
        // A picture check's own screenshot wins over the one shown.
        stale.screenshot = 9;
        let saved = harness.ask(save(stale)).expect("saves a new app");
        assert_eq!(
            saved,
            SavedValue {
                app: "example".to_owned(),
                value: "icon_visible".to_owned()
            }
        );
        harness
            .ask(save(spec("example", "Progress bar", BAR)))
            .expect("adds to the app");
        assert_eq!(
            harness.ask(save(spec("example", "Progress BAR", BAR))),
            Err(
                "Example App already has a value named like \"Progress BAR\". Pick another name."
                    .to_owned()
            )
        );
        assert_eq!(
            harness.ask(save(spec("gone", "x", BAR))),
            Err("That application is no longer saved.".to_owned())
        );

        let apps = harness.ask(Request::ListApps).expect("lists");
        assert_eq!(apps.len(), 1);
        let app = &apps[0];
        assert_eq!((app.id.as_str(), &app.error), ("example", &None));
        assert_eq!(app.windows, [7]);
        let file = app.file.as_ref().expect("a good file");
        assert_eq!(file.name, "Example App");
        let ids: Vec<&str> = file.values.iter().map(|value| value.id.as_str()).collect();
        assert_eq!(ids, ["icon_visible", "progress_bar"]);
        assert_eq!(
            file.values[0].screen.window_size,
            SizeDef {
                width: 4,
                height: 2
            }
        );
        let folder = apps_root.join("example");
        assert!(folder.join("icon_visible.png").exists());

        let automation: AutomationDef = serde_json::from_value(serde_json::json!({
            "id": "",
            "name": "Pause music",
            "enabled": true,
            "when": {"app": "example", "value": "icon_visible", "is": "equals", "target": true},
            "actions": [
                {"kind": "media", "command": "pause", "app": " Player.exe "},
                {"kind": "volume", "percent": 20, "app": null}
            ],
            "undo": true,
            "cooldownMs": 1500
        }))
        .expect("the UI's spelling");
        let id = harness
            .ask(|reply| Request::SaveAutomation {
                existing: None,
                automation: Box::new(automation),
                reply,
            })
            .expect("saves");
        assert_eq!(id, "pause_music");
        assert_eq!(
            harness.ask(|reply| Request::DeleteValue {
                app: "example".to_owned(),
                value: "icon_visible".to_owned(),
                reply,
            }),
            Err("Pause music uses Icon visible. Change or delete it first.".to_owned())
        );
        harness
            .ask(|reply| Request::SetAutomationEnabled {
                id: id.clone(),
                enabled: false,
                reply,
            })
            .expect("turns off");
        let listed = harness.ask(Request::ListAutomations).expect("lists");
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].automation.enabled);
        assert_eq!(listed[0].automation.cooldown_ms, 1500);
        assert_eq!(
            listed[0].automation.actions[0],
            ActionDef::Media {
                command: MediaCommandDef::Pause,
                app: Some("Player.exe".to_owned()),
            }
        );
        harness
            .ask(|reply| Request::DeleteAutomation { id, reply })
            .expect("removes");
        assert_eq!(harness.ask(Request::ListAutomations), Ok(Vec::new()));

        // Keys are pressed only for an application whose anti-cheat is none.
        let press: AutomationDef = serde_json::from_value(serde_json::json!({
            "id": "",
            "name": "Press keys",
            "enabled": true,
            "when": {"app": "example", "value": "icon_visible", "is": "equals", "target": true},
            "actions": [{"kind": "keys", "keys": " shift + ctrl + m "}],
            "undo": false,
            "cooldownMs": 0
        }))
        .expect("the UI's spelling");
        let save_press = |harness: &mut Harness| {
            harness.ask(|reply| Request::SaveAutomation {
                existing: None,
                automation: Box::new(press.clone()),
                reply,
            })
        };
        assert!(
            save_press(&mut harness)
                .is_err_and(|error| error.contains("anti-cheat is set to none")),
            "an application whose anti-cheat is not set cannot press keys"
        );
        let set = |harness: &mut Harness, class| {
            harness.ask(|reply| Request::SetAntiCheat {
                app: "example".to_owned(),
                class,
                reply,
            })
        };
        set(&mut harness, AntiCheatDef::None).expect("sets");
        let apps = harness.ask(Request::ListApps).expect("lists");
        assert_eq!(
            apps[0].file.as_ref().map(|file| file.anti_cheat),
            Some(AntiCheatDef::None)
        );
        let id = save_press(&mut harness).expect("saves");
        let listed = harness.ask(Request::ListAutomations).expect("lists");
        assert_eq!(
            listed[0].automation.actions,
            [ActionDef::Keys {
                keys: "Ctrl+Shift+M".to_owned()
            }],
            "the file keeps one spelling"
        );
        assert_eq!(listed[0].problem, None);
        set(&mut harness, AntiCheatDef::KernelLevel).expect("sets");
        let listed = harness.ask(Request::ListAutomations).expect("lists");
        assert!(
            listed[0]
                .problem
                .as_ref()
                .is_some_and(|problem| problem.contains("anti-cheat is set to none")),
            "{listed:?}"
        );
        harness
            .ask(|reply| Request::DeleteAutomation { id, reply })
            .expect("removes");
        assert_eq!(
            set(&mut harness, AntiCheatDef::None).map(drop),
            Ok(()),
            "lowering it again is allowed too"
        );

        harness
            .ask(|reply| Request::DeleteValue {
                app: "example".to_owned(),
                value: "icon_visible".to_owned(),
                reply,
            })
            .expect("removes");
        assert!(!folder.join("icon_visible.png").exists());
        let apps = harness.ask(Request::ListApps).expect("lists");
        let file = apps[0].file.as_ref().expect("a good file");
        assert_eq!(file.values.len(), 1);
        // The saved values still list when the windows cannot be.
        harness.owner.host.unlisted = true;
        let apps = harness.ask(Request::ListApps).expect("lists");
        assert_eq!((apps.len(), apps[0].windows.len()), (1, 0));
        harness.owner.host.unlisted = false;
        assert_eq!(
            harness.ask(|reply| Request::DeleteValue {
                app: "gone".to_owned(),
                value: "x".to_owned(),
                reply,
            }),
            Err("That application is no longer saved.".to_owned())
        );
        assert_eq!(
            harness.ask(|reply| Request::SetAntiCheat {
                app: "gone".to_owned(),
                class: AntiCheatDef::None,
                reply,
            }),
            Err("That application is no longer saved.".to_owned())
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn without_a_store_saving_and_listing_say_why() {
        let mut harness = harness(false);
        assert_eq!(
            harness.ask(Request::ListApps),
            Err("no store in this test".to_owned())
        );
        assert_eq!(
            harness.ask(|reply| Request::SaveValue {
                spec: Box::new(spec("example", "x", BAR)),
                reply
            }),
            Err("no store in this test".to_owned())
        );
        assert_eq!(
            harness.ask(|reply| Request::DeleteValue {
                app: "a".to_owned(),
                value: "b".to_owned(),
                reply
            }),
            Err("no store in this test".to_owned())
        );
        assert_eq!(
            harness.ask(|reply| Request::SaveApp {
                existing: None,
                spec: crate::applications::AppSpec {
                    name: "Example App".to_owned(),
                    window: WindowDef::exact(appytappy_config::window_match::WindowFacts {
                        title: "Example App",
                        class: "ExampleWindow",
                        executable: "Example.exe",
                    }),
                },
                reply
            }),
            Err("no store in this test".to_owned())
        );
        assert_eq!(
            harness.ask(|reply| Request::RemoveApp {
                app: "a".to_owned(),
                reply
            }),
            Err("no store in this test".to_owned())
        );
        assert_eq!(
            harness.ask(Request::ListAutomations),
            Err("no store in this test".to_owned())
        );
        let automation: AutomationDef = serde_json::from_value(serde_json::json!({
            "id": "", "name": "x", "enabled": true,
            "when": {"app": "a", "value": "b", "is": "equals", "target": true},
            "actions": [{"kind": "media", "command": "pause"}], "undo": false
        }))
        .expect("the UI's spelling");
        assert_eq!(
            harness.ask(|reply| Request::SaveAutomation {
                existing: None,
                automation: Box::new(automation),
                reply
            }),
            Err("no store in this test".to_owned())
        );
        assert_eq!(
            harness.ask(|reply| Request::SetAutomationEnabled {
                id: "x".to_owned(),
                enabled: true,
                reply
            }),
            Err("no store in this test".to_owned())
        );
        assert_eq!(
            harness.ask(|reply| Request::DeleteAutomation {
                id: "x".to_owned(),
                reply
            }),
            Err("no store in this test".to_owned())
        );
    }
}
