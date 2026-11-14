//! Saved values: saving and deleting them, and reading a value's check back
//! as the runtime will.

use appytappy_config::app_file::{AppFile, PinDef, ScreenDef, ValueDef};
use appytappy_config::names::new_value_id;
use appytappy_sources::screen::saved::{self, SavedError};
use serde::{Deserialize, Serialize};

use crate::automations;
use crate::check_spec::{self, CheckSpec, Pictures, SpecError};
use crate::drafts::DraftKind;
use crate::stores::SavedApp;

use super::apps::checked_now;
use super::host::Host;
use super::{Changed, Edit, Owner};

/// A value to save, as the UI describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct SaveSpec {
    /// The saved application to add the value to.
    pub(crate) app: String,
    /// The value's name, such as `Progress`.
    pub(crate) value_name: String,
    /// The check, with the session's screenshots it names.
    pub(crate) check: CheckSpec,
    /// The screenshot the region was drawn on, whose size the value keeps.
    pub(crate) screenshot: usize,
    /// The side of the window the region stays at as the window changes
    /// size.
    pub(crate) pin: PinDef,
    /// Whether the region was drawn as a reading area, larger than what it
    /// reads. Only a check that reads anywhere in its region may have one.
    pub(crate) reading_area: bool,
}

/// Where a value was saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct SavedValue {
    /// The application's ID.
    pub(crate) app: String,
    /// The value's ID.
    pub(crate) value: String,
}

impl<H: Host> Owner<H> {
    /// Saves a value and answers where it was saved, or what to fix.
    pub(crate) fn save(&mut self, spec: &SaveSpec) -> Result<SavedValue, String> {
        // Without the stores, nothing else the user could fix matters.
        self.stores()?;
        let name = spec.value_name.trim();
        if name.is_empty() {
            return Err("Name the value first.".to_owned());
        }
        if self.capture.screenshots().get(spec.screenshot).is_none() {
            return Err("Take a screenshot first.".to_owned());
        }
        // The value keeps the window size its region was drawn at, which
        // any screenshot it cuts at the region must have too.
        let size = check_spec::drawn_size(&spec.check, self.capture.screenshots(), spec.screenshot)
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
            let saved =
                check_spec::to_saved(&spec.check, owner.capture.screenshots(), &spec.app, &id)
                    .map_err(|error| error.to_string())?;
            let screen = ScreenDef {
                window_size: size.into(),
                region: saved.region,
                check: saved.check,
                pin: Some(spec.pin),
                reading_area: spec.reading_area,
                fixes: Vec::new(),
            };
            read_back(
                &screen,
                found,
                saved::Readers::new(&found.file.references),
                &saved.pictures,
            )?;
            let file = &mut found.file;
            let first = file.values.is_empty();
            file.values.push(ValueDef {
                id: id.clone(),
                name: name.to_owned(),
                origin: check_spec::origin(
                    &spec.check,
                    owner.capture.screenshots(),
                    spec.screenshot,
                ),
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
        .map(|saved| saved.answer)?;
        // Nothing can finish a fix of a value that is gone.
        self.delete_drafts(|draft| {
            draft.kind == DraftKind::Fix
                && draft.app.as_deref() == Some(app)
                && draft.changes.as_deref() == Some(value)
        });
        Ok(())
    }
}

/// Fails when a value of `file` other than `own` already has `name`,
/// ignoring spaces at either end and the capitals of ASCII letters, as
/// automations and applications compare names. Two values with one name
/// could not be told apart where an automation picks one. Their IDs never
/// clash however alike the names are, since [`new_value_id`] numbers a taken
/// one.
pub(super) fn named_apart(file: &AppFile, own: Option<&str>, name: &str) -> Result<(), String> {
    if let Some(other) = file.values.iter().find(|value| {
        Some(value.id.as_str()) != own && value.name.trim().eq_ignore_ascii_case(name.trim())
    }) {
        return Err(format!(
            "{} already has a value named \"{}\". Pick another name.",
            file.name,
            other.name.trim()
        ));
    }
    Ok(())
}

/// Reads `screen` back as the runtime will, with `readers`, those of the
/// references of `app`'s file, and its size changes, from `pictures`, about
/// to be saved, and those already in its folder. It is the one judge of a
/// value's check: one that reads back is one the runtime can read, whether
/// it is being tried on the live window or saved.
///
/// # Errors
///
/// Returns what to fix, in the page's words where it has some, as
/// [`SpecError`] says.
pub(super) fn read_back(
    screen: &ScreenDef,
    app: &SavedApp,
    mut readers: saved::Readers<'_>,
    pictures: &Pictures,
) -> Result<saved::SavedValue, String> {
    let file = &app.file;
    saved::build(screen, &mut readers, file.size_changes, |picture| {
        pictures
            .iter()
            .find(|(name, _)| name == picture)
            .map_or_else(|| app.picture(picture), |(_, png)| Ok(png.clone()))
    })
    .map_err(|error| match error {
        SavedError::NoReference { .. } => format!("{} no longer has that reference.", file.name),
        SavedError::NoCharacters { id, characters } => {
            let saved = file.references.iter().find(|reference| reference.id == id);
            SpecError::NoCharacters {
                reference: saved.map_or(id, |reference| reference.name.clone()),
                characters,
            }
            .to_string()
        },
        error => file
            .references
            .iter()
            .find_map(|reference| check_spec::sample_problem_of(&error, reference))
            .unwrap_or_else(|| SpecError::from(error))
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use appytappy_config::app_file::{SizeDef, WindowDef};
    use appytappy_config::automation_file::AutomationDef;
    use appytappy_sources::screen::{Image, Size};

    use crate::drafts::DraftDef;
    use crate::owner::test_support::{
        BAR, Harness, NO_STORE, add_example_app, capturing, frame_image, harness, harness_with,
        scratch_store, spec,
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
            reading_area: false,
            fixes: Vec::new(),
        };
        std::fs::write(app.folder.join("old.png"), &png).expect("writes");
        let new = vec![("new.png".to_owned(), png)];
        assert_eq!(
            read_back(
                &screen(["new.png", "old.png"]),
                &app,
                saved::Readers::new(&app.file.references),
                &new
            )
            .map(drop),
            Ok(())
        );
        assert_eq!(
            read_back(
                &screen(["new.png", "gone.png"]),
                &app,
                saved::Readers::new(&app.file.references),
                &new
            )
            .map(drop),
            Err(
                "AppyTappy could not read this value as saved: its picture gone.png cannot be \
                 read."
                    .to_owned()
            )
        );
        assert!(
            read_back(
                &screen(["old.png", "new.png"]),
                &app,
                saved::Readers::new(&app.file.references),
                &Vec::new()
            )
            .is_err(),
            "a new picture is not on disk yet"
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn drafts_go_with_the_value_or_the_application_they_belong_to() {
        let (stores, root) = scratch_store("drafts-gone");
        let mut harness = harness_with(false, Ok(stores));
        add_example_app(&mut harness);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        harness
            .ask(|owner| owner.save(&spec("example", "Progress bar", BAR)))
            .expect("saves");
        let draft = |kind, app: Option<&str>, changes: Option<&str>| DraftDef {
            kind,
            app: app.map(str::to_owned),
            changes: changes.map(str::to_owned),
            name: "Draft".to_owned(),
            missing: Vec::new(),
            pictures: Vec::new(),
            state: serde_json::Value::Null,
        };
        let mut keep = |draft: DraftDef| {
            harness
                .ask(move |owner| owner.save_draft(None, &draft, false))
                .expect("saves the draft")
        };
        let fix = keep(draft(DraftKind::Fix, Some("example"), Some("progress_bar")));
        let other_fix = keep(draft(DraftKind::Fix, Some("example"), Some("other")));
        let elsewhere = keep(draft(DraftKind::Fix, Some("another"), Some("progress_bar")));
        let value = keep(draft(DraftKind::Value, Some("example"), None));
        let changes = keep(draft(DraftKind::Application, None, Some("example")));
        let new_app = keep(draft(DraftKind::Application, None, None));
        let automation = keep(draft(DraftKind::Automation, None, Some("example")));
        let left = |harness: &mut Harness| -> Vec<String> {
            harness
                .ask(|owner| owner.list_drafts())
                .expect("lists")
                .into_iter()
                .map(|summary| summary.id)
                .collect()
        };

        harness
            .ask(|owner| owner.delete("example", "progress_bar"))
            .expect("deletes the value");
        assert!(!left(&mut harness).contains(&fix), "its fix draft goes");
        assert_eq!(left(&mut harness).len(), 6, "and no other draft");

        harness
            .ask(|owner| owner.remove_app("example"))
            .expect("removes the application");
        assert_eq!(
            left(&mut harness),
            [elsewhere, new_app, automation],
            "every draft of the application goes, and its draft of changes"
        );
        drop((other_fix, value, changes));
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn names_are_told_apart_by_what_they_say_not_by_the_ids_they_make() {
        let (mut harness, _handler, root) = capturing("value-names");
        let mut save = |name: &str| harness.ask(|owner| owner.save(&spec("example", name, BAR)));
        let long = "Mana of the currently selected party member, slot";
        // Names in a script without ASCII letters, and names that differ
        // only past the ID's length, make one ID, which a number keeps apart.
        let saved = [
            "Здоровье",
            "Мана",
            &format!("{long} 1"),
            &format!("{long} 2"),
        ]
        .map(|name| save(name).map(|saved| saved.value));
        assert_eq!(
            saved,
            [
                Ok("value".to_owned()),
                Ok("value_2".to_owned()),
                Ok("mana_of_the_currently_selected_party_member_slot".to_owned()),
                Ok("mana_of_the_currently_selected_party_member_2".to_owned()),
            ]
        );
        // Only ASCII letters' case is ignored, as automations ignore it.
        assert_eq!(
            save(" мана ").map(|saved| saved.value),
            Ok("value_3".to_owned())
        );
        // The refusal names the value that has the name, as it was saved.
        assert_eq!(
            save(&format!(" {} 2 ", long.to_uppercase())),
            Err(format!(
                "Example App already has a value named \"{long} 2\". Pick another name."
            ))
        );
        assert_eq!(
            save(" Мана "),
            Err("Example App already has a value named \"Мана\". Pick another name.".to_owned())
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
                    anti_cheat: None,
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
