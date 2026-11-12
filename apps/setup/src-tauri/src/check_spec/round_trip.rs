//! The reference the editor tries on the live window is the reference the
//! runtime reads with: what [`build`] learns from labeled screenshots,
//! [`to_saved_reference`] saves, and `saved::build` learns again from the
//! saved file and its pictures.

use std::collections::BTreeMap;
use std::io;

use appytappy_config::app_file::{ScreenDef, SizeChangesDef, SizeDef};
use appytappy_sources::screen::saved;
use proptest::prelude::*;

use super::*;

/// The property's screenshots are a list, each named by its place in it.
impl Shots for Vec<Image> {
    fn get(&self, index: usize) -> Option<&Image> {
        self.as_slice().get(index)
    }
}

/// A font of 3 by 5 pixels.
const GLYPHS: [(char, [&str; 5]); 13] = [
    ('0', ["###", "#.#", "#.#", "#.#", "###"]),
    ('1', [".#.", "##.", ".#.", ".#.", "###"]),
    ('2', ["###", "..#", "###", "#..", "###"]),
    ('3', ["###", "..#", "###", "..#", "###"]),
    ('4', ["#.#", "#.#", "###", "..#", "..#"]),
    ('5', ["###", "#..", "###", "..#", "###"]),
    ('6', ["###", "#..", "###", "#.#", "###"]),
    ('7', ["###", "..#", "..#", "..#", "..#"]),
    ('8', ["###", "#.#", "###", "#.#", "###"]),
    ('9', ["###", "#.#", "###", "..#", "###"]),
    ('.', ["...", "...", "...", "...", ".#."]),
    ('%', ["#.#", "..#", ".#.", "#..", "#.#"]),
    ('-', ["...", "...", "###", "...", "..."]),
];

/// The window's size on every screenshot.
const WINDOW: Size = Size {
    width: 96,
    height: 24,
};

/// The region the reference is taught on, and the value reads.
const REGION: RegionSpec = RegionSpec {
    x: 4,
    y: 2,
    width: 88,
    height: 20,
};

const INK: ColorSpec = ColorSpec {
    red: 250,
    green: 250,
    blue: 250,
};

/// One thing the window shows: text, each pixel of its font `scale`
/// pixels wide and tall, `across` and `down` from the region's corner.
#[derive(Debug, Clone)]
struct Shown {
    text: String,
    scale: u32,
    across: u32,
    down: u32,
}

/// A screenshot of the window showing `shown`, its characters a pixel of
/// the font apart, in a color within 5 of [`INK`] on a dark background.
fn screenshot(shown: &Shown) -> Image {
    // The text's rows of pixels: each cell of the font `scale` wide, a
    // blank cell after each character, and each row `scale` times.
    let lines: Vec<Vec<bool>> = (0..5)
        .flat_map(|row| {
            let cells = shown.text.chars().flat_map(move |character| {
                let rows = GLYPHS
                    .iter()
                    .find_map(|(glyph, rows)| (*glyph == character).then_some(rows))
                    .expect("a character of the font");
                rows[row].chars().chain(['.'])
            });
            let line: Vec<bool> = cells
                .flat_map(|cell| std::iter::repeat_n(cell == '#', widened(shown.scale)))
                .collect();
            std::iter::repeat_n(line, widened(shown.scale))
        })
        .collect();
    let blank = Vec::new();
    let above = widened(REGION.y.saturating_add(shown.down));
    let left = widened(REGION.x.saturating_add(shown.across));
    let bgra = (0..widened(WINDOW.height))
        .flat_map(|y| {
            let line = y
                .checked_sub(above)
                .and_then(|row| lines.get(row))
                .unwrap_or(&blank);
            std::iter::repeat_n(false, left)
                .chain(line.iter().copied())
                .chain(std::iter::repeat(false))
                .take(widened(WINDOW.width))
        })
        .flat_map(|ink| {
            if ink {
                [255, 250, 245, 0]
            } else {
                [30, 20, 10, 0]
            }
        })
        .collect();
    Image::new(WINDOW, bgra).expect("a pixel for each of the window's")
}

/// `pixels` as a count.
fn widened(pixels: u32) -> usize {
    usize::try_from(pixels).expect("a small picture")
}

/// Text of the font that fits the region at any scale and place
/// [`shown`] draws it.
fn shown() -> impl Strategy<Value = Shown> {
    let character = prop::sample::select(GLYPHS.map(|(character, _)| character).to_vec());
    (
        prop::collection::vec(character, 1..=6),
        1_u32..=3,
        0_u32..=8,
        0_u32..=4,
    )
        .prop_map(|(text, scale, across, down)| Shown {
            text: text.into_iter().collect(),
            scale,
            across,
            down,
        })
}

/// The reference being taught: `labeled`, one screenshot each, added to
/// the saved reference `editing`.
fn taught(editing: Option<ReferenceRef>, labeled: &[Shown], tolerance: u8) -> CheckSpec {
    CheckSpec::Characters {
        region: REGION,
        screenshot: 0,
        reference: editing,
        ink: INK,
        tolerance,
        labels: labeled
            .iter()
            .enumerate()
            .map(|(screenshot, shown)| Label {
                screenshot,
                region: None,
                text: shown.text.clone(),
            })
            .collect(),
        recorded: Vec::new(),
    }
}

/// The check the runtime builds for a value that reads [`REGION`] with
/// `reference`, from the saved `pictures`.
fn runtime_check(
    reference: &ReferenceDef,
    pictures: &BTreeMap<String, Vec<u8>>,
) -> Result<Check, saved::SavedError> {
    let value = ScreenDef {
        window_size: SizeDef {
            width: WINDOW.width,
            height: WINDOW.height,
        },
        region: region_def(to_region(REGION).expect("not empty")),
        check: CheckDef::Number {
            reference: reference.id.clone(),
            whole: false,
        },
        pin: None,
        fixes: Vec::new(),
        reading_area: false,
    };
    saved::build(
        &value,
        std::slice::from_ref(reference),
        SizeChangesDef::Often,
        |name| {
            pictures
                .get(name)
                .cloned()
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        },
    )
    .map(|built| built.drawn.check)
}

/// Saves the reference `spec` teaches, as the editor's **Save reference**
/// does, after checking that the editor and the runtime agree on it: one
/// cannot learn it unless the other can, they learn the same characters,
/// and so they read `live` the same. Answers the saved reference, or
/// `None` when it cannot be learned.
fn save(
    spec: &CheckSpec,
    session: Session<'_>,
    pictures: &mut BTreeMap<String, Vec<u8>>,
    live: &Image,
) -> Result<Option<ReferenceDef>, TestCaseError> {
    let tried = match build(spec, session) {
        Ok(tried) => tried,
        Err(error) => {
            let saved = to_saved_reference(spec, session, "percent", "Percent");
            prop_assert_eq!(saved.err(), Some(error));
            return Ok(None);
        },
    };
    let (reference, new) = to_saved_reference(spec, session, "percent", "Percent")
        .map_err(|error| TestCaseError::fail(error.to_string()))?;
    pictures.extend(new);
    let read_with = runtime_check(&reference, pictures)
        .map_err(|error| TestCaseError::fail(error.to_string()))?;
    prop_assert_eq!(&read_with, &tried.check);
    let region = live.crop(tried.region).expect("inside the window");
    prop_assert_eq!(read_with.read(&region), tried.check.read(&region));
    Ok(Some(reference))
}

proptest! {
    #[test]
    fn the_runtime_learns_from_the_saved_file_what_the_editor_tried(
        first in prop::collection::vec(shown(), 1..=3),
        more in prop::collection::vec(shown(), 1..=2),
        tolerances in (0_u8..=12, 0_u8..=12),
        live in shown(),
    ) {
        let no_references = |_: &ReferenceRef| -> Result<SavedReference, String> {
            Err("That reference is not saved.".to_owned())
        };
        let live = screenshot(&live);
        let mut pictures = BTreeMap::new();
        let screenshots: Vec<Image> = first.iter().map(screenshot).collect();
        let new = Session { screenshots: &screenshots, recording: None, references: &no_references };
        let saved = save(&taught(None, &first, tolerances.0), new, &mut pictures, &live)?;
        // The reference edited: its saved samples are learned again, with
        // the tolerance the editor has now, before the new labels.
        let Some(saved) = saved else { return Ok(()) };
        let editing = ReferenceRef { app: "app".to_owned(), id: saved.id.clone() };
        let from_file = |_: &ReferenceRef| {
            Ok(SavedReference {
                reference: saved.clone(),
                pictures: saved
                    .samples
                    .iter()
                    .map(|sample| Image::decode_png(&pictures[&sample.image]).expect("a PNG"))
                    .collect(),
            })
        };
        let screenshots: Vec<Image> = more.iter().map(screenshot).collect();
        let edit = Session { screenshots: &screenshots, recording: None, references: &from_file };
        let mut edited = pictures.clone();
        save(&taught(Some(editing), &more, tolerances.1), edit, &mut edited, &live)?;
    }
}
