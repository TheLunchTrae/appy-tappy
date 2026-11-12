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
//! 5. **Matching.** Each shape takes the character of the closest learned
//!    glyph, if the two are the same but for their edges. A cell one of them
//!    sets counts as apart when the other sets neither that cell nor one
//!    beside it. An application that draws its text at fractions of a pixel,
//!    or at another size, moves a character's edge by a cell, which is not
//!    apart; a stroke that one character has and another lacks is. The
//!    closest glyph has the fewest cells apart, then the fewest that differ
//!    at all, and it matches when at most 1 in 32 of the cells either sets
//!    are apart.

use std::cmp::Ordering;
use std::error::Error;
use std::fmt;
use std::ops::Range;

use appytappy_config::signal::Number;

use super::Image;
use crate::system::Rgb;

#[cfg(test)]
pub(crate) mod drawn;

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

/// The shapes of an image's characters, from [`DigitReader::shapes`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shapes(Vec<Shape>);

/// The cells either of two shapes sets, for each cell that may be apart
/// while the shapes still match.
const CELLS_PER_APART: u32 = 32;

/// One learned character and its shape.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Glyph {
    character: char,
    shape: Shape,
    /// The shape and every cell beside one of its cells.
    grown: Shape,
}

impl Glyph {
    fn new(character: char, shape: Shape) -> Self {
        Self {
            character,
            shape,
            grown: grown(&shape),
        }
    }
}

/// How far a shape is from a learned glyph. The derived order compares the
/// cells apart first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Distance {
    /// Cells one of the two sets where the other sets neither that cell nor
    /// one beside it: a part the other lacks, not a moved edge.
    apart: u32,
    /// Cells only one of the two sets.
    differing: u32,
}

/// The learned glyph closest to a shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Closest {
    character: char,
    /// Whether the shape is close enough to read as the character.
    matches: bool,
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
        let (found_count, expected_count) = (found.len(), expected.clone().count());
        match found_count.cmp(&expected_count) {
            Ordering::Equal => {},
            Ordering::Less => {
                return Err(LearnError::FewerCharacters {
                    found: found_count,
                    expected: expected_count,
                });
            },
            Ordering::Greater => {
                return Err(LearnError::MoreCharacters {
                    found: found_count,
                    expected: expected_count,
                });
            },
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
                None => learned.push(Glyph::new(character, shape)),
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
    /// optional `.` with more digits. A number that starts with its `.`
    /// reads as `0.`, so `-.5` reads as -0.5, unless letters came before,
    /// so `Lv.5` reads as 5; a `.` with no digit after it is no number. A
    /// `,` between groups of three digits is skipped, so `1,234` reads as
    /// 1234; any other `,`, and other learned characters such as `%` or
    /// `/`, separate numbers, so `12,5` and `12/50` read as 12. With
    /// `whole`, the number ends at the `.`, so the digits after it are
    /// ignored and need no glyphs: `87.5%` reads as 87.
    ///
    /// # Errors
    ///
    /// Returns a [`ReadError`] when the image has no ink, too many
    /// characters, a character that matches no learned glyph, or no digits.
    /// The last two carry the characters the read made out, so a number is
    /// never made of only the characters that matched.
    pub fn read(&self, image: &Image, whole: bool) -> Result<Number, ReadError> {
        let mut spans = [Span::default(); MAX_CHARACTERS];
        let (line, count) = self
            .segment(image, &mut spans)
            .map_err(|error| match error {
                Segmentation::NoInk => ReadError::NoInk,
                Segmentation::TooMany => ReadError::TooManyCharacters,
            })?;
        let mut number = NumberText {
            point: Point::for_whole(whole),
            ..NumberText::default()
        };
        let mut read = ReadText::default();
        // The first character that matched no glyph, and the glyph closest
        // to it. The read then fails, whatever `number` holds; the
        // characters after it are still matched, for the error, up to
        // where a number among them ends.
        let mut unknown = None;
        for (position, span) in spans.iter().take(count).enumerate() {
            let closest = self.closest(&self.shape(image, line, *span));
            match closest.filter(|closest| closest.matches) {
                Some(closest) => {
                    read.push(closest.character);
                    number.push(closest.character);
                },
                // Text after a finished number, such as the "HP" in "37% HP",
                // needs no glyphs.
                None if number.ended => break,
                None => {
                    read.push(ReadText::UNKNOWN);
                    if unknown.is_none() {
                        unknown = Some((position, closest.map(|closest| closest.character)));
                    }
                },
            }
        }
        if let Some((position, closest)) = unknown {
            return Err(ReadError::UnknownCharacter {
                position,
                closest,
                read,
            });
        }
        number.parse().ok_or(ReadError::NoNumber { read })
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

    /// The shapes of the characters in `image`, left to right, as a read
    /// samples them. Two images with equal shapes teach a reader the same
    /// glyphs and read the same, so one adds nothing the other does not.
    ///
    /// # Errors
    ///
    /// [`ReadError::NoInk`] when the image has no ink, and
    /// [`ReadError::TooManyCharacters`] when it has more characters than a
    /// read takes.
    pub fn shapes(&self, image: &Image) -> Result<Shapes, ReadError> {
        let mut spans = [Span::default(); MAX_CHARACTERS];
        let (line, count) = self
            .segment(image, &mut spans)
            .map_err(|failure| match failure {
                Segmentation::NoInk => ReadError::NoInk,
                Segmentation::TooMany => ReadError::TooManyCharacters,
            })?;
        Ok(Shapes(
            spans
                .iter()
                .take(count)
                .map(|span| self.shape(image, line, *span))
                .collect(),
        ))
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

    /// The learned glyph closest to `shape`, the earliest learned of equals,
    /// or `None` before the reader learned any.
    fn closest(&self, shape: &Shape) -> Option<Closest> {
        let around = grown(shape);
        let (glyph, distance) = self
            .glyphs
            .iter()
            .map(|glyph| {
                let apart = cells(shape, &glyph.grown, |a, b| a & !b).saturating_add(cells(
                    &glyph.shape,
                    &around,
                    |a, b| a & !b,
                ));
                let differing = cells(shape, &glyph.shape, |a, b| a ^ b);
                (glyph, Distance { apart, differing })
            })
            .min_by_key(|(_, distance)| *distance)?;
        let set = cells(shape, &glyph.shape, |a, b| a | b);
        Some(Closest {
            character: glyph.character,
            matches: distance.apart.saturating_mul(CELLS_PER_APART) <= set,
        })
    }
}

/// `shape` and every cell beside one of its cells: left, right, above, or
/// below.
fn grown(shape: &Shape) -> Shape {
    let above = std::iter::once(&0).chain(shape);
    let below = shape.iter().skip(1).chain(std::iter::once(&0));
    let mut grown = Shape::default();
    for (((cells, &here), &up), &down) in grown.iter_mut().zip(shape).zip(above).zip(below) {
        *cells = here | here << 1 | here >> 1 | up | down;
    }
    grown
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
    ended: bool,
    point: Point,
    lead: Lead,
    /// The length of `text` at the last `,` between digit groups, while
    /// the digits after it may still turn out not to be a group of three.
    group: Option<usize>,
    /// The digits since the number started or since its last `,`.
    run: usize,
}

/// What came before a number's first digit, for a `.` there.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Lead {
    /// Nothing that changes how a `.` reads.
    #[default]
    Plain,
    /// A `.`, as in `.5`: the number reads as `0.` if a digit follows, and
    /// the `.` is skipped otherwise.
    Point,
    /// A letter, such as the `v` of `Lv.5`, so a `.` is not the number's.
    Letter,
}

/// What a `.` after the digits does to a number being read.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Point {
    /// It starts the number's fraction.
    #[default]
    Starts,
    /// The fraction has started, so another `.` is not part of the number.
    Taken,
    /// It ends the number: the value reads whole numbers only.
    Ends,
}

impl Point {
    const fn for_whole(whole: bool) -> Self {
        if whole { Self::Ends } else { Self::Starts }
    }
}

impl NumberText {
    fn push(&mut self, character: char) {
        if self.ended {
            return;
        }
        if self.lead == Lead::Point && !character.is_ascii_digit() {
            // The `.` started no number, so it is skipped like any other
            // character before one, as in `...`.
            self.lead = Lead::Plain;
            self.len = 0;
        }
        if character != ',' && !character.is_ascii_digit() && !self.close_groups() {
            self.ended = true;
            return;
        }
        let accepted = match character {
            '0'..='9' => {
                if self.lead == Lead::Point {
                    self.lead = Lead::Plain;
                    self.push('0');
                    self.push('.');
                    if self.ended {
                        return;
                    }
                }
                self.digits = true;
                self.run = self.run.saturating_add(1);
                true
            },
            '-' => self.len == 0,
            // A number may start with its point, such as `.5`, which reads
            // as `0.5` once its digit comes.
            '.' if !self.digits && self.lead == Lead::Plain => {
                self.lead = Lead::Point;
                return;
            },
            '.' if self.digits && self.point == Point::Starts => {
                self.point = Point::Taken;
                true
            },
            ',' if self.starts_group() => {
                self.group = Some(self.len);
                self.run = 0;
                return;
            },
            // Any other character, or a `.` once the fraction is taken or
            // for a whole number, ends the number after its digits.
            _ => false,
        };
        if !accepted {
            self.ended = self.digits;
            if !self.digits {
                self.len = 0;
                if character.is_alphabetic() {
                    self.lead = Lead::Letter;
                }
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

    /// Whether a `,` here separates digit groups, as in `1,234`: it comes
    /// after one to three digits, or after a whole group of three, and
    /// before the point.
    fn starts_group(&self) -> bool {
        let group_before = match self.group {
            Some(_) => self.run == 3,
            None => (1..=3).contains(&self.run),
        };
        self.digits && self.point != Point::Taken && group_before
    }

    /// Where the number ends when the digits after its last `,` are not a
    /// group of three, as in `12,5`: at that `,`.
    fn short_group(&self) -> Option<usize> {
        self.group.filter(|_| self.run != 3)
    }

    /// Ends the digit groups before a character that is not a digit or a
    /// `,`. When the last group is short, the number ends at its `,` and
    /// this returns false.
    fn close_groups(&mut self) -> bool {
        let short = self.short_group();
        self.group = None;
        if let Some(start) = short {
            self.len = start;
        }
        short.is_none()
    }

    fn parse(&self) -> Option<Number> {
        if !self.digits {
            return None;
        }
        let len = self.short_group().unwrap_or(self.len);
        let text = std::str::from_utf8(self.text.get(..len)?).ok()?;
        Number::new(text.trim_end_matches('.').parse().ok()?)
    }
}

/// The characters a read made out, left to right, with a `?` for each
/// whose shape matched no learned glyph. It shows as `8 7 ?`.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadText {
    characters: [char; MAX_CHARACTERS],
    len: usize,
}

impl ReadText {
    /// Stands for a character whose shape matched no learned glyph.
    const UNKNOWN: char = '?';

    /// Adds `character` at the right. A segmented image holds no more
    /// characters than there is room for.
    fn push(&mut self, character: char) {
        if let Some(slot) = self.characters.get_mut(self.len) {
            *slot = character;
            self.len = self.len.saturating_add(1);
        }
    }
}

impl fmt::Debug for ReadText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ReadText({self})")
    }
}

impl fmt::Display for ReadText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (position, character) in self.characters.iter().take(self.len).enumerate() {
            if position > 0 {
                formatter.write_str(" ")?;
            }
            write!(formatter, "{character}")?;
        }
        Ok(())
    }
}

/// A screenshot could not teach a reader its glyphs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LearnError {
    /// No pixel was near the text color.
    NoInk,
    /// The image held more than 24 characters.
    TooManyCharacters,
    /// The image held fewer characters than the text, as when characters
    /// share a column of ink.
    FewerCharacters {
        /// Characters in the image.
        found: usize,
        /// Characters in the text, without whitespace.
        expected: usize,
    },
    /// The image held more characters than the text, as when a character
    /// is drawn as marks with a gap between them, or something else in the
    /// region has the characters' color.
    MoreCharacters {
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
            Self::NoInk => formatter.write_str(NO_INK),
            Self::TooManyCharacters => formatter.write_str(TOO_MANY),
            Self::FewerCharacters { found, expected } => write!(
                formatter,
                "the region shows {found} characters but the text has {expected}; characters \
                 that touch count as one"
            ),
            Self::MoreCharacters { found, expected } => write!(
                formatter,
                "the region shows {found} characters but the text has {expected}; a character \
                 drawn as separate marks side by side, as some percent signs are, counts once \
                 for each mark, and so does anything else in the characters' color"
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

/// What a read or a lesson says when the region has no ink.
const NO_INK: &str = "no pixel in the region has the characters' color";

/// What a read or a lesson says when the region has too many characters.
const TOO_MANY: &str = "the region holds more than 24 characters";
const _: () = assert!(MAX_CHARACTERS == 24, "TOO_MANY states the limit");

/// A reader could not read a number from a region. Each variant is one
/// stage of a read, in the order a read goes through them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// No pixel was near the text color.
    NoInk,
    /// The region held more than 24 characters.
    TooManyCharacters,
    /// A character matched no learned glyph.
    UnknownCharacter {
        /// Its position from the left, from 0. It is the first such
        /// character.
        position: usize,
        /// The character of the learned glyph its shape is closest to, or
        /// `None` when the reader has learned no glyph.
        closest: Option<char>,
        /// Every character the read made out.
        read: ReadText,
    },
    /// Every character matched a learned glyph, but they held no number.
    NoNumber {
        /// The characters.
        read: ReadText,
    },
}

impl ReadError {
    /// The stage that failed, without what this read saw there. A region
    /// that keeps failing at one stage keeps this text while its characters
    /// change, so a log can say it once.
    #[must_use]
    pub const fn stage(&self) -> &'static str {
        match self {
            Self::NoInk => NO_INK,
            Self::TooManyCharacters => TOO_MANY,
            Self::UnknownCharacter { .. } => {
                "a character has a shape the reference has not learned"
            },
            Self::NoNumber { .. } => "the characters hold no number",
        }
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.stage())?;
        match self {
            Self::NoInk | Self::TooManyCharacters => Ok(()),
            Self::UnknownCharacter {
                position,
                closest,
                read,
            } => {
                // People count characters from 1. The read may list fewer
                // characters than the region shows, so it gives no total.
                let number = position.saturating_add(1);
                write!(formatter, ": it read `{read}`, and character {number} is ")?;
                match closest {
                    Some(closest) => {
                        write!(formatter, "closest to the `{closest}` it learned")
                    },
                    None => formatter.write_str("unlike any, since it has learned none"),
                }
            },
            Self::NoNumber { read } => write!(formatter, ": it read `{read}`"),
        }
    }
}

impl Error for ReadError {}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::drawn::{Pen, Percent, draw as draw_text};
    use super::*;
    use crate::screen::Size;

    /// The characters of `text` as a read made them out.
    fn read_text(text: &str) -> ReadText {
        let mut read = ReadText::default();
        text.chars().for_each(|character| read.push(character));
        read
    }

    #[test]
    fn errors_explain_themselves() {
        for (error, message) in [
            (
                LearnError::NoInk,
                "no pixel in the region has the characters' color",
            ),
            (
                LearnError::TooManyCharacters,
                "the region holds more than 24 characters",
            ),
            (
                LearnError::FewerCharacters {
                    found: 2,
                    expected: 3,
                },
                "the region shows 2 characters but the text has 3; characters that touch count \
                 as one",
            ),
            (
                LearnError::MoreCharacters {
                    found: 5,
                    expected: 3,
                },
                "the region shows 5 characters but the text has 3; a character drawn as \
                 separate marks side by side, as some percent signs are, counts once for each \
                 mark, and so does anything else in the characters' color",
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
        for (error, stage, seen) in [
            (
                ReadError::NoInk,
                "no pixel in the region has the characters' color",
                "",
            ),
            (
                ReadError::TooManyCharacters,
                "the region holds more than 24 characters",
                "",
            ),
            (
                ReadError::UnknownCharacter {
                    position: 2,
                    closest: Some('%'),
                    read: read_text("87?"),
                },
                "a character has a shape the reference has not learned",
                ": it read `8 7 ?`, and character 3 is closest to the `%` it learned",
            ),
            (
                ReadError::UnknownCharacter {
                    position: 0,
                    closest: None,
                    read: read_text("?"),
                },
                "a character has a shape the reference has not learned",
                ": it read `?`, and character 1 is unlike any, since it has learned none",
            ),
            (
                ReadError::NoNumber {
                    read: read_text("%-"),
                },
                "the characters hold no number",
                ": it read `% -`",
            ),
        ] {
            assert_eq!(error.stage(), stage);
            assert_eq!(error.to_string(), format!("{stage}{seen}"));
        }
    }

    #[test]
    fn a_read_keeps_no_more_characters_than_an_image_holds() {
        let crowded = "9".repeat(MAX_CHARACTERS + 1);
        assert_eq!(read_text(&crowded), read_text(&"9".repeat(MAX_CHARACTERS)));
        assert_eq!(read_text("").to_string(), "");
        assert_eq!(format!("{:?}", read_text("8?")), "ReadText(8 ?)");
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
        number_text(characters, false)
    }

    fn number_text(characters: &str, whole: bool) -> NumberText {
        let mut number = NumberText {
            point: Point::for_whole(whole),
            ..NumberText::default()
        };
        characters
            .chars()
            .for_each(|character| number.push(character));
        number
    }

    #[test]
    fn numbers_take_the_first_run_of_number_characters() {
        let parsed = |characters| text(characters).parse();
        assert_eq!(parsed("37%"), Number::new(37.0));
        assert_eq!(parsed("0%"), Number::new(0.0));
        assert_eq!(parsed("100%"), Number::new(100.0));
        assert_eq!(parsed("-3%"), Number::new(-3.0));
        assert_eq!(parsed("87%%"), Number::new(87.0));
        assert_eq!(parsed("%"), None);
        assert_eq!(parsed("90.5%"), Number::new(90.5));
        assert_eq!(parsed("87.52%"), Number::new(87.52));
        assert_eq!(parsed("-12"), Number::new(-12.0));
        assert_eq!(parsed("%12/50"), Number::new(12.0));
        assert_eq!(parsed("12/50"), Number::new(12.0));
        assert_eq!(parsed("1.2.3"), Number::new(1.2));
        assert_eq!(parsed("7."), Number::new(7.0));
        assert_eq!(parsed("-%5"), Number::new(5.0));
        assert_eq!(parsed("%-"), None);
        assert_eq!(parsed("1-2"), Number::new(1.0));
        let longest = "9".repeat(MAX_NUMBER_LEN);
        assert_eq!(
            parsed(&"9".repeat(MAX_NUMBER_LEN + 4)),
            Number::new(longest.parse().unwrap())
        );
    }

    #[test]
    fn a_number_that_starts_with_its_point_reads_as_zero_point() {
        let parsed = |characters| text(characters).parse();
        assert_eq!(parsed(".5"), Number::new(0.5));
        assert_eq!(parsed("-.5"), Number::new(-0.5));
        assert_eq!(parsed(".25%"), Number::new(0.25));
        assert_eq!(parsed("-.5.5"), Number::new(-0.5));
        assert_eq!(parsed("$.50"), Number::new(0.5));
        assert_eq!(parsed("+.5"), Number::new(0.5));
        // After letters, such as those of a level label, a `.` is not the
        // number's.
        assert_eq!(parsed("x.5"), Number::new(5.0));
        assert_eq!(parsed("-x.5"), Number::new(5.0));
        assert_eq!(parsed("Lv.5"), Number::new(5.0));
        // A point with no digit after it is no number, such as a loading
        // ellipsis.
        for characters in [".", "...", "-.--", ".%", "-."] {
            assert_eq!(parsed(characters), None, "{characters}");
        }
        assert_eq!(parsed(".%5"), Number::new(5.0));
    }

    #[test]
    fn a_comma_is_skipped_only_between_groups_of_three_digits() {
        let parsed = |characters| text(characters).parse();
        assert_eq!(parsed("1,234"), Number::new(1234.0));
        assert_eq!(parsed("-12,345.5%"), Number::new(-12345.5));
        assert_eq!(parsed("1,234,567"), Number::new(1_234_567.0));
        assert_eq!(parsed("999,000/1,000"), Number::new(999_000.0));
        // Anything else ends the number at the comma.
        assert_eq!(parsed("12,5"), Number::new(12.0));
        assert_eq!(parsed("12,5%"), Number::new(12.0));
        assert_eq!(parsed("1,2345"), Number::new(1.0));
        assert_eq!(parsed("1,234,56"), Number::new(1234.0));
        assert_eq!(parsed("1,23,456"), Number::new(1.0));
        assert_eq!(parsed("1234,567"), Number::new(1234.0));
        assert_eq!(parsed("1,"), Number::new(1.0));
        assert_eq!(parsed("1,,234"), Number::new(1.0));
        assert_eq!(parsed("1,.5"), Number::new(1.0));
        assert_eq!(parsed("1.5,000"), Number::new(1.5));
        assert_eq!(parsed(",5"), Number::new(5.0));
        let whole = |characters| number_text(characters, true).parse();
        assert_eq!(whole("1,234.5"), Number::new(1234.0));
        assert_eq!(whole("1,23.5"), Number::new(1.0));
    }

    #[test]
    fn a_whole_number_ends_at_its_point() {
        let whole = |characters| number_text(characters, true).parse();
        assert_eq!(whole("90.5%"), Number::new(90.0));
        assert_eq!(whole("-7.9"), Number::new(-7.0));
        assert_eq!(whole("12"), Number::new(12.0));
        assert_eq!(whole(".5"), Number::new(0.0));
        assert_eq!(whole("-.5"), Number::new(-0.0));
        assert!(number_text("3.", true).ended);
        assert!(!number_text("3.", false).ended);
    }

    /// `rows` drawn `scale` times larger.
    fn scaled(rows: &[&str], scale: usize) -> Vec<String> {
        rows.iter()
            .flat_map(|row| {
                let wide: String = row
                    .chars()
                    .flat_map(|cell| std::iter::repeat_n(cell, scale))
                    .collect();
                std::iter::repeat_n(wide, scale)
            })
            .collect()
    }

    #[test]
    fn characters_learned_at_one_size_read_at_another() {
        let mut reader = DigitReader::new(WHITE, 0);
        reader.learn(&draw(&TWELVE), "12").unwrap();
        let big = scaled(&TWELVE, 3);
        let rows: Vec<&str> = big.iter().map(String::as_str).collect();
        assert_eq!(
            reader.read(&draw(&rows), false),
            Ok(Number::new(12.0).unwrap())
        );
    }

    #[test]
    fn a_whole_read_ignores_the_digits_after_the_point() {
        let mut reader = DigitReader::new(WHITE, 0);
        // "1" and a "." that sits at the baseline.
        reader.learn(&draw(&["#..", "#..", "#.#"]), "1.").unwrap();
        // "1.", then a character never learned.
        let fraction = draw(&["#...#.#", "#...###", "#.#.#.#"]);
        assert_eq!(
            reader.read(&fraction, false),
            Err(ReadError::UnknownCharacter {
                position: 2,
                closest: Some('1'),
                read: read_text("1.?"),
            })
        );
        assert_eq!(reader.read(&fraction, true), Ok(Number::new(1.0).unwrap()));
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
        assert_eq!(
            reader.read(&swapped, false),
            Ok(Number::new(212.0).unwrap())
        );
    }

    #[test]
    fn learning_fails_without_changing_the_reader() {
        let digits = draw(&["#.#"]);
        let mut reader = DigitReader::new(WHITE, 0);
        assert_eq!(
            reader.learn(&digits, "1"),
            Err(LearnError::MoreCharacters {
                found: 2,
                expected: 1
            })
        );
        assert_eq!(
            reader.learn(&digits, "123"),
            Err(LearnError::FewerCharacters {
                found: 2,
                expected: 3
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
            reader.read(&draw(&["###", "#.#", "###"]), false),
            Err(ReadError::NoNumber {
                read: read_text("%")
            })
        );
        // A character never learned fails the read, and the characters
        // around it are not read as a number without it.
        assert_eq!(
            reader.read(&draw(&["#.#.#.#", "#...#.#", "#.###.#"]), false),
            Err(ReadError::UnknownCharacter {
                position: 1,
                closest: Some('%'),
                read: read_text("1?1"),
            })
        );
        assert_eq!(
            DigitReader::new(WHITE, 0).read(&draw(&["#"]), false),
            Err(ReadError::UnknownCharacter {
                position: 0,
                closest: None,
                read: read_text("?"),
            })
        );
        assert_eq!(reader.read(&draw(&["..."]), false), Err(ReadError::NoInk));
        let crowded = "#.".repeat(MAX_CHARACTERS + 1);
        assert_eq!(
            reader.read(&draw(&[crowded.as_str()]), false),
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
        assert_eq!(reader.read(&labeled, false), Ok(Number::new(1.0).unwrap()));
        // After an unlearned character the read fails, and its error lists
        // the characters the same way: up to where a number ends.
        let led = draw(&["#.#.#.###.#.#", "###.#.#.#.###", "#.#.#.###.#.#"]);
        assert_eq!(
            reader.read(&led, false),
            Err(ReadError::UnknownCharacter {
                position: 0,
                closest: Some('%'),
                read: read_text("?1%"),
            })
        );
    }

    #[test]
    fn a_stray_pixel_does_not_stretch_the_line() {
        let mut reader = DigitReader::new(WHITE, 0);
        reader.learn(&draw(&TWELVE), "12").unwrap();
        let mut noisy = TWELVE.to_vec();
        noisy.extend([".....", "....#"]);
        assert_eq!(
            reader.read(&draw(&noisy), false),
            Ok(Number::new(12.0).unwrap())
        );
        // Of two runs of inked rows, the one with more ink is the line.
        let mut above = vec!["#....", "....."];
        above.extend(TWELVE);
        assert_eq!(
            reader.read(&draw(&above), false),
            Ok(Number::new(12.0).unwrap())
        );
    }

    /// `rows` with `left` and `right` blank columns beside them and `above`
    /// and `below` blank rows.
    fn padded(rows: &[&str], left: usize, right: usize, above: usize, below: usize) -> Vec<String> {
        let wide: Vec<String> = rows
            .iter()
            .map(|row| [".".repeat(left).as_str(), row, ".".repeat(right).as_str()].concat())
            .collect();
        let blank = ".".repeat(wide[0].len());
        let mut all = vec![blank.clone(); above];
        all.extend(wide);
        all.extend(vec![blank; below]);
        all
    }

    #[test]
    fn a_number_reads_wherever_it_sits_in_a_larger_region() {
        let mut reader = DigitReader::new(WHITE, 0);
        reader.learn(&draw(&TWELVE), "12").unwrap();
        for (left, right, above, below) in [(0, 9, 0, 6), (4, 5, 3, 3), (9, 0, 6, 0)] {
            let region = padded(&TWELVE, left, right, above, below);
            let rows: Vec<&str> = region.iter().map(String::as_str).collect();
            assert_eq!(
                reader.read(&draw(&rows), false),
                Ok(Number::new(12.0).unwrap()),
                "{left} columns to its left and {above} rows above it"
            );
        }
    }

    #[test]
    fn ink_beside_a_number_in_a_larger_region_is_read_with_it() {
        let mut reader = DigitReader::new(WHITE, 0);
        reader.learn(&draw(&TWELVE), "12").unwrap();
        // A "1" three columns to the left of "12", on its rows.
        let digit: Vec<String> = TWELVE.iter().map(|row| format!("#...{row}")).collect();
        let rows: Vec<&str> = digit.iter().map(String::as_str).collect();
        assert_eq!(
            reader.read(&draw(&rows), false),
            Ok(Number::new(112.0).unwrap())
        );
        // A shape the reader never learned there fails the read.
        let shape: Vec<String> = ["#...#", ".#.#.", "..#..", ".#.#.", "#...#"]
            .iter()
            .zip(TWELVE)
            .map(|(shape, row)| format!("{shape}...{row}"))
            .collect();
        let rows: Vec<&str> = shape.iter().map(String::as_str).collect();
        assert!(matches!(
            reader.read(&draw(&rows), false),
            Err(ReadError::UnknownCharacter { position: 0, .. })
        ));
    }

    #[test]
    fn shapes_tell_characters_apart_but_not_their_size() {
        let reader = DigitReader::new(WHITE, 0);
        let twelve = reader.shapes(&draw(&TWELVE)).unwrap();
        assert_eq!(twelve.0.len(), 2);
        let mut noisy = TWELVE.to_vec();
        noisy.extend([".....", "....#"]);
        assert_eq!(reader.shapes(&draw(&noisy)), Ok(twelve.clone()));
        let mut seventeen = TWELVE;
        seventeen[3] = "#...#";
        assert_ne!(reader.shapes(&draw(&seventeen)), Ok(twelve));
        assert_eq!(reader.shapes(&draw(&["..."])), Err(ReadError::NoInk));
        let crowded = "#.".repeat(MAX_CHARACTERS + 1);
        assert_eq!(
            reader.shapes(&draw(&[crowded.as_str()])),
            Err(ReadError::TooManyCharacters)
        );
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

    /// A shape from `rows` of cells, `#` set, from the top left.
    fn shape_of(rows: &[&str]) -> Shape {
        let mut shape = Shape::default();
        for (cells, row) in shape.iter_mut().zip(rows) {
            for (column, cell) in row.chars().enumerate() {
                if cell == '#' {
                    *cells |= 1 << column;
                }
            }
        }
        shape
    }

    /// A reader that learned each of `glyphs`, in order.
    fn reader_of(glyphs: &[(char, Shape)]) -> DigitReader {
        let mut reader = DigitReader::new(WHITE, 0);
        reader.glyphs = glyphs
            .iter()
            .map(|&(character, shape)| Glyph::new(character, shape))
            .collect();
        reader
    }

    /// Eight rows of eight cells: 64 cells.
    const BLOCK: [&str; 8] = ["########"; 8];

    #[test]
    fn a_shape_grows_by_the_cells_beside_its_cells() {
        assert_eq!(
            grown(&shape_of(&["...", ".#.", "..."])),
            shape_of(&[".#.", "###", ".#."])
        );
        // A cell beside two of the shape's cells is set once, whichever
        // sides they are on: left and right, above and below, a cell and
        // the one right of it, or the one right of it and the one above.
        assert_eq!(
            grown(&shape_of(&["...", "#.#", "..."])),
            shape_of(&["#.#", "####", "#.#"])
        );
        assert_eq!(
            grown(&shape_of(&[".#.", "...", ".#."])),
            shape_of(&["###", ".#.", "###", ".#."])
        );
        assert_eq!(
            grown(&shape_of(&["...", ".##", "..."])),
            shape_of(&[".##", "####", ".##"])
        );
        assert_eq!(
            grown(&shape_of(&["...", ".#.", ".#.", "..."])),
            shape_of(&[".#.", "###", "###", ".#."])
        );
        assert_eq!(
            grown(&shape_of(&["...", ".#.", "..#", "..."])),
            shape_of(&[".#.", "###", ".###", "..#"])
        );
        // The grid's first row and column have nothing above or left.
        assert_eq!(grown(&shape_of(&["#"])), shape_of(&["##", "#."]));
        let mut last = Shape::default();
        last[15] = 1 << 31;
        let mut around = Shape::default();
        around[14] = 1 << 31;
        around[15] = 0b11 << 30;
        assert_eq!(grown(&last), around);
    }

    #[test]
    fn a_shape_whose_edges_moved_a_cell_matches() {
        let reader = reader_of(&[('7', shape_of(&BLOCK))]);
        let matches = |rows: &[&str]| {
            reader
                .closest(&shape_of(rows))
                .is_some_and(|closest| closest.matches)
        };
        // A column wider, which no rule on differing cells alone lets
        // through next to a missing stroke of the same size.
        assert!(matches(&["#########"; 8]));
        // A row taller and a column narrower.
        assert!(matches(&[".#######"; 9]));
        // Two columns wider: the second is apart, 8 cells of 80.
        assert!(!matches(&["##########"; 8]));
    }

    #[test]
    fn a_shape_matches_while_at_most_1_cell_in_32_is_apart() {
        let reader = reader_of(&[('7', shape_of(&BLOCK))]);
        let with = |extra: &str| {
            let mut rows = BLOCK.to_vec();
            rows.extend(["........", extra]);
            reader
                .closest(&shape_of(&rows))
                .is_some_and(|closest| closest.matches)
        };
        // Cells two rows below the block are beside none of its cells.
        // Two apart of 66 set match; three of 67 do not.
        assert!(with("#.#....."));
        assert!(!with("#.#.#..."));
        // The share is of the cells either sets, not of those both set:
        // a shape a column narrower, with 56 cells of the block, still
        // matches with two cells apart of 66.
        let mut narrower = vec![".#######"; 8];
        narrower.extend(["........", "#.#....."]);
        assert!(
            reader
                .closest(&shape_of(&narrower))
                .is_some_and(|closest| closest.matches)
        );
        // Exactly 1 in 32: two apart of the 64 that a block of 62 and they
        // set.
        let mut smaller = vec!["########"; 7];
        smaller.push("######..");
        let exact = reader_of(&[('7', shape_of(&smaller))]);
        smaller.extend(["........", "#.#....."]);
        assert_eq!(
            exact.closest(&shape_of(&smaller)),
            Some(Closest {
                character: '7',
                matches: true
            })
        );
        // A part the shape lacks is as far apart as one it adds.
        let mut rows = BLOCK.to_vec();
        rows.extend(["........", "#.#.#..."]);
        let larger = reader_of(&[('7', shape_of(&rows))]);
        assert_eq!(
            larger.closest(&shape_of(&BLOCK)),
            Some(Closest {
                character: '7',
                matches: false
            })
        );
    }

    #[test]
    fn the_closest_glyph_has_the_fewest_cells_apart_then_the_fewest_differing() {
        let mut far = BLOCK.to_vec();
        far.extend(["........", "#......."]);
        let wide = ["#########"; 8];
        // The shape is the block with one cell apart from it. A glyph a
        // column wider than the shape differs in 8 cells, none apart, and
        // one without the shape's far cell differs in 1, which is apart.
        let mut shifted = wide.to_vec();
        shifted.extend(["........", "#........"]);
        let reader = reader_of(&[('1', shape_of(&BLOCK)), ('2', shape_of(&shifted))]);
        assert_eq!(
            reader
                .closest(&shape_of(&far))
                .map(|closest| closest.character),
            Some('2')
        );
        // With no cell apart, the glyph that differs least is closest,
        // wherever it was learned.
        for glyphs in [
            [('1', shape_of(&wide)), ('2', shape_of(&BLOCK))],
            [('2', shape_of(&BLOCK)), ('1', shape_of(&wide))],
        ] {
            assert_eq!(
                reader_of(&glyphs)
                    .closest(&shape_of(&BLOCK))
                    .map(|closest| closest.character),
                Some('2')
            );
        }
        // The cells that differ count, not the cells either sets: a glyph a
        // column wider differs from the block in 8 cells, and one a column
        // narrower on each side in 16, though it sets none the block does
        // not. Neither has a cell apart.
        let smaller = [".######."; 8];
        for glyphs in [
            [('1', shape_of(&smaller)), ('2', shape_of(&wide))],
            [('2', shape_of(&wide)), ('1', shape_of(&smaller))],
        ] {
            assert_eq!(
                reader_of(&glyphs)
                    .closest(&shape_of(&BLOCK))
                    .map(|closest| closest.character),
                Some('2')
            );
        }
        // Of two equally close, the one learned first.
        let twins = reader_of(&[('1', shape_of(&BLOCK)), ('2', shape_of(&BLOCK))]);
        assert_eq!(
            twins
                .closest(&shape_of(&wide))
                .map(|closest| closest.character),
            Some('1')
        );
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
        assert_eq!(
            reader.read(&draw(&tied), false),
            Ok(Number::new(12.0).unwrap())
        );
    }

    #[test]
    fn a_reader_learns_at_most_256_glyphs() {
        let mut reader = DigitReader::new(WHITE, 0);
        for index in 0..MAX_GLYPHS {
            let mut shape = Shape::default();
            shape[0] = u32::try_from(index).unwrap().wrapping_add(1) << 16;
            reader.glyphs.push(Glyph::new('x', shape));
        }
        assert_eq!(reader.learn(&draw(&["#"]), "1"), Err(LearnError::Full));
    }

    /// A canvas with room for a percentage whose digits are `height`
    /// pixels tall.
    fn canvas(height: u32) -> Size {
        Size {
            width: height.checked_mul(8).expect("a small canvas"),
            height: height.checked_mul(2).expect("a small canvas"),
        }
    }

    /// `text` drawn as an application draws it, with digits `height` pixels
    /// tall, starting `across` pixels from the left.
    fn drawn(text: &str, height: u32, across: f64) -> Image {
        draw_text(text, Pen::new(f64::from(height), across), canvas(height))
    }

    /// A reader that learned `labels`, drawn with digits `height` pixels
    /// tall, each at another fraction of a pixel, as a user's few labeled
    /// screenshots show them.
    fn taught(height: u32, labels: &[&str]) -> DigitReader {
        let mut reader = DigitReader::new(WHITE, 40);
        for (text, across) in labels.iter().zip([3.1, 3.47, 3.84]) {
            reader.learn(&drawn(text, height, across), text).unwrap();
        }
        reader
    }

    /// Three labels that show every digit, the point, and the percent sign.
    const LABELS: [&str; 3] = ["12.34%", "56.78%", "90%"];

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        /// An application that scales its interface, or centers a number
        /// whose width changes, draws the same character at another
        /// fraction of a pixel each time, so its edge pixels differ from
        /// the ones the reference learned.
        #[test]
        fn a_percentage_reads_wherever_it_lands_on_the_pixel_grid(
            height in prop::sample::select(vec![16_u32, 20, 28, 40]),
            hundredths in 0_u32..=10_000,
            decimals in any::<bool>(),
            across in 3.0..4.0_f64,
        ) {
            let whole = hundredths.div_euclid(100);
            let (text, shown) = if decimals {
                (format!("{whole}.{:02}%", hundredths.rem_euclid(100)), f64::from(hundredths) / 100.0)
            } else {
                (format!("{whole}%"), f64::from(whole))
            };
            let reader = taught(height, &LABELS);
            prop_assert_eq!(
                reader.read(&drawn(&text, height, across), false),
                Ok(Number::new(shown).unwrap()),
                "{} at {}", text, across
            );
        }
    }

    #[test]
    fn a_character_drawn_half_a_pixel_aside_still_reads() {
        // The 3 was learned from "12.34%", drawn from 3.1 pixels across.
        let reader = taught(16, &LABELS);
        assert_eq!(
            reader.read(&drawn("23%", 16, 3.58), false),
            Ok(Number::new(23.0).unwrap())
        );
    }

    #[test]
    fn text_an_application_draws_reads_at_another_size_than_it_was_learned_at() {
        for (learned, shown) in [(20, 30), (30, 20), (16, 32)] {
            let reader = taught(learned, &LABELS);
            for (text, whole, across) in [
                ("74.05%", 74.0, 3.0),
                ("100%", 100.0, 3.25),
                ("8.69%", 8.0, 3.6),
            ] {
                assert_eq!(
                    reader.read(&drawn(text, shown, across), true),
                    Ok(Number::new(whole).unwrap()),
                    "{text} learned at {learned}, shown at {shown}"
                );
            }
        }
    }

    #[test]
    fn a_digit_the_reference_never_learned_is_not_read_as_one_it_looks_like() {
        // Every digit but 8, which is a 6 or a 3 with a stroke more.
        let reader = taught(28, &["12.34%", "56.79%", "0%"]);
        for across in [3.0, 3.3, 3.7] {
            let read = reader.read(&drawn("68.3%", 28, across), false);
            assert!(
                matches!(
                    read,
                    Err(ReadError::UnknownCharacter {
                        position: 1,
                        closest: Some(_),
                        ..
                    })
                ),
                "{read:?}"
            );
        }
    }

    #[test]
    fn a_percent_sign_drawn_as_three_marks_is_three_characters() {
        let apart = |text: &str| {
            let pen = Pen {
                percent: Percent::Apart,
                ..Pen::new(20.0, 3.0)
            };
            draw_text(text, pen, canvas(20))
        };
        let mut reader = DigitReader::new(WHITE, 40);
        assert_eq!(
            reader.learn(&apart("87%"), "87%"),
            Err(LearnError::MoreCharacters {
                found: 5,
                expected: 3
            })
        );
        // A character typed for each mark teaches it, and the first of
        // them ends the number.
        reader.learn(&apart("87%"), "87o/o").unwrap();
        assert_eq!(
            reader.read(&apart("778%"), false),
            Ok(Number::new(778.0).unwrap())
        );
    }
}
