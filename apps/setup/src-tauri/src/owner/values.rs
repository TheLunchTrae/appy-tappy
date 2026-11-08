//! Saved values and applications: listing them with their running windows
//! and builds, saving and deleting them, each application's anti-cheat
//! setting, and the build its values were checked against.

use std::time::{SystemTime, UNIX_EPOCH};

use appytappy_config::app_file::{
    AntiCheatDef, AppFile, BuildDef, CheckedDef, ScreenDef, SizeDef, ValueDef, value_id,
};

use crate::automations;
use crate::check_spec;
use crate::stores::Stores;
use crate::window_list::{self, WindowChoice};

use super::Owner;
use super::host::Host;
use super::messages::{AppSummary, SaveSpec, SavedValue};

impl<H: Host> Owner<H> {
    pub(super) fn stores(&self) -> Result<&Stores, String> {
        self.stores.as_ref().map_err(Clone::clone)
    }

    pub(super) fn apps(&mut self) -> Result<Vec<AppSummary>, String> {
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
        // A process that has gone may give its ID to another build.
        self.builds
            .retain(|process, _| windows.iter().any(|window| window.process == *process));
        Ok(stored
            .into_iter()
            .map(|app| {
                let (file, error) = match app.file {
                    Ok(file) => (Some(file), None),
                    Err(error) => (None, Some(error)),
                };
                let matched: Vec<u64> = file
                    .as_ref()
                    .map(|file| {
                        window_list::matching(&windows, &file.window)
                            .into_iter()
                            .map(|fit| fit.handle)
                            .collect()
                    })
                    .unwrap_or_default();
                // Only an application with values shows its build, and one
                // that cannot be read only leaves the warning out.
                let running = file
                    .as_ref()
                    .filter(|file| !file.values.is_empty())
                    .and_then(|_| self.build_of(&windows, &matched).ok().flatten());
                AppSummary {
                    id: app.id,
                    file,
                    error,
                    windows: matched,
                    running,
                }
            })
            .collect())
    }

    pub(super) fn save(&mut self, spec: &SaveSpec) -> Result<SavedValue, String> {
        let stores = self.stores()?;
        let name = spec.value_name.trim();
        if name.is_empty() {
            return Err("Name the value first.".to_owned());
        }
        if self.screenshots.get(spec.screenshot).is_none() {
            return Err("Take a screenshot first.".to_owned());
        }
        // The value keeps the window size its region was drawn at, which
        // any screenshot it cuts at the region must have too.
        let size = self
            .with_session(|session| check_spec::drawn_size(&spec.check, session, spec.screenshot))
            .map_err(|error| error.to_string())?;
        let app = spec.app.clone();
        let mut file = stores.app_file(&app)?;
        let id = value_id(name);
        if file.values.iter().any(|value| value.id == id) {
            return Err(format!(
                "{} already has a value named like \"{name}\". Pick another name.",
                file.name
            ));
        }
        let saved = self
            .with_session(|session| check_spec::to_saved(&spec.check, session, &spec.app, &id))
            .map_err(|error| error.to_string())?;
        let first = file.values.is_empty();
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
        // The first value is checked against the build that runs now. Later
        // ones leave that to Mark as checked, since one new value says
        // nothing about the others.
        if first {
            file.checked = self.running_build(&file).ok().flatten().map(checked_now);
        }
        self.stores()?
            .apps
            .save(&app, &file, &saved.pictures)
            .map_err(|error| error.to_string())?;
        Ok(SavedValue { app, value: id })
    }

    /// The build that the best fit for `file`'s window runs, or `None`
    /// while no window fits.
    fn running_build(&mut self, file: &AppFile) -> Result<Option<BuildDef>, String> {
        let windows = self.window_list()?.windows;
        let matched: Vec<u64> = window_list::matching(&windows, &file.window)
            .into_iter()
            .map(|fit| fit.handle)
            .collect();
        self.build_of(&windows, &matched)
    }

    /// The build of the first of `matched`, which name windows of `windows`.
    /// A build read once is kept for its process, and a failed read is tried
    /// again next time.
    fn build_of(
        &mut self,
        windows: &[WindowChoice],
        matched: &[u64],
    ) -> Result<Option<BuildDef>, String> {
        let Some(process) = matched
            .first()
            .and_then(|&handle| windows.iter().find(|window| window.handle == handle))
            .map(|window| window.process)
        else {
            return Ok(None);
        };
        if let Some(build) = self.builds.get(&process) {
            return Ok(Some(build.clone()));
        }
        let build = self
            .host
            .executable_build(process)
            .map_err(|error| error.to_string())?;
        self.builds.insert(process, build.clone());
        Ok(Some(build))
    }

    /// Saves the build the application `app` runs now as the one its values
    /// were checked against.
    pub(super) fn mark_checked(&mut self, app: &str) -> Result<(), String> {
        let mut file = self.stores()?.app_file(app)?;
        let build = self
            .running_build(&file)
            .map_err(|error| {
                format!(
                    "AppyTappy Setup cannot tell which build of {} runs: {error}",
                    file.name
                )
            })?
            .ok_or_else(|| {
                format!(
                    "Start {} first, so AppyTappy Setup can tell which build runs.",
                    file.name
                )
            })?;
        file.checked = Some(checked_now(build));
        self.stores()?
            .apps
            .save(app, &file, &[])
            .map_err(|error| error.to_string())
    }

    pub(super) fn delete(&self, app: &str, value: &str) -> Result<(), String> {
        let stores = self.stores()?;
        let store = &stores.apps;
        let mut file = stores.app_file(app)?;
        if let Some(saved) = file.values.iter().find(|saved| saved.id == value) {
            automations::check_unused(stores, app, value, &saved.name)?;
        }
        file.values.retain(|saved| saved.id != value);
        // Values made again later are checked against the build they are
        // made on.
        if file.values.is_empty() {
            file.checked = None;
        }
        store
            .save(app, &file, &[])
            .and_then(|()| store.prune(app, &file))
            .map_err(|error| error.to_string())
    }

    /// Saves `class` as the anti-cheat class of the application `app`. A
    /// class other than none stops its automations that press keys, which
    /// the Automations page then shows as unable to run.
    pub(super) fn set_anti_cheat(&self, app: &str, class: AntiCheatDef) -> Result<(), String> {
        let stores = self.stores()?;
        let mut file = stores.app_file(app)?;
        file.anti_cheat = class;
        stores
            .apps
            .save(app, &file, &[])
            .map_err(|error| error.to_string())
    }
}

/// `build` as checked now.
fn checked_now(build: BuildDef) -> CheckedDef {
    // A clock set before 1970 counts as its start.
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    CheckedDef { build, at }
}

#[cfg(test)]
mod tests {
    use appytappy_config::app_file::WindowDef;
    use appytappy_config::automation_file::{ActionDef, AutomationDef, MediaCommandDef};
    use appytappy_sources::screen::{Image, Size};

    use crate::owner::messages::Request;
    use crate::owner::test_support::{
        BAR, Harness, STATE, add_example_app, example_build, frame_image, harness, harness_with,
        scratch_store, spec, with_saved_bar,
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
            harness.ask(save(spec("example", "Icon visible", STATE))),
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
        assert_eq!(harness.ask(Request::Screenshot), Ok(1));
        assert_eq!(
            harness.ask(save(spec("example", "  ", STATE))),
            Err("Name the value first.".to_owned())
        );
        let mut elsewhere = spec("example", "Icon visible", STATE);
        elsewhere.screenshot = 1;
        assert_eq!(
            harness.ask(save(elsewhere)),
            Err(
                "Screenshot 1, which a state is taken from, is 4x2, but the region was drawn on \
                 screenshot 2, which is 2x1. Take that state from a screenshot of that size."
                    .to_owned()
            ),
            "a value is saved at one window size"
        );
        let saved = harness
            .ask(save(spec("example", "Icon visible", STATE)))
            .expect("saves a new app");
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
        assert!(folder.join("icon_visible-1.png").exists());

        let automation: AutomationDef = serde_json::from_value(serde_json::json!({
            "id": "",
            "name": "Pause music",
            "enabled": true,
            "when": {"app": "example", "value": "icon_visible", "is": "equals", "target": "Shown"},
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
            "when": {"app": "example", "value": "icon_visible", "is": "equals", "target": "Shown"},
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

        // Opening and playing a sound are saved without the spaces around
        // what they name.
        let open_and_play: AutomationDef = serde_json::from_value(serde_json::json!({
            "id": "",
            "name": "Open and play",
            "enabled": true,
            "when": {"app": "example", "value": "icon_visible", "is": "equals", "target": "Shown"},
            "actions": [
                {"kind": "open", "target": " https://example.com "},
                {"kind": "sound", "file": " C:\\Ding.wav "}
            ],
            "undo": false,
            "cooldownMs": 0
        }))
        .expect("the UI's spelling");
        let id = harness
            .ask(|reply| Request::SaveAutomation {
                existing: None,
                automation: Box::new(open_and_play),
                reply,
            })
            .expect("saves");
        let listed = harness.ask(Request::ListAutomations).expect("lists");
        assert_eq!(
            listed[0].automation.actions,
            [
                ActionDef::Open {
                    target: "https://example.com".to_owned()
                },
                ActionDef::Sound {
                    file: r"C:\Ding.wav".to_owned()
                }
            ]
        );
        harness
            .ask(|reply| Request::DeleteAutomation { id, reply })
            .expect("removes");

        harness
            .ask(|reply| Request::DeleteValue {
                app: "example".to_owned(),
                value: "icon_visible".to_owned(),
                reply,
            })
            .expect("removes");
        assert!(!folder.join("icon_visible-1.png").exists());
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
    fn the_first_value_records_the_running_build_until_marked_checked_again() {
        let (mut harness, root) = with_saved_bar("checked");
        let checked = |harness: &mut Harness| {
            harness.ask(Request::ListApps).expect("lists")[0]
                .file
                .as_ref()
                .and_then(|file| file.checked.clone())
        };
        let first = checked(&mut harness).expect("the first value records the build");
        assert_eq!(first.build, example_build());
        assert!(first.at > 0);
        // The application updates and starts again.
        let updated = BuildDef {
            version: Some("1.5.0.0".to_owned()),
            ..example_build()
        };
        harness.owner.host.process = 4;
        harness.owner.host.build = Some(updated.clone());
        let apps = harness.ask(Request::ListApps).expect("lists");
        assert_eq!(apps[0].running.as_ref(), Some(&updated));
        harness
            .ask(|reply| Request::SaveValue {
                spec: Box::new(spec("example", "Icon visible", STATE)),
                reply,
            })
            .expect("saves");
        assert_eq!(
            checked(&mut harness).map(|checked| checked.build),
            Some(example_build()),
            "a later value leaves the checked build alone"
        );
        let mark = |harness: &mut Harness, app: &str| {
            harness.ask(|reply| Request::MarkChecked {
                app: app.to_owned(),
                reply,
            })
        };
        assert_eq!(mark(&mut harness, "example"), Ok(()));
        assert_eq!(
            checked(&mut harness).map(|checked| checked.build),
            Some(updated.clone())
        );

        // A build read once is kept for as long as its process runs.
        harness.owner.host.build = None;
        let apps = harness.ask(Request::ListApps).expect("lists");
        assert_eq!(apps[0].running.as_ref(), Some(&updated));
        harness.owner.host.process = 5;
        let apps = harness.ask(Request::ListApps).expect("lists");
        assert_eq!(
            apps[0].running, None,
            "an unreadable build only goes unshown"
        );
        assert_eq!(
            mark(&mut harness, "example"),
            Err(
                "AppyTappy Setup cannot tell which build of Example App runs: unreadable here"
                    .to_owned()
            )
        );
        let other = harness.ask(|reply| Request::SaveApp {
            existing: None,
            spec: crate::applications::AppSpec {
                name: "Other App".to_owned(),
                window: WindowDef::exact(appytappy_config::window_match::WindowFacts {
                    title: "Other App",
                    class: "OtherWindow",
                    executable: "Other.exe",
                }),
            },
            reply,
        });
        assert_eq!(other, Ok("other".to_owned()));
        assert_eq!(
            mark(&mut harness, "other"),
            Err("Start Other App first, so AppyTappy Setup can tell which build runs.".to_owned())
        );
        assert_eq!(
            checked(&mut harness).map(|checked| checked.build),
            Some(updated),
            "a failed mark keeps the build"
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn values_made_again_or_another_executable_need_a_new_check() {
        let (mut harness, root) = with_saved_bar("rechecked");
        let checked = |harness: &mut Harness| {
            harness.ask(Request::ListApps).expect("lists")[0]
                .file
                .as_ref()
                .and_then(|file| file.checked.clone())
                .map(|checked| checked.build)
        };
        assert_eq!(checked(&mut harness), Some(example_build()));
        harness
            .ask(|reply| Request::DeleteValue {
                app: "example".to_owned(),
                value: "progress_bar".to_owned(),
                reply,
            })
            .expect("deletes");
        assert_eq!(checked(&mut harness), None, "no values, nothing checked");
        let updated = BuildDef {
            version: Some("1.5.0.0".to_owned()),
            ..example_build()
        };
        harness.owner.host.process = 4;
        harness.owner.host.build = Some(updated.clone());
        harness
            .ask(|reply| Request::SaveValue {
                spec: Box::new(spec("example", "Progress bar", BAR)),
                reply,
            })
            .expect("saves");
        assert_eq!(checked(&mut harness), Some(updated.clone()));

        let rename = |harness: &mut Harness, executable: &str| {
            harness.ask(|reply| Request::SaveApp {
                existing: Some("example".to_owned()),
                spec: crate::applications::AppSpec {
                    name: "Example App".to_owned(),
                    window: WindowDef::exact(appytappy_config::window_match::WindowFacts {
                        title: "Example App",
                        class: "ExampleWindow",
                        executable,
                    }),
                },
                reply,
            })
        };
        assert_eq!(
            rename(&mut harness, "EXAMPLE.exe"),
            Ok("example".to_owned())
        );
        assert_eq!(
            checked(&mut harness),
            Some(updated),
            "the same executable keeps it"
        );
        assert_eq!(rename(&mut harness, "Other.exe"), Ok("example".to_owned()));
        assert_eq!(checked(&mut harness), None, "another executable drops it");
        assert_eq!(
            rename(&mut harness, "Example.exe"),
            Ok("example".to_owned())
        );
        harness
            .ask(|reply| Request::SaveValue {
                spec: Box::new(spec("example", "Icon visible", STATE)),
                reply,
            })
            .expect("saves");
        assert_eq!(
            checked(&mut harness),
            None,
            "a value beside unchecked ones leaves them to Mark as checked"
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
