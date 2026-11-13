//! The characters of one kept picture, a recorded reading's or a find's, as
//! the page shows them: where each is in the picture and which shape it
//! has, so the page can show one character by itself and tell which
//! pictures show the same one.

use std::hash::{DefaultHasher, Hash, Hasher};

use appytappy_sources::screen::Image;
use appytappy_sources::screen::digits::{DigitReader, Shape};
use serde::Serialize;

/// One character of a picture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "ShownCharacter"))]
pub(crate) struct CharacterView {
    /// A name for its shape, which two characters share exactly when their
    /// shapes are the same, whichever recording or search they are from.
    pub(crate) shape: String,
    /// The first column of the picture it covers.
    pub(crate) left: u32,
    /// The column after the last it covers.
    pub(crate) right: u32,
}

/// A name for `shape` that two shapes share only when they are the same,
/// but for a chance too small to matter: 64 bits of a hash. It holds while
/// the setup app runs and is never saved.
fn name(shape: &Shape) -> String {
    let mut hasher = DefaultHasher::new();
    shape.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// The characters `image` shows, left to right, as `reader` splits and
/// samples them; none when it cannot, as for a picture without ink.
pub(crate) fn of(image: &Image, reader: &DigitReader) -> Vec<CharacterView> {
    let Ok(placed) = reader.placed(image) else {
        return Vec::new();
    };
    placed
        .into_iter()
        .map(|(shape, columns)| CharacterView {
            shape: name(&shape),
            left: columns.start,
            right: columns.end,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use appytappy_sources::screen::{Rgb, Size};

    use super::*;

    /// A 9 by 4 picture with ink in columns where `ink` says so.
    fn picture(ink: [bool; 9]) -> Image {
        let row: Vec<u8> = ink
            .into_iter()
            .flat_map(|ink| if ink { [0, 0, 255, 0] } else { [40, 40, 40, 0] })
            .collect();
        Image::new(
            Size {
                width: 9,
                height: 4,
            },
            row.repeat(4),
        )
        .expect("matching length")
    }

    #[test]
    fn each_character_has_its_columns_and_the_same_shape_has_the_same_name() {
        let red = Rgb {
            red: 255,
            green: 0,
            blue: 0,
        };
        let reader = DigitReader::new(red, 0);
        // A stroke one column wide, one three wide, and the first again.
        let shown = [true, false, true, true, true, false, false, true, false];
        let characters = of(&picture(shown), &reader);
        let columns: Vec<(u32, u32)> = characters
            .iter()
            .map(|character| (character.left, character.right))
            .collect();
        assert_eq!(columns, [(0, 1), (2, 5), (7, 8)]);
        assert_eq!(characters[0].shape, characters[2].shape);
        assert_ne!(characters[0].shape, characters[1].shape);
        assert_eq!(of(&picture([false; 9]), &reader), []);
    }
}
