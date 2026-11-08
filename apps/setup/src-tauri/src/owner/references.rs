//! References: the characters number values read with, saved per
//! application. Saving one learns its samples, as the check that reads with
//! it would, and deleting one waits until no value reads with it.

use std::fs;

use appytappy_config::app_file::{new_reference_id, reference_id};
use appytappy_sources::screen::Image;

use crate::check_spec::{self, ReferenceRef, SavedReference, Session};

use super::Owner;
use super::host::Host;
use super::messages::{ReferenceSaveSpec, SavedReferenceId};

impl<H: Host> Owner<H> {
    /// Hands `work` the session's screenshots, recording and saved
    /// references, which the checks the UI describes name.
    pub(super) fn with_session<T>(&self, work: impl FnOnce(Session<'_>) -> T) -> T {
        let references = |reference: &ReferenceRef| self.saved_reference(reference);
        work(Session {
            screenshots: &self.screenshots,
            recording: self.recording.as_ref(),
            references: &references,
        })
    }

    /// The saved reference `reference` with its samples' pictures.
    pub(super) fn saved_reference(
        &self,
        reference: &ReferenceRef,
    ) -> Result<SavedReference, String> {
        let file = self.app_file(&reference.app)?;
        let saved = file
            .reference(&reference.id)
            .ok_or_else(|| format!("{} no longer has that reference.", file.name))?
            .clone();
        let folder = self.stores()?.apps.folder(&reference.app);
        let pictures = saved
            .samples
            .iter()
            .map(|sample| {
                fs::read(folder.join(&sample.image))
                    .ok()
                    .and_then(|bytes| Image::decode_png(&bytes).ok())
                    .ok_or_else(|| {
                        format!(
                            "The reference {} is missing its picture {}. Remove the values that \
                             read with it, then delete the reference and make it again.",
                            saved.name, sample.image
                        )
                    })
            })
            .collect::<Result<_, _>>()?;
        Ok(SavedReference {
            reference: saved,
            pictures,
        })
    }

    /// Saves the reference `spec` teaches to its application, and says
    /// where it went.
    pub(super) fn save_reference(
        &self,
        spec: &ReferenceSaveSpec,
    ) -> Result<SavedReferenceId, String> {
        let name = spec.reference_name.trim();
        if name.is_empty() {
            return Err("Name the reference first.".to_owned());
        }
        let editing = match &spec.check {
            check_spec::CheckSpec::Characters { reference, .. } => reference.clone(),
            _ => return Err("Pick the characters' color to make a reference.".to_owned()),
        };
        if let Some(editing) = &editing
            && editing.app != spec.app
        {
            return Err("That reference belongs to another application.".to_owned());
        }
        let app = spec.app.clone();
        let mut file = self.app_file(&app)?;
        let id = editing.as_ref().map_or_else(
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
        // Names that make the same ID, such as "Digits" and "digits ", are
        // one name.
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
        let (reference, pictures) = self
            .with_session(|session| check_spec::to_saved_reference(&spec.check, session, &id, name))
            .map_err(|error| error.to_string())?;
        match file.references.iter_mut().find(|saved| saved.id == id) {
            Some(saved) => *saved = reference,
            None => file.references.push(reference),
        }
        self.stores()?
            .apps
            .save(&app, &file, &pictures)
            .map_err(|error| error.to_string())?;
        Ok(SavedReferenceId { app, reference: id })
    }

    /// Removes the reference `reference` of the application `app` and its
    /// pictures, unless a value reads with it.
    pub(super) fn delete_reference(&self, app: &str, reference: &str) -> Result<(), String> {
        let store = &self.stores()?.apps;
        let mut file = self.app_file(app)?;
        let Some(saved) = file.reference(reference) else {
            return Ok(());
        };
        let users: Vec<&str> = file
            .values
            .iter()
            .filter(|value| {
                matches!(
                    &value.screen.check,
                    appytappy_config::app_file::CheckDef::Number { reference: used, .. }
                        if used == reference
                )
            })
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
        store
            .save(app, &file, &[])
            .and_then(|()| store.prune(app, &file))
            .map_err(|error| error.to_string())
    }
}

/// `names` as a list in a sentence, such as `A, B and C`.
fn and_list(names: &[&str]) -> String {
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => (*last).to_owned(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::app_file::CheckDef;

    use crate::owner::messages::{Request, SavedValue};
    use crate::owner::test_support::{
        add_example_app, frame_image, harness_with, scratch_store, spec,
    };

    use super::*;

    /// A reference named `name` to save to the application `app`, teaching
    /// that the red square on screenshot 0 is a 1. With `editing`, the
    /// application and ID of a saved reference, it adds to that reference.
    fn reference(app: &str, name: &str, editing: Option<(&str, &str)>) -> Box<ReferenceSaveSpec> {
        let editing = editing.map_or_else(
            || "null".to_owned(),
            |(owner, id)| format!(r#"{{"app":"{owner}","id":"{id}"}}"#),
        );
        Box::new(ReferenceSaveSpec {
            app: app.to_owned(),
            reference_name: name.to_owned(),
            check: serde_json::from_str(&format!(
                r#"{{"kind":"characters","region":{{"x":0,"y":0,"width":2,"height":2}},"screenshot":0,
                    "reference":{editing},"ink":{{"red":255,"green":0,"blue":0}},
                    "tolerance":0,"labels":[{{"screenshot":0,"text":"1"}}]}}"#
            ))
            .expect("valid JSON"),
        })
    }

    const NUMBER: &str = r#"{"kind":"number","region":{"x":0,"y":0,"width":2,"height":2},
        "reference":{"app":"example","id":"digits"},"whole":true}"#;

    #[test]
    fn a_reference_is_saved_read_with_and_deleted_once_unused() {
        let (stores, root) = scratch_store("references");
        let apps_root = root.join("apps");
        let mut harness = harness_with(false, Ok(stores));
        add_example_app(&mut harness);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        let save = |spec| move |reply| Request::SaveReference { spec, reply };
        let delete = |reference: &str| {
            let reference = reference.to_owned();
            move |reply| Request::DeleteReference {
                app: "example".to_owned(),
                reference,
                reply,
            }
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
        let apps = harness.ask(Request::ListApps).expect("lists");
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
            harness.ask(|reply| Request::SaveValue {
                spec: Box::new(spec("example", "Progress", NUMBER)),
                reply
            }),
            Ok(SavedValue {
                app: "example".to_owned(),
                value: "progress".to_owned()
            })
        );
        let apps = harness.ask(Request::ListApps).expect("lists");
        let file = apps[0].file.as_ref().expect("a good file");
        assert_eq!(
            file.values[0].screen.check,
            CheckDef::Number {
                reference: "digits".to_owned(),
                whole: true
            }
        );
        harness
            .ask(|reply| Request::SaveValue {
                spec: Box::new(spec("example", "Best", NUMBER)),
                reply,
            })
            .expect("a second value");
        assert_eq!(
            harness.ask(delete("digits")),
            Err("Progress and Best read with Red digits. Change or remove them first.".to_owned())
        );
        harness
            .ask(|reply| Request::DeleteValue {
                app: "example".to_owned(),
                value: "best".to_owned(),
                reply,
            })
            .expect("removes the second value");
        assert_eq!(
            harness.ask(delete("digits")),
            Err("Progress reads with Red digits. Change or remove it first.".to_owned())
        );
        harness
            .ask(|reply| Request::DeleteValue {
                app: "example".to_owned(),
                value: "progress".to_owned(),
                reply,
            })
            .expect("removes the value");
        assert_eq!(harness.ask(delete("digits")), Ok(()));
        assert_eq!(harness.ask(delete("digits")), Ok(()), "already gone");
        assert!(!folder.join("ref-digits-1.png").exists());
        let apps = harness.ask(Request::ListApps).expect("lists");
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
    fn a_saved_reference_missing_a_picture_says_how_to_recover() {
        let (stores, root) = scratch_store("references-missing");
        let mut harness = harness_with(false, Ok(stores));
        add_example_app(&mut harness);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        harness
            .ask(|reply| Request::SaveReference {
                spec: reference("example", "Digits", None),
                reply,
            })
            .expect("saves");
        std::fs::remove_file(root.join("apps").join("example").join("ref-digits-1.png"))
            .expect("remove the picture");
        assert_eq!(
            harness.ask(|reply| Request::SetCheck {
                spec: serde_json::from_str(NUMBER).expect("valid JSON"),
                reply
            }),
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
