//! Saved applications: listing them with their running windows and builds,
//! saving and removing them, each one's anti-cheat class, and the build its
//! values were checked against.

use std::time::{SystemTime, UNIX_EPOCH};

use appytappy_config::app_file::{AppFile, BuildDef, CheckedDef};

use appytappy_config::anti_cheat::AntiCheatClass;
use appytappy_config::process::ProcessId;
use appytappy_sources::system::{ProcessEntry, WindowEntry};
use serde::Serialize;

use crate::applications::{self, AppSpec};
use crate::drafts::DraftKind;
use crate::window_list::{self, WindowFit, WindowList};

use super::host::Host;
use super::{Changed, Edit, Owner, Write};

/// A saved application, as the Values page lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSummary {
    /// The application's ID.
    pub(crate) id: String,
    /// The file, or `None` when it is broken.
    pub(crate) file: Option<AppFile>,
    /// Why the file is broken.
    pub(crate) error: Option<String>,
    /// The handles of the running windows that match, the best fit first.
    #[cfg_attr(test, ts(type = "Array<number>"))]
    pub(crate) windows: Vec<u64>,
    /// The build the best fit's executable runs, or `None` while no window
    /// matches or its build cannot be read.
    pub(crate) running: Option<BuildDef>,
}

/// A saved application with the open windows its match takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppFits {
    /// The application.
    pub(crate) app: AppSummary,
    /// The open windows its match takes, the one a new session would read
    /// first; none for a broken file.
    pub(crate) fits: Vec<WindowFit>,
}

/// The saved applications and the open windows, from one listing of the
/// desktop, as the picker of a value or reference shows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppWindows {
    /// The open windows, which every application's fits are among.
    pub(crate) open: WindowList,
    /// Each saved application.
    pub(crate) apps: Vec<AppFits>,
}

impl<H: Host> Owner<H> {
    /// The saved applications.
    pub(crate) fn apps(&mut self) -> Result<Vec<AppSummary>, String> {
        // The saved files read fine without the running windows, so a
        // failed window listing only leaves each one's windows empty.
        let (processes, windows) = self.desktop().unwrap_or_default();
        let found = self.app_windows_among(&processes, windows)?;
        Ok(found.apps.into_iter().map(|each| each.app).collect())
    }

    /// The saved applications and the open windows, with the windows each
    /// application's match takes, all from one listing of the desktop, so
    /// a window that opens or closes meanwhile cannot make them disagree.
    pub(crate) fn app_windows(&mut self) -> Result<AppWindows, String> {
        let (processes, windows) = self.desktop()?;
        self.app_windows_among(&processes, windows)
    }

    /// The saved applications, with what each one's match takes among
    /// `processes` and `all`, and the windows of `all` to offer.
    fn app_windows_among(
        &mut self,
        processes: &[ProcessEntry],
        all: Vec<WindowEntry>,
    ) -> Result<AppWindows, String> {
        let stored = self
            .stores()?
            .apps
            .load()
            .map_err(|error| error.to_string())?;
        let open = window_list::list(all.clone(), processes, self.own_process);
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
                        .read_window(&file.window.to_match(), processes.to_vec(), all.clone())
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
        let apps = apps
            .into_iter()
            .map(|(id, file, error, read)| {
                let fits = file
                    .as_ref()
                    .map(|file| {
                        let handle = read.as_ref().map(|read| read.entry.handle);
                        window_list::matching(&open.windows, &file.window, handle)
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
                AppFits {
                    app: AppSummary {
                        id,
                        file,
                        error,
                        windows: fits.iter().map(|fit| fit.handle).collect(),
                        running,
                    },
                    fits,
                }
            })
            .collect();
        Ok(AppWindows { open, apps })
    }

    /// The build that the window a new session would read for `file` runs,
    /// or `None` while no window fits.
    pub(super) fn running_build(&mut self, file: &AppFile) -> Result<Option<BuildDef>, String> {
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

    /// Saves `class` as the anti-cheat class of the application `app`. A
    /// class other than none stops its automations that press keys, which
    /// the Automations page then shows as unable to run.
    pub(crate) fn set_anti_cheat(
        &mut self,
        app: &str,
        class: AntiCheatClass,
    ) -> Result<(), String> {
        self.update_app(app, Changed::Runtime, |_owner, found| {
            found.file.anti_cheat = class;
            Ok(Edit::Save((), Vec::new()))
        })
        .map(|saved| saved.answer)
    }

    /// Saves `spec` over `existing`, or as a new application in a folder
    /// of its own, which a failed save leaves no trace of, and answers its
    /// ID.
    pub(crate) fn save_app(
        &mut self,
        existing: Option<&str>,
        spec: AppSpec,
    ) -> Result<String, String> {
        let file = applications::apply(self.stores()?, existing, spec)?;
        if let Some(id) = existing {
            self.write_app(id, Changed::Values, Write::Save(&file, &Vec::new()))?;
            return Ok(id.to_owned());
        }
        let id = self
            .stores()?
            .apps
            .create(&file, &[])
            .map_err(|error| error.to_string())?;
        self.tell(&id, Changed::Values);
        Ok(id)
    }

    /// Removes the application `app`, its values and their pictures, and
    /// closes its overlay's window. Answers why an overlay window could not
    /// open or close, if one could not; the application is removed either
    /// way.
    pub(crate) fn remove_app(&mut self, app: &str) -> Result<Option<String>, String> {
        applications::check_removable(self.stores()?, app)?;
        let removed = self.write_app(app, Changed::Applications, Write::Remove)?;
        // Nothing can finish a value, fix, or reference of an application
        // that is gone, and its draft of changes has nothing to change.
        self.delete_drafts(|draft| {
            draft.app.as_deref() == Some(app)
                || (draft.kind == DraftKind::Application && draft.changes.as_deref() == Some(app))
        });
        Ok(removed)
    }
}

/// `build` as checked now.
pub(super) fn checked_now(build: BuildDef) -> CheckedDef {
    // A clock set before 1970 counts as its start.
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    CheckedDef { build, at }
}

#[cfg(test)]
mod tests {
    use appytappy_config::action::MediaCommand;
    use appytappy_config::app_file::{SizeChangesDef, SizeDef, WindowDef};
    use appytappy_config::automation_file::{ActionDef, AutomationDef};
    use appytappy_sources::screen::{Image, Size};

    use crate::owner::SavedValue;
    use crate::owner::test_support::{
        BAR, FakeHost, Harness, STATE, add_example_app, example_build, frame_image, harness_with,
        scratch_store, spec, with_saved_bar,
    };
    use crate::window_list::WindowFit;

    use super::*;

    #[test]
    fn the_applications_and_the_open_windows_come_from_one_listing() {
        let (stores, root) = scratch_store("app-windows");
        let mut harness = harness_with(false, Ok(stores));
        add_example_app(&mut harness);
        let fit = |handle, best| WindowFit {
            handle,
            fallback: false,
            best,
            tied: true,
        };
        let handles = |found: &AppWindows| -> Vec<u64> {
            found.open.windows.iter().map(|each| each.handle).collect()
        };

        let found = harness.ask(Owner::app_windows).expect("lists");
        assert_eq!(
            handles(&found),
            [7],
            "the setup app's own window is left out"
        );
        assert_eq!(found.apps.len(), 1);
        let example = &found.apps[0];
        assert_eq!(example.app.id, "example");
        assert_eq!(example.app.windows, [7]);
        assert_eq!(example.fits, [fit(7, true)]);
        assert_eq!(
            example.app.file.as_ref().map(|file| file.name.as_str()),
            Some("Example App")
        );

        // Another copy's window fits as well, and both are offered.
        harness.owner.host.twin = Some(40);
        let found = harness.ask(Owner::app_windows).expect("lists");
        assert_eq!(handles(&found), [6, 7]);
        assert_eq!(found.apps[0].fits, [fit(7, true), fit(6, false)]);
        assert_eq!(found.apps[0].app.windows, [7, 6]);

        // Without the windows there is nothing to capture, so the look
        // fails where the list of applications alone would not.
        harness.owner.host.unlisted = true;
        assert_eq!(
            harness.ask(Owner::app_windows),
            Err("no windows here".to_owned())
        );
        assert!(harness.ask(Owner::apps).is_ok());
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
        // Only a check that reads anywhere in its region has a reading area.
        let mut area = spec("example", "Icon visible", STATE);
        area.reading_area = true;
        assert_eq!(
            harness.ask(save(area)),
            Err(
                "value \"icon_visible\": its region is a reading area, which only a number or \
                 bar fill check can read"
                    .to_owned()
            )
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
        let mut bar = spec("example", "Progress bar", BAR);
        bar.reading_area = true;
        harness.ask(save(bar)).expect("adds to the app");
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
        let areas: Vec<bool> = file
            .values
            .iter()
            .map(|value| value.screen.reading_area)
            .collect();
        assert_eq!(areas, [false, true], "a value keeps what its save said");
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
                command: MediaCommand::Pause,
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
        set(&mut harness, AntiCheatClass::None).expect("sets");
        let apps = harness.ask(Owner::apps).expect("lists");
        assert_eq!(
            apps[0].file.as_ref().map(|file| file.anti_cheat),
            Some(AntiCheatClass::None)
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
        set(&mut harness, AntiCheatClass::KernelLevel).expect("sets");
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
            set(&mut harness, AntiCheatClass::None).map(drop),
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
            harness.ask(|owner| owner.set_anti_cheat("gone", AntiCheatClass::None)),
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

    /// Linux makes a folder whose path is just short of its 4096-byte
    /// limit but no file in it, so the new application's folder is made
    /// and its first save fails.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_new_application_whose_first_save_fails_leaves_no_folder() {
        use appytappy_config::paths::Locations;

        use crate::stores::Stores;

        let (_stores, root) = scratch_store("first-save-fails");
        let stores_at = |base: &std::path::Path| {
            Stores::from(
                Locations::from_app_data(Some(base.into()), Some(base.into()))
                    .expect("the temporary folder is absolute"),
            )
        };
        let short = stores_at(&root).apps.folder("example").as_os_str().len();
        // `app.json`'s temporary file adds more than 30 bytes to this.
        let mut missing = 4080_usize
            .checked_sub(short)
            .expect("a short temporary folder");
        if missing % 200 == 1 {
            missing += 1;
        }
        let mut deep = root.clone();
        while missing > 0 {
            let part = missing.min(200);
            deep.push("a".repeat(part - 1));
            missing -= part;
        }
        let stores = stores_at(&deep);
        let mut harness = harness_with(false, Ok(stores.clone()));
        harness.forget_reloads();

        let saved = harness.ask(|owner| {
            owner.save_app(
                None,
                crate::owner::test_support::example_app(SizeChangesDef::Often),
            )
        });
        assert!(saved.is_err(), "the first save fails: {saved:?}");
        assert!(!stores.apps.folder("example").exists(), "no folder stays");
        assert_eq!(stores.apps.load().expect("lists"), []);
        assert_eq!(harness.reloads(), 0, "a save that failed tells no one");
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
}
