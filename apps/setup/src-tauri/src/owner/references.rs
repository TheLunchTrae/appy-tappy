//! References: the characters number values read with, saved per
//! application. Saving one learns its samples, as the check that reads with
//! it would, and deleting one waits until no value reads with it.

use crate::check_spec::{ReferenceRef, SavedReference, Session};
use appytappy_config::app_file::{new_reference_id, reference_id};

use super::host::Host;
use super::messages::{ReferenceSaveSpec, SavedReferenceId};
use super::{Changed, Edit, Owner};

impl<H: Host> Owner<H> {
    /// Hands `work` the session's screenshots, recording and saved
    /// references, which the checks the UI describes name.
    pub(super) fn with_session<T>(&self, work: impl FnOnce(Session<'_>) -> T) -> T {
        let references = |reference: &ReferenceRef| self.saved_reference(reference);
        work(Session {
            screenshots: &self.screenshots,
            recording: self.readings(),
            references: &references,
        })
    }

    /// The saved reference `reference` with those of its samples' pictures
    /// that can be read. Learning it says which one is missing.
    fn saved_reference(&self, reference: &ReferenceRef) -> Result<SavedReference, String> {
        let app = self.stores()?.app(&reference.app)?;
        let saved = app
            .file
            .reference(&reference.id)
            .ok_or_else(|| format!("{} no longer has that reference.", app.file.name))?
            .clone();
        let pictures = saved
            .samples
            .iter()
            .filter_map(|sample| Some((sample.image.clone(), app.picture(&sample.image).ok()?)))
            .collect();
        Ok(SavedReference {
            reference: saved,
            pictures,
        })
    }

    /// Saves the reference `spec` teaches to its application, and says
    /// where it went.
    pub(crate) fn save_reference(
        &mut self,
        spec: &ReferenceSaveSpec,
    ) -> Result<SavedReferenceId, String> {
        let name = spec.reference_name.trim();
        if name.is_empty() {
            return Err("Name the reference first.".to_owned());
        }
        let editing = spec.characters.reference.as_ref();
        if let Some(editing) = editing
            && editing.app != spec.app
        {
            return Err("That reference belongs to another application.".to_owned());
        }
        let app = &spec.app;
        self.update_app(app, Changed::Values, |owner, found| {
            let file = &mut found.file;
            let id = editing.map_or_else(
                || {
                    let taken: Vec<&str> = file
                        .references
                        .iter()
                        .map(|saved| saved.id.as_str())
                        .collect();
                    new_reference_id(name, &taken)
                },
                |editing| editing.id.clone(),
            );
            // Names that make the same ID, such as "Digits" and "digits ",
            // are one name.
            let same = reference_id(name);
            if let Some(other) = file
                .references
                .iter()
                .find(|saved| saved.id != id && reference_id(&saved.name) == same)
            {
                return Err(format!(
                    "{} already has a reference named \"{}\". Pick another name.",
                    file.name, other.name
                ));
            }
            let taught = owner
                .with_session(|session| {
                    spec.characters
                        .teach(session, Some(&id), name, &spec.expects)
                })
                .map_err(|error| error.to_string())?;
            match file.references.iter_mut().find(|saved| saved.id == id) {
                Some(saved) => *saved = taught.reference,
                None => file.references.push(taught.reference),
            }
            Ok(Edit::Save(
                SavedReferenceId {
                    app: app.clone(),
                    reference: id,
                },
                taught.pictures,
            ))
        })
        .map(|saved| saved.answer)
    }

    /// Removes the reference `reference` of the application `app` and its
    /// pictures, unless a value reads with it, in its check or in a fix. A
    /// reference already gone changed nothing, so no one is told, but
    /// pictures an earlier delete left behind are removed.
    pub(crate) fn delete_reference(&mut self, app: &str, reference: &str) -> Result<(), String> {
        let gone = self.update_app(app, Changed::Values, |_owner, found| {
            let file = &mut found.file;
            let Some(saved) = file.reference(reference) else {
                return Ok(Edit::Keep(Some(file.clone())));
            };
            let users: Vec<&str> = file
                .values
                .iter()
                .filter(|value| value.screen.reads_with(reference))
                .map(|value| value.name.as_str())
                .collect();
            if !users.is_empty() {
                return Err(format!(
                    "{} {} with {}. Change or remove {} first.",
                    and_list(&users),
                    if users.len() == 1 { "reads" } else { "read" },
                    saved.name,
                    if users.len() == 1 { "it" } else { "them" }
                ));
            }
            file.references.retain(|saved| saved.id != reference);
            Ok(Edit::Save(None, Vec::new()))
        })?;
        match gone.answer {
            Some(file) => self.prune(app, &file),
            None => Ok(()),
        }
    }
}

/// `names` as a list in a sentence, such as `A, B and C`.
pub(super) fn and_list(names: &[&str]) -> String {
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => (*last).to_owned(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::app_file::{CheckDef, FixDef, SizeDef};

    use crate::owner::messages::SavedValue;
    use crate::owner::test_support::{
        FakeHost, add_example_app, frame_image, harness_with, scratch_store, spec, trial,
    };

    use super::*;

    /// A reference named `name` to save to the application `app`, teaching
    /// that the red square on screenshot 0 is a 1. With `editing`, the
    /// application and ID of a saved reference, it adds to that reference.
    fn reference(app: &str, name: &str, editing: Option<(&str, &str)>) -> ReferenceSaveSpec {
        let editing = editing.map_or_else(
            || "null".to_owned(),
            |(owner, id)| format!(r#"{{"app":"{owner}","id":"{id}"}}"#),
        );
        ReferenceSaveSpec {
            app: app.to_owned(),
            reference_name: name.to_owned(),
            expects: String::new(),
            characters: serde_json::from_str(&format!(
                r#"{{"region":{{"x":0,"y":0,"width":2,"height":2}},"screenshot":0,
                    "reference":{editing},"ink":{{"red":255,"green":0,"blue":0}},
                    "tolerance":0,"labels":[{{"screenshot":0,"text":"1"}}]}}"#
            ))
            .expect("valid JSON"),
        }
    }

    const NUMBER: &str = r#"{"kind":"number","region":{"x":0,"y":0,"width":2,"height":2},
        "reference":{"app":"example","id":"digits"},"whole":true}"#;

    #[test]
    fn a_reference_is_saved_read_with_and_deleted_once_unused() {
        let (stores, root) = scratch_store("references");
        let apps_root = stores.apps.folder("");
        let mut harness = harness_with(false, Ok(stores));
        add_example_app(&mut harness);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        let save = |spec| move |owner: &mut Owner<FakeHost>| owner.save_reference(&spec);
        let delete = |reference: &str| {
            let reference = reference.to_owned();
            move |owner: &mut Owner<FakeHost>| owner.delete_reference("example", &reference)
        };
        assert_eq!(
            harness.ask(save(reference("example", " ", None))),
            Err("Name the reference first.".to_owned())
        );
        assert_eq!(
            harness.ask(save(reference("missing", "Digits", None))),
            Err("That application is no longer saved.".to_owned())
        );
        let saved = harness
            .ask(save(reference("example", "Digits", None)))
            .expect("saves");
        assert_eq!(
            saved,
            SavedReferenceId {
                app: "example".to_owned(),
                reference: "digits".to_owned()
            }
        );
        assert_eq!(
            harness.ask(save(reference("example", "digits", None))),
            Err(
                "Example App already has a reference named \"Digits\". Pick another name."
                    .to_owned()
            ),
            "a new reference named like another"
        );
        assert_eq!(
            harness.ask(save(reference(
                "example",
                "Digits",
                Some(("other", "digits"))
            ))),
            Err("That reference belongs to another application.".to_owned())
        );
        // Editing keeps the ID and its own name, adds the new sample, and
        // leaves the saved samples' pictures as they were.
        let folder = apps_root.join("example");
        let first = std::fs::read(folder.join("ref-digits-1.png")).expect("saved picture");
        harness
            .ask(save(reference(
                "example",
                "Digits",
                Some(("example", "digits")),
            )))
            .expect("keeps its own name");
        harness
            .ask(save(reference("example", "Small", None)))
            .expect("a second reference");
        assert_eq!(
            harness.ask(save(reference(
                "example",
                "Digits",
                Some(("example", "small")),
            ))),
            Err(
                "Example App already has a reference named \"Digits\". Pick another name."
                    .to_owned()
            ),
            "renaming to another reference's name"
        );
        harness
            .ask(save(reference(
                "example",
                "Red digits",
                Some(("example", "digits")),
            )))
            .expect("renames the saved reference");
        assert_eq!(
            harness.ask(save(reference(
                "example",
                " Red digits ",
                Some(("example", "small")),
            ))),
            Err(
                "Example App already has a reference named \"Red digits\". Pick another name."
                    .to_owned()
            ),
            "renaming to another reference's name, whose ID differs"
        );
        // The old name is free again, and its ID goes to a new reference
        // with a number.
        assert_eq!(
            harness.ask(save(reference("example", "Digits", None))),
            Ok(SavedReferenceId {
                app: "example".to_owned(),
                reference: "digits_2".to_owned()
            })
        );
        assert_eq!(
            harness.ask(delete("digits_2")),
            Ok(()),
            "an unused reference goes"
        );
        let apps = harness.ask(Owner::apps).expect("lists");
        let file = apps[0].file.as_ref().expect("a good file");
        let saved = file.reference("digits").expect("saved");
        assert_eq!(saved.name, "Red digits");
        assert_eq!(saved.samples.len(), 3);
        assert_eq!(
            std::fs::read(folder.join("ref-digits-1.png")).expect("kept"),
            first
        );
        assert!(folder.join("ref-digits-3.png").exists());

        // A number value reads with it, and keeps it until the value goes.
        assert_eq!(
            harness.ask(|owner| owner.save(&spec("example", "Progress", NUMBER))),
            Ok(SavedValue {
                app: "example".to_owned(),
                value: "progress".to_owned()
            })
        );
        let apps = harness.ask(Owner::apps).expect("lists");
        let file = apps[0].file.as_ref().expect("a good file");
        assert_eq!(
            file.values[0].screen.check,
            CheckDef::Number {
                reference: "digits".to_owned(),
                whole: true,
                characters: None,
            }
        );
        harness
            .ask(|owner| owner.save(&spec("example", "Best", NUMBER)))
            .expect("a second value");
        assert_eq!(
            harness.ask(delete("digits")),
            Err("Progress and Best read with Red digits. Change or remove them first.".to_owned())
        );
        harness
            .ask(|owner| owner.delete("example", "best"))
            .expect("removes the second value");
        assert_eq!(
            harness.ask(delete("digits")),
            Err("Progress reads with Red digits. Change or remove it first.".to_owned())
        );
        // A value that reads with another reference keeps it while a fix
        // still reads with it.
        harness
            .ask(|owner| {
                let stores = owner.stores()?;
                let mut file = stores.app("example")?.file;
                let screen = &mut file.values[0].screen;
                screen.check = CheckDef::Number {
                    reference: "small".to_owned(),
                    whole: true,
                    characters: None,
                };
                screen.fixes = vec![FixDef {
                    window_size: SizeDef {
                        width: 20,
                        height: 20,
                    },
                    region: screen.region,
                    images: Vec::new(),
                    reference: Some("digits".to_owned()),
                }];
                stores
                    .apps
                    .save("example", &file, &[])
                    .map_err(|error| error.to_string())
            })
            .expect("a fix reads with it");
        assert_eq!(
            harness.ask(delete("digits")),
            Err("Progress reads with Red digits. Change or remove it first.".to_owned())
        );
        harness.forget_reloads();
        harness
            .ask(|owner| owner.delete("example", "progress"))
            .expect("removes the value");
        assert_eq!(harness.reloads(), 1, "the runtime reads the values again");
        assert_eq!(harness.ask(delete("digits")), Ok(()));
        assert_eq!(harness.reloads(), 1, "and the references");
        assert!(!folder.join("ref-digits-1.png").exists());
        // A picture an earlier delete left behind goes with the next delete,
        // even of a reference already gone, while the kept pictures stay.
        let kept: Vec<_> = std::fs::read_dir(&folder)
            .expect("lists the pictures")
            .map(|entry| entry.expect("an entry").file_name())
            .collect();
        std::fs::write(folder.join("ref-digits-1.png"), b"left behind").expect("a leftover");
        assert_eq!(harness.ask(delete("digits")), Ok(()), "already gone");
        assert_eq!(harness.reloads(), 0, "nothing changed");
        assert!(
            !folder.join("ref-digits-1.png").exists(),
            "the leftover is gone"
        );
        for name in &kept {
            assert!(folder.join(name).exists(), "{name:?} stays");
        }
        let apps = harness.ask(Owner::apps).expect("lists");
        let file = apps[0].file.as_ref().expect("a good file");
        assert_eq!(
            file.references
                .iter()
                .map(|kept| kept.id.as_str())
                .collect::<Vec<_>>(),
            ["small"],
            "only the deleted reference is gone"
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_reference_is_saved_with_the_characters_it_expects_until_they_change() {
        let (stores, root) = scratch_store("references-expected");
        let mut harness = harness_with(false, Ok(stores));
        add_example_app(&mut harness);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        let mut save = |editing, expects: &str| {
            let spec = ReferenceSaveSpec {
                expects: expects.to_owned(),
                ..reference("example", "Digits", editing)
            };
            harness.ask(move |owner| owner.save_reference(&spec))?;
            let apps = harness.ask(Owner::apps)?;
            let file = apps[0].file.as_ref().ok_or("a good file")?;
            let saved = file.reference("digits").ok_or("saved")?;
            Ok::<_, String>((saved.expects.clone(), saved.samples.len()))
        };
        // It learns the one sample it is taught, whatever it expects.
        assert_eq!(
            save(None, "0123456789%"),
            Ok((Some("0123456789%".to_owned()), 1))
        );
        let saved = Some(("example", "digits"));
        assert_eq!(
            save(saved, "% 1 0 1"),
            Ok((Some("01%".to_owned()), 2)),
            "listed once each, in order"
        );
        let too_many: String = ('\u{100}'..='\u{180}').collect();
        assert_eq!(
            save(saved, &too_many),
            Err("A reference needs at most 128 characters. Remove some.".to_owned()),
        );
        assert_eq!(save(saved, ""), Ok((None, 3)), "none chosen");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_saved_reference_missing_a_picture_says_how_to_recover() {
        let (stores, root) = scratch_store("references-missing");
        let mut harness = harness_with(false, Ok(stores.clone()));
        add_example_app(&mut harness);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        harness.forget_reloads();
        harness
            .ask(|owner| owner.save_reference(&reference("example", "Digits", None)))
            .expect("saves");
        assert_eq!(harness.reloads(), 1, "the runtime reads the references");
        std::fs::remove_file(stores.apps.folder("example").join("ref-digits-1.png"))
            .expect("remove the picture");
        assert_eq!(
            harness.ask(|owner| owner.set_check(&trial(NUMBER))),
            Err(
                "The reference Digits is missing its picture ref-digits-1.png. Remove the values \
                 that read with it, then delete the reference and make it again."
                    .to_owned()
            )
        );
        assert_eq!(
            and_list(&["A", "B", "C"]),
            "A, B and C",
            "a list in a sentence"
        );
        assert_eq!(and_list(&[]), "");
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
