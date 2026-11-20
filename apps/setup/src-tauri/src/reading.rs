//! A value's reading as the UI shows it, and the words for a value that
//! is not read: at a window size, or because its check failed.

use appytappy_config::app_file::{CheckDef, ReferenceDef, ScreenDef};
use appytappy_config::signal::Value;
use appytappy_sources::screen::Size;
use appytappy_sources::screen::check::CheckError;
use appytappy_sources::screen::digits::ReadError;
use appytappy_sources::screen::saved::Unread;
use serde::Serialize;

/// A value the check read, as the UI shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub(crate) enum Reading {
    /// A number, such as a percentage.
    Number(f64),
    /// Yes or no, such as whether a color shows.
    Flag(bool),
    /// A state's name, such as `Paused`, or `none`.
    State(String),
}

impl From<Value> for Reading {
    fn from(value: Value) -> Self {
        match value {
            Value::Number(number) => Self::Number(number.raw()),
            Value::Flag(flag) => Self::Flag(flag),
            Value::State(state) => Self::State(state.as_str().to_owned()),
        }
    }
}

/// Where and why a value is not read on a window of `size`, as the end of
/// a sentence that says it is not read. The live check, the automation
/// editor's live reading, and the overlays all say it in these words.
pub(crate) fn at_size(size: Size, unread: Unread) -> String {
    format!(
        "at {} × {}, the window's size now: {unread}",
        size.width, size.height
    )
}

/// The name of the reference `screen` reads its number with, or `None`
/// for a check that reads no number, a reference `references` lacks, and
/// a value with a fix that reads with a reference of its own, since a
/// failed read does not say which of the two read.
pub(crate) fn reference_of<'file>(
    screen: &ScreenDef,
    references: &'file [ReferenceDef],
) -> Option<&'file str> {
    let CheckDef::Number { reference, .. } = &screen.check else {
        return None;
    };
    if screen.fixes.iter().any(|fix| fix.reference.is_some()) {
        return None;
    }
    references
        .iter()
        .find(|saved| saved.id == *reference)
        .map(|saved| saved.name.as_str())
}

/// Why `error` kept a check from reading its region, in the setup app's
/// words: the end of a sentence that says the value can't be read, with
/// its full stop. `reference` names the reference a number reads with,
/// where the caller knows it. The runtime's log says the same in the
/// check's own words, which stay the same from read to read.
pub(crate) fn cant_read(error: &CheckError, reference: Option<&str>) -> String {
    let reference = reference.unwrap_or("its reference");
    match error {
        CheckError::NoBar => {
            "nothing in its region is near the bar's filled or empty color.".to_owned()
        },
        CheckError::SizeMismatch { expected, found } => {
            format!("its region is {found} but its saved picture is {expected}.")
        },
        CheckError::Number(ReadError::NoInk) => format!(
            "nothing in its region stands out from the background, or has the text color of \
             {reference}."
        ),
        CheckError::Number(ReadError::TooManyCharacters) => {
            "its region shows more than 24 characters.".to_owned()
        },
        CheckError::Number(ReadError::UnknownCharacter {
            position,
            closest,
            read,
        }) => {
            // People count characters from 1.
            let number = position.saturating_add(1);
            let unlearned =
                format!("character {number} of \"{read}\" has a shape {reference} hasn't learned.");
            match closest {
                Some(closest) => format!("{unlearned} It looks closest to {closest}."),
                None => unlearned,
            }
        },
        CheckError::Number(ReadError::LeftOut { read }) => format!(
            "the number shows a character that isn't in Characters it needs. It read \"{read}\"."
        ),
        CheckError::Number(ReadError::NoNumber { read }) => {
            format!("it read \"{read}\", which holds no number.")
        },
    }
}

/// [`cant_read`] for a saved value, with what the user can do about it on
/// the Values page or the References page.
pub(crate) fn cant_read_saved(error: &CheckError, reference: Option<&str>) -> String {
    let why = cant_read(error, reference);
    let step = match error {
        CheckError::NoBar => "If the bar shows there now, edit the value to pick its colors again.",
        // A value is saved with pictures of its region's size, so only a
        // file changed outside the setup app gets here.
        CheckError::SizeMismatch { .. } => return why,
        CheckError::Number(ReadError::NoInk | ReadError::NoNumber { .. }) => {
            "If the number shows there now, edit the value to draw its region again."
        },
        CheckError::Number(ReadError::TooManyCharacters) => {
            "Edit the value to draw its region around the number alone."
        },
        CheckError::Number(ReadError::UnknownCharacter { .. }) => {
            "Teach it on the References page."
        },
        CheckError::Number(ReadError::LeftOut { .. }) => "Edit the value to add it.",
    };
    format!("{why} {step}")
}

#[cfg(test)]
mod tests {
    use appytappy_sources::screen::Image;
    use appytappy_sources::screen::Rgb;
    use appytappy_sources::screen::digits::{DigitReader, InkRule};

    use super::*;

    /// A picture with a white pixel for each `#` of `rows` and a black one
    /// for each `.`.
    fn image(rows: &[&str]) -> Image {
        let size = Size {
            width: u32::try_from(rows[0].len()).expect("small"),
            height: u32::try_from(rows.len()).expect("small"),
        };
        let pixels = rows
            .concat()
            .bytes()
            .flat_map(|cell| {
                let shade = if cell == b'#' { 255 } else { 0 };
                [shade, shade, shade, 255]
            })
            .collect();
        Image::new(size, pixels).expect("matching length")
    }

    #[test]
    fn a_failed_read_says_why_and_what_to_do_in_the_setup_apps_words() {
        let untaught = || {
            DigitReader::new(InkRule::Color {
                color: Rgb {
                    red: 255,
                    green: 255,
                    blue: 255,
                },
                tolerance: 0,
            })
        };
        let mut reader = untaught();
        reader.learn(&image(&["#", "#", "#"]), "1").expect("learns");
        reader
            .learn(&image(&["#.#", ".#.", "#.#"]), "%")
            .expect("learns");
        reader
            .learn(&image(&["#.....", "#.###.", "#....."]), "1-")
            .expect("learns");
        let digits_only = reader.clone().reading(Some("1")).expect("knows 1");
        let failed = |reader: &DigitReader, rows: &[&str]| {
            CheckError::Number(reader.read(&image(rows), false).expect_err("fails"))
        };
        for (error, reference, says) in [
            (
                failed(&reader, &["#.###", "#.#.#", "#.###"]),
                Some("Digits"),
                "character 2 of \"1 ?\" has a shape Digits hasn't learned. It looks closest to \
                 %. Teach it on the References page.",
            ),
            (
                failed(&untaught(), &["#"]),
                None,
                "character 1 of \"?\" has a shape its reference hasn't learned. Teach it on the \
                 References page.",
            ),
            (
                failed(&digits_only, &["....#", "###.#", "....#"]),
                Some("Digits"),
                "the number shows a character that isn't in Characters it needs. It read \"- \
                 1\". Edit the value to add it.",
            ),
            (
                failed(&reader, &["#.#", ".#.", "#.#"]),
                Some("Digits"),
                "it read \"%\", which holds no number. If the number shows there now, edit the \
                 value to draw its region again.",
            ),
            (
                failed(&reader, &["..."]),
                Some("Digits"),
                "nothing in its region stands out from the background, or has the text color of \
                 Digits. If the number shows there now, edit the value to draw its region again.",
            ),
            (
                failed(&reader, &["#.".repeat(25).as_str()]),
                Some("Digits"),
                "its region shows more than 24 characters. Edit the value to draw its region \
                 around the number alone.",
            ),
            (
                CheckError::NoBar,
                None,
                "nothing in its region is near the bar's filled or empty color. If the bar shows \
                 there now, edit the value to pick its colors again.",
            ),
        ] {
            assert_eq!(cant_read_saved(&error, reference), says);
            assert!(
                says.starts_with(&cant_read(&error, reference)),
                "the next step follows the reason: {says}"
            );
        }
    }

    #[test]
    fn a_number_names_its_reference_unless_a_fix_reads_with_its_own() {
        let references: Vec<ReferenceDef> = serde_json::from_value(serde_json::json!([
            { "id": "digits", "name": "Digits", "tolerance": 0, "samples": [] }
        ]))
        .expect("the file's spelling");
        let screen = |check: serde_json::Value, fix_reference: Option<&str>| -> ScreenDef {
            serde_json::from_value(serde_json::json!({
                "windowSize": { "width": 8, "height": 4 },
                "region": { "x": 0, "y": 0, "width": 4, "height": 2 },
                "check": check,
                "fixes": [{
                    "windowSize": { "width": 16, "height": 8 },
                    "region": { "x": 0, "y": 0, "width": 8, "height": 4 },
                    "reference": fix_reference
                }]
            }))
            .expect("the file's spelling")
        };
        let number = |reference: &str| serde_json::json!({ "kind": "number", "reference": reference, "whole": true });
        assert_eq!(
            reference_of(&screen(number("digits"), None), &references),
            Some("Digits")
        );
        assert_eq!(
            reference_of(&screen(number("digits"), Some("large")), &references),
            None
        );
        assert_eq!(
            reference_of(&screen(number("gone"), None), &references),
            None
        );
        let color = serde_json::json!({
            "kind": "color", "color": { "red": 1, "green": 2, "blue": 3 }, "tolerance": 0
        });
        assert_eq!(reference_of(&screen(color, None), &references), None);
    }
}
