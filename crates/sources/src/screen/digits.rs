//! Reads a number from a region by matching its characters against glyphs
//! learned from the user's screenshots.
//!
//! An app draws its counter with one font at one size, so the screenshot's
//! own glyphs are the best possible templates. Reading is a fixed sequence of
//! integer passes with no allocation, so it is deterministic and costs
//! microseconds, where general text recognition costs milliseconds and can
//! differ between machines.
//!
//! 1. **Ink.** A pixel is ink when each of its red, green, and blue is within
//!    the tolerance of the text color the user picked.
//! 2. **Line.** The rows from the first to the last that hold ink.
//! 3. **Characters.** Runs of columns that hold ink in the line, left to
//!    right. Characters must not touch.
//! 4. **Shapes.** Each character is sampled onto a grid 16 cells tall, scaled
//!    by the line's height so a narrow `1` stays narrow and a `.` stays at the
//!    baseline. A cell is set when at least half its pixels are ink.
//! 5. **Matching.** Each shape takes the character of the learned glyph that
//!    differs from it in the fewest cells, if those differ in at most a
//!    quarter of the cells either sets.

use std::error::Error;
use std::fmt;
use std::ops::Range;

use appytappy_config::signal::Number;

use super::Image;
use crate::system::Rgb;

/// Grid rows: each character is scaled to this many cells tall.
const GRID_ROWS: u32 = 16;

/// Grid columns: twice the rows, so a character up to twice as wide as the
/// line is tall, such as `%`, fits.
const GRID_COLUMNS: u32 = 32;

/// The most characters one read or one screenshot may hold.
const MAX_CHARACTERS: usize = 24;

/// The most glyphs a reader learns.
const MAX_GLYPHS: usize = 256;

/// The longest number a read assembles, in characters.
const MAX_NUMBER_LEN: usize = 24;

/// A character's shape: one `u32` of cells per grid row.
type Shape = [u32; 16];

/// One learned character and its shape.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Glyph {
    character: char,
    shape: Shape,
}

/// Columns `left..right` of a character.
#[derive(Debug, Clone, Copy, Default)]
struct Span {
    left: u32,
    right: u32,
}

/// Rows `top..bottom` that hold ink.
#[derive(Debug, Clone, Copy)]
struct Line {
    top: u32,
    bottom: u32,
}

/// Reads a number drawn in one text color, from glyphs learned from
/// screenshots of the same counter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigitReader {
    ink: Rgb,
    tolerance: u8,
    glyphs: Vec<Glyph>,
}

impl DigitReader {
    /// A reader for text of color `ink`, where a pixel counts as ink when
    /// each of its red, green, and blue is within `tolerance` of the ink's.
    /// It knows no glyphs until it learns some.
    #[must_use]
    pub const fn new(ink: Rgb, tolerance: u8) -> Self {
        Self {
            ink,
            tolerance,
            glyphs: Vec::new(),
        }
    }

    /// Learns the glyphs in `image`, which shows `text`. Whitespace in `text`
    /// is ignored; every other character must match one character in the
    /// image, left to right.
    ///
    /// # Errors
    ///
    /// Returns a [`LearnError`] when the image's characters do not line up
    /// with `text`, when a shape is already learned as another character, or
    /// when the reader is full. On error the reader is unchanged.
    pub fn learn(&mut self, image: &Image, text: &str) -> Result<(), LearnError> {
        let mut spans = [Span::default(); MAX_CHARACTERS];
        let (line, count) = self
            .segment(image, &mut spans)
            .map_err(|error| match error {
                Segmentation::NoInk => LearnError::NoInk,
                Segmentation::TooMany => LearnError::TooManyCharacters,
            })?;
        let expected = text.chars().filter(|character| !character.is_whitespace());
        let found = spans.get(..count).unwrap_or_default();
        if expected.clone().count() != found.len() {
            return Err(LearnError::CountMismatch {
                found: found.len(),
                expected: expected.count(),
            });
        }
        let mut learned = self.glyphs.clone();
        for (character, span) in expected.zip(found) {
            let shape = self.shape(image, line, *span);
            match learned.iter().find(|glyph| glyph.shape == shape) {
                Some(glyph) if glyph.character == character => {},
                Some(glyph) => {
                    return Err(LearnError::Conflict {
                        learned: glyph.character,
                        new: character,
                    });
                },
                None if learned.len() >= MAX_GLYPHS => return Err(LearnError::Full),
                None => learned.push(Glyph { character, shape }),
            }
        }
        self.glyphs = learned;
        Ok(())
    }

    /// Whether the reader has learned a glyph for `character`.
    #[must_use]
    pub fn knows(&self, character: char) -> bool {
        self.glyphs.iter().any(|glyph| glyph.character == character)
    }

    /// Reads the first number in `image`: an optional `-`, digits, and an
    /// optional `.` with more digits. Other learned characters, such as `%`,
    /// separate numbers.
    ///
    /// # Errors
    ///
    /// Returns a [`ReadError`] when the image has no ink, too many
    /// characters, a character that matches no learned glyph, or no digits.
    pub fn read(&self, image: &Image) -> Result<Number, ReadError> {
        let mut spans = [Span::default(); MAX_CHARACTERS];
        let (line, count) = self
            .segment(image, &mut spans)
            .map_err(|error| match error {
                Segmentation::NoInk => ReadError::NoInk,
                Segmentation::TooMany => ReadError::TooManyCharacters,
            })?;
        let mut number = NumberText::default();
        for (position, span) in spans.iter().take(count).enumerate() {
            let shape = self.shape(image, line, *span);
            match self.best_match(&shape) {
                Some(character) => number.push(character),
                // Text after a finished number, such as the "HP" in "37% HP",
                // needs no glyphs.
                None if number.ended => break,
                None => return Err(ReadError::UnknownCharacter { position }),
            }
        }
        number.parse().ok_or(ReadError::NoNumber)
    }

    /// Whether `pixel`, as `[blue, green, red, unused]` like
    /// [`Image::pixels`] holds it, is ink: each of its red, green, and blue
    /// within the tolerance of the text color.
    #[must_use]
    pub fn is_ink(&self, [blue, green, red, _]: [u8; 4]) -> bool {
        blue.abs_diff(self.ink.blue) <= self.tolerance
            && green.abs_diff(self.ink.green) <= self.tolerance
            && red.abs_diff(self.ink.red) <= self.tolerance
    }

    fn is_ink_at(&self, image: &Image, x: u32, y: u32) -> bool {
        let index = u64::from(y)
            .saturating_mul(u64::from(image.size().width))
            .saturating_add(u64::from(x));
        usize::try_from(index)
            .ok()
            .and_then(|index| image.pixels().get(index))
            .is_some_and(|&pixel| self.is_ink(pixel))
    }

    /// Counts the ink pixels in a rectangle, and all its pixels.
    fn ink_in(&self, image: &Image, columns: Range<u32>, rows: Range<u32>) -> (u32, u32) {
        let mut ink = 0_u32;
        let mut area = 0_u32;
        for y in rows {
            for x in columns.clone() {
                ink = ink.saturating_add(u32::from(self.is_ink_at(image, x, y)));
                area = area.saturating_add(1);
            }
        }
        (ink, area)
    }

    /// Finds the line and its characters, writing their spans into `spans`
    /// and returning how many it wrote.
    fn segment(
        &self,
        image: &Image,
        spans: &mut [Span; MAX_CHARACTERS],
    ) -> Result<(Line, usize), Segmentation> {
        let line = self.line(image).ok_or(Segmentation::NoInk)?;

        let mut count = 0_usize;
        let mut start = None;
        for x in 0..=image.size().width {
            let inked = x < image.size().width
                && (line.top..line.bottom).any(|y| self.is_ink_at(image, x, y));
            match (start, inked) {
                (None, true) => start = Some(x),
                (Some(left), false) => {
                    let slot = spans.get_mut(count).ok_or(Segmentation::TooMany)?;
                    *slot = Span { left, right: x };
                    count = count.saturating_add(1);
                    start = None;
                },
                _ => {},
            }
        }
        Ok((line, count))
    }

    /// The run of adjacent inked rows that holds the most ink, so a stray
    /// pixel elsewhere in the region does not stretch the line.
    fn line(&self, image: &Image) -> Option<Line> {
        let rows = image
            .rows()
            .map(|row| row.iter().filter(|&&pixel| self.is_ink(pixel)).count());
        let mut best: Option<(Line, usize)> = None;
        let mut run: Option<(Line, usize)> = None;
        // The trailing zero ends a run that reaches the bottom edge.
        for (ink, y) in rows.chain([0]).zip(0_u32..) {
            if ink > 0 {
                let (line, total) = run.get_or_insert((Line { top: y, bottom: y }, 0));
                line.bottom = y.saturating_add(1);
                *total = total.saturating_add(ink);
            } else if let Some(ended) = run.take()
                && best.is_none_or(|(_, most)| ended.1 > most)
            {
                best = Some(ended);
            }
        }
        best.map(|(line, _)| line)
    }

    /// Samples the character in `span` onto the grid, scaled by the line's
    /// height.
    fn shape(&self, image: &Image, line: Line, span: Span) -> Shape {
        let height = line.bottom.saturating_sub(line.top);
        // The start of grid cell `index`, in pixels from the cell grid's origin.
        let edge = |index: u32| index.saturating_mul(height) / GRID_ROWS;
        let mut shape = Shape::default();
        for (row, cells) in (0..GRID_ROWS).zip(shape.iter_mut()) {
            let y0 = line.top.saturating_add(edge(row));
            let y1 = line
                .top
                .saturating_add(edge(row.saturating_add(1)))
                .max(y0.saturating_add(1));
            for column in 0..GRID_COLUMNS {
                let x0 = span.left.saturating_add(edge(column));
                if x0 >= span.right {
                    break;
                }
                let x1 = span
                    .left
                    .saturating_add(edge(column.saturating_add(1)))
                    .max(x0.saturating_add(1))
                    .min(span.right);
                let (ink, area) = self.ink_in(image, x0..x1, y0..y1);
                if ink.saturating_mul(2) >= area {
                    *cells |= 1 << column;
                }
            }
        }
        shape
    }

    /// The character of the closest learned glyph, if it is close enough.
    fn best_match(&self, shape: &Shape) -> Option<char> {
        let (glyph, differing) = self
            .glyphs
            .iter()
            .map(|glyph| (glyph, cells(shape, &glyph.shape, |a, b| a ^ b)))
            .min_by_key(|(_, differing)| *differing)?;
        let set = cells(shape, &glyph.shape, |a, b| a | b);
        (differing.saturating_mul(4) <= set).then_some(glyph.character)
    }
}

/// Counts the cells that `combine` sets across two shapes.
fn cells(a: &Shape, b: &Shape, combine: impl Fn(u32, u32) -> u32) -> u32 {
    a.iter()
        .zip(b)
        .map(|(&a, &b)| combine(a, b).count_ones())
        .sum()
}

#[derive(Debug, Clone, Copy)]
enum Segmentation {
    NoInk,
    TooMany,
}

/// The characters of a number as a read finds them, held on the stack.
#[derive(Debug, Default)]
struct NumberText {
    text: [u8; MAX_NUMBER_LEN],
    len: usize,
    digits: bool,
    point: bool,
    ended: bool,
}

impl NumberText {
    fn push(&mut self, character: char) {
        if self.ended {
            return;
        }
        let accepted = match character {
            '0'..='9' => {
                self.digits = true;
                true
            },
            '-' => self.len == 0,
            '.' if self.digits && !self.point => {
                self.point = true;
                true
            },
            _ => false,
        };
        if !accepted {
            self.ended = self.digits;
            if !self.digits {
                self.len = 0;
            }
            return;
        }
        let Ok(byte) = u8::try_from(character) else {
            return;
        };
        match self.text.get_mut(self.len) {
            Some(slot) => {
                *slot = byte;
                self.len = self.len.saturating_add(1);
            },
            None => self.ended = true,
        }
    }

    fn parse(&self) -> Option<Number> {
        if !self.digits {
            return None;
        }
        let text = std::str::from_utf8(self.text.get(..self.len)?).ok()?;
        Number::new(text.trim_end_matches('.').parse().ok()?)
    }
}

/// A screenshot could not teach a reader its glyphs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LearnError {
    /// No pixel was near the text color.
    NoInk,
    /// The image held more than 24 characters.
    TooManyCharacters,
    /// The image held a different number of characters than the text.
    CountMismatch {
        /// Characters in the image.
        found: usize,
        /// Characters in the text, without whitespace.
        expected: usize,
    },
    /// A character had the same shape as one learned as another character.
    Conflict {
        /// The character the shape was learned as.
        learned: char,
        /// The character the text gave it now.
        new: char,
    },
    /// The reader already knows 256 glyphs.
    Full,
}

impl fmt::Display for LearnError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoInk => formatter.write_str("no pixel in the region has the text color"),
            Self::TooManyCharacters => write!(
                formatter,
                "the region holds more than {MAX_CHARACTERS} characters"
            ),
            Self::CountMismatch { found, expected } => write!(
                formatter,
                "the region shows {found} characters but the text has {expected}; characters \
                 that touch count as one"
            ),
            Self::Conflict { learned, new } => write!(
                formatter,
                "a character shaped like {learned:?} was given as {new:?}"
            ),
            Self::Full => write!(formatter, "a reader learns at most {MAX_GLYPHS} glyphs"),
        }
    }
}

impl Error for LearnError {}

/// A reader could not read a number from a region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// No pixel was near the text color.
    NoInk,
    /// The region held more than 24 characters.
    TooManyCharacters,
    /// A character matched no learned glyph.
    UnknownCharacter {
        /// Its position from the left, from 0.
        position: usize,
    },
    /// The characters held no digits.
    NoNumber,
}

impl fmt::Display for ReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoInk => formatter.write_str("no pixel in the region has the text color"),
            Self::TooManyCharacters => write!(
                formatter,
                "the region holds more than {MAX_CHARACTERS} characters"
            ),
            Self::UnknownCharacter { position } => write!(
                formatter,
                "character {position} from the left matches no learned glyph"
            ),
            Self::NoNumber => formatter.write_str("the region holds no digits"),
        }
    }
}

impl Error for ReadError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::Size;

    #[test]
    fn errors_explain_themselves() {
        for (error, message) in [
            (
                LearnError::NoInk,
                "no pixel in the region has the text color",
            ),
            (
                LearnError::TooManyCharacters,
                "the region holds more than 24 characters",
            ),
            (
                LearnError::CountMismatch {
                    found: 2,
                    expected: 3,
                },
                "the region shows 2 characters but the text has 3; characters that touch count \
                 as one",
            ),
            (
                LearnError::Conflict {
                    learned: '1',
                    new: '7',
                },
                "a character shaped like '1' was given as '7'",
            ),
            (LearnError::Full, "a reader learns at most 256 glyphs"),
        ] {
            assert_eq!(error.to_string(), message);
        }
        for (error, message) in [
            (
                ReadError::NoInk,
                "no pixel in the region has the text color",
            ),
            (
                ReadError::TooManyCharacters,
                "the region holds more than 24 characters",
            ),
            (
                ReadError::UnknownCharacter { position: 2 },
                "character 2 from the left matches no learned glyph",
            ),
            (ReadError::NoNumber, "the region holds no digits"),
        ] {
            assert_eq!(error.to_string(), message);
        }
    }

    const WHITE: Rgb = Rgb {
        red: 255,
        green: 255,
        blue: 255,
    };

    /// Draws `rows` of `#` (ink) and `.` (background) as an image.
    /// "12" in a five-pixel font, with sixteen ink pixels.
    const TWELVE: [&str; 5] = [
        "#.###", //
        "#...#", //
        "#.###", //
        "#.#..", //
        "#.###", //
    ];

    fn draw(rows: &[&str]) -> Image {
        let mut bgra = Vec::new();
        for row in rows {
            for cell in row.chars() {
                let value = if cell == '#' { 255 } else { 0 };
                bgra.extend_from_slice(&[value, value, value, 255]);
            }
        }
        let size = Size {
            width: u32::try_from(rows[0].len()).unwrap(),
            height: u32::try_from(rows.len()).unwrap(),
        };
        Image::new(size, bgra).unwrap()
    }

    fn text(characters: &str) -> NumberText {
        let mut number = NumberText::default();
        characters
            .chars()
            .for_each(|character| number.push(character));
        number
    }

    #[test]
    fn numbers_take_the_first_run_of_number_characters() {
        let parsed = |characters| text(characters).parse();
        assert_eq!(parsed("37%"), Number::new(37.0));
        assert_eq!(parsed("90.5%"), Number::new(90.5));
        assert_eq!(parsed("-12"), Number::new(-12.0));
        assert_eq!(parsed("%12/50"), Number::new(12.0));
        assert_eq!(parsed("1.2.3"), Number::new(1.2));
        assert_eq!(parsed("7."), Number::new(7.0));
        assert_eq!(parsed("-%5"), Number::new(5.0));
        assert_eq!(parsed(".5"), Number::new(5.0));
        assert_eq!(parsed("%-"), None);
        assert_eq!(parsed("1-2"), Number::new(1.0));
        let longest = "9".repeat(MAX_NUMBER_LEN);
        assert_eq!(
            parsed(&"9".repeat(MAX_NUMBER_LEN + 4)),
            Number::new(longest.parse().unwrap())
        );
    }

    #[test]
    fn learns_and_reads_separate_characters() {
        let digits = draw(&[
            "#.###.###", //
            "#...#...#",
            "#.###..##",
            "#.#.....#",
            "#.###.###",
        ]);
        let mut reader = DigitReader::new(WHITE, 0);
        reader.learn(&digits, "1 2 3").unwrap();
        assert!(reader.knows('2'));
        assert!(!reader.knows('4'));
        let swapped = draw(&[
            "###.#.###", //
            "..#.#...#",
            "###.#.###",
            "#...#.#..",
            "###.#.###",
        ]);
        assert_eq!(reader.read(&swapped), Ok(Number::new(212.0).unwrap()));
    }

    #[test]
    fn learning_fails_without_changing_the_reader() {
        let digits = draw(&["#.#"]);
        let mut reader = DigitReader::new(WHITE, 0);
        assert_eq!(
            reader.learn(&digits, "1"),
            Err(LearnError::CountMismatch {
                found: 2,
                expected: 1
            })
        );
        assert_eq!(
            reader.learn(&digits, "12"),
            Err(LearnError::Conflict {
                learned: '1',
                new: '2'
            })
        );
        assert!(!reader.knows('1'));
        assert_eq!(reader.learn(&draw(&["..."]), "1"), Err(LearnError::NoInk));
        reader.learn(&draw(&["#"]), "1").unwrap();
        reader.learn(&draw(&["#"]), "1").unwrap();
        assert_eq!(reader.glyphs.len(), 1);
    }

    #[test]
    fn reading_fails_on_unknown_characters_and_missing_digits() {
        let mut reader = DigitReader::new(WHITE, 0);
        reader
            .learn(&draw(&["#.###", "#.#.#", "#.###"]), "1%")
            .unwrap();
        assert_eq!(
            reader.read(&draw(&["###", "#.#", "###"])),
            Err(ReadError::NoNumber)
        );
        assert_eq!(
            reader.read(&draw(&["#.#.#", "#...#", "#.###"])),
            Err(ReadError::UnknownCharacter { position: 1 })
        );
        assert_eq!(reader.read(&draw(&["..."])), Err(ReadError::NoInk));
        let crowded = "#.".repeat(MAX_CHARACTERS + 1);
        assert_eq!(
            reader.read(&draw(&[crowded.as_str()])),
            Err(ReadError::TooManyCharacters)
        );
    }

    #[test]
    fn text_after_a_finished_number_needs_no_glyphs() {
        let mut reader = DigitReader::new(WHITE, 0);
        reader
            .learn(&draw(&["#.###", "#.#.#", "#.###"]), "1%")
            .unwrap();
        // "1%" and then a character never learned.
        let labeled = draw(&["#.###.#.#", "#.#.#.###", "#.###.#.#"]);
        assert_eq!(reader.read(&labeled), Ok(Number::new(1.0).unwrap()));
    }

    #[test]
    fn a_stray_pixel_does_not_stretch_the_line() {
        let mut reader = DigitReader::new(WHITE, 0);
        reader.learn(&draw(&TWELVE), "12").unwrap();
        let mut noisy = TWELVE.to_vec();
        noisy.extend([".....", "....#"]);
        assert_eq!(reader.read(&draw(&noisy)), Ok(Number::new(12.0).unwrap()));
        // Of two runs of inked rows, the one with more ink is the line.
        let mut above = vec!["#....", "....."];
        above.extend(TWELVE);
        assert_eq!(reader.read(&draw(&above)), Ok(Number::new(12.0).unwrap()));
    }

    #[test]
    fn ink_is_each_channel_within_the_tolerance() {
        let reader = DigitReader::new(
            Rgb {
                red: 200,
                green: 100,
                blue: 50,
            },
            5,
        );
        assert!(reader.is_ink([55, 95, 205, 0]));
        assert!(!reader.is_ink([56, 100, 200, 0]));
        assert!(!reader.is_ink([50, 94, 200, 0]));
        assert!(!reader.is_ink([50, 100, 206, 0]));
    }

    #[test]
    fn a_shape_matches_when_at_most_a_quarter_of_its_cells_differ() {
        let mut reader = DigitReader::new(WHITE, 0);
        let mut shape = Shape::default();
        shape[0] = 0b1111;
        reader.glyphs.push(Glyph {
            character: '7',
            shape,
        });
        let mut one_off = shape;
        one_off[1] = 0b1;
        assert_eq!(reader.best_match(&one_off), Some('7'));
        let mut two_off = shape;
        two_off[1] = 0b11;
        assert_eq!(reader.best_match(&two_off), None);
        // A missing cell counts against the cells either shape sets: one of
        // four differs, a quarter, so it still matches.
        let mut one_missing = shape;
        one_missing[0] = 0b0111;
        assert_eq!(reader.best_match(&one_missing), Some('7'));
    }

    #[test]
    fn a_cell_is_set_when_at_least_half_its_pixels_are_ink() {
        let reader = DigitReader::new(WHITE, 0);
        // A line 32 pixels tall makes each grid cell 2 by 2 pixels.
        let line = Line { top: 0, bottom: 32 };
        let span = Span { left: 0, right: 2 };
        let first_cell = |top: [&'static str; 2]| {
            let mut rows = vec![".."; 32];
            rows[..2].copy_from_slice(&top);
            reader.shape(&draw(&rows), line, span)[0] & 1
        };
        assert_eq!(first_cell(["#.", "#."]), 1, "half the cell is ink");
        assert_eq!(first_cell(["#.", ".."]), 0, "a quarter of the cell is ink");
    }

    #[test]
    fn of_two_lines_with_equal_ink_the_upper_one_is_read() {
        let mut reader = DigitReader::new(WHITE, 0);
        reader.learn(&draw(&TWELVE), "12").unwrap();
        // Sixteen ink pixels, as many as "12", in one unlearned block.
        let mut tied = TWELVE.to_vec();
        tied.extend([".....", "#####", "#####", "#####", "#...."]);
        assert_eq!(reader.read(&draw(&tied)), Ok(Number::new(12.0).unwrap()));
    }

    #[test]
    fn a_reader_learns_at_most_256_glyphs() {
        let mut reader = DigitReader::new(WHITE, 0);
        for index in 0..MAX_GLYPHS {
            let mut shape = Shape::default();
            shape[0] = u32::try_from(index).unwrap().wrapping_add(1) << 16;
            reader.glyphs.push(Glyph {
                character: 'x',
                shape,
            });
        }
        assert_eq!(reader.learn(&draw(&["#"]), "1"), Err(LearnError::Full));
    }
}
