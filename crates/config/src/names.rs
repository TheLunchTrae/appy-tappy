//! The names AppyTappy gives things on disk: the IDs of applications,
//! values, references, and automations, and the file names of their
//! pictures.
//!
//! An ID names a folder or part of a picture's file name, and a picture's
//! kind shows in its name alone, so the rules live together: the length
//! budgets are checked against the names they have to fit in when this
//! module compiles, and no two kinds of picture can share a name.

use crate::app_file::{MAX_FIXES, MAX_STATES};
use crate::signal::{MAX_SIGNAL_NAME_LEN, SignalName};

/// The longest picture file name, in bytes.
const MAX_IMAGE_NAME_LEN: usize = 64;

/// The longest application ID, in bytes.
const MAX_APP_ID_LEN: usize = 48;

/// The longest ID [`value_id`] makes, in bytes. A value's ID is a signal
/// name and names its states' pictures.
const MAX_VALUE_ID_LEN: usize = 48;

/// The longest ID [`reference_id`] makes, in bytes. A reference's ID names
/// its samples' pictures.
const MAX_REFERENCE_ID_LEN: usize = 40;

/// The longest ID [`new_automation_id`] makes from a name alone, in bytes.
/// One it numbers may reach [`MAX_SIGNAL_NAME_LEN`], which the setup app
/// holds an automation's ID to.
const MAX_AUTOMATION_ID_LEN: usize = 44;

/// What [`state_image_name`] adds to a value's ID at its longest: a set and
/// a state of two digits each. The sets number up to two past the fixes: one
/// for each fix, one for the drawing, and one more for the drawing once an
/// edit takes its states again while every fix is in use.
const LONGEST_STATE_SUFFIX: &str = "-99-99.png";

/// What [`sample_name`] adds to a reference's ID at its longest, around the
/// ID: a sample number of nine digits.
const LONGEST_SAMPLE_AFFIXES: &str = "ref--999999999.png";

// Each ID leaves room for the longest picture name made from it, and each
// ID made from a name is a signal name.
const _: () = assert!(MAX_FIXES <= 97);
const _: () = assert!(MAX_STATES <= 99);
const _: () = assert!(MAX_VALUE_ID_LEN + LONGEST_STATE_SUFFIX.len() <= MAX_IMAGE_NAME_LEN);
const _: () = assert!(MAX_REFERENCE_ID_LEN + LONGEST_SAMPLE_AFFIXES.len() <= MAX_IMAGE_NAME_LEN);
const _: () = assert!(MAX_VALUE_ID_LEN <= MAX_SIGNAL_NAME_LEN);
const _: () = assert!(MAX_REFERENCE_ID_LEN <= MAX_SIGNAL_NAME_LEN);
const _: () = assert!(MAX_AUTOMATION_ID_LEN <= MAX_SIGNAL_NAME_LEN);

/// Whether `id` can name an application's folder.
///
/// That is lowercase ASCII letters, digits, hyphens, and underscores,
/// starting with a letter or digit, so it is the same folder on every file
/// system and never leaves the store.
#[must_use]
pub fn is_app_id(id: &str) -> bool {
    id.len() <= MAX_APP_ID_LEN
        && id
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && id.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
                || character == '_'
        })
}

/// Whether `name` is a picture file name a folder can hold.
///
/// That is lowercase ASCII letters, digits, hyphens, and underscores, then
/// `.png`. No separator or dot can appear before the extension, so the name never
/// leaves its folder.
#[must_use]
pub fn is_image_name(name: &str) -> bool {
    name.len() <= MAX_IMAGE_NAME_LEN
        && name.strip_suffix(".png").is_some_and(|stem| {
            !stem.is_empty()
                && stem.chars().all(|character| {
                    character.is_ascii_lowercase()
                        || character.is_ascii_digit()
                        || character == '-'
                        || character == '_'
                })
        })
}

/// An application ID made from an executable's name that no folder in
/// `taken` uses yet, such as `example` from `Example.exe`, or
/// `example-2` when that is taken.
#[must_use]
pub fn new_app_id(executable: &str, taken: &[&str]) -> String {
    let stem = executable
        .rsplit_once('.')
        .map_or(executable, |(stem, _extension)| stem);
    let mut base: String = stem
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || *character == '-'
        })
        .collect();
    base.truncate(MAX_APP_ID_LEN.saturating_sub(NUMBER_ROOM));
    let base = base.trim_start_matches('-');
    let base = if base.is_empty() { "app" } else { base };
    free_id(base.to_owned(), '-', MAX_APP_ID_LEN, taken)
}

/// A value ID made from a name the user typed, such as `progress` from
/// `Progress`.
///
/// It keeps lowercase ASCII letters and digits, with an underscore for each
/// run of anything else. A name with no letter or digit, or
/// one that starts with a digit, gets a `value` prefix so the ID is valid.
#[must_use]
pub fn value_id(name: &str) -> String {
    id_from_name(name, "value", MAX_VALUE_ID_LEN)
}

/// The ID for a new value named `name` that no ID in `taken` uses yet:
/// [`value_id`] of the name, or that with `_2`, `_3` and so on when a renamed
/// value still has it.
#[must_use]
pub fn new_value_id(name: &str, taken: &[&str]) -> String {
    free_id(value_id(name), '_', MAX_VALUE_ID_LEN, taken)
}

/// A reference ID made from a name the user typed, such as `percent_font`
/// from `Percent font`, the way [`value_id`] makes one, with a `reference`
/// prefix where it needs one.
#[must_use]
pub fn reference_id(name: &str) -> String {
    id_from_name(name, "reference", MAX_REFERENCE_ID_LEN)
}

/// The ID for a new reference named `name` that no ID in `taken` uses yet:
/// [`reference_id`] of the name, or that with `_2`, `_3` and so on when a
/// renamed reference still has it.
#[must_use]
pub fn new_reference_id(name: &str, taken: &[&str]) -> String {
    free_id(reference_id(name), '_', MAX_REFERENCE_ID_LEN, taken)
}

/// An automation ID made from a name the user typed that `taken` does not
/// hold yet, such as `pause_music` from `Pause music`, or `pause_music_2`
/// when that is taken.
#[must_use]
pub fn new_automation_id(name: &str, taken: &[&str]) -> String {
    free_id(
        id_from_name(name, "automation", MAX_AUTOMATION_ID_LEN),
        '_',
        MAX_SIGNAL_NAME_LEN,
        taken,
    )
}

/// How many bytes [`free_id`] keeps free for a separator and a number.
const NUMBER_ROOM: usize = 4;

/// `id`, or `id` with `separator` and 2, 3 and so on, cut to fit `max_len`
/// bytes, whichever comes first that no ID in `taken` uses.
fn free_id(id: String, separator: char, max_len: usize, taken: &[&str]) -> String {
    if !taken.contains(&id.as_str()) {
        return id;
    }
    let mut base = id;
    base.truncate(max_len.saturating_sub(NUMBER_ROOM));
    let base = base.trim_end_matches('_');
    // `taken` is finite, so one of its length plus one numbers is free.
    (2_usize..=taken.len().saturating_add(2))
        .map(|number| format!("{base}{separator}{number}"))
        .find(|candidate| candidate.len() <= max_len && !taken.contains(&candidate.as_str()))
        .unwrap_or_else(|| base.to_owned())
}

/// An ID made from a name the user typed, as [`value_id`] describes, with
/// `prefix` in front when the name does not start with a letter, and at most
/// `max_len` bytes long.
fn id_from_name(name: &str, prefix: &str, max_len: usize) -> String {
    let mut id = String::new();
    for character in name.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            id.push(character);
        } else if !id.is_empty() && !id.ends_with('_') {
            id.push('_');
        }
    }
    while id.ends_with('_') {
        id.pop();
    }
    if !id.starts_with(|first: char| first.is_ascii_lowercase()) {
        id.insert_str(
            0,
            &if id.is_empty() {
                prefix.to_owned()
            } else {
                format!("{prefix}_")
            },
        );
    }
    id.truncate(max_len);
    while id.ends_with('_') {
        id.pop();
    }
    id
}

/// The file name of a reference's `number`th sample, such as
/// `ref-percent_font-3.png`.
///
/// No value's picture can have it: [`state_image_name`] puts a number where
/// this puts the reference's ID, which starts with a letter.
#[must_use]
pub fn sample_name(reference: &str, number: usize) -> String {
    format!("ref-{reference}-{number}.png")
}

/// The file name of the picture of a value's `state`th state, counted from
/// 1.
///
/// It is `{value}-{state}.png` for a new value's drawing, such as
/// `status-2.png`, and `{value}-{set}-{state}.png` for its picture in the
/// set numbered `set`, such as `status-1-2.png`: a fix's pictures, or the
/// drawing's once an edit took its states again. A value's ID has no hyphen,
/// so neither name is another value's.
#[must_use]
pub fn state_image_name(value: &str, set: Option<usize>, state: usize) -> String {
    match set {
        Some(set) => format!("{value}-{set}-{state}.png"),
        None => format!("{value}-{state}.png"),
    }
}

/// Whether `name` is a file name AppyTappy itself gives a picture in an
/// application's folder.
///
/// That is a reference's sample as [`sample_name`] names it, or a state's
/// picture as [`state_image_name`] names it, for a first drawing or a fix.
/// A file with any other name, such as one the user put in the folder, is
/// not AppyTappy's to remove.
#[must_use]
pub(crate) fn is_own_image_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".png").filter(|_| is_image_name(name)) else {
        return false;
    };
    let id = |part: &str| SignalName::new(part).is_ok();
    let mut parts = stem.split('-');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(value), Some(state), None, None) => id(value) && is_number(state),
        // A sample's middle part is an ID, which starts with a letter, and a
        // fix's is a number, so a name is one or the other.
        (Some(first), Some(middle), Some(last), None) => {
            is_number(last) && ((first == "ref" && id(middle)) || (id(first) && is_number(middle)))
        },
        _ => false,
    }
}

/// The file name of the click picture numbered `number`, such as
/// `click-3.png`. Click pictures have a folder of their own, so the name
/// only has to tell them from files the user put there.
#[must_use]
pub(crate) fn click_picture_name(number: u32) -> String {
    format!("click-{number}.png")
}

/// The number in a name [`click_picture_name`] makes, such as 3 in
/// `click-3.png`, or `None` for any other name.
pub(crate) fn click_picture_number(name: &str) -> Option<u32> {
    let number = name.strip_prefix("click-")?.strip_suffix(".png")?;
    is_number(number).then(|| number.parse().ok()).flatten()
}

/// Whether `part` is one or more ASCII digits.
fn is_number(part: &str) -> bool {
    !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picture_names_stay_inside_the_folder() {
        for name in ["a.png", "progress-1.png", "x9.png"] {
            assert!(is_image_name(name), "{name}");
        }
        let long = format!("{}.png", "a".repeat(61));
        let longest = format!("{}.png", "a".repeat(60));
        assert!(is_image_name(&longest));
        for name in [
            ".png",
            "a.PNG",
            "A.png",
            "a.jpg",
            "a b.png",
            "../a.png",
            "a/b.png",
            "a.b.png",
            long.as_str(),
        ] {
            assert!(!is_image_name(name), "{name}");
        }
    }

    #[test]
    fn only_the_names_appytappy_gives_pictures_are_its_own() {
        for name in [
            "ref-percent_font-3.png",
            "status-1.png",
            "status-2-1.png",
            // A value named `ref`, and a fix of it.
            "ref-1.png",
            "ref-1-2.png",
        ] {
            assert!(is_own_image_name(name), "{name}");
        }
        let long = format!("a-{}.png", "1".repeat(61));
        for name in [
            "photo.png",
            "icon_visible.png",
            "holiday-snap.png",
            "holiday-snap-1.png",
            "status-2-x.png",
            "status-1-2-3.png",
            "ref-9x-1.png",
            "1-2.png",
            "9-1-1.png",
            "status-.png",
            "-1.png",
            "Status-1.png",
            "status-1.PNG",
            "status-1.jpg",
            "status-1",
            "status-1.png.bak",
            long.as_str(),
        ] {
            assert!(!is_own_image_name(name), "{name}");
        }
    }

    #[test]
    fn value_ids_come_from_the_names_users_type() {
        let cases = [
            ("Progress", "progress"),
            ("  Icon visible? ", "icon_visible"),
            ("HP / max HP", "hp_max_hp"),
            ("2 steps", "value_2_steps"),
            ("!!!", "value"),
            ("", "value"),
            ("Ünïcode name", "n_code_name"),
        ];
        for (name, id) in cases {
            assert_eq!(value_id(name), id, "{name:?}");
            assert!(SignalName::new(&value_id(name)).is_ok());
        }
        // A cut that ends on an underscore drops it.
        assert_eq!(value_id(&"ab ".repeat(40)).len(), MAX_VALUE_ID_LEN - 1);
        let long = value_id(&"abc".repeat(30));
        assert_eq!(long.len(), MAX_VALUE_ID_LEN);
        assert!(SignalName::new(&long).is_ok());
        // The last state's picture named after the longest ID still fits.
        assert!(is_image_name(&format!("{long}-{MAX_STATES}.png")));
        // A renamed value keeps its ID, so a new value named like its old
        // name takes that ID with a number, which still fits a fix's
        // pictures.
        assert_eq!(new_value_id("Progress", &["other"]), "progress");
        assert_eq!(
            new_value_id("Progress", &["progress", "progress_2"]),
            "progress_3"
        );
        let numbered = new_value_id(&"abc".repeat(30), &[long.as_str()]);
        assert_eq!(numbered.len(), MAX_VALUE_ID_LEN.saturating_sub(2));
        assert!(numbered.ends_with("_2"));
        assert!(SignalName::new(&numbered).is_ok());
        assert!(is_image_name(&state_image_name(
            &long,
            Some(MAX_FIXES.saturating_add(2)),
            MAX_STATES
        )));
    }

    #[test]
    fn reference_ids_and_their_sample_names_fit() {
        assert_eq!(reference_id("Percent font"), "percent_font");
        assert_eq!(reference_id("0-9"), "reference_0_9");
        let long = reference_id(&"abc".repeat(30));
        assert_eq!(long.len(), MAX_REFERENCE_ID_LEN);
        assert!(SignalName::new(&long).is_ok());
        assert_eq!(sample_name("percent_font", 3), "ref-percent_font-3.png");
        assert_eq!(state_image_name("status", None, 2), "status-2.png");
        assert_eq!(state_image_name("status", Some(1), 2), "status-1-2.png");
        assert_eq!(new_reference_id("Digits", &["other"]), "digits");
        assert_eq!(
            new_reference_id("Digits", &["digits", "digits_2"]),
            "digits_3"
        );
        let numbered = new_reference_id(&"abc".repeat(30), &[long.as_str()]);
        assert_eq!(numbered.len(), MAX_REFERENCE_ID_LEN.saturating_sub(2));
        assert!(numbered.ends_with("_2"));
        assert!(SignalName::new(&numbered).is_ok());
        assert!(is_image_name(&sample_name(&long, 999_999_999)));
    }

    #[test]
    fn app_ids_are_plain_folder_names() {
        for id in ["example", "app64", "my-app_2", "9lives"] {
            assert!(is_app_id(id), "{id}");
        }
        let longest = "a".repeat(48);
        assert!(is_app_id(&longest));
        let long = "a".repeat(49);
        for id in [
            "",
            "-a",
            "_a",
            "A",
            "a b",
            "a/b",
            "..",
            "a.b",
            long.as_str(),
        ] {
            assert!(!is_app_id(id), "{id:?}");
        }
    }

    #[test]
    fn new_app_ids_come_from_the_executable_and_avoid_taken_ones() {
        assert_eq!(new_app_id("Example.exe", &[]), "example");
        assert_eq!(new_app_id("Example.exe", &["example"]), "example-2");
        assert_eq!(
            new_app_id("Example.exe", &["example", "example-2"]),
            "example-3"
        );
        assert_eq!(new_app_id("My-Example-App64.exe", &[]), "my-example-app64");
        assert_eq!(new_app_id("app64", &[]), "app64");
        assert_eq!(new_app_id("--.exe", &[]), "app");
        assert_eq!(new_app_id("プレイ.exe", &[]), "app");
        let long = new_app_id(&format!("{}.exe", "a".repeat(80)), &[]);
        assert_eq!(long.len(), 44);
        assert!(is_app_id(&long));
        let numbered = new_app_id(&format!("{}.exe", "a".repeat(80)), &[long.as_str()]);
        assert_eq!(numbered, format!("{long}-2"));
        assert!(is_app_id(&numbered));
    }

    #[test]
    fn new_automation_ids_come_from_the_name_and_avoid_taken_ones() {
        assert_eq!(new_automation_id("Pause music", &[]), "pause_music");
        assert_eq!(
            new_automation_id("Pause music", &["pause_music"]),
            "pause_music_2"
        );
        assert_eq!(
            new_automation_id("Pause music", &["pause_music", "pause_music_2"]),
            "pause_music_3"
        );
        assert_eq!(new_automation_id("", &[]), "automation");
        assert_eq!(new_automation_id("!!", &["automation"]), "automation_2");
        assert_eq!(new_automation_id("2 steps", &[]), "automation_2_steps");
        assert_eq!(new_automation_id("Value check", &[]), "value_check");
        assert_eq!(new_automation_id("Éclair", &[]), "clair");
        let long = new_automation_id(&"a".repeat(80), &[]);
        assert_eq!(long.len(), MAX_AUTOMATION_ID_LEN);
        let underscore = format!("{} b", "a".repeat(MAX_AUTOMATION_ID_LEN.saturating_sub(1)));
        assert!(!new_automation_id(&underscore, &[]).ends_with('_'));
        for id in [
            long,
            new_automation_id(&"a".repeat(80), &[&"a".repeat(44)]),
            new_automation_id("2 steps", &[]),
        ] {
            assert!(SignalName::new(&id).is_ok(), "{id}");
        }
    }

    #[test]
    fn click_picture_names_carry_their_number() {
        assert_eq!(click_picture_name(3), "click-3.png");
        assert_eq!(
            click_picture_number(&click_picture_name(u32::MAX)),
            Some(u32::MAX)
        );
        for name in [
            "click-.png",
            "click-+3.png",
            "click-3.jpg",
            "3.png",
            "click-4294967296.png",
        ] {
            assert_eq!(click_picture_number(name), None, "{name}");
        }
    }
}
