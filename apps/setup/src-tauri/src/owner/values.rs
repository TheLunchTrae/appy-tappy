//! Saved values and applications: listing them with their running windows
//! and builds, saving and deleting them, each application's anti-cheat
//! setting, and the build its values were checked against.

use std::time::{SystemTime, UNIX_EPOCH};

use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::app_file::{
    AntiCheatDef, AppFile, BuildDef, CheckedDef, ScreenDef, SizeDef, ValueDef, new_value_id,
    value_id,
};
use appytappy_sources::screen::saved;

use crate::applications::{self, AppSpec};
use crate::automations;
use crate::check_spec::{self, Pictures};
use crate::stores::SavedApp;
use crate::window_list;

use super::host::Host;
use super::messages::{AppSummary, SaveSpec, SavedValue};
use super::{Changed, Edit, Owner, Write};

impl<H: Host> Owner<H> {
    /// The saved applications.
    pub(crate) fn apps(&mut self) -> Result<Vec<AppSummary>, String> {
        let stored = self
            .stores()?
            .apps
            .load()
            .map_err(|error| error.to_string())?;
        // The saved files read fine without the running windows, so a
        // failed window listing only leaves each one's windows empty.
        let (processes, all) = self.desktop().unwrap_or_default();
        let windows = window_list::list(all.clone(), &processes, self.own_process).windows;
        // Each application with the window a new session would read for it.
        let apps: Vec<_> = stored
            .into_iter()
            .map(|app| {
                let (file, error) = match app.file {
                    Ok(file) => (Some(file), None),
                    Err(error) => (None, Some(error)),
                };
                let read = file.as_ref().and_then(|file| {
                    self.host
                        .read_window(&file.window.to_match(), processes.clone(), all.clone())
                });
                (app.id, file, error, read)
            })
            .collect();
        // A process that has gone may give its ID to another build.
        self.builds.retain(|process, _| {
            apps.iter().any(|(_, _, _, read)| {
                read.as_ref()
                    .is_some_and(|read| read.entry.process == *process)
            })
        });
        Ok(apps
            .into_iter()
            .map(|(id, file, error, read)| {
                let matched: Vec<u64> = file
                    .as_ref()
                    .map(|file| {
                        let handle = read.as_ref().map(|read| read.entry.handle);
                        window_list::matching(&windows, &file.window, handle)
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
                    .and_then(|_| {
                        self.build_of(read.map(|read| read.entry.process))
                            .ok()
                            .flatten()
                    });
                AppSummary {
                    id,
                    file,
                    error,
                    windows: matched,
                    running,
                }
            })
            .collect())
    }

    /// Saves a value and answers where it was saved, or what to fix.
    pub(crate) fn save(&mut self, spec: &SaveSpec) -> Result<SavedValue, String> {
        // Without the stores, nothing else the user could fix matters.
        self.stores()?;
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
        self.update_app(&spec.app, Changed::Values, |owner, found| {
            named_apart(&found.file, None, name)?;
            let taken: Vec<&str> = found
                .file
                .values
                .iter()
                .map(|value| value.id.as_str())
                .collect();
            let id = new_value_id(name, &taken);
            let saved = owner
                .with_session(|session| check_spec::to_saved(&spec.check, session, &spec.app, &id))
                .map_err(|error| error.to_string())?;
            let screen = ScreenDef {
                window_size: SizeDef {
                    width: size.width,
                    height: size.height,
                },
                region: saved.region,
                check: saved.check,
                pin: Some(spec.pin),
                fixes: Vec::new(),
            };
            read_back(&screen, found, &saved.pictures)?;
            let file = &mut found.file;
            let first = file.values.is_empty();
            file.values.push(ValueDef {
                id: id.clone(),
                name: name.to_owned(),
                screen,
            });
            // The first value is checked against the build that runs now.
            // Later ones leave that to Mark as checked, since one new value
            // says nothing about the others.
            if first {
                file.checked = owner.running_build(file).ok().flatten().map(checked_now);
            }
            Ok(Edit::Save(
                SavedValue {
                    app: spec.app.clone(),
                    value: id,
                },
                saved.pictures,
            ))
        })
        .map(|saved| saved.answer)
    }

    /// The build that the window a new session would read for `file` runs,
    /// or `None` while no window fits.
    fn running_build(&mut self, file: &AppFile) -> Result<Option<BuildDef>, String> {
        let (processes, windows) = self.desktop()?;
        let read = self
            .host
            .read_window(&file.window.to_match(), processes, windows);
        self.build_of(read.map(|read| read.entry.process))
    }

    /// The build of `process`, or `None` without one. A build read once is
    /// kept for its process, and a failed read is tried again next time.
    fn build_of(&mut self, process: Option<ProcessId>) -> Result<Option<BuildDef>, String> {
        let Some(process) = process else {
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
    pub(crate) fn mark_checked(&mut self, app: &str) -> Result<(), String> {
        self.update_app(app, Changed::Runtime, |owner, found| {
            let file = &mut found.file;
            let build = owner
                .running_build(file)
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
            Ok(Edit::Save((), Vec::new()))
        })
        .map(|saved| saved.answer)
    }

    /// Deletes the value `value` of the application `app`, unless an
    /// automation uses it.
    pub(crate) fn delete(&mut self, app: &str, value: &str) -> Result<(), String> {
        self.update_app(app, Changed::Values, |owner, found| {
            let file = &mut found.file;
            if let Some(saved) = file.values.iter().find(|saved| saved.id == value) {
                automations::check_unused(owner.stores()?, app, value, &saved.name)?;
            }
            file.values.retain(|saved| saved.id != value);
            // Values made again later are checked against the build they are
            // made on.
            if file.values.is_empty() {
                file.checked = None;
            }
            Ok(Edit::Save((), Vec::new()))
        })
        .map(|saved| saved.answer)
    }

    /// Saves `class` as the anti-cheat class of the application `app`. A
    /// class other than none stops its automations that press keys, which
    /// the Automations page then shows as unable to run.
    pub(crate) fn set_anti_cheat(&mut self, app: &str, class: AntiCheatDef) -> Result<(), String> {
        self.update_app(app, Changed::Runtime, |_owner, found| {
            found.file.anti_cheat = class;
            Ok(Edit::Save((), Vec::new()))
        })
        .map(|saved| saved.answer)
    }

    /// Saves `spec` as a new application, or over `existing`, and answers
    /// its ID.
    pub(crate) fn save_app(
        &mut self,
        existing: Option<&str>,
        spec: AppSpec,
    ) -> Result<String, String> {
        let (id, file) = applications::apply(self.stores()?, existing, spec)?;
        self.write_app(&id, Changed::Values, Write::Save(&file, &Vec::new()))?;
        Ok(id)
    }

    /// Removes the application `app`, its values and their pictures, and
    /// closes its overlay's window. Answers why an overlay window could not
    /// open or close, if one could not; the application is removed either
    /// way.
    pub(crate) fn remove_app(&mut self, app: &str) -> Result<Option<String>, String> {
        applications::check_removable(self.stores()?, app)?;
        self.write_app(app, Changed::Applications, Write::Remove)
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

/// Fails when a value of `file` other than `own` has a name like `name`.
/// Names that make the same ID, such as "Progress" and "progress ", are one
/// name, whatever ID the value named so has: a renamed value keeps its ID.
pub(super) fn named_apart(file: &AppFile, own: Option<&str>, name: &str) -> Result<(), String> {
    let same = value_id(name);
    if file
        .values
        .iter()
        .any(|value| Some(value.id.as_str()) != own && value_id(&value.name) == same)
    {
        return Err(format!(
            "{} already has a value named like \"{name}\". Pick another name.",
            file.name
        ));
    }
    Ok(())
}

/// Reads `screen` back as the runtime will, with the references and size
/// changes of `app`'s file, from `pictures`, about to be saved, and those
/// already in its folder, so a value that saves is one the runtime can read.
pub(super) fn read_back(
    screen: &ScreenDef,
    app: &SavedApp,
    pictures: &Pictures,
) -> Result<saved::SavedValue, String> {
    saved::build(
        screen,
        &app.file.references,
        app.file.size_changes,
        |picture| {
            pictures
                .iter()
                .find(|(name, _)| name == picture)
                .map_or_else(|| app.picture(picture), |(_, png)| Ok(png.clone()))
        },
    )
    .map_err(|error| format!("AppyTappy could not read this value as saved: {error}."))
}

#[cfg(test)]
mod tests {
    use appytappy_config::app_file::WindowDef;
    use appytappy_config::automation_file::{ActionDef, AutomationDef, MediaCommandDef};
    use appytappy_sources::screen::{Image, Size};

    use crate::owner::test_support::{
        BAR, FakeHost, Harness, NO_STORE, STATE, add_example_app, example_build, frame_image,
        harness, harness_with, scratch_store, spec, with_saved_bar,
    };

    use super::*;

    #[test]
    fn a_value_is_read_back_from_its_new_pictures_and_the_saved_ones() {
        let (stores, root) = scratch_store("read-back");
        let mut harness = harness_with(false, Ok(stores.clone()));
        add_example_app(&mut harness);
        let app = stores.app("example").expect("saved");
        let png = Image::new(
            Size {
                width: 2,
                height: 2,
            },
            vec![0; 16],
        )
        .expect("fits")
        .encode_png()
        .expect("encodes");
        let screen = |images: [&str; 2]| ScreenDef {
            window_size: SizeDef {
                width: 4,
                height: 2,
            },
            region: appytappy_config::app_file::RegionDef {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            check: appytappy_config::app_file::CheckDef::State {
                states: ["Shown", "Hidden"]
                    .iter()
                    .zip(images)
                    .map(|(name, image)| appytappy_config::app_file::StateDef {
                        name: (*name).to_owned(),
                        image: image.to_owned(),
                    })
                    .collect(),
                tolerance: 0,
            },
            pin: None,
            fixes: Vec::new(),
        };
        std::fs::write(app.folder.join("old.png"), &png).expect("writes");
        let new = vec![("new.png".to_owned(), png)];
        assert_eq!(
            read_back(&screen(["new.png", "old.png"]), &app, &new).map(drop),
            Ok(())
        );
        assert_eq!(
            read_back(&screen(["new.png", "gone.png"]), &app, &new).map(drop),
            Err(
                "AppyTappy could not read this value as saved: its picture gone.png cannot be \
                 read."
                    .to_owned()
            )
        );
        assert!(
            read_back(&screen(["old.png", "new.png"]), &app, &Vec::new()).is_err(),
            "a new picture is not on disk yet"
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn saved_values_are_listed_with_their_running_windows_and_can_be_removed() {
        let (stores, root) = scratch_store("save");
        let apps_root = stores.apps.folder("");
        let mut harness = harness_with(false, Ok(stores));
        add_example_app(&mut harness);
        let mut handler = harness.pick();
        let save = |spec| move |owner: &mut Owner<FakeHost>| owner.save(&spec);
        assert_eq!(
            harness.ask(save(spec("example", "Icon visible", STATE))),
            Err("Take a screenshot first.".to_owned())
        );
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
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
        assert_eq!(harness.ask(Owner::screenshot), Ok(1));
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

        let apps = harness.ask(Owner::apps).expect("lists");
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
        harness.forget_reloads();
        let id = harness
            .ask(|owner| owner.save_automation(None, automation))
            .expect("saves");
        assert_eq!(id, "pause_music");
        assert_eq!(harness.reloads(), 1, "the runtime reads the automations");
        assert_eq!(
            harness.ask(|owner| owner.delete("example", "icon_visible")),
            Err("Pause music uses Icon visible. Change or delete it first.".to_owned())
        );
        assert_eq!(harness.reloads(), 0, "a delete that failed tells no one");
        harness
            .ask(|owner| owner.set_automation_enabled(&id, false))
            .expect("turns off");
        assert_eq!(harness.reloads(), 1);
        let listed = harness
            .ask(|owner| owner.list_automations())
            .expect("lists");
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
            .ask(|owner| owner.delete_automation(&id))
            .expect("removes");
        assert_eq!(harness.reloads(), 1);
        assert_eq!(
            harness.ask(|owner| owner.list_automations()),
            Ok(Vec::new())
        );

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
        let save_press =
            |harness: &mut Harness| harness.ask(|owner| owner.save_automation(None, press.clone()));
        assert!(
            save_press(&mut harness)
                .is_err_and(|error| error.contains("anti-cheat is set to none")),
            "an application whose anti-cheat is not set cannot press keys"
        );
        let set = |harness: &mut Harness, class| {
            harness.ask(|owner| owner.set_anti_cheat("example", class))
        };
        set(&mut harness, AntiCheatDef::None).expect("sets");
        let apps = harness.ask(Owner::apps).expect("lists");
        assert_eq!(
            apps[0].file.as_ref().map(|file| file.anti_cheat),
            Some(AntiCheatDef::None)
        );
        let id = save_press(&mut harness).expect("saves");
        let listed = harness
            .ask(|owner| owner.list_automations())
            .expect("lists");
        assert_eq!(
            listed[0].automation.actions,
            [ActionDef::Keys {
                keys: "Ctrl+Shift+M".to_owned()
            }],
            "the file keeps one spelling"
        );
        assert_eq!(listed[0].problem, None);
        set(&mut harness, AntiCheatDef::KernelLevel).expect("sets");
        let listed = harness
            .ask(|owner| owner.list_automations())
            .expect("lists");
        assert!(
            listed[0]
                .problem
                .as_ref()
                .is_some_and(|problem| problem.contains("anti-cheat is set to none")),
            "{listed:?}"
        );
        harness
            .ask(|owner| owner.delete_automation(&id))
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
            .ask(|owner| owner.save_automation(None, open_and_play))
            .expect("saves");
        let listed = harness
            .ask(|owner| owner.list_automations())
            .expect("lists");
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
            .ask(|owner| owner.delete_automation(&id))
            .expect("removes");

        harness
            .ask(|owner| owner.delete("example", "icon_visible"))
            .expect("removes");
        assert!(!folder.join("icon_visible-1.png").exists());
        let apps = harness.ask(Owner::apps).expect("lists");
        let file = apps[0].file.as_ref().expect("a good file");
        assert_eq!(file.values.len(), 1);
        // The saved values still list when the windows cannot be.
        harness.owner.host.unlisted = true;
        let apps = harness.ask(Owner::apps).expect("lists");
        assert_eq!((apps.len(), apps[0].windows.len()), (1, 0));
        harness.owner.host.unlisted = false;
        assert_eq!(
            harness.ask(|owner| owner.delete("gone", "x")),
            Err("That application is no longer saved.".to_owned())
        );
        assert_eq!(
            harness.ask(|owner| owner.set_anti_cheat("gone", AntiCheatDef::None)),
            Err("That application is no longer saved.".to_owned())
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn the_first_value_records_the_running_build_until_marked_checked_again() {
        let (mut harness, root) = with_saved_bar("checked");
        let checked = |harness: &mut Harness| {
            harness.ask(Owner::apps).expect("lists")[0]
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
        let apps = harness.ask(Owner::apps).expect("lists");
        assert_eq!(apps[0].running.as_ref(), Some(&updated));
        harness
            .ask(|owner| owner.save(&spec("example", "Icon visible", STATE)))
            .expect("saves");
        assert_eq!(
            checked(&mut harness).map(|checked| checked.build),
            Some(example_build()),
            "a later value leaves the checked build alone"
        );
        let mark = |harness: &mut Harness, app: &str| harness.ask(|owner| owner.mark_checked(app));
        harness.forget_reloads();
        assert_eq!(mark(&mut harness, "example"), Ok(()));
        assert_eq!(harness.reloads(), 1, "the runtime reads the checked build");
        assert_eq!(
            checked(&mut harness).map(|checked| checked.build),
            Some(updated.clone())
        );

        // A build read once is kept for as long as its process runs.
        harness.owner.host.build = None;
        let apps = harness.ask(Owner::apps).expect("lists");
        assert_eq!(apps[0].running.as_ref(), Some(&updated));
        harness.owner.host.process = 5;
        let apps = harness.ask(Owner::apps).expect("lists");
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
        let other = harness.ask(|owner| {
            owner.save_app(
                None,
                crate::applications::AppSpec {
                    name: "Other App".to_owned(),
                    window: WindowDef::exact(appytappy_config::window_match::WindowFacts {
                        title: "Other App",
                        class: "OtherWindow",
                        executable: "Other.exe",
                    }),
                    size_changes: appytappy_config::app_file::SizeChangesDef::Often,
                },
            )
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
            harness.ask(Owner::apps).expect("lists")[0]
                .file
                .as_ref()
                .and_then(|file| file.checked.clone())
                .map(|checked| checked.build)
        };
        assert_eq!(checked(&mut harness), Some(example_build()));
        harness
            .ask(|owner| owner.delete("example", "progress_bar"))
            .expect("deletes");
        assert_eq!(checked(&mut harness), None, "no values, nothing checked");
        let updated = BuildDef {
            version: Some("1.5.0.0".to_owned()),
            ..example_build()
        };
        harness.owner.host.process = 4;
        harness.owner.host.build = Some(updated.clone());
        harness
            .ask(|owner| owner.save(&spec("example", "Progress bar", BAR)))
            .expect("saves");
        assert_eq!(checked(&mut harness), Some(updated.clone()));

        let rename = |harness: &mut Harness, executable: &str| {
            harness.ask(|owner| {
                owner.save_app(
                    Some("example"),
                    crate::applications::AppSpec {
                        name: "Example App".to_owned(),
                        window: WindowDef::exact(appytappy_config::window_match::WindowFacts {
                            title: "Example App",
                            class: "ExampleWindow",
                            executable,
                        }),
                        size_changes: appytappy_config::app_file::SizeChangesDef::Often,
                    },
                )
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
            .ask(|owner| owner.save(&spec("example", "Icon visible", STATE)))
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
        assert_eq!(harness.ask(Owner::apps), Err(NO_STORE.to_owned()));
        assert_eq!(
            harness.ask(|owner| owner.save(&spec("example", "x", BAR))),
            Err(NO_STORE.to_owned())
        );
        assert_eq!(
            harness.ask(|owner| owner.delete("a", "b")),
            Err(NO_STORE.to_owned())
        );
        assert_eq!(
            harness.ask(|owner| owner.save_app(
                None,
                crate::applications::AppSpec {
                    name: "Example App".to_owned(),
                    window: WindowDef::exact(appytappy_config::window_match::WindowFacts {
                        title: "Example App",
                        class: "ExampleWindow",
                        executable: "Example.exe",
                    }),
                    size_changes: appytappy_config::app_file::SizeChangesDef::Often,
                }
            )),
            Err(NO_STORE.to_owned())
        );
        assert_eq!(
            harness.ask(|owner| owner.remove_app("a")),
            Err(NO_STORE.to_owned())
        );
        assert_eq!(
            harness.ask(|owner| owner.list_automations()),
            Err(NO_STORE.to_owned())
        );
        let automation: AutomationDef = serde_json::from_value(serde_json::json!({
            "id": "", "name": "x", "enabled": true,
            "when": {"app": "a", "value": "b", "is": "equals", "target": true},
            "actions": [{"kind": "media", "command": "pause"}], "undo": false
        }))
        .expect("the UI's spelling");
        assert_eq!(
            harness.ask(|owner| owner.save_automation(None, automation)),
            Err(NO_STORE.to_owned())
        );
        assert_eq!(
            harness.ask(|owner| owner.set_automation_enabled("x", true)),
            Err(NO_STORE.to_owned())
        );
        assert_eq!(
            harness.ask(|owner| owner.delete_automation("x")),
            Err(NO_STORE.to_owned())
        );
    }
}
