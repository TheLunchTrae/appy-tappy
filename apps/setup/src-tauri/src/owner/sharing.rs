//! Exporting a saved application as one file, and importing such a file as
//! a new application.
//!
//! An import never replaces an application: it takes a new ID and a name no
//! saved application has. The user picks its anti-cheat class, so the class
//! in the file sets nothing, and the import brings no automation, so nothing
//! reads its values for an action until the user makes one.

use appytappy_config::app_file::{AntiCheatDef, AppFile, OriginDef};
use serde::{Deserialize, Serialize};

use crate::applications;
use crate::sharing::{ShareError, Shared};

use super::host::Host;
use super::{Changed, Owner};

/// The longest file name an import keeps as where it came from, in
/// characters.
const MAX_FROM_CHARS: usize = 60;

/// A shared file as the import dialog shows it, before anything is saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct SharedView {
    /// The application's file as the shared file holds it, with the
    /// anti-cheat class its maker set.
    pub(crate) file: AppFile,
    /// The name to offer: the file's, or that with a number when a saved
    /// application has it already.
    pub(crate) name: String,
}

/// What the user chose for an import.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ImportSpec {
    /// The application's name.
    pub(crate) name: String,
    /// The anti-cheat class the user confirmed, which is saved whatever the
    /// file holds.
    pub(crate) anti_cheat: AntiCheatDef,
    /// The name of the file the bytes came from, kept as where the
    /// application was imported from.
    pub(crate) from: String,
}

impl<H: Host> Owner<H> {
    /// The saved application `app` as the bytes of a file to share.
    pub(crate) fn export_app(&self, app: &str) -> Result<Vec<u8>, String> {
        let saved = self.stores()?.app(app)?;
        Shared::of(&saved)
            .and_then(|shared| shared.pack())
            .map_err(|error| cannot_export(&saved.file.name, &error))
    }

    /// What the shared file `bytes`, named `from`, holds, for the user to
    /// look over before it is imported. Nothing is saved.
    pub(crate) fn read_shared(&self, bytes: &[u8], from: &str) -> Result<SharedView, String> {
        let stores = self.stores()?;
        let file = unpack(bytes, &file_label(from))?.file().clone();
        let saved = stores.apps.load().map_err(|error| error.to_string())?;
        let name = applications::free_name(&saved, &file.name);
        Ok(SharedView { file, name })
    }

    /// Saves the shared file `bytes` as a new application, as `spec` names
    /// and classes it, and answers its ID.
    pub(crate) fn import_app(&mut self, bytes: &[u8], spec: ImportSpec) -> Result<String, String> {
        let from = file_label(&spec.from);
        let (mut file, pictures) = unpack(bytes, &from)?.into_parts();
        let name = spec.name.trim();
        if name.is_empty() {
            return Err("Name the application first.".to_owned());
        }
        let saved = self
            .stores()?
            .apps
            .load()
            .map_err(|error| error.to_string())?;
        applications::check_name_free(&saved, None, name)?;
        name.clone_into(&mut file.name);
        file.anti_cheat = spec.anti_cheat;
        let origins = file
            .values
            .iter_mut()
            .map(|value| &mut value.origin)
            .chain(file.references.iter_mut().map(|saved| &mut saved.origin));
        for origin in origins {
            *origin = OriginDef::Imported { file: from.clone() };
        }
        let id = self
            .stores()?
            .apps
            .create(&file, &pictures)
            .map_err(|error| {
                format!("AppyTappy cannot import {from}: it could not be saved ({error}).")
            })?;
        self.tell(&id, Changed::Values);
        Ok(id)
    }
}

/// Why the application named `name` cannot be exported, as the user reads
/// it, with what to change when the cause is the user's to change.
fn cannot_export(name: &str, error: &ShareError) -> String {
    let change = match error {
        ShareError::TooManyPixels => {
            " Draw smaller regions or fewer states or fixes, then export it again."
        },
        ShareError::DecodesTooMuch => {
            " Teach its references with smaller pictures, then export it again."
        },
        _ => "",
    };
    format!("AppyTappy cannot export {name}: {error}.{change}")
}

/// The shared file `bytes`, or why the file named `from` cannot be
/// imported, as the user reads it.
fn unpack(bytes: &[u8], from: &str) -> Result<Shared, String> {
    Shared::unpack(bytes)
        .map_err(|error: ShareError| format!("AppyTappy cannot import {from}: {error}."))
}

/// Whether `character` shows nothing itself and changes how the text
/// around it shows: a control character, or one of the format characters
/// that reorder or hide text, such as U+202E, which turns the text after
/// it around so that `piz.exe` can read as `exe.zip`, and the tag
/// characters, which spell text that does not show. Not every format
/// character: the ones a script writes its numbers with show.
const fn is_unseen(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{ad}'
                | '\u{61c}'
                | '\u{180e}'
                | '\u{200b}'..='\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2060}'..='\u{206f}'
                | '\u{feff}'
                | '\u{fff9}'..='\u{fffb}'
                | '\u{e0000}'..='\u{e007f}'
        )
}

/// The file name `name` as it is shown and kept: without the characters
/// [`is_unseen`] names or the space around it, and no longer than
/// [`MAX_FROM_CHARS`]. A name with nothing to show reads as `the file`.
fn file_label(name: &str) -> String {
    let shown: String = name
        .chars()
        .filter(|character| !is_unseen(*character))
        .collect();
    let shown: String = shown.trim().chars().take(MAX_FROM_CHARS).collect();
    if shown.trim().is_empty() {
        "the file".to_owned()
    } else {
        shown.trim().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::app_file::OverlayModeDef;
    use appytappy_sources::screen::{Image, Size};

    use crate::check_spec::ColorSpec;
    use crate::owner::ReferenceSaveSpec;
    use crate::owner::test_support::{
        BAR, FakeHost, Harness, STATE, capturing, characters_in, frame_image, spec,
    };
    use crate::stores::SavedApp;

    use super::*;

    /// A harness whose `Example App` has a state with a picture and a bar,
    /// reads with no anti-cheat, and shows its overlay.
    fn with_values(name: &str) -> (Harness, std::path::PathBuf) {
        let (mut harness, _handler, root) = capturing(name);
        for (value, check) in [("Icon visible", STATE), ("Progress bar", BAR)] {
            harness
                .ask(|owner| owner.save(&spec("example", value, check)))
                .expect("saves");
        }
        harness
            .ask(|owner| owner.set_anti_cheat("example", AntiCheatDef::None))
            .expect("sets the class");
        harness
            .ask(|owner| owner.set_overlay("example", true, OverlayModeDef::PictureInPicture))
            .expect("turns the overlay on");
        (harness, root)
    }

    fn saved(harness: &mut Harness, app: &str) -> SavedApp {
        let app = app.to_owned();
        harness
            .ask(move |owner| owner.stores()?.app(&app))
            .expect("saved")
    }

    fn import(
        bytes: &[u8],
        name: &str,
        anti_cheat: AntiCheatDef,
    ) -> impl FnOnce(&mut Owner<FakeHost>) -> Result<String, String> {
        let spec = ImportSpec {
            name: name.to_owned(),
            anti_cheat,
            from: " friend's\u{7}.zip ".to_owned(),
        };
        let bytes = bytes.to_vec();
        move |owner| owner.import_app(&bytes, spec)
    }

    #[test]
    fn an_exported_application_imports_as_a_new_one_with_the_class_the_user_picked() {
        let (mut harness, root) = with_values("share-round-trip");
        harness
            .ask(|owner| owner.save_reference(&reference("Digits", None, LABEL, "")))
            .expect("saves");
        let exported = saved(&mut harness, "example");
        assert!(exported.file.checked.is_some(), "the build is recorded");
        let bytes = harness
            .ask(|owner| owner.export_app("example"))
            .expect("exports");
        let view = harness
            .ask(|owner| owner.read_shared(&bytes, "friend.zip"))
            .expect("reads");
        assert_eq!(view.name, "Example App 2", "its own name is taken");
        assert_eq!(
            view.file.anti_cheat,
            AntiCheatDef::None,
            "shown as its maker set it"
        );
        assert_eq!(view.file.overlay, None);
        harness.forget_reloads();
        assert_eq!(
            harness.ask(import(&bytes, " Friend's app ", AntiCheatDef::Unknown)),
            Ok("example-2".to_owned())
        );
        assert_eq!(harness.reloads(), 1, "the runtime reads it");
        assert_eq!(
            saved(&mut harness, "example"),
            exported,
            "the application it came from is as it was"
        );
        let imported = saved(&mut harness, "example-2");
        let mut expected = exported.file.clone();
        "Friend's app".clone_into(&mut expected.name);
        expected.anti_cheat = AntiCheatDef::Unknown;
        expected.overlay = None;
        let from = OriginDef::Imported {
            file: "friend's.zip".to_owned(),
        };
        assert!(!expected.values.is_empty() && !expected.references.is_empty());
        for value in &mut expected.values {
            value.origin = from.clone();
        }
        for reference in &mut expected.references {
            reference.origin = from.clone();
        }
        assert_eq!(imported.file, expected);
        for picture in exported.file.images() {
            assert_eq!(
                imported.picture(picture).expect("its picture"),
                exported.picture(picture).expect("the picture"),
            );
        }
        let again = harness
            .ask(|owner| owner.export_app("example-2"))
            .expect("exports");
        let again = harness
            .ask(|owner| owner.read_shared(&again, "again.zip"))
            .expect("reads")
            .file;
        let origins: Vec<&OriginDef> = again
            .values
            .iter()
            .map(|value| &value.origin)
            .chain(again.references.iter().map(|reference| &reference.origin))
            .collect();
        assert_eq!(
            origins,
            vec![
                &OriginDef::Imported {
                    file: String::new()
                };
                origins.len()
            ],
            "the name of the user's own file stays with them"
        );
        assert_eq!(
            harness.ask(import(&bytes, "friend's APP", AntiCheatDef::Unknown)),
            Err(
                "Another application is already called friend's APP. Pick another name or edit \
                 Friend's app."
                    .to_owned()
            )
        );
        assert_eq!(
            harness.ask(import(&bytes, " ", AntiCheatDef::Unknown)),
            Err("Name the application first.".to_owned())
        );
        assert_eq!(
            harness.ask(import(b"text", "Other", AntiCheatDef::Unknown)),
            Err(
                "AppyTappy cannot import friend's.zip: it is not a zip file AppyTappy can read."
                    .to_owned()
            )
        );
        // Things the store does not list take every name a new folder tries.
        let apps = exported.folder.parent().expect("the applications folder");
        for number in 3..=18 {
            std::fs::write(apps.join(format!("example-{number}")), "").expect("writes");
        }
        assert_eq!(
            harness.ask(import(&bytes, "Other", AntiCheatDef::Unknown)),
            Err(
                "AppyTappy cannot import friend's.zip: it could not be saved (the 16 names it \
                 tries for the folder of Example.exe are taken)."
                    .to_owned()
            )
        );
        let ids: Vec<String> = harness
            .ask(|owner| {
                owner
                    .stores()?
                    .apps
                    .load()
                    .map_err(|error| error.to_string())
            })
            .expect("lists")
            .into_iter()
            .map(|app| app.id)
            .collect();
        assert_eq!(
            ids,
            ["example", "example-2"],
            "a refused import saves nothing"
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn an_application_that_could_not_be_imported_is_not_exported() {
        let (mut harness, root) = with_values("share-missing");
        let app = saved(&mut harness, "example");
        let kept = app.picture("icon_visible-1.png").expect("its picture");
        std::fs::remove_file(app.folder.join("icon_visible-1.png")).expect("removes the picture");
        assert_eq!(
            harness.ask(|owner| owner.export_app("example")),
            Err(
                "AppyTappy cannot export Example App: the picture icon_visible-1.png is missing."
                    .to_owned()
            )
        );
        // Larger than an import reads, though each value still reads.
        let other = app.folder.join("icon_visible-1.png");
        let mut large = kept;
        large.resize(crate::sharing::MAX_SHARED_BYTES, 0);
        std::fs::write(&other, large).expect("writes the picture");
        assert_eq!(
            harness.ask(|owner| owner.export_app("example")),
            Err("AppyTappy cannot export Example App: it holds more than 16 MB.".to_owned())
        );
        assert_eq!(
            harness.ask(|owner| owner.export_app("gone")),
            Err("That application is no longer saved.".to_owned())
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn an_export_too_large_to_read_back_says_what_to_change() {
        assert_eq!(
            cannot_export("Example App", &ShareError::TooManyPixels),
            "AppyTappy cannot export Example App: its pictures together hold more pixels than \
             one 7680x4320 picture. Draw smaller regions or fewer states or fixes, then export it again."
        );
        assert_eq!(
            cannot_export("Example App", &ShareError::DecodesTooMuch),
            "AppyTappy cannot export Example App: its values read their references' pictures \
             so many times that reading them would decode more pixels than four 7680x4320 \
             pictures. Teach its references with smaller pictures, then export it again."
        );
        assert_eq!(
            cannot_export("Example App", &ShareError::NoAppFile),
            "AppyTappy cannot export Example App: it holds no app.json.",
            "nothing to change"
        );
    }

    #[test]
    fn a_file_name_is_kept_without_what_cannot_be_shown() {
        assert_eq!(file_label(" example.zip\n"), "example.zip");
        assert_eq!(file_label("\u{1b}[2J \t"), "[2J");
        assert_eq!(file_label(" \u{0} "), "the file");
        assert_eq!(
            file_label("appytappy-\u{202e}piz\u{e0041}\u{e007f}.exe\u{200b}\u{feff}"),
            "appytappy-piz.exe",
            "as it is spelled, not turned around"
        );
        let long = "é".repeat(MAX_FROM_CHARS.saturating_add(5));
        assert_eq!(file_label(&long), "é".repeat(MAX_FROM_CHARS));
    }

    /// The reference `name` of `Example App`, new or the saved one
    /// `editing`, taught the whole 4 by 2 frame's red by `labels` and the
    /// recorded readings `recorded`, in the UI's JSON.
    fn reference(
        name: &str,
        editing: Option<&str>,
        labels: &str,
        recorded: &str,
    ) -> ReferenceSaveSpec {
        let editing = editing.map_or_else(String::new, |id| {
            format!(r#""reference":{{"app":"example","id":"{id}"}},"#)
        });
        ReferenceSaveSpec {
            app: "example".to_owned(),
            reference_name: name.to_owned(),
            expects: String::new(),
            characters: serde_json::from_str(&format!(
                r#"{{"region":{{"x":0,"y":0,"width":4,"height":2}},"screenshot":0,{editing}
                    "ink":{{"red":255,"green":0,"blue":0}},"tolerance":0,
                    "labels":[{labels}],"recorded":[{recorded}]}}"#
            ))
            .expect("valid JSON"),
        }
    }

    const LABEL: &str = r#"{"screenshot":0,"text":"1"}"#;
    const READING: &str = r#"{"reading":0,"text":"1"}"#;

    #[test]
    fn a_reference_says_whether_a_recording_taught_it() {
        let (mut harness, mut handler, root) = capturing("share-origin");
        let origins = |harness: &mut Harness| -> Vec<(String, OriginDef)> {
            saved(harness, "example")
                .file
                .references
                .into_iter()
                .map(|reference| (reference.id, reference.origin))
                .collect()
        };
        let save = |harness: &mut Harness, spec: ReferenceSaveSpec| {
            harness
                .ask(|owner| owner.save_reference(&spec))
                .expect("saves");
        };
        save(&mut harness, reference("By hand", None, LABEL, ""));
        save(&mut harness, reference("Other", None, LABEL, ""));
        let region = serde_json::from_str(r#"{"x":0,"y":0,"width":4,"height":2}"#).expect("JSON");
        let ink: ColorSpec =
            serde_json::from_str(r#"{"red":255,"green":0,"blue":0}"#).expect("JSON");
        harness
            .ask(|owner| owner.start_recording(characters_in(region, ink), false))
            .expect("records");
        harness.deliver(&mut handler, Ok(&frame_image()));
        save(&mut harness, reference("Recorded", None, "", READING));
        assert_eq!(
            origins(&mut harness),
            [
                ("by_hand".to_owned(), OriginDef::ByHand),
                ("other".to_owned(), OriginDef::ByHand),
                ("recorded".to_owned(), OriginDef::Recording),
            ],
            "each says it of itself"
        );
        // A recording that adds to one made by hand makes it a recorded
        // one, and labels added by hand leave a recorded one recorded.
        save(
            &mut harness,
            reference("By hand", Some("by_hand"), "", READING),
        );
        save(
            &mut harness,
            reference("Recorded", Some("recorded"), LABEL, ""),
        );
        // Where one was imported from outlasts what is taught in it later.
        let mut file = saved(&mut harness, "example").file;
        let imported = OriginDef::Imported {
            file: "friend.zip".to_owned(),
        };
        file.references[1].origin = imported.clone();
        harness
            .ask(|owner| {
                owner
                    .stores()?
                    .apps
                    .save("example", &file, &[])
                    .map_err(|error| error.to_string())
            })
            .expect("saves");
        save(&mut harness, reference("Other", Some("other"), "", READING));
        assert_eq!(
            origins(&mut harness),
            [
                ("by_hand".to_owned(), OriginDef::Recording),
                ("other".to_owned(), imported),
                ("recorded".to_owned(), OriginDef::Recording),
            ]
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_label_on_a_screenshot_a_recording_of_looks_kept_counts_as_recorded() {
        let (mut harness, mut handler, root) = capturing("share-origin-looks");
        let region = serde_json::from_str(r#"{"x":2,"y":0,"width":2,"height":2}"#).expect("JSON");
        harness
            .ask(|owner| owner.start_look_recording(region, 0, 0))
            .expect("records");
        // The right half is red here and gray on screenshot 0, so the
        // recording keeps this frame as screenshot 1.
        let red = Image::new(
            Size {
                width: 4,
                height: 2,
            },
            [0, 0, 255, 0].repeat(8),
        )
        .expect("matching length");
        harness.deliver(&mut handler, Ok(&red));
        let label = r#"{"screenshot":1,"text":"1"}"#;
        harness
            .ask(|owner| owner.save_reference(&reference("Looks", None, label, "")))
            .expect("saves");
        assert_eq!(
            saved(&mut harness, "example").file.references[0].origin,
            OriginDef::Recording
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
