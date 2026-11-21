//! Reads a number from a region by matching its characters against glyphs
//! learned from the user's screenshots.
//!
//! An application draws its counter in one font, so the screenshot's own
//! glyphs are the best possible templates. It may draw it in another color
//! or at another size a moment later, so a reader keeps neither: it knows
//! each character by its shape alone. Reading is a fixed sequence of
//! integer passes, so it is deterministic and costs well under a
//! millisecond, where general text recognition costs milliseconds and can
//! differ between machines.
//!
//! 1. **Layers.** The image is split into its few colors, each a layer
//!    that may hold the text ([`layers`]). The steps below run on every
//!    layer.
//! 2. **Line.** The run of adjacent inked rows that holds the most ink
//!    ([`Ink::line`]).
//! 3. **Characters.** Runs of columns that hold ink in the line, left to
//!    right. Characters must not touch.
//! 4. **Shapes.** Each character is scaled to a grid by its own bounds, and
//!    keeps how tall it is and where it starts in its line ([`shape`]).
//! 5. **Matching.** Each shape takes the character of the closest learned
//!    glyph, if the two are within [`MATCH_WITHIN`] and no glyph of another
//!    character is within [`CLEAR_BY`] more. Only the closest glyph is
//!    asked, and a shape two characters are about as close to is neither:
//!    a read fails rather than guess.
//! 6. **The layer.** The layer the reader knows the most of is the text.
//!    Where another layer it knows about as much of says otherwise, or
//!    other colors show something of a character's size beside the
//!    number, the read fails.

use std::cmp::Ordering;
use std::error::Error;
use std::fmt;
use std::ops::Range;

use appytappy_config::app_file::is_label_character;
use appytappy_config::signal::Number;

use self::layers::{Layer, Layers};
use self::shape::{Bounds, FARTHEST, Line, Place, Sample};
use super::{Image, Region, Rgb, Size};

#[cfg(test)]
pub(crate) mod drawn;
mod ink;
mod layers;
mod shape;

pub use ink::Ink;
pub use shape::Shape;

/// The most characters one read or one screenshot may hold.
pub const MAX_CHARACTERS: usize = 24;

/// The most glyphs a reader learns.
pub const MAX_GLYPHS: usize = 256;

/// The longest number a read assembles, in characters.
const MAX_NUMBER_LEN: usize = 24;

/// How far a shape may be from a glyph and still be its character, in the
/// thousandths [`GlyphScore::distance`] counts.
const MATCH_WITHIN: u16 = 200;

/// How much further than the closest glyph the closest glyph of another
/// character has to be for the shape to be the first one's character.
const CLEAR_BY: u16 = 50;

/// How close a new glyph may be to a learned one of the same character and
/// add nothing to it: the reader keeps the one it has.
const SAME_GLYPH: u16 = 30;

/// What tells the text a search looks for from the rest of an image, from
/// [`DigitReader::tone`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Text of this color.
    Color(Rgb),
    /// Text of no one color, on a background of this color.
    Unlike(Rgb),
}

impl Tone {
    /// Which pixels of `image` are ink of this tone.
    #[must_use]
    pub fn ink(self, image: &Image) -> Ink {
        let inked = image
            .pixels()
            .iter()
            .map(|&pixel| {
                let pixel = Rgb::of_pixel(pixel);
                match self {
                    Self::Color(color) => layers::same(pixel, color),
                    Self::Unlike(background) => !layers::same(pixel, background),
                }
            })
            .collect();
        Ink::new(inked, image.size())
    }
}

/// The shapes of an image's characters, from [`DigitReader::shapes`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shapes(Vec<Shape>);

impl Shapes {
    /// Each character's shape, left to right. Two characters with equal
    /// shapes look the same to a reader, so a picture whose shapes a
    /// reader has all seen teaches it next to nothing.
    #[must_use]
    pub fn characters(&self) -> &[Shape] {
        &self.0
    }
}

/// One learned character and its shape.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Glyph {
    character: char,
    sample: Sample,
}

/// What a read takes one character's shape for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Taken {
    /// A character the value reads.
    Kept(char),
    /// A character the value leaves out, which the number takes as text.
    LeftOut(char),
    /// No character: the shape matches no glyph, or two characters' glyphs
    /// about as well. It carries the character of the glyph closest to it
    /// among those the value reads, if one is nearer than the farthest a
    /// shape can be.
    Unknown(Option<char>),
}

/// One character of a layer: where it is in the image and in its line.
#[derive(Debug, Clone)]
struct Mark {
    bounds: Bounds,
    /// How many of its pixels are ink.
    ink: u32,
    place: Place,
}

/// The characters of one layer, as a read splits them.
#[derive(Debug)]
struct Sight {
    layer: Layer,
    ink: Ink,
    /// The rows of the line of text.
    rows: Range<u32>,
    /// What the characters' heights are measured against.
    line: Line,
    marks: Vec<Mark>,
}

impl Sight {
    /// What a reader compares of the character `mark` of this layer of
    /// `layers`.
    fn sample(&self, layers: &Layers, mark: &Mark) -> Sample {
        let share = |x, y| layers.share(self.layer, &self.ink, x, y);
        Sample::of(share, &mark.bounds, mark.place)
    }

    /// What a reader compares of each character, left to right.
    fn samples<'a>(&'a self, layers: &'a Layers) -> impl Iterator<Item = Sample> + 'a {
        self.marks.iter().map(|mark| self.sample(layers, mark))
    }
}

/// What a reader makes of one layer.
#[derive(Debug)]
struct Attempt {
    sight: Sight,
    /// What it takes each character for, left to right.
    taken: Vec<Taken>,
}

impl Attempt {
    /// How much of the layer the reader knows: the pixels across of the
    /// characters it matched. A layer of specks can match a `.` many
    /// times over, so the characters are not counted.
    fn known(&self) -> u32 {
        self.taken
            .iter()
            .zip(&self.sight.marks)
            .filter(|(taken, _)| !matches!(taken, Taken::Unknown(_)))
            .map(|(_, mark)| mark.bounds.columns.end.abs_diff(mark.bounds.columns.start))
            .fold(0, u32::saturating_add)
    }

    /// The layers the reader knows the most of, in the order of
    /// `attempts`. One that it knows less than half as much of as another
    /// is no contender: a speck that happens to match, or the inside of a
    /// `0`.
    fn contenders(attempts: &[Self]) -> impl Iterator<Item = &Self> {
        let most = attempts.iter().map(Self::known).max().unwrap_or(0);
        attempts
            .iter()
            .filter(move |attempt| attempt.known().saturating_mul(2) >= most)
    }

    /// The layer a read takes its characters from: of the contenders
    /// whose every character the reader knows, the one it knows the most
    /// of, or of all the contenders when there is none, the earliest of
    /// equals. Where the reader knows no character of any layer, it is
    /// the layer a lesson would learn from ([`whole_text`]).
    fn chosen<'a>(attempts: &'a [Self], layers: &Layers) -> Option<&'a Self> {
        let whole = |attempt: &&Self| {
            !attempt
                .taken
                .iter()
                .any(|taken| matches!(taken, Taken::Unknown(_)))
        };
        let most = |attempts: &mut dyn Iterator<Item = &'a Self>| {
            attempts.fold(None, |best: Option<&'a Self>, attempt| match best {
                Some(best) if best.known() >= attempt.known() => Some(best),
                _ => Some(attempt),
            })
        };
        let chosen = most(&mut Self::contenders(attempts).filter(whole))
            .or_else(|| most(&mut Self::contenders(attempts)))?;
        if chosen.known() > 0 {
            return Some(chosen);
        }
        let last = |attempt: &Self| {
            layers.is_background(attempt.sight.layer) || plate(layers, &attempt.sight)
        };
        let of = |wanted: &dyn Fn(&Self) -> bool| attempts.iter().find(|attempt| wanted(attempt));
        let color = of(&|attempt| attempt.sight.layer != Layer::Every && !last(attempt));
        let every = of(&|attempt| attempt.sight.layer == Layer::Every);
        let enclosed = of(&|attempt| attempt.sight.layer != Layer::Every && last(attempt));
        let plated = attempts.iter().any(|attempt| plate(layers, &attempt.sight));
        match (color, every) {
            (Some(color), Some(every)) if !plated && !whole_text(&color.sight, &every.sight) => {
                Some(every)
            },
            _ => color.or(every).or(enclosed),
        }
    }

    /// Whether `other` says the text is another than this layer says: it
    /// takes a character for another one than this layer takes it for, or
    /// has a character a number can hold where this layer has none. A
    /// shape `other` does not know says nothing, and neither does a letter
    /// or a symbol beside this layer's text, which is a label in another
    /// color.
    fn disagrees(&self, other: &Self) -> bool {
        other
            .taken
            .iter()
            .zip(&other.sight.marks)
            .any(|(taken, mark)| {
                let (Taken::Kept(character) | Taken::LeftOut(character)) = *taken else {
                    return false;
                };
                let columns = &mark.bounds.columns;
                let mut here = self
                    .taken
                    .iter()
                    .zip(&self.sight.marks)
                    .filter(|(_, own)| {
                        own.bounds.columns.start < columns.end
                            && columns.start < own.bounds.columns.end
                    })
                    .map(|(own, _)| *own)
                    .peekable();
                if here.peek().is_none() {
                    return character.is_ascii_digit() || matches!(character, '-' | '.' | ',');
                }
                here.any(|own| match own {
                    Taken::Kept(own) | Taken::LeftOut(own) => own != character,
                    Taken::Unknown(_) => false,
                })
            })
    }

    /// The characters this layer holds as the reader made them out.
    fn read(&self) -> ReadText {
        let mut read = ReadText::default();
        for taken in &self.taken {
            read.push(match *taken {
                Taken::Kept(character) | Taken::LeftOut(character) => character,
                Taken::Unknown(_) => ReadText::UNKNOWN,
            });
        }
        read
    }

    /// The ink of the characters the reader matched, in pixels.
    fn known_ink(&self) -> u32 {
        self.taken
            .iter()
            .zip(&self.sight.marks)
            .filter(|(taken, _)| !matches!(taken, Taken::Unknown(_)))
            .map(|(_, mark)| mark.ink)
            .fold(0, u32::saturating_add)
    }
}

/// How many times the ink of a layer the ink of the characters matched in
/// another may be before the layer is passed over: a layer with so little
/// ink holds none of the text, only the blend along its edges.
const INK_TO_COUNT: u32 = 4;

/// Reads a number from glyphs learned from screenshots of the same
/// counter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DigitReader {
    /// The glyphs a read takes its characters from.
    glyphs: Vec<Glyph>,
    /// The glyphs of the characters a value leaves out, which
    /// [`DigitReader::reading`] moves here. A read knows their shapes as
    /// text that is no part of its number.
    left_out: Vec<Glyph>,
}

impl DigitReader {
    /// A reader that knows no glyphs until it learns some.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            glyphs: Vec::new(),
            left_out: Vec::new(),
        }
    }

    /// Learns the glyphs in `image`, which shows `text`. Whitespace in `text`
    /// is ignored; every other character must match one character in the
    /// image, left to right.
    ///
    /// The characters are those of the color with the most ink that splits
    /// into as many characters as `text` has: the inside of text with an
    /// outline, where the outlines run into each other. Text that is no
    /// one color, such as a gradient, is every pixel that is not
    /// background.
    ///
    /// # Errors
    ///
    /// Returns a [`LearnError`] when the image's characters do not line up
    /// with `text`, when a shape is already learned as another character, or
    /// when the reader is full. On error the reader is unchanged.
    pub fn learn(&mut self, image: &Image, text: &str) -> Result<(), LearnError> {
        let expected = text
            .chars()
            .filter(|&character| is_label_character(character));
        let layers = Layers::new(image);
        let sight = lesson(&layers, expected.clone().count())?;
        let mut learned = self.glyphs.clone();
        for (character, sample) in expected.zip(sight.samples(&layers)) {
            let close = |glyph: &&Glyph| glyph.sample.distance(&sample) <= SAME_GLYPH;
            match learned.iter().find(|glyph| glyph.sample == sample) {
                Some(glyph) if glyph.character == character => {},
                Some(glyph) => {
                    return Err(LearnError::Conflict {
                        learned: glyph.character,
                        new: character,
                    });
                },
                None if learned
                    .iter()
                    .filter(|glyph| glyph.character == character)
                    .any(|glyph| close(&glyph)) => {},
                None if learned.len() >= MAX_GLYPHS => return Err(LearnError::Full),
                None => learned.push(Glyph { character, sample }),
            }
        }
        self.glyphs = learned;
        Ok(())
    }

    /// The reader a number check reads with when it names `characters`, as
    /// `CheckDef::Number` does, or `None` when it has learned none of them.
    /// It takes its number from its glyphs for them alone, in the order it
    /// learned them: a shape matches the closest of those glyphs first, so
    /// a reader of digits cannot read a `0` as the `O` its font also has.
    /// A digit it leaves out is the exception: it is matched beside the
    /// glyphs the reader reads, so leaving out a `9` cannot turn a `9`
    /// into the `3` it looks like. The other characters it learned are
    /// still characters to it, which [`DigitReader::read`] takes as text
    /// and not as shapes it has yet to learn, and so are those an earlier
    /// call left out. A check that names no characters reads with the
    /// reader as it is.
    #[must_use]
    pub fn reading(mut self, characters: Option<&str>) -> Option<Self> {
        let Some(characters) = characters else {
            return Some(self);
        };
        let (read, left_out): (Vec<_>, Vec<_>) = self
            .glyphs
            .into_iter()
            .partition(|glyph| characters.contains(glyph.character));
        self.glyphs = read;
        self.left_out.extend(left_out);
        (!self.glyphs.is_empty()).then_some(self)
    }

    /// The pairs of characters this reader learned glyphs so alike for
    /// that a shape of one can be within reach of the other: a read then
    /// takes it for neither. Each pair once, in the order the reader
    /// learned the pair's first glyph, and the character learned first
    /// comes first.
    #[must_use]
    pub fn lookalikes(&self) -> Vec<(char, char)> {
        let mut pairs: Vec<(char, char)> = Vec::new();
        for (position, glyph) in self.glyphs.iter().enumerate() {
            for other in self.glyphs.iter().skip(position.saturating_add(1)) {
                let pair = (glyph.character, other.character);
                if pair.0 != pair.1
                    && !pairs.iter().any(|&(a, b)| (a, b) == pair || (b, a) == pair)
                    && GlyphScore::of(&glyph.sample, other).matches()
                {
                    pairs.push(pair);
                }
            }
        }
        pairs
    }

    /// Reads the first number in `image`: an optional `-`, digits, and an
    /// optional `.` with more digits. A number that starts with its `.`
    /// reads as `0.`, so `-.5` reads as -0.5, unless letters came before,
    /// so `Lv.5` reads as 5; a `.` with no digit after it is no number. A
    /// `,` between groups of three digits is skipped, so `1,234` reads as
    /// 1234; any other `,`, and other learned characters such as `%`, `:`,
    /// or `/`, separate numbers, so `12,5`, `12:30`, and `12/50` read as
    /// 12. With `whole`, the number ends at the `.`, so the digits after it
    /// are ignored and need no glyphs: `87.5%` reads as 87.
    ///
    /// A character the reader's value leaves out, as
    /// [`DigitReader::reading`] describes, is text: it ends the number, or
    /// comes before it. So a value of digits reads `53%` as 53 and `Lv.5`
    /// as 5. The read fails when the text is a part of the number, which
    /// would be another number with it: a value of digits reads neither
    /// `-5`, `.5`, `87.5`, nor `1,234`.
    ///
    /// The number is in whichever color its characters are: the layer the
    /// reader knows the most of. The read fails when another layer it
    /// knows about as much of says otherwise: it takes a character for
    /// another one, or shows a character a number can hold where the first
    /// layer has none, such as a digit in another color.
    ///
    /// # Errors
    ///
    /// Returns a [`ReadError`] when the image has no ink, too many
    /// characters, a character that matches no learned glyph, a character
    /// the value leaves out that its number holds, no digits, or two
    /// colors that read as different text. The last four carry the
    /// characters the read made out, so a number is never made of only the
    /// characters that matched.
    pub fn read(&self, image: &Image, whole: bool) -> Result<Number, ReadError> {
        let layers = Layers::new(image);
        let attempts = self.attempts(&layers)?;
        let chosen = Attempt::chosen(&attempts, &layers).ok_or(ReadError::NoInk)?;
        let (said, end) = chosen.number(whole);
        let (number, read) = said?;
        if let Some(other) = Attempt::contenders(&attempts).find(|other| chosen.disagrees(other)) {
            return Err(ReadError::ColorsDisagree {
                read,
                other: Box::new(other.read()),
            });
        }
        match chosen.beside(&layers, end) {
            Some(position) => Err(ReadError::UnknownCharacter {
                position,
                closest: None,
                read: read.with_unknown_at(position),
            }),
            None => Ok(number),
        }
    }

    /// The shapes of the characters in `image`, left to right, as a read
    /// samples them: those of the layer it knows the most of, or of the
    /// layer with the most ink while it knows none.
    ///
    /// # Errors
    ///
    /// [`ReadError::NoInk`] when the image has no ink, and
    /// [`ReadError::TooManyCharacters`] when it has more characters than a
    /// read takes.
    pub fn shapes(&self, image: &Image) -> Result<Shapes, ReadError> {
        let placed = self.placed(image)?;
        Ok(Shapes(
            placed.into_iter().map(|(shape, _columns)| shape).collect(),
        ))
    }

    /// The characters of `image`, left to right, as a read splits and
    /// samples them: each shape that [`DigitReader::shapes`] gives, with
    /// the columns of the image it came from, so a picture can show which
    /// part of the image has that shape.
    ///
    /// # Errors
    ///
    /// The errors of [`DigitReader::shapes`].
    pub fn placed(&self, image: &Image) -> Result<Vec<(Shape, Range<u32>)>, ReadError> {
        let layers = Layers::new(image);
        let sight = self.attempt(&layers)?.sight;
        Ok(sight
            .samples(&layers)
            .zip(&sight.marks)
            .map(|(sample, mark)| (sample.shape(), mark.bounds.columns.clone()))
            .collect())
    }

    /// Which pixels of `image` are the characters a read finds: the ink of
    /// the layer [`DigitReader::shapes`] takes them from. None are where
    /// the image has no ink or too many characters.
    #[must_use]
    pub fn ink(&self, image: &Image) -> Ink {
        self.attempt(&Layers::new(image))
            .map_or_else(|_| Ink::none(image.size()), |attempt| attempt.sight.ink)
    }

    /// What tells text like that in `region` of `image` from the rest of
    /// an image, for a search for more such text: the color
    /// [`DigitReader::ink`] finds the text of `region` in. Where that text
    /// is no one color, it is whatever is unlike the background of
    /// `region`. `None` when `region` reaches outside `image`, or holds no
    /// text.
    #[must_use]
    pub fn tone(&self, image: &Image, region: Region) -> Option<Tone> {
        let layers = Layers::new(&cut(image, region)?);
        let layer = self.attempt(&layers).ok()?.sight.layer;
        match (layers.color(layer), layers.background()) {
            (Some(color), _) => Some(Tone::Color(color)),
            (None, Some(background)) => Some(Tone::Unlike(background)),
            (None, None) => None,
        }
    }

    /// How each character in `image` scores against the learned glyphs:
    /// its closest glyph and the closest of another character, left to
    /// right. A read decides with the same scores.
    ///
    /// # Errors
    ///
    /// The errors of [`DigitReader::shapes`].
    pub fn explain(&self, image: &Image) -> Result<Vec<CharacterScore>, ReadError> {
        let layers = Layers::new(image);
        let attempt = self.attempt(&layers)?;
        Ok(attempt
            .sight
            .samples(&layers)
            .map(|sample| self.score(&sample))
            .collect())
    }

    /// What this reader makes of each layer of `layers` that splits into
    /// characters, in the order [`Layers::all`] gives them.
    ///
    /// # Errors
    ///
    /// [`ReadError::NoInk`] when no layer holds ink, and
    /// [`ReadError::TooManyCharacters`] when every layer that does holds
    /// more characters than a read takes.
    fn attempts(&self, layers: &Layers) -> Result<Vec<Attempt>, ReadError> {
        let mut failure = ReadError::NoInk;
        let mut attempts: Vec<Attempt> = Vec::new();
        let mut most_ink = 0_u32;
        for (layer, ink) in layers.all() {
            if ink.saturating_mul(INK_TO_COUNT) < most_ink {
                continue;
            }
            match sight(layers, layer) {
                Ok(sight) => {
                    let taken = sight
                        .marks
                        .iter()
                        .map(|mark| self.take_mark(layers, &sight, mark))
                        .collect();
                    let attempt = Attempt { sight, taken };
                    most_ink = most_ink.max(attempt.known_ink());
                    attempts.push(attempt);
                },
                Err(Segmentation::TooMany) => failure = ReadError::TooManyCharacters,
                Err(Segmentation::NoInk) => {},
            }
        }
        if attempts.is_empty() {
            Err(failure)
        } else {
            Ok(attempts)
        }
    }

    /// The layer a read takes its characters from ([`Attempt::chosen`]).
    ///
    /// # Errors
    ///
    /// The errors of [`Self::attempts`].
    fn attempt(&self, layers: &Layers) -> Result<Attempt, ReadError> {
        let mut attempts = self.attempts(layers)?;
        let chosen = Attempt::chosen(&attempts, layers)
            .and_then(|chosen| {
                attempts
                    .iter()
                    .position(|attempt| std::ptr::eq(attempt, chosen))
            })
            .ok_or(ReadError::NoInk)?;
        Ok(attempts.swap_remove(chosen))
    }

    /// What a read takes `mark` of `sight` for, as [`Self::take`] says. A
    /// mark every glyph is [unlike](Place::unlike) is as far from each as
    /// any shape can be, so its shape is not sampled: most of what a layer
    /// that is not the text holds.
    fn take_mark(&self, layers: &Layers, sight: &Sight, mark: &Mark) -> Taken {
        let unlike_all = self
            .glyphs
            .iter()
            .chain(&self.left_out)
            .all(|glyph| glyph.sample.place().unlike(mark.place));
        if unlike_all {
            Taken::Unknown(None)
        } else {
            self.take(&sight.sample(layers, mark))
        }
    }

    /// What a read takes `sample` for. The closest glyph among those the
    /// value reads and the digits it leaves out says which character it
    /// is, when it matches and no glyph of another character among them is
    /// about as close: a left-out digit is matched beside the kept glyphs,
    /// since taking it for a kept lookalike would read another number.
    /// Otherwise it is the closest glyph the value leaves out, when that
    /// matches. A left-out letter or symbol never wins over a kept glyph
    /// that matches, so a value of digits reads a `0` that is closer to
    /// the font's `O` as `0`.
    fn take(&self, sample: &Sample) -> Taken {
        let kept = Ranked::among(&self.glyphs, sample);
        let digits = || {
            self.left_out
                .iter()
                .filter(|glyph| glyph.character.is_ascii_digit())
        };
        let ranked = if digits().next().is_some() {
            Ranked::among(self.glyphs.iter().chain(digits()), sample)
        } else {
            kept
        };
        // A glyph as far as any shape can be is no closer than another.
        let kept = || kept.best.filter(|kept| kept.distance < FARTHEST);
        match ranked.best {
            Some(best) if ranked.reads() => {
                if self
                    .glyphs
                    .iter()
                    .any(|glyph| glyph.character == best.character)
                {
                    Taken::Kept(best.character)
                } else {
                    Taken::LeftOut(best.character)
                }
            },
            Some(best) if best.matches() => Taken::Unknown(kept().map(|kept| kept.character)),
            _ => Ranked::among(&self.left_out, sample)
                .best
                .filter(|left_out| left_out.matches())
                .map_or_else(
                    || Taken::Unknown(kept().map(|kept| kept.character)),
                    |left_out| Taken::LeftOut(left_out.character),
                ),
        }
    }

    /// The closest glyph to `sample`, and the closest of another character,
    /// among the glyphs the value reads or, when a read takes the shape for
    /// a character the value leaves out, among those. Of equals, the glyph
    /// learned first.
    fn score(&self, sample: &Sample) -> CharacterScore {
        let (glyphs, left_out) = match self.take(sample) {
            Taken::LeftOut(_) => (&self.left_out, true),
            Taken::Kept(_) | Taken::Unknown(_) => (&self.glyphs, false),
        };
        let Ranked { best, runner_up } = Ranked::among(glyphs, sample);
        CharacterScore {
            best,
            runner_up,
            left_out,
        }
    }
}

impl Attempt {
    /// The number this layer's characters hold, read as
    /// [`DigitReader::read`] describes, with the characters it made out,
    /// and the column the number ends at: what is past it is not read.
    fn number(&self, whole: bool) -> (Result<(Number, ReadText), ReadError>, u32) {
        let mut number = NumberText::new(whole);
        // The number as it reads with the characters the value leaves out
        // too. Where it is another number than `number`, one of them is a
        // part of it, and the read fails.
        let mut with_left_out = NumberText::new(whole);
        let mut read = ReadText::default();
        // The first character that matched no glyph, and the glyph closest
        // to it. The read then fails, whatever `number` holds; the
        // characters after it are still matched, for the error, up to
        // where a number among them ends.
        let mut unknown = None;
        let mut end = self.sight.ink.size().width;
        for (position, (taken, mark)) in self.taken.iter().zip(&self.sight.marks).enumerate() {
            match *taken {
                Taken::Kept(character) => {
                    read.push(character);
                    number.push(character);
                    with_left_out.push(character);
                },
                // Text after a finished number, such as the "HP" in
                // "37% HP", needs no glyphs.
                _ if with_left_out.ended => {
                    end = mark.bounds.columns.start;
                    break;
                },
                Taken::LeftOut(character) => {
                    read.push(character);
                    number.push_text(character);
                    with_left_out.push(character);
                },
                Taken::Unknown(closest) => {
                    read.push(ReadText::UNKNOWN);
                    if unknown.is_none() {
                        unknown = Some((position, closest));
                    }
                },
            }
        }
        if let Some((position, closest)) = unknown {
            return (
                Err(ReadError::UnknownCharacter {
                    position,
                    closest,
                    read,
                }),
                end,
            );
        }
        let parsed = number.parse();
        let said = if parsed == with_left_out.parse() {
            parsed
                .map(|number| (number, read))
                .ok_or(ReadError::NoNumber { read })
        } else {
            Err(ReadError::LeftOut { read })
        };
        (said, end)
    }

    /// Where other colors show something of a character's size beside
    /// this layer's number, before the column `end`: how many of this
    /// layer's characters are left of the first such thing, or `None`
    /// without one. A read takes its number from one color, so a digit in
    /// another color beside it would be dropped without a word.
    ///
    /// What counts is a run of columns that holds pixels of other colors
    /// which those colors close in, over at least half the line's height,
    /// and clear of every character of this layer by more than a quarter
    /// of the line's height: an outline, a shadow, and the inside of a `0`
    /// all touch the characters they belong to. A layer of every color has
    /// no other.
    fn beside(&self, layers: &Layers, end: u32) -> Option<usize> {
        let sight = &self.sight;
        if sight.layer == Layer::Every {
            return None;
        }
        let height = sight.line.height();
        let reach = height.div_euclid(4);
        let near = |columns: &Range<u32>| {
            sight.marks.iter().any(|mark| {
                let own = &mark.bounds.columns;
                columns.start < own.end.saturating_add(reach)
                    && own.start.saturating_sub(reach) < columns.end
            })
        };
        runs(&layers.others(sight.layer), &sight.rows)
            .iter()
            .filter(|bounds| {
                bounds.columns.start < end
                    && bounds.height().saturating_mul(2) >= height
                    && !near(&bounds.columns)
            })
            .map(|bounds| {
                sight
                    .marks
                    .iter()
                    .filter(|mark| mark.bounds.columns.start < bounds.columns.start)
                    .count()
            })
            .min()
    }
}

/// The pixels of a window of size `window` that a reader is given to read
/// `region`: `region` and the pixels around it, one on each side the
/// window has one.
///
/// A layer's outermost pixels are never ink, and say which color is the
/// background, so a region may be as tight as its text. On a side at the
/// window's edge, the region's own outermost pixels are the ones around
/// it.
#[must_use]
pub fn reads(region: Region, window: Size) -> Region {
    let grown = |start: u32, end: u32, most: u32| {
        let from = start.saturating_sub(1);
        (
            from,
            end.saturating_add(1).min(most.max(end)).abs_diff(from),
        )
    };
    let (x, width) = grown(region.x(), region.right(), window.width);
    let (y, height) = grown(region.y(), region.bottom(), window.height);
    Region::new(x, y, width, height).unwrap_or(region)
}

/// The pixels of `frame`, a whole window's frame, that a reader is given
/// to read `region` ([`reads`]), or `None` when `region` reaches outside
/// the frame.
#[must_use]
pub fn cut(frame: &Image, region: Region) -> Option<Image> {
    frame.crop(reads(region, frame.size()))
}

/// The characters of `layer`: its line, and the runs of inked columns in
/// it.
fn sight(layers: &Layers, layer: Layer) -> Result<Sight, Segmentation> {
    let ink = layers.ink(layer);
    let Size { width, height } = ink.size();
    let rows = ink.line(0..width, 0..height).ok_or(Segmentation::NoInk)?;
    let bounds = runs(&ink, &rows);
    if bounds.len() > MAX_CHARACTERS {
        return Err(Segmentation::TooMany);
    }
    let line = Line::of(&bounds).ok_or(Segmentation::NoInk)?;
    let marks = bounds
        .into_iter()
        .map(|bounds| Mark {
            place: Place::of(&bounds, line),
            ink: bounds
                .rows
                .clone()
                .map(|y| ink.count(bounds.columns.clone(), y))
                .fold(0, |sum, count| {
                    sum.saturating_add(u32::try_from(count).unwrap_or(u32::MAX))
                }),
            bounds,
        })
        .collect();
    Ok(Sight {
        layer,
        ink,
        rows,
        line,
        marks,
    })
}

/// The runs of columns that hold ink in `rows`, left to right, each with
/// the rows of `rows` its ink is in.
fn runs(ink: &Ink, rows: &Range<u32>) -> Vec<Bounds> {
    let mut all = Vec::new();
    let mut start = None;
    // The column past the last ends a run at the right edge.
    let columns = ink.columns(rows.clone());
    for (x, inked) in (0..).zip(columns.into_iter().chain([false])) {
        match (start, inked) {
            (None, true) => start = Some(x),
            (Some(left), false) => {
                let columns = left..x;
                let held = |y: &u32| ink.count(columns.clone(), *y) > 0;
                let top = rows.clone().find(held);
                let bottom = rows.clone().rev().find(held);
                if let (Some(top), Some(bottom)) = (top, bottom) {
                    all.push(Bounds {
                        rows: top..bottom.saturating_add(1),
                        columns,
                    });
                }
                start = None;
            },
            _ => {},
        }
    }
    all
}

/// The layer of `layers` that teaches a reader text of `expected`
/// characters: the first of these that splits into as many.
///
/// 1. The color with the most ink, but for the background's. Where that
///    color's line is much shorter than the line of every color together,
///    it is one band of text that is several colors, and the next is
///    taken.
/// 2. Every pixel that is not background, for text that is no one color.
/// 3. The background's color where other colors close it in: text of the
///    background's color with an outline whose characters run into each
///    other. It comes last because the inside of a `0` is such pixels too.
///    So does a color that is a plate behind the text ([`plate`]).
fn lesson(layers: &Layers, expected: usize) -> Result<Sight, LearnError> {
    // How far each layer's characters are from the text's, for the layer
    // to blame when none fits: the one that came closest, the earliest of
    // equals.
    let mut closest: Option<(usize, LearnError)> = None;
    let (mut color, mut every, mut enclosed) = (None, None, None);
    let mut plated = false;
    for (layer, _) in layers.all() {
        let (count, sight) = match sight(layers, layer) {
            Ok(sight) => {
                plated |= plate(layers, &sight);
                (sight.marks.len(), Some(sight))
            },
            Err(Segmentation::TooMany) => (usize::MAX, None),
            Err(Segmentation::NoInk) => continue,
        };
        let failure = match (count.cmp(&expected), sight) {
            (_, None) => LearnError::TooManyCharacters,
            (Ordering::Less, _) => LearnError::FewerCharacters {
                found: count,
                expected,
            },
            (Ordering::Greater, _) => LearnError::MoreCharacters {
                found: count,
                expected,
            },
            (Ordering::Equal, Some(sight)) => {
                let slot = match layer {
                    Layer::Every => &mut every,
                    Layer::Color(_) if layers.is_background(layer) || plate(layers, &sight) => {
                        &mut enclosed
                    },
                    Layer::Color(_) => &mut color,
                };
                if slot.is_none() {
                    *slot = Some(sight);
                }
                continue;
            },
        };
        let off = count.abs_diff(expected);
        if closest.as_ref().is_none_or(|(least, _)| off < *least) {
            closest = Some((off, failure));
        }
    }
    match (color, every, enclosed) {
        (Some(color), Some(every), _) if !plated && !whole_text(&color, &every) => Ok(every),
        (Some(sight), _, _) | (None, Some(sight), _) | (None, None, Some(sight)) => Ok(sight),
        (None, None, None) => Err(closest.map_or(LearnError::NoInk, |(_, failure)| failure)),
    }
}

/// Whether `sight` is a plate behind the text rather than the text: one
/// color whose one mark fills the image from side to side and top to
/// bottom, where the image has ink of another color too. A box drawn along
/// the edge of a button or a panel shows such a plate, with more ink than
/// its text. Beside a plate, every color together is the plate again and
/// no text of several colors.
fn plate(layers: &Layers, sight: &Sight) -> bool {
    if sight.layer == Layer::Every {
        return false;
    }
    let Size { width, height } = sight.ink.size();
    let [mark] = sight.marks.as_slice() else {
        return false;
    };
    mark.bounds.columns == (1..width.saturating_sub(1))
        && mark.bounds.rows == (1..height.saturating_sub(1))
        && layers.all().iter().any(|&(other, _)| {
            other != sight.layer && other != Layer::Every && !layers.is_background(other)
        })
}

/// Whether `color`, the layer of one color with the most ink, holds the
/// whole height of the text: its line is over three fifths as tall as the
/// line of `every` color together. A shorter one is a band of text that is
/// several colors, as a gradient or a two-tone font draws it.
fn whole_text(color: &Sight, every: &Sight) -> bool {
    color.line.height().saturating_mul(5) > every.line.height().saturating_mul(3)
}

/// The closest glyph to a shape, and the closest of another character.
#[derive(Debug, Clone, Copy)]
struct Ranked {
    best: Option<GlyphScore>,
    runner_up: Option<GlyphScore>,
}

impl Ranked {
    /// How `sample` ranks among `glyphs`: the earliest of glyphs as close.
    fn among<'a>(glyphs: impl IntoIterator<Item = &'a Glyph>, sample: &Sample) -> Self {
        let mut best: Option<GlyphScore> = None;
        // The closest glyph of each other character than the closest
        // glyph's is the runner-up, so the closest two characters so far
        // are kept: `best` and `next`.
        let mut next: Option<GlyphScore> = None;
        for glyph in glyphs {
            let score = GlyphScore::of(sample, glyph);
            match best {
                Some(closest) if score.distance >= closest.distance => {
                    if score.character != closest.character
                        && next.is_none_or(|next| score.distance < next.distance)
                    {
                        next = Some(score);
                    }
                },
                Some(closest) => {
                    if score.character != closest.character {
                        next = Some(closest);
                    }
                    best = Some(score);
                },
                None => best = Some(score),
            }
        }
        let runner_up = next;
        Self { best, runner_up }
    }

    /// Whether the shape reads as the closest glyph's character: the glyph
    /// matches, and the closest glyph of another character is clearly
    /// further off.
    fn reads(self) -> bool {
        self.best.is_some_and(|best| {
            best.matches()
                && self
                    .runner_up
                    .is_none_or(|next| next.distance >= best.distance.saturating_add(CLEAR_BY))
        })
    }
}

/// How one character in a region scores, from [`DigitReader::explain`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharacterScore {
    /// The closest learned glyph, or `None` when the reader knows none.
    pub best: Option<GlyphScore>,
    /// The closest glyph of another character, or `None` when the reader
    /// knows no other character.
    pub runner_up: Option<GlyphScore>,
    /// Whether the glyphs are of characters the value leaves out: the
    /// character matches one of them and none the value reads, so a read
    /// takes it as text.
    pub left_out: bool,
}

impl CharacterScore {
    /// Whether a read takes the character for its closest glyph's: the
    /// glyph matches, and the closest glyph of another character is
    /// clearly further off.
    #[must_use]
    pub fn reads(&self) -> bool {
        Ranked {
            best: self.best,
            runner_up: self.runner_up,
        }
        .reads()
    }
}

/// How close a character is to one learned glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphScore {
    /// The glyph's character.
    pub character: char,
    /// How far the character's shape is from the glyph's: of the ink
    /// either has, the thousandths only one of them has, from 0 for the
    /// same shape to 1000. A glyph much taller or shorter in its line, or
    /// much higher or lower, is 1000 away.
    pub distance: u16,
}

impl GlyphScore {
    /// How close `sample` is to `glyph`.
    fn of(sample: &Sample, glyph: &Glyph) -> Self {
        Self {
            character: glyph.character,
            distance: sample.distance(&glyph.sample),
        }
    }

    /// Whether the character is close enough to be this glyph's, by the
    /// same rule as [`DigitReader::read`]. A read also asks that no glyph
    /// of another character is about as close
    /// ([`CharacterScore::reads`]).
    #[must_use]
    pub const fn matches(self) -> bool {
        self.distance <= MATCH_WITHIN
    }
}

const _: () = assert!(
    MATCH_WITHIN < FARTHEST,
    "a glyph too tall or too low never matches"
);

#[derive(Debug, Clone, Copy)]
enum Segmentation {
    NoInk,
    TooMany,
}

impl From<Segmentation> for ReadError {
    fn from(failure: Segmentation) -> Self {
        match failure {
            Segmentation::NoInk => Self::NoInk,
            Segmentation::TooMany => Self::TooManyCharacters,
        }
    }
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
    /// No characters yet, of a number that ends at its point when `whole`.
    fn new(whole: bool) -> Self {
        Self {
            point: Point::for_whole(whole),
            ..Self::default()
        }
    }

    /// Takes `character` as text that is no part of a number, whatever it
    /// is: a character the value leaves out. A letter still tells a `.`
    /// after it that it is not the number's, as in `Lv.5`.
    fn push_text(&mut self, character: char) {
        self.push(if character.is_alphabetic() {
            character
        } else {
            ' '
        });
    }

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

impl ReadText {
    /// These characters with a [`Self::UNKNOWN`] before the one at
    /// `position`, or after the last when there are no more.
    fn with_unknown_at(self, position: usize) -> Self {
        let mut read = Self::default();
        let known = || self.characters.iter().take(self.len).copied();
        known()
            .take(position)
            .for_each(|character| read.push(character));
        read.push(Self::UNKNOWN);
        known()
            .skip(position)
            .for_each(|character| read.push(character));
        read
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
    /// No pixel was ink.
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
    /// region is ink.
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
                 for each mark, and so does anything else that stands out as the characters do"
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
const NO_INK: &str = "nothing in the region stands out from its background";

/// What a read or a lesson says when the region has too many characters.
const TOO_MANY: &str = "the region holds more than 24 characters";
const _: () = assert!(MAX_CHARACTERS == 24, "TOO_MANY states the limit");

/// A reader could not read a number from a region. Each variant is one
/// stage of a read, in the order a read goes through them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// No pixel was ink.
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
    /// A character the value does not read is a part of the number: with
    /// it, the characters hold another number.
    LeftOut {
        /// Every character the read made out.
        read: ReadText,
    },
    /// Every character matched a learned glyph, but they held no number.
    NoNumber {
        /// The characters.
        read: ReadText,
    },
    /// Two colors of the region each hold characters the reader knows,
    /// and they read as different text.
    ColorsDisagree {
        /// The characters in the color the read took.
        read: ReadText,
        /// The characters in the other. Boxed so that a read's error stays
        /// small beside its number.
        other: Box<ReadText>,
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
            Self::LeftOut { .. } => "the value leaves out a character its number holds",
            Self::NoNumber { .. } => "the characters hold no number",
            Self::ColorsDisagree { .. } => "two colors in the region read as different text",
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
            Self::LeftOut { read } | Self::NoNumber { read } => {
                write!(formatter, ": it read `{read}`")
            },
            Self::ColorsDisagree { read, other } => {
                write!(formatter, ": it read `{read}` and `{other}`")
            },
        }
    }
}

impl Error for ReadError {}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::drawn::{Bgr, Colors, Pen, Percent, draw as draw_text};
    use super::*;
    use crate::screen::Size;

    /// Whether `reader` has learned a glyph for `character`.
    fn knows(reader: &DigitReader, character: char) -> bool {
        reader
            .glyphs
            .iter()
            .any(|glyph| glyph.character == character)
    }

    /// The characters of `text` as a read made them out.
    fn read_text(text: &str) -> ReadText {
        let mut read = ReadText::default();
        text.chars().for_each(|character| read.push(character));
        read
    }

    fn number(value: f64) -> Number {
        Number::new(value).unwrap()
    }

    #[test]
    fn errors_explain_themselves() {
        for (error, message) in [
            (
                LearnError::NoInk,
                "nothing in the region stands out from its background",
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
                 mark, and so does anything else that stands out as the characters do",
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
                "nothing in the region stands out from its background",
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
                ReadError::LeftOut {
                    read: read_text("17"),
                },
                "the value leaves out a character its number holds",
                ": it read `1 7`",
            ),
            (
                ReadError::NoNumber {
                    read: read_text("%-"),
                },
                "the characters hold no number",
                ": it read `% -`",
            ),
            (
                ReadError::ColorsDisagree {
                    read: read_text("12"),
                    other: Box::new(read_text("7")),
                },
                "two colors in the region read as different text",
                ": it read `1 2` and `7`",
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
        assert_eq!(read_text("12").with_unknown_at(1), read_text("1?2"));
        assert_eq!(read_text("12").with_unknown_at(2), read_text("12?"));
    }

    const WHITE: Bgr = [255, 255, 255];
    const BLACK: Bgr = [0, 0, 0];

    /// Draws `rows` in the colors `palette` gives their cells.
    fn paint(rows: &[&str], palette: impl Fn(char) -> Bgr) -> Image {
        let size = Size {
            width: u32::try_from(rows[0].len()).unwrap(),
            height: u32::try_from(rows.len()).unwrap(),
        };
        let bgra = rows
            .iter()
            .flat_map(|row| row.chars())
            .flat_map(|cell| {
                let [blue, green, red] = palette(cell);
                [blue, green, red, 255]
            })
            .collect();
        Image::new(size, bgra).unwrap()
    }

    /// `rows` with a blank cell on every side: a reader takes none of an
    /// image's outermost pixels for ink.
    fn margined(rows: &[&str]) -> Vec<String> {
        padded(rows, 1, 1, 1, 1)
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

    /// Draws `rows` of `#` in `ink` and `.` in `paper`, with a margin of
    /// `paper` a pixel wide around them.
    fn draw_in(rows: &[&str], ink: Bgr, paper: Bgr) -> Image {
        let rows = margined(rows);
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        paint(&rows, |cell| if cell == '#' { ink } else { paper })
    }

    /// Draws `rows` of `#` in white on black, with a margin a pixel wide.
    fn draw(rows: &[&str]) -> Image {
        draw_in(rows, WHITE, BLACK)
    }

    /// "12" in a five-pixel font.
    const TWELVE: [&str; 5] = [
        "#.###", //
        "#...#", //
        "#.###", //
        "#.#..", //
        "#.###", //
    ];

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
        assert_eq!(parsed("1:30"), Number::new(1.0));
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

    /// A reader that learned "12" from [`TWELVE`].
    fn twelve() -> DigitReader {
        let mut reader = DigitReader::new();
        reader.learn(&draw(&TWELVE), "12").unwrap();
        reader
    }

    #[test]
    fn characters_learned_at_one_size_read_at_another() {
        let big = scaled(&TWELVE, 3);
        let rows: Vec<&str> = big.iter().map(String::as_str).collect();
        assert_eq!(twelve().read(&draw(&rows), false), Ok(number(12.0)));
    }

    #[test]
    fn characters_learned_in_one_color_read_in_any_other() {
        let reader = twelve();
        let gold = [40, 200, 250];
        let navy = [90, 30, 10];
        for (ink, paper) in [(gold, BLACK), (BLACK, WHITE), (navy, gold), (WHITE, navy)] {
            assert_eq!(
                reader.read(&draw_in(&TWELVE, ink, paper), false),
                Ok(number(12.0)),
                "{ink:?} on {paper:?}"
            );
        }
    }

    #[test]
    fn a_whole_read_ignores_the_digits_after_the_point() {
        let mut reader = DigitReader::new();
        // "1" and a "." that sits at the baseline.
        reader.learn(&draw(&["#..", "#..", "#.#"]), "1.").unwrap();
        // "1.", then a character never learned.
        let fraction = draw(&["#...#.#", "#...###", "#.#.#.#"]);
        assert_eq!(
            reader.read(&fraction, false),
            Err(ReadError::UnknownCharacter {
                position: 2,
                closest: None,
                read: read_text("1.?"),
            })
        );
        assert_eq!(reader.read(&fraction, true), Ok(number(1.0)));
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
        let mut reader = DigitReader::new();
        reader.learn(&digits, "1 2 3").unwrap();
        assert!(knows(&reader, '2'));
        assert!(!knows(&reader, '4'));
        let swapped = draw(&[
            "###.#.###", //
            "..#.#...#",
            "###.#.###",
            "#...#.#..",
            "###.#.###",
        ]);
        assert_eq!(reader.read(&swapped, false), Ok(number(212.0)));
    }

    #[test]
    fn learning_fails_without_changing_the_reader() {
        let digits = draw(&["#.#"]);
        let mut reader = DigitReader::new();
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
        assert!(!knows(&reader, '1'));
        assert_eq!(reader.learn(&draw(&["..."]), "1"), Err(LearnError::NoInk));
        reader.learn(&draw(&["#"]), "1").unwrap();
        reader.learn(&draw(&["#"]), "1").unwrap();
        assert_eq!(reader.glyphs.len(), 1);
    }

    #[test]
    fn reading_fails_on_unknown_characters_and_missing_digits() {
        let mut reader = DigitReader::new();
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
                closest: None,
                read: read_text("1?1"),
            })
        );
        assert_eq!(
            DigitReader::new().read(&draw(&["#"]), false),
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

    /// A ring `width` pixels wide and 16 tall.
    fn ring(width: usize) -> Vec<String> {
        let wall = format!("#{}#", ".".repeat(width.checked_sub(2).unwrap()));
        let mut rows = vec![wall; 16];
        rows[0] = "#".repeat(width);
        rows[15] = "#".repeat(width);
        rows
    }

    #[test]
    fn a_reader_of_some_characters_matches_only_their_glyphs() {
        // A letter and the digit that looks like it, a column wider: each
        // shape is about as close to both glyphs.
        let both: Vec<String> = ring(10)
            .iter()
            .zip(ring(11))
            .map(|(letter, digit)| format!("{letter}.{digit}"))
            .collect();
        let rows: Vec<&str> = both.iter().map(String::as_str).collect();
        let mut reader = DigitReader::new();
        reader.learn(&draw(&rows), "O0").unwrap();
        assert_eq!(reader.lookalikes(), [('O', '0')]);
        let letter = ring(10);
        let letter = draw(&letter.iter().map(String::as_str).collect::<Vec<_>>());
        // A reader of both takes the shape for neither.
        assert_eq!(
            reader.read(&letter, false),
            Err(ReadError::UnknownCharacter {
                position: 0,
                closest: Some('O'),
                read: read_text("?"),
            })
        );
        let digits = reader.clone().reading(Some("0123456789")).unwrap();
        assert!(knows(&digits, '0'));
        assert!(!knows(&digits, 'O'));
        assert_eq!(digits.read(&letter, false), Ok(number(0.0)));
        // A value that also leaves out a digit still takes the shape for its
        // own `0`, since only a left-out digit is matched beside the kept
        // glyphs.
        let mut with_one = reader.clone();
        with_one.learn(&draw(&["#"; 16]), "1").unwrap();
        let zero = with_one.reading(Some("0")).unwrap();
        assert_eq!(zero.read(&letter, false), Ok(number(0.0)));
        // The glyphs kept stay in the order they were learned.
        assert_eq!(reader.clone().reading(Some("0 O")), Some(reader.clone()));
        assert_eq!(reader.clone().reading(Some("%")), None);
        // A check that names none reads with them all, even before any.
        assert_eq!(reader.clone().reading(None), Some(reader));
        let blank = DigitReader::new();
        assert_eq!(blank.clone().reading(None), Some(blank));
    }

    #[test]
    fn text_after_a_finished_number_needs_no_glyphs() {
        let mut reader = DigitReader::new();
        reader
            .learn(&draw(&["#.###", "#.#.#", "#.###"]), "1%")
            .unwrap();
        // "1%" and then a character never learned.
        let labeled = draw(&["#.###.#.#", "#.#.#.###", "#.###.#.#"]);
        assert_eq!(reader.read(&labeled, false), Ok(number(1.0)));
        // After an unlearned character the read fails, and its error lists
        // the characters the same way: up to where a number ends.
        let led = draw(&["#.#.#.###.#.#", "###.#.#.#.###", "#.#.#.###.#.#"]);
        assert_eq!(
            reader.read(&led, false),
            Err(ReadError::UnknownCharacter {
                position: 0,
                closest: None,
                read: read_text("?1%"),
            })
        );
    }

    #[test]
    fn a_stray_pixel_does_not_stretch_the_line() {
        let reader = twelve();
        let mut noisy = TWELVE.to_vec();
        noisy.extend([".....", "....#"]);
        assert_eq!(reader.read(&draw(&noisy), false), Ok(number(12.0)));
        // Of two runs of inked rows, the one with more ink is the line.
        let mut above = vec!["#....", "....."];
        above.extend(TWELVE);
        assert_eq!(reader.read(&draw(&above), false), Ok(number(12.0)));
    }

    #[test]
    fn of_two_lines_with_equal_ink_the_upper_one_is_read() {
        // Sixteen ink pixels, as many as "12", in one unlearned block.
        let mut tied = TWELVE.to_vec();
        tied.extend([".....", "#####", "#####", "#####", "#...."]);
        assert_eq!(twelve().read(&draw(&tied), false), Ok(number(12.0)));
    }

    #[test]
    fn a_number_reads_wherever_it_sits_in_a_larger_region() {
        let reader = twelve();
        for (left, right, above, below) in [(0, 9, 0, 6), (4, 5, 3, 3), (9, 0, 6, 0)] {
            let region = padded(&TWELVE, left, right, above, below);
            let rows: Vec<&str> = region.iter().map(String::as_str).collect();
            assert_eq!(
                reader.read(&draw(&rows), false),
                Ok(number(12.0)),
                "{left} columns to its left and {above} rows above it"
            );
        }
    }

    #[test]
    fn ink_beside_a_number_in_a_larger_region_is_read_with_it() {
        let reader = twelve();
        // A "1" three columns to the left of "12", on its rows.
        let digit: Vec<String> = TWELVE.iter().map(|row| format!("#...{row}")).collect();
        let rows: Vec<&str> = digit.iter().map(String::as_str).collect();
        assert_eq!(reader.read(&draw(&rows), false), Ok(number(112.0)));
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
    fn text_in_another_color_beside_the_number_is_read_with_it_or_fails_the_read() {
        let reader = twelve();
        let colors = |cell| match cell {
            '#' => WHITE,
            'r' => [0, 0, 255],
            'b' => [255, 0, 0],
            _ => BLACK,
        };
        let read = |rows: &[String]| {
            let rows = margined(&rows.iter().map(String::as_str).collect::<Vec<_>>());
            let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
            reader.read(&paint(&rows, colors), false)
        };
        // A red "1" three columns to the right of a white "12": the reader
        // knows every character of the two colors together.
        let after: Vec<String> = TWELVE.iter().map(|row| format!("{row}...r")).collect();
        assert_eq!(read(&after), Ok(number(121.0)));
        // With a blue speck it does not know between them, it knows every
        // character of the red "2" alone, and the white "1" is a digit
        // where the red has none.
        // The "1" of each row, and the "2" after the column between them.
        let halves = |row: &str| -> (String, String) {
            (row.chars().take(1).collect(), row.chars().skip(2).collect())
        };
        let recolored: Vec<String> = TWELVE
            .iter()
            .map(|row| {
                let (one, two) = halves(row);
                format!("{one}...{}", two.replace('#', "r"))
            })
            .collect();
        let mut specked = recolored.clone();
        specked[4].replace_range(2..3, "b");
        assert_eq!(read(&recolored), Ok(number(12.0)));
        assert_eq!(
            read(&specked),
            Err(ReadError::ColorsDisagree {
                read: read_text("2"),
                other: Box::new(read_text("1?2")),
            })
        );
        // A red shape the reader does not know, between the two digits.
        let between: Vec<String> = TWELVE
            .iter()
            .map(|row| {
                let (one, two) = halves(row);
                format!("{one}...rr...{two}")
            })
            .collect();
        assert_eq!(
            read(&between),
            Err(ReadError::UnknownCharacter {
                position: 1,
                closest: None,
                read: read_text("1?2"),
            })
        );
        // The same shape too small to be a character is no part of the
        // text.
        let mut speck: Vec<String> = TWELVE
            .iter()
            .map(|row| {
                let (one, two) = halves(row);
                format!("{one}.......{two}")
            })
            .collect();
        speck[0].replace_range(4..5, "r");
        assert_eq!(read(&speck), Ok(number(12.0)));
    }

    #[test]
    fn shapes_tell_characters_apart_but_not_their_size_or_color() {
        let reader = DigitReader::new();
        let twelve = reader.shapes(&draw(&TWELVE)).unwrap();
        let [one, two] = twelve.characters() else {
            panic!("two characters, not {twelve:?}");
        };
        assert_ne!(one, two);
        let mut noisy = TWELVE.to_vec();
        noisy.extend([".....", "....#"]);
        assert_eq!(reader.shapes(&draw(&noisy)), Ok(twelve.clone()));
        let big = scaled(&TWELVE, 4);
        let big: Vec<&str> = big.iter().map(String::as_str).collect();
        assert_eq!(
            reader.shapes(&draw_in(&big, [40, 200, 250], [90, 30, 10])),
            Ok(twelve.clone())
        );
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
    fn placed_characters_are_the_shapes_with_where_each_came_from() {
        let reader = DigitReader::new();
        // A stray pixel under the line widens no character.
        let mut noisy = TWELVE.to_vec();
        noisy.extend([".....", ".#..."]);
        let image = draw(&noisy);
        let placed = reader.placed(&image).expect("two characters");
        let columns: Vec<Range<u32>> = placed.iter().map(|(_, columns)| columns.clone()).collect();
        // The image has a margin a pixel wide.
        assert_eq!(columns, [1..2, 3..6]);
        let shapes: Vec<Shape> = placed.into_iter().map(|(shape, _)| shape).collect();
        assert_eq!(
            shapes,
            reader.shapes(&image).expect("the same image").characters()
        );
        assert_eq!(reader.placed(&draw(&["..."])), Err(ReadError::NoInk));
    }

    #[test]
    fn text_of_more_than_one_color_is_every_pixel_that_is_not_background() {
        // "12" with a gold top and a dark red bottom on a gray panel, and
        // "21" in two other tones on another panel.
        let two_tone = |light: Bgr, dark: Bgr, back: Bgr| {
            move |cell| match cell {
                'a' => light,
                'b' => dark,
                _ => back,
            }
        };
        let gold = two_tone([40, 200, 250], [20, 30, 140], [90, 90, 90]);
        let frost = two_tone([255, 240, 200], [200, 120, 60], [10, 20, 30]);
        let twelve = [
            ".......", //
            ".a.aaa.", //
            ".a...a.", //
            ".b.bbb.", //
            ".b.b...", //
            ".b.bbb.", //
            ".......", //
        ];
        let twenty_one = [
            ".......", //
            ".aaa.a.", //
            "...a.a.", //
            ".bbb.b.", //
            ".b...b.", //
            ".bbb.b.", //
            ".......", //
        ];
        let mut reader = DigitReader::new();
        reader.learn(&paint(&twelve, gold), "12").unwrap();
        // What it learned is the whole of each character, so the text
        // reads in one color too.
        assert_eq!(reader, self::twelve());
        assert_eq!(
            reader.read(&paint(&twenty_one, frost), false),
            Ok(number(21.0))
        );
    }

    #[test]
    fn a_reader_that_knows_no_character_takes_text_of_two_colors_whole() {
        let colors = |cell| match cell {
            'r' => [255, 0, 0],
            'b' => [0, 0, 255],
            'w' => WHITE,
            _ => [20, 20, 20],
        };
        // A stroke with a red top half and a blue bottom half.
        let banded = paint(
            &[
                ".....", //
                "..r..", //
                "..r..", //
                "..b..", //
                "..b..", //
                ".....", //
            ],
            colors,
        );
        let ink = DigitReader::new().ink(&banded);
        for y in 1..5 {
            assert!(ink.at(2, y), "row {y} of the stroke");
        }
        // A search for more such text takes whatever is unlike its
        // background.
        let tone = DigitReader::new()
            .tone(&banded, Region::new(1, 1, 3, 4).unwrap())
            .unwrap();
        let dark = Rgb {
            red: 20,
            green: 20,
            blue: 20,
        };
        assert_eq!(tone, Tone::Unlike(dark));
        let like = tone.ink(&banded);
        assert!(like.at(2, 1) && like.at(2, 4) && !like.at(1, 1));
        // A red stroke with a white mark beside its foot is as tall in red
        // alone, so its ink is the red.
        let marked = paint(
            &[
                ".......", //
                "..r....", //
                "..r....", //
                "..r....", //
                "..r.w..", //
                ".......", //
            ],
            colors,
        );
        let ink = DigitReader::new().ink(&marked);
        assert!(ink.at(2, 4));
        assert!(!ink.at(4, 4), "the mark is another color");
    }

    #[test]
    fn text_on_a_plate_that_fills_the_image_is_taken_and_not_the_plate() {
        let colors = |cell| match cell {
            'w' => [0, 0, 255],
            'p' => [20, 20, 20],
            _ => [200, 200, 200],
        };
        // A stroke on a dark plate, which the image shows the whole of and
        // one pixel around.
        let button = paint(
            &[
                ".......", //
                ".ppppp.", //
                ".ppwpp.", //
                ".ppwpp.", //
                ".ppwpp.", //
                ".ppppp.", //
                ".......", //
            ],
            colors,
        );
        let ink = DigitReader::new().ink(&button);
        assert!(ink.at(3, 3), "the stroke");
        assert!(!ink.at(1, 1), "the plate has more ink and is not the text");
        // A lesson learns the stroke, so it reads without the plate.
        let mut reader = DigitReader::new();
        reader.learn(&button, "1").unwrap();
        let bare = paint(&[".....", "..w..", "..w..", "..w..", "....."], colors);
        assert_eq!(reader.read(&bare, false), Ok(number(1.0)));
        // The pixel of the window around a plate is no blend of the
        // plate's color and the text's, though it lies between them.
        let badge = paint(
            &[
                ".......", //
                ".ppppp.", //
                ".pWWWp.", //
                ".pWWWp.", //
                ".pWWWp.", //
                ".pWWWp.", //
                ".ppppp.", //
                ".......", //
            ],
            |cell| if cell == 'W' { WHITE } else { colors(cell) },
        );
        let ink = DigitReader::new().ink(&badge);
        assert!(
            ink.at(3, 3) && !ink.at(1, 1),
            "white on dark in a gray window"
        );
        // A lone mark that fills its image is the text: no other color
        // shows that it is a plate.
        let lone = paint(&["....", ".pp.", ".pp.", "...."], colors);
        assert!(DigitReader::new().ink(&lone).at(1, 1));
        // And so is a mark with another color beside it, not on it.
        let beside = paint(
            &[
                "......", //
                ".pp.w.", //
                ".pp.w.", //
                "......", //
            ],
            colors,
        );
        assert!(DigitReader::new().ink(&beside).at(1, 1));
    }

    #[test]
    fn the_tone_of_a_region_is_the_color_of_that_regions_text() {
        // A panel of `p` holding a character `#`, on a scene of `s` with a
        // white pixel of its own.
        let scene = paint(
            &[
                "sssssss", //
                "s#sssss", //
                "ssppppp", //
                "sspp#pp", //
                "ssppppp", //
            ],
            |cell| match cell {
                's' => [200, 200, 200],
                'p' => [20, 20, 20],
                _ => WHITE,
            },
        );
        let reader = DigitReader::new();
        let panel = Region::new(4, 3, 1, 1).unwrap();
        let like = reader.tone(&scene, panel).unwrap().ink(&scene);
        // (column, row): the scene, its white pixel, the panel, and the
        // character.
        for ((x, y), inked) in [
            ((2, 1), false),
            ((1, 1), true),
            ((3, 3), false),
            ((4, 3), true),
        ] {
            assert_eq!(like.at(x, y), inked, "at {x}, {y}");
        }
        assert_eq!(like.size(), scene.size());
        // A region with no text, and one that reaches outside the scene.
        assert!(
            reader
                .tone(&scene, Region::new(3, 0, 2, 1).unwrap())
                .is_none()
        );
        assert!(
            reader
                .tone(&scene, Region::new(5, 3, 3, 2).unwrap())
                .is_none()
        );
        // By itself, the panel is ink of the scene: the scene's outermost
        // pixels are its background.
        let alone = reader.ink(&scene);
        assert!(alone.at(3, 3));
        assert!(!alone.at(2, 1));
    }

    #[test]
    fn a_region_is_read_from_the_pixels_around_it_too() {
        let window = Size {
            width: 20,
            height: 10,
        };
        let region = |x, y, width, height| Region::new(x, y, width, height).unwrap();
        for (drawn, read) in [
            (region(5, 3, 4, 2), region(4, 2, 6, 4)),
            // At the window's edges there is no pixel around it.
            (region(0, 0, 4, 2), region(0, 0, 5, 3)),
            (region(16, 8, 4, 2), region(15, 7, 5, 3)),
            (region(0, 0, 20, 10), region(0, 0, 20, 10)),
        ] {
            assert_eq!(reads(drawn, window), read, "{drawn:?}");
        }
        // Text as tight as its region reads: "12" from (1, 1) of a frame.
        let frame = draw(&TWELVE);
        let tight = cut(&frame, region(1, 1, 5, 5)).unwrap();
        assert_eq!(tight, frame);
        assert_eq!(twelve().read(&tight, false), Ok(number(12.0)));
        assert_eq!(cut(&frame, region(4, 4, 5, 5)), None);
    }

    #[test]
    fn a_reader_learns_at_most_256_glyphs() {
        let mut reader = twelve();
        let glyph = reader.glyphs[1].clone();
        reader.glyphs.resize(MAX_GLYPHS, glyph);
        assert_eq!(reader.learn(&draw(&["#"]), "x"), Err(LearnError::Full));
        // A character it knows needs no room.
        assert_eq!(reader.learn(&draw(&TWELVE), "12"), Ok(()));
    }

    #[test]
    fn a_glyph_close_to_one_learned_for_its_character_adds_nothing() {
        // A block, and the same block with a corner pixel gone: under
        // `SAME_GLYPH` apart.
        let chipped = block(&[(0, 0)]);
        let mut reader = DigitReader::new();
        reader.learn(&block(&[]), "8").unwrap();
        reader.learn(&chipped, "8").unwrap();
        assert_eq!(reader.glyphs.len(), 1);
        // As another character it is another glyph, and the two look
        // alike.
        reader.learn(&chipped, "0").unwrap();
        assert_eq!(reader.glyphs.len(), 2);
        assert_eq!(reader.lookalikes(), [('8', '0')]);
        // A shape further off than that is another glyph of the same
        // character.
        reader.learn(&block(&square(0, 0, 3)), "8").unwrap();
        assert_eq!(reader.glyphs.len(), 3);
    }

    /// A canvas with room for a percentage whose digits are `height`
    /// pixels tall.
    fn canvas(height: u32) -> Size {
        Size {
            width: height.checked_mul(9).expect("a small canvas"),
            height: height.checked_mul(2).expect("a small canvas"),
        }
    }

    /// A pen for digits `height` pixels tall, starting `across` pixels
    /// from the left.
    fn pen(height: u32, across: f64) -> Pen {
        Pen::new(f64::from(height), f64::from(height) / 4.0 + across)
    }

    /// `text` drawn as an application draws it, with digits `height` pixels
    /// tall, starting `across` pixels from the left.
    fn drawn(text: &str, height: u32, across: f64) -> Image {
        draw_text(text, pen(height, across), canvas(height))
    }

    /// A reader that learned `labels`, drawn with digits `height` pixels
    /// tall, each at another fraction of a pixel, as a user's few labeled
    /// screenshots show them.
    fn taught(height: u32, labels: &[&str]) -> DigitReader {
        let mut reader = DigitReader::new();
        for (text, across) in labels.iter().zip([3.1, 3.47, 3.84]) {
            reader.learn(&drawn(text, height, across), text).unwrap();
        }
        reader
    }

    /// Three labels that show every digit, the point, and the percent sign.
    const LABELS: [&str; 3] = ["12.34%", "56.78%", "90%"];

    /// Colors text is drawn in: its own, its outline's, and the
    /// background's. The last two are white text on white and black text
    /// on black, which only their outlines show.
    const SCHEMES: [Colors; 6] = [
        Colors::PLAIN,
        Colors {
            ink: [40, 200, 250],
            outline: [0, 0, 0],
            background: [120, 40, 90],
        },
        Colors {
            ink: [30, 30, 220],
            outline: [255, 255, 255],
            background: [20, 20, 20],
        },
        Colors {
            ink: [90, 230, 60],
            outline: [20, 60, 10],
            background: [230, 230, 230],
        },
        Colors {
            ink: [255, 255, 255],
            outline: [0, 0, 0],
            background: [255, 255, 255],
        },
        Colors {
            ink: [0, 0, 0],
            outline: [255, 255, 255],
            background: [0, 0, 0],
        },
    ];

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        /// An application that scales its interface, or centers a number
        /// whose width changes, draws the same character at another
        /// fraction of a pixel each time, so its edge pixels differ from
        /// the ones the reference learned. It may draw it in another
        /// color too.
        #[test]
        fn a_percentage_reads_in_any_colors_wherever_it_lands_on_the_pixel_grid(
            height in prop::sample::select(vec![16_u32, 20, 28, 40]),
            hundredths in 0_u32..=10_000,
            decimals in any::<bool>(),
            across in 3.0..4.0_f64,
            colors in prop::sample::select(SCHEMES.to_vec()),
        ) {
            let whole = hundredths.div_euclid(100);
            let (text, shown) = if decimals {
                (format!("{whole}.{:02}%", hundredths.rem_euclid(100)), f64::from(hundredths) / 100.0)
            } else {
                (format!("{whole}%"), f64::from(whole))
            };
            // Text of the background's own color is only its outline,
            // which has to be more than a pixel wide to close it in.
            prop_assume!(height >= 28 || colors.ink != colors.background);
            let reader = taught(height, &LABELS);
            let image = draw_text(&text, Pen { colors, ..pen(height, across) }, canvas(height));
            prop_assert_eq!(
                reader.read(&image, false),
                Ok(number(shown)),
                "{} at {} in {:?}", text, across, colors
            );
        }
    }

    #[test]
    fn text_whose_outlines_run_together_learns_from_a_correct_label() {
        // Characters set so tight that their outlines are one shape: by
        // their outlines, the text is a single character.
        let tight = |text: &str, colors: Colors| {
            let pen = Pen {
                gap: -0.04,
                colors,
                ..pen(28, 3.3)
            };
            draw_text(text, pen, canvas(28))
        };
        let label = tight("12.34%", Colors::PLAIN);
        let outlines = Layers::new(&label);
        let joined = sight(&outlines, Layer::Every).unwrap();
        assert_eq!(joined.marks.len(), 1, "the outlines are one shape");
        let mut reader = DigitReader::new();
        for text in LABELS {
            reader.learn(&tight(text, Colors::PLAIN), text).unwrap();
        }
        // In any colors of its own. Text of the background's color is
        // only what its outline closes in, and outlines that run together
        // close in the background between two characters too.
        for colors in SCHEMES
            .into_iter()
            .filter(|colors| colors.ink != colors.background)
        {
            assert_eq!(
                reader.read(&tight("70.93%", colors), false),
                Ok(number(70.93)),
                "{colors:?}"
            );
        }
    }

    /// A block 16 pixels square, but for the pixels of `gone`, each given
    /// as its column and row.
    fn block(gone: &[(usize, usize)]) -> Image {
        let rows: Vec<String> = (0..16)
            .map(|row| {
                (0..16)
                    .map(|column| {
                        if gone.contains(&(column, row)) {
                            '.'
                        } else {
                            '#'
                        }
                    })
                    .collect()
            })
            .collect();
        draw(&rows.iter().map(String::as_str).collect::<Vec<_>>())
    }

    /// The pixels of a square `side` pixels long whose first is at
    /// (`column`, `row`).
    fn square(column: usize, row: usize, side: usize) -> Vec<(usize, usize)> {
        (0..side)
            .flat_map(|down| {
                (0..side).map(move |across| {
                    (
                        column.checked_add(across).expect("a small picture"),
                        row.checked_add(down).expect("a small picture"),
                    )
                })
            })
            .collect()
    }

    #[test]
    fn lookalikes_name_each_pair_of_characters_once_in_the_order_learned() {
        // A block, and two with a corner gone, each within reach of the
        // whole block.
        let bar = draw(&["#"; 16]);
        let mut reader = DigitReader::new();
        for (image, character) in [
            (block(&[]), "8"),
            (block(&square(14, 14, 2)), "6"),
            (bar, "1"),
            (block(&square(0, 14, 2)), "6"),
        ] {
            reader.learn(&image, character).unwrap();
        }
        assert_eq!(reader.glyphs.len(), 4);
        assert_eq!(reader.lookalikes(), [('8', '6')]);
        // Digits an application draws are not alike.
        assert_eq!(taught(28, &LABELS).lookalikes(), []);
    }

    #[test]
    fn a_character_drawn_half_a_pixel_aside_still_reads() {
        // The 3 was learned from "12.34%", drawn from 3.1 pixels across.
        let reader = taught(16, &LABELS);
        assert_eq!(
            reader.read(&drawn("23%", 16, 3.58), false),
            Ok(number(23.0))
        );
    }

    #[test]
    fn text_an_application_draws_reads_at_another_size_than_it_was_learned_at() {
        for (learned, shown) in [(20, 30), (30, 20), (16, 32), (40, 14)] {
            let reader = taught(learned, &LABELS);
            for (text, value, across) in [
                ("74.05%", 74.05, 3.0),
                ("100%", 100.0, 3.25),
                ("8.69%", 8.69, 3.6),
            ] {
                assert_eq!(
                    reader.read(&drawn(text, shown, across), false),
                    Ok(number(value)),
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
    fn a_character_the_value_leaves_out_ends_the_number() {
        let digits = taught(28, &LABELS).reading(Some("0123456789")).unwrap();
        let read = |text, whole| digits.read(&drawn(text, 28, 3.3), whole);
        assert_eq!(read("53%", false), Ok(number(53.0)));
        assert_eq!(read("87.5%", true), Ok(number(87.0)));
        assert_eq!(
            read("%", false),
            Err(ReadError::NoNumber {
                read: read_text("%")
            })
        );
    }

    #[test]
    fn a_character_the_value_leaves_out_fails_a_number_it_is_part_of() {
        // A comma hangs below the line, so the characters beside one are
        // learned beside one.
        let mut all = taught(28, &LABELS);
        for text in ["-12", "12,345"] {
            all.learn(&drawn(text, 28, 3.3), text).unwrap();
        }
        let digits = all.clone().reading(Some("0123456789")).unwrap();
        for (text, with_it) in [
            ("-5", -5.0),
            (".5", 0.5),
            ("87.5%", 87.5),
            ("1,234", 1234.0),
            ("12,345", 12345.0),
        ] {
            let image = drawn(text, 28, 3.3);
            assert_eq!(all.read(&image, false), Ok(number(with_it)));
            assert_eq!(
                digits.read(&image, false),
                Err(ReadError::LeftOut {
                    read: read_text(text)
                }),
                "{text}"
            );
        }
        // A comma that separates no groups of three ends the number for a
        // value that reads it too.
        let twelve = Ok(number(12.0));
        assert_eq!(all.read(&drawn("12,5", 28, 3.3), false), twelve);
        assert_eq!(digits.read(&drawn("12,5", 28, 3.3), false), twelve);
    }

    #[test]
    fn a_character_the_value_leaves_out_is_text_before_the_number() {
        // "L", a "." at the baseline, and "5".
        let label = draw(&["#.....###", "#.....#..", "###.#.###"]);
        let mut reader = DigitReader::new();
        reader.learn(&label, "L.5").unwrap();
        let five = Ok(number(5.0));
        assert_eq!(reader.read(&label, false), five);
        // The letter still says that the point is not the number's.
        let point = reader.clone().reading(Some("5.")).unwrap();
        assert_eq!(point.read(&label, false), five);
        let digit = reader.clone().reading(Some("5")).unwrap();
        assert_eq!(digit.read(&label, false), five);
        // Narrowing a reader again keeps what it left out before.
        let narrowed = point.reading(Some("5")).unwrap();
        assert_eq!(narrowed, digit);
        // Without the letter the point is the number's, so a value that
        // leaves it out does not read ".5" as 5.
        let pointed = draw(&["..###", "..#..", "#.###"]);
        assert_eq!(reader.read(&pointed, false), Ok(number(0.5)));
        assert_eq!(
            digit.read(&pointed, false),
            Err(ReadError::LeftOut {
                read: read_text(".5")
            })
        );
    }

    #[test]
    fn a_digit_the_value_leaves_out_fails_the_read() {
        // "1", a "." at the baseline, and "5".
        let label = draw(&["#...###", "#...#..", "#.#.###"]);
        let mut reader = DigitReader::new();
        reader.learn(&label, "1.5").unwrap();
        let ones = reader.reading(Some("1.")).unwrap();
        assert_eq!(
            ones.read(&label, false),
            Err(ReadError::LeftOut {
                read: read_text("1.5"),
            })
        );
        // A whole number ends at its point, and what follows is not read.
        assert_eq!(ones.read(&label, true), Ok(number(1.0)));
        // "5", a blank, "1": the digit before the number fails it too.
        let led = draw(&["###.#.###", "#...#.#..", "###.#.###"]);
        assert_eq!(
            ones.read(&led, false),
            Err(ReadError::LeftOut {
                read: read_text("515"),
            })
        );
        // A shape no glyph matches comes first: the reference has to learn
        // it before the value can leave it out.
        let unknown = draw(&["###.#.#", "#...###", "###.#.#"]);
        assert_eq!(
            ones.read(&unknown, false),
            Err(ReadError::UnknownCharacter {
                position: 1,
                closest: None,
                read: read_text("5?"),
            })
        );
    }

    #[test]
    fn a_digit_the_value_leaves_out_is_not_read_as_a_kept_lookalike() {
        // A 3 and a 9, 16 pixels tall. The 9 is the 3 with one pixel more
        // at the top left, so it matches the 3 but is closer to its own
        // glyph.
        let three = |row: usize| match row {
            0 | 7 | 15 => "########",
            _ => ".......#",
        };
        let nine = |row: usize| match row {
            1 => "#......#",
            _ => three(row),
        };
        let both: Vec<String> = (0..16)
            .map(|row| format!("{}.{}", three(row), nine(row)))
            .collect();
        let both: Vec<&str> = both.iter().map(String::as_str).collect();
        let nine = draw(&(0..16).map(nine).collect::<Vec<_>>());
        let mut reader = DigitReader::new();
        reader.learn(&draw(&both), "39").unwrap();
        assert_eq!(reader.lookalikes(), [('3', '9')]);
        // The two are too alike to tell apart, so a read of both takes the
        // shape for neither.
        assert!(matches!(
            reader.read(&nine, false),
            Err(ReadError::UnknownCharacter { position: 0, .. })
        ));
        let threes = reader.reading(Some("3")).unwrap();
        let scores = threes.explain(&nine).unwrap();
        assert!(
            scores[0]
                .best
                .is_some_and(|best| best.character == '3' && best.matches())
        );
        // A value of 3 alone does not take it for its 3.
        assert_eq!(
            threes.read(&nine, false),
            Err(ReadError::UnknownCharacter {
                position: 0,
                closest: Some('3'),
                read: read_text("?"),
            })
        );
    }

    #[test]
    fn the_workings_show_a_character_the_value_leaves_out_as_text() {
        let digits = taught(28, &LABELS).reading(Some("0123456789")).unwrap();
        let image = drawn("53%", 28, 3.3);
        assert_eq!(digits.read(&image, false), Ok(number(53.0)));
        let scores = digits.explain(&image).unwrap();
        let shown: Vec<_> = scores
            .iter()
            .map(|score| {
                let best = score.best.unwrap();
                (best.character, score.reads(), score.left_out)
            })
            .collect();
        assert_eq!(
            shown,
            [('5', true, false), ('3', true, false), ('%', true, true)]
        );
        // The other glyphs the value leaves out are what it is ranked
        // among.
        assert_eq!(scores[2].runner_up.map(|score| score.character), Some('.'));
        // A shape that matches no glyph at all scores against the glyphs
        // the value reads, as a read's error names the closest of them.
        let no_eight = taught(28, &["12.34%", "56.79%", "0%"])
            .reading(Some("0123456789"))
            .unwrap();
        let unread = no_eight.explain(&drawn("8", 28, 3.3)).unwrap();
        assert!(!unread[0].left_out);
        assert!(unread[0].best.is_some_and(|best| !best.matches()));
        assert!(!unread[0].reads());
        assert_eq!(
            DigitReader::new().explain(&image).unwrap()[0],
            CharacterScore {
                best: None,
                runner_up: None,
                left_out: false,
            }
        );
    }

    #[test]
    fn a_percent_sign_drawn_as_three_marks_is_three_characters() {
        let apart = |text: &str| {
            let pen = Pen {
                percent: Percent::Apart,
                ..pen(20, 3.0)
            };
            draw_text(text, pen, canvas(20))
        };
        let mut reader = DigitReader::new();
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
        assert_eq!(reader.read(&apart("778%"), false), Ok(number(778.0)));
    }

    /// What a reader that learned [`LABELS`] at `learned` pixels makes of
    /// percentages shown at each of `heights`, in every one of
    /// [`SCHEMES`] and at several fractions of a pixel: how many it read,
    /// of how many. A read of another number than the one shown fails the
    /// test.
    fn sweep(learned: u32, heights: &[u32]) -> (u32, u32) {
        let reader = taught(learned, &LABELS);
        // Whether it reads `text`, which shows `value`, as `pen` draws it.
        let reads = |text: &str, value: f64, pen: Pen, height: u32| {
            let said = reader.read(&draw_text(text, pen, canvas(height)), false);
            if let Ok(number) = said {
                assert_eq!(
                    number,
                    self::number(value),
                    "{text} at {height} pixels in {:?}, learned at {learned}",
                    pen.colors
                );
            }
            said.is_ok()
        };
        let (mut read, mut shown) = (0_u32, 0_u32);
        for &height in heights {
            // Text of the background's own color is only its outline,
            // which has to be more than a pixel wide to close it in.
            let schemes = SCHEMES
                .into_iter()
                .filter(|colors| height >= 28 || colors.ink != colors.background);
            for colors in schemes {
                let texts = [
                    ("74.05%", 74.05, 0.0),
                    ("13.96%", 13.96, 0.3),
                    ("100%", 100.0, 0.55),
                    ("8.2%", 8.2, 0.8),
                ];
                let pens = texts.map(|(text, value, across)| {
                    let pen = Pen {
                        colors,
                        ..pen(height, across)
                    };
                    reads(text, value, pen, height)
                });
                let count = |reads: &[bool]| u32::try_from(reads.len()).expect("few reads");
                let passed: Vec<bool> = pens.into_iter().filter(|&read| read).collect();
                shown = shown.checked_add(count(&pens)).expect("few reads");
                read = read.checked_add(count(&passed)).expect("few reads");
            }
        }
        (read, shown)
    }

    #[test]
    fn one_reference_reads_other_sizes_and_colors_and_never_a_wrong_number() {
        // From a third of the size it was learned at to three times it.
        // From less than half the size it was learned at to three times
        // it. A point two pixels wide is too small to know from one
        // learned at more than twice the size, and fails its reads.
        let (read, shown) = sweep(28, &[12, 16, 20, 28, 40]);
        assert_eq!((shown, shown.checked_sub(read)), (96, Some(3)));
        assert_eq!(sweep(16, &[12, 16, 24, 48]), (72, 72));
        assert_eq!(sweep(40, &[14, 20]), (32, 32));
    }
}
