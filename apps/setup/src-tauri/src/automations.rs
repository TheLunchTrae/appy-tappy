//! Saving, listing, and removing automations.
//!
//! Automations live in their own file, apart from the applications' files,
//! and name a value by its application's ID and the value's ID. Saving one
//! makes the same check the runtime makes, [`Runnable::new`], so it
//! never names a value that is not there or compares a value with the wrong
//! kind of target. A value that later goes away shows as a problem on the
//! automation that used it, and so does an automation that breaks a rule on
//! its own, such as after a hand edit; either can still be turned off and
//! deleted. The ID is this app's own key for an automation: the runtime does
//! not read it, so a hand edit that breaks or repeats it stops nothing, and
//! [`unique_ids`] gives each automation one the page can go by.

mod problem;
mod uses;

use appytappy_config::action::KeyChord;
use appytappy_config::automation_file::{ActionDef, AutomationDef, AutomationsFile};
use appytappy_config::names::new_automation_id;
use appytappy_config::signal::SignalName;
use serde::Serialize;

use crate::stores::Stores;
use crate::words::sentence;

pub(crate) use self::problem::watched_values;
use self::problem::{apps, problem};
pub(crate) use self::uses::{check_app_unused, check_still_suits, check_unused};

/// A saved automation, as the Automations page lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationSummary {
    /// The automation.
    pub(crate) automation: AutomationDef,
    /// Why the runtime cannot run it as saved, such as a value that was
    /// removed since.
    pub(crate) problem: Option<String>,
}

/// Loads the automations, saying what went wrong for the user. Each one
/// has an ID that no other one has, as [`unique_ids`] makes them.
fn load(stores: &Stores) -> Result<AutomationsFile, String> {
    let mut file = stores
        .automations
        .load()
        .map_err(|error| sentence(&format!("AppyTappy cannot read your automations: {error}")))?;
    unique_ids(&mut file);
    Ok(file)
}

/// Gives a new ID, made from its name, to each automation whose ID a hand
/// edit made invalid or gave to an automation before it, so the page, which
/// goes by ID, can tell every automation apart. Only the first of those
/// sharing an ID keeps it, so the same file always gets the same IDs, and
/// the next save writes them.
fn unique_ids(file: &mut AutomationsFile) {
    let mut taken: Vec<String> = file
        .automations
        .iter()
        .map(|automation| automation.id.clone())
        .collect();
    let mut kept: Vec<String> = Vec::with_capacity(taken.len());
    for automation in &mut file.automations {
        if SignalName::new(&automation.id).is_err() || kept.contains(&automation.id) {
            let names: Vec<&str> = taken.iter().map(String::as_str).collect();
            automation.id = new_automation_id(&automation.name, &names);
            taken.push(automation.id.clone());
        }
        kept.push(automation.id.clone());
    }
}

/// Every automation, with what stops each one from running.
pub(crate) fn list(stores: &Stores) -> Result<Vec<AutomationSummary>, String> {
    let file = load(stores)?;
    let apps = apps(stores)?;
    Ok(file
        .automations
        .iter()
        .map(|automation| AutomationSummary {
            automation: automation.clone(),
            problem: problem(automation, &apps, stores),
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
            ActionDef::Open { target: text } | ActionDef::Sound { file: text } => {
                *text = text.trim().to_owned();
            },
            // The file keeps the keys in one spelling, such as Ctrl+Shift+M.
            ActionDef::Keys { keys } => {
                if let Ok(chord) = KeyChord::parse(keys) {
                    *keys = chord.to_string();
                }
            },
            // A click holds only names and numbers the setup app chose.
            ActionDef::Click { .. } => {},
        }
    }
    if let Some(problem) = problem(&draft, &apps(stores)?, stores) {
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
    write(stores, &file)?;
    Ok(id)
}

/// Saves `file` as the automations, then removes the click pictures that
/// neither they nor a draft name, such as those of a target that was
/// replaced or deleted.
fn write(stores: &Stores, file: &AutomationsFile) -> Result<(), String> {
    stores
        .automations
        .save(file)
        .map_err(|error| error.to_string())?;
    // No one reads a picture nothing names, so one that stays changes
    // nothing, and the next save tries again.
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the save worked; an unused picture that stays is harmless"
    )]
    let _ = prune_pictures(stores, file);
    Ok(())
}

/// Removes the click pictures that no automation in `file` and no draft
/// names. When the drafts cannot be listed, it removes none, since a draft
/// may name any.
fn prune_pictures(stores: &Stores, file: &AutomationsFile) -> Result<(), String> {
    let drafted = stores
        .drafts
        .pictures()
        .map_err(|error| error.to_string())?;
    stores
        .automations
        .prune_pictures(file, &drafted)
        .map_err(|error| error.to_string())
}

/// Removes the click pictures that no saved automation and no draft names:
/// those taken for a target of an automation the user then neither saved nor
/// kept as a draft, and those of a draft that was deleted.
pub(crate) fn forget_unsaved_pictures(stores: &Stores) -> Result<(), String> {
    prune_pictures(stores, &load(stores)?)
}

/// Turns the automation `id` on or off. One that cannot run as saved stays
/// off until it is fixed.
pub(crate) fn set_enabled(stores: &Stores, id: &str, enabled: bool) -> Result<(), String> {
    let mut file = load(stores)?;
    let position = file
        .automations
        .iter()
        .position(|automation| automation.id == id)
        .ok_or_else(|| "That automation is no longer saved.".to_owned())?;
    #[expect(
        clippy::indexing_slicing,
        reason = "`position` was just found in this list, which has not changed"
    )]
    {
        let automation = &file.automations[position];
        if enabled && let Some(problem) = problem(automation, &apps(stores)?, stores) {
            return Err(format!(
                "{} cannot run until it is fixed. {problem}",
                automation.name
            ));
        }
        file.automations[position].enabled = enabled;
    }
    write(stores, &file)
}

/// Removes the automation `id`. One that is already gone is fine.
pub(crate) fn delete(stores: &Stores, id: &str) -> Result<(), String> {
    let mut file = load(stores)?;
    file.automations.retain(|automation| automation.id != id);
    write(stores, &file)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    use appytappy_config::app_file::{
        AntiCheatDef, AppFile, CheckDef, ColorDef, FORMAT, OriginDef, RegionDef, ScreenDef,
        SizeChangesDef, SizeDef, ValueDef, WindowDef,
    };
    use appytappy_config::automation_file::{
        ActionDef, ClickAtDef, ClickButtonDef, ClickDeliveryDef, ComparisonDef,
        FORMAT as AUTOMATIONS_FORMAT, MediaCommandDef, PointDef, TargetDef, WhenDef,
    };
    use appytappy_config::click::InvalidClick;
    use appytappy_config::window_match::WindowFacts;
    use appytappy_sources::screen::{Image, Size};

    use super::*;
    use crate::drafts::{DraftDef, DraftKind};
    use crate::stores;

    /// Fresh stores for one test, holding the application `example` with a
    /// yes-or-no value `icon_visible` and a number value `health`.
    fn stores(name: &str) -> (Stores, PathBuf) {
        let (stores, folder) = stores::scratch(&format!("automations-{name}"));
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
                pin: None,
                reading_area: false,
                fixes: Vec::new(),
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
            size_changes: SizeChangesDef::Often,
            anti_cheat: AntiCheatDef::Unknown,
            overlay: None,
            checked: None,
            origin: OriginDef::ByHand,
            references: Vec::new(),
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
                        empty: ColorDef {
                            red: 0,
                            green: 0,
                            blue: 0,
                        },
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
            Err("Its application other is no longer saved or cannot be read.".to_owned())
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
        // A hand edit that leaves the file readable but the value not, as
        // the runtime reads it, shows on the automation that watches it.
        let mut broken = app.clone();
        for value in &mut broken.values {
            value.screen.region.width = 0;
        }
        stores.apps.save("example", &broken, &[]).expect("saves");
        assert!(
            list(&stores).expect("lists")[0]
                .problem
                .as_deref()
                .is_some_and(|problem| problem.starts_with("Icon visible cannot be read: ")),
            "{:?}",
            list(&stores)
        );
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
        fs::write(stores.apps.folder("example").join("app.json"), b"{").expect("breaks");
        let error = stores.apps.load().expect("loads")[0]
            .file
            .clone()
            .expect_err("broken");
        assert_eq!(
            list(&stores).expect("lists")[0].problem,
            Some(format!(
                "Its application's app.json, in the example folder, cannot be read: {error}."
            ))
        );
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn only_automations_that_can_run_watch_their_values() {
        let (stores, folder) = stores("watched");
        let none = BTreeSet::new();
        assert_eq!(watched_values(&stores, "example"), Ok(none.clone()));
        save(&stores, None, draft("Pause music")).expect("saves");
        let watched = watched_values(&stores, "example").expect("reads");
        assert_eq!(watched, BTreeSet::from(["icon_visible".to_owned()]));
        assert_eq!(watched_values(&stores, "other"), Ok(none.clone()));
        // A value the runtime cannot build stops the automation, so the
        // runtime reads nothing for it.
        let app = stores.apps.load().expect("loads")[0]
            .file
            .clone()
            .expect("good");
        let mut broken = app.clone();
        for value in &mut broken.values {
            value.screen.region.width = 0;
        }
        stores.apps.save("example", &broken, &[]).expect("saves");
        assert_eq!(watched_values(&stores, "example"), Ok(none.clone()));
        stores.apps.save("example", &app, &[]).expect("saves");
        assert_eq!(watched_values(&stores, "example"), Ok(watched));
        set_enabled(&stores, "pause_music", false).expect("turns off");
        assert_eq!(watched_values(&stores, "example"), Ok(none));
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn a_broken_automation_is_listed_and_can_be_deleted() {
        let (stores, folder) = stores("broken");
        save(&stores, None, draft("Pause music")).expect("saves");
        let mut file = stores.automations.load().expect("loads");
        let mut broken = draft("Broken");
        broken.id = "broken".to_owned();
        broken.actions.clear();
        file.automations.push(broken);
        // A hand edit, which the setup app's own save refuses.
        fs::write(automations_file(&folder), file.to_json()).expect("writes");
        let problems: Vec<Option<String>> = list(&stores)
            .expect("one broken automation leaves the list readable")
            .into_iter()
            .map(|summary| summary.problem)
            .collect();
        assert_eq!(problems, [None, Some("Add an action.".to_owned())]);
        set_enabled(&stores, "pause_music", false).expect("the others still change");
        set_enabled(&stores, "broken", false).expect("turns off");
        assert_eq!(
            set_enabled(&stores, "broken", true),
            Err("Broken cannot run until it is fixed. Add an action.".to_owned())
        );
        delete(&stores, "broken").expect("removes");
        assert_eq!(ids(&stores), ["pause_music"]);
        fs::remove_dir_all(folder).expect("clean up");
    }

    /// The automations' file of the stores in `folder`.
    fn automations_file(folder: &Path) -> PathBuf {
        folder.join("AppyTappy").join("automations.json")
    }

    /// Writes `automations` as the whole file, as a hand edit could.
    fn hand_edit(folder: &Path, automations: Vec<AutomationDef>) {
        let file = AutomationsFile {
            format: AUTOMATIONS_FORMAT,
            automations,
        };
        fs::write(automations_file(folder), file.to_json()).expect("writes");
    }

    fn with_id(name: &str, id: &str) -> AutomationDef {
        let mut automation = draft(name);
        id.clone_into(&mut automation.id);
        automation
    }

    #[test]
    fn automations_sharing_an_id_are_listed_apart_and_a_save_writes_new_ids() {
        let (stores, folder) = stores("shared-id");
        hand_edit(
            &folder,
            vec![
                with_id("Pause music", "pause_music"),
                with_id("Lower volume", "pause_music"),
                with_id("Skip ahead", "Skip ahead"),
            ],
        );
        assert_eq!(ids(&stores), ["pause_music", "lower_volume", "skip_ahead"]);
        let listed: Vec<(String, Option<String>)> = list(&stores)
            .expect("lists")
            .into_iter()
            .map(|summary| (summary.automation.id, summary.problem))
            .collect();
        assert_eq!(
            listed,
            [
                ("pause_music".to_owned(), None),
                ("lower_volume".to_owned(), None),
                ("skip_ahead".to_owned(), None),
            ],
            "the page tells them apart, and the runtime runs them all as saved"
        );
        let mut edited = draft("Lower volume");
        edited.cooldown_ms = 5000;
        assert_eq!(
            save(&stores, Some("lower_volume"), edited.clone()),
            Ok("lower_volume".to_owned())
        );
        let saved = stores.automations.load().expect("loads");
        assert_eq!(
            saved.automations.first(),
            Some(&with_id("Pause music", "pause_music")),
            "the other automation with that ID is unchanged"
        );
        edited.id = "lower_volume".to_owned();
        assert_eq!(saved.automations.get(1), Some(&edited));
        let problems: Vec<Option<String>> = list(&stores)
            .expect("lists")
            .into_iter()
            .map(|summary| summary.problem)
            .collect();
        assert_eq!(problems, [None, None, None]);
        assert_eq!(
            stores
                .automations
                .load()
                .expect("loads")
                .automations
                .iter()
                .map(|automation| automation.id.as_str())
                .collect::<Vec<_>>(),
            ["pause_music", "lower_volume", "skip_ahead"],
            "the save wrote every new ID"
        );
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn deleting_one_of_two_sharing_an_id_keeps_the_other() {
        let (stores, folder) = stores("shared-id-delete");
        hand_edit(
            &folder,
            vec![
                with_id("Pause music", "pause_music"),
                with_id("Lower volume", "pause_music"),
            ],
        );
        delete(&stores, "lower_volume").expect("removes");
        assert_eq!(ids(&stores), ["pause_music"]);
        hand_edit(
            &folder,
            vec![
                with_id("Pause music", "pause_music"),
                with_id("Lower volume", "pause_music"),
            ],
        );
        delete(&stores, "pause_music").expect("removes");
        let names: Vec<String> = list(&stores)
            .expect("lists")
            .into_iter()
            .map(|summary| summary.automation.name)
            .collect();
        assert_eq!(names, ["Lower volume"]);
        set_enabled(&stores, "lower_volume", false).expect("turns off");
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
        fs::write(automations_file(&folder), b"{").expect("breaks");
        assert!(
            unused(&stores)
                .is_err_and(|error| error.starts_with("AppyTappy cannot read your automations"))
        );
        assert!(list(&stores).is_err());
        assert!(save(&stores, None, draft("New")).is_err());
        fs::remove_dir_all(folder).expect("clean up");
    }

    /// A draft that clicks on the click picture `image` in `example`, whose
    /// anti-cheat is set to none so it can click.
    fn clicking(stores: &Stores, name: &str, image: &str) -> AutomationDef {
        let mut app = stores.app("example").expect("saved").file;
        app.anti_cheat = AntiCheatDef::None;
        stores.apps.save("example", &app, &[]).expect("saves");
        let mut clicking = draft(name);
        clicking.undo = false;
        clicking.actions = vec![ActionDef::Click {
            app: "example".to_owned(),
            button: ClickButtonDef::Left,
            delivery: ClickDeliveryDef::Cursor,
            return_focus: true,
            at: ClickAtDef::Picture {
                image: image.to_owned(),
                window_size: SizeDef {
                    width: 100,
                    height: 50,
                },
                search: RegionDef {
                    x: 0,
                    y: 0,
                    width: 20,
                    height: 20,
                },
                press: PointDef { x: 1, y: 1 },
                tolerance: 20,
            },
        }];
        clicking
    }

    /// A 4 by 4 picture's PNG bytes, every pixel `shade`.
    fn png(shade: u8) -> Vec<u8> {
        let size = Size {
            width: 4,
            height: 4,
        };
        Image::new(size, [shade, shade, shade, 255].repeat(16))
            .expect("fits")
            .encode_png()
            .expect("encodes")
    }

    #[test]
    fn clicks_keep_their_pictures_and_the_rest_go() {
        let (stores, folder) = stores("click-pictures");
        assert_eq!(
            save(&stores, None, clicking(&stores, "Click", "click-1.png")),
            Err("The picture action 1 clicks on is gone. Set up its target again.".to_owned())
        );
        let kept = stores.automations.add_picture(&png(1)).expect("adds");
        let dropped = stores.automations.add_picture(&png(2)).expect("adds");
        let id = save(&stores, None, clicking(&stores, "Click", &kept)).expect("saves");
        assert!(stores.automations.picture(&kept).is_ok());
        assert!(stores.automations.picture(&dropped).is_err(), "pruned");
        // Turning one off is a save of the automations too.
        let stray = stores.automations.add_picture(&png(3)).expect("adds");
        set_enabled(&stores, &id, false).expect("turns off");
        assert!(stores.automations.picture(&kept).is_ok());
        assert!(stores.automations.picture(&stray).is_err(), "pruned");
        // A target set up for an automation that was then not saved.
        let unsaved = stores.automations.add_picture(&png(4)).expect("adds");
        assert_eq!(forget_unsaved_pictures(&stores), Ok(()));
        assert!(stores.automations.picture(&kept).is_ok());
        assert!(stores.automations.picture(&unsaved).is_err(), "forgotten");
        let another = stores.automations.add_picture(&png(5)).expect("adds");
        delete(&stores, &id).expect("deletes");
        assert!(stores.automations.picture(&kept).is_err(), "pruned");
        assert!(stores.automations.picture(&another).is_err(), "pruned");
        // While the automations cannot be read, nothing says which pictures
        // they name, so every picture stays.
        let waiting = stores.automations.add_picture(&png(6)).expect("adds");
        let saved = stores
            .automations
            .pictures()
            .with_file_name("automations.json");
        assert!(saved.exists(), "the file the automations are saved in");
        fs::write(saved, b"{").expect("breaks");
        assert!(forget_unsaved_pictures(&stores).is_err());
        assert!(stores.automations.picture(&waiting).is_ok());
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn a_picture_a_draft_names_stays() {
        let (stores, folder) = stores("click-drafted");
        let drafted = stores.automations.add_picture(&png(1)).expect("adds");
        let draft = DraftDef {
            kind: DraftKind::Automation,
            app: None,
            changes: None,
            name: "Click".to_owned(),
            missing: vec!["Pick a value to watch".to_owned()],
            pictures: vec![drafted.clone()],
            state: serde_json::Value::Null,
        };
        let id = stores
            .drafts
            .save(None, &draft, &[])
            .expect("saves the draft");
        let kept = stores.automations.add_picture(&png(2)).expect("adds");
        save(&stores, None, clicking(&stores, "Click", &kept)).expect("saves");
        assert!(
            stores.automations.picture(&drafted).is_ok(),
            "the draft keeps it"
        );
        // The end of an automation's flow keeps what a draft names too.
        let unsaved = stores.automations.add_picture(&png(3)).expect("adds");
        assert_eq!(forget_unsaved_pictures(&stores), Ok(()));
        assert!(stores.automations.picture(&unsaved).is_err(), "forgotten");
        assert!(
            stores.automations.picture(&drafted).is_ok(),
            "the draft keeps it"
        );
        // While a draft cannot be read, nothing says which pictures it
        // names, so every picture stays, through a save too.
        let file = folder
            .join("AppyTappy")
            .join("drafts")
            .join(&id)
            .join("draft.json");
        let text = fs::read_to_string(&file).expect("the draft's file");
        fs::write(&file, "{").expect("breaks the draft");
        let spare = stores.automations.add_picture(&png(7)).expect("adds");
        assert!(forget_unsaved_pictures(&stores).is_err());
        save(&stores, None, clicking(&stores, "Meanwhile", &kept)).expect("saves");
        assert!(stores.automations.picture(&drafted).is_ok());
        assert!(stores.automations.picture(&spare).is_ok());
        fs::write(&file, text).expect("mends the draft");
        assert_eq!(forget_unsaved_pictures(&stores), Ok(()));
        assert!(stores.automations.picture(&spare).is_err(), "pruned again");
        assert!(stores.automations.picture(&drafted).is_ok());
        // Once the draft is deleted, nothing names its picture.
        stores.drafts.delete(&id).expect("deletes the draft");
        assert_eq!(forget_unsaved_pictures(&stores), Ok(()));
        assert!(stores.automations.picture(&drafted).is_err(), "pruned");
        assert!(stores.automations.picture(&kept).is_ok());
        // While the drafts cannot be listed, nothing says which pictures
        // they name, so every picture stays.
        let waiting = stores.automations.add_picture(&png(4)).expect("adds");
        let drafts = folder.join("AppyTappy").join("drafts");
        fs::remove_dir_all(&drafts).expect("removes the drafts folder");
        fs::write(&drafts, b"not a folder").expect("blocks the listing");
        assert!(forget_unsaved_pictures(&stores).is_err());
        save(&stores, None, clicking(&stores, "Another", &kept)).expect("saves");
        assert!(stores.automations.picture(&waiting).is_ok());
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn a_click_s_picture_fits_its_area_and_holds_its_press() {
        let (stores, folder) = stores("click-fit");
        let picture = stores.automations.add_picture(&png(5)).expect("adds");
        let unreadable = stores.automations.add_picture(b"not a png").expect("adds");
        assert_eq!(
            save(&stores, None, clicking(&stores, "Unreadable", &unreadable)),
            Err("The picture action 1 clicks on is gone. Set up its target again.".to_owned())
        );
        let mut small = clicking(&stores, "Small", &picture);
        let mut off = clicking(&stores, "Off", &picture);
        if let [
            ActionDef::Click {
                at: ClickAtDef::Picture { search, .. },
                ..
            },
        ] = small.actions.as_mut_slice()
        {
            search.width = 3;
        }
        if let [
            ActionDef::Click {
                at: ClickAtDef::Picture { press, .. },
                ..
            },
        ] = off.actions.as_mut_slice()
        {
            press.x = 4;
        }
        assert_eq!(
            save(&stores, None, small),
            Err(format!(
                "Action 1's target cannot be used: {}. Set up its target again.",
                InvalidClick::LargePicture
            ))
        );
        assert_eq!(
            save(&stores, None, off),
            Err(format!(
                "Action 1's target cannot be used: {}. Set up its target again.",
                InvalidClick::PressOutside
            ))
        );
        // A picture removed outside the app shows on the list, and the
        // automation cannot be turned back on, as the runtime would refuse it.
        let mut kept = clicking(&stores, "Kept", &picture);
        kept.enabled = false;
        let id = save(&stores, None, kept).expect("saves");
        fs::remove_file(stores.automations.pictures().join(&picture)).expect("removes");
        let gone = "The picture action 1 clicks on is gone. Set up its target again.".to_owned();
        let listed = list(&stores).expect("lists");
        assert_eq!(
            listed.first().and_then(|summary| summary.problem.clone()),
            Some(gone.clone())
        );
        assert_eq!(
            set_enabled(&stores, &id, true),
            Err(format!("Kept cannot run until it is fixed. {gone}"))
        );
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn an_application_clicked_in_whose_file_does_not_read_says_why() {
        let (stores, folder) = stores("clicked-in-broken");
        let picture = stores.automations.add_picture(&png(4)).expect("adds");
        let mut clicks = clicking(&stores, "Click", &picture);
        if let [ActionDef::Click { app, .. }] = clicks.actions.as_mut_slice() {
            "target".clone_into(app);
        }
        let target = stores.app("example").expect("saved").file;
        stores.apps.save("target", &target, &[]).expect("saves");
        let mut file = load(&stores).expect("loads");
        file.automations = vec![clicks];
        stores.automations.save(&file).expect("saves");
        assert_eq!(list(&stores).expect("lists")[0].problem, None);
        fs::write(stores.apps.folder("target").join("app.json"), b"{").expect("breaks");
        let error = stores
            .apps
            .load()
            .expect("loads")
            .into_iter()
            .find(|app| app.id == "target")
            .expect("listed")
            .file
            .expect_err("broken");
        assert_eq!(
            list(&stores).expect("lists")[0].problem,
            Some(format!(
                "The app.json of the application it clicks in, in the target folder, cannot be \
                 read: {error}."
            ))
        );
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn an_application_clicked_in_names_its_automations() {
        let (stores, folder) = stores("clicked-in");
        let picture = stores.automations.add_picture(&png(4)).expect("adds");
        let mut clicks = clicking(&stores, "Click", &picture);
        clicks.when.app = "other".to_owned();
        let mut file = load(&stores).expect("loads");
        file.automations.push(clicks);
        stores.automations.save(&file).expect("saves");
        assert_eq!(
            check_app_unused(&stores, "example", "Example App"),
            Err(
                "Click uses Example App as the application it clicks in. Change or delete it \
                 first."
                    .to_owned()
            )
        );
        fs::remove_dir_all(folder).expect("clean up");
    }
}
