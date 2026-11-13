//! A saved value's check, rebuilt from its application's file.
//!
//! The setup app saves a value as plain data: the window size it was drawn
//! at, its region, and its check, which names a picture or a reference, then
//! the side of the window it is pinned to and any fixes, each the value
//! drawn again at another size. A reference is the characters of a font,
//! learned from its own pictures. The runtime turns that back into a
//! [`SavedValue`] here, which says where and how the value reads at each
//! window size. Anything else that reads a saved value should too, so a
//! value reads the same wherever it is read.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::io;

use appytappy_config::app_file::{
    CheckDef, CheckDefError, ColorDef, FixDef, MAX_FIXES, PinDef, ReferenceDef, RegionDef,
    ScreenDef, SizeChangesDef, SizeDef, StateDef,
};
use appytappy_config::signal::{InvalidStateName, StateName};

use super::check::{BarFill, Check, ColorMatch, ImageMatch, StateMatch};
use super::digits::{DigitReader, LearnError};
use super::mask::Mask;
use super::scale::scaled;
use super::{Image, ImageError, Region, Size};
use crate::system::Rgb;

/// A saved value's first drawing: the window size it was drawn at, its
/// region, and its check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedCheck {
    /// The window's size when the region was drawn. The region means
    /// something only at this size, or scaled from it.
    pub window_size: Size,
    /// The region of the window's content the check reads.
    pub region: Region,
    /// The check.
    pub check: Check,
}

/// A saved value drawn again at another window size, which it reads there
/// exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedFix {
    /// The window's size when it was drawn.
    pub window_size: Size,
    /// The region at that size.
    pub region: Region,
    /// Its own check: its state pictures, or the reference learned at this
    /// size. `None` reads with the first drawing's check.
    pub check: Option<Check>,
}

/// A saved value, ready to read at each window size it reads at.
///
/// [`SavedValue::at`] resolves a window size: the drawing made at that
/// size exactly, the first or a fix; else, while its application's window
/// changes size often, the first drawing scaled to it with
/// [`scaled`]; else nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedValue {
    /// Its first drawing, which other sizes scale from.
    pub drawn: SavedCheck,
    /// Its fixes, each at a window size of its own.
    pub fixes: Vec<SavedFix>,
    /// The side of the window its region stays at as the window changes
    /// size.
    pub pin: PinDef,
    /// Whether it reads at window sizes it was not drawn at.
    pub size_changes: SizeChangesDef,
}

/// Where and with what a value reads at one window size, from
/// [`SavedValue::at`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtSize {
    /// The region of the window's content it reads.
    pub region: Region,
    /// Which check reads it.
    check: Drawing,
}

/// The check a value reads with at one size.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Drawing {
    /// The first drawing's.
    First,
    /// The fix at this position's own.
    Fix(usize),
    /// The first drawing's state check with its pictures redrawn at the
    /// region's scaled size.
    Resized(Box<Check>),
}

impl AtSize {
    /// Whether it reads the drawing made at this size exactly, the first
    /// or a fix, rather than one scaled from the first.
    #[must_use]
    pub const fn is_drawn(&self) -> bool {
        !matches!(self.check, Drawing::Resized(_))
    }
}

/// Why a value does not read at a window size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unread {
    /// Its application's window changes size rarely, and it has no drawing
    /// at this size.
    NotDrawn,
    /// Scaled to this size, its region is empty or reaches outside the
    /// window.
    Outside,
    /// Scaled to this size, its state pictures leave no pixel to compare.
    TooSmall,
}

impl fmt::Display for Unread {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotDrawn => "it is read only at the window sizes it was drawn at",
            Self::Outside => "scaled to this window size, its region reaches outside the window",
            Self::TooSmall => "scaled to this window size, its states leave no pixel to compare",
        })
    }
}

impl SavedValue {
    /// The region it reads on a window of `size`, without redrawing its
    /// pictures, as [`SavedValue::at`] resolves it.
    ///
    /// # Errors
    ///
    /// Returns why it does not read at that size: [`Unread::NotDrawn`] or
    /// [`Unread::Outside`]. [`SavedValue::at`] can also find
    /// [`Unread::TooSmall`].
    pub fn region_at(&self, size: Size) -> Result<Region, Unread> {
        self.placed(size).map(|(region, _)| region)
    }

    /// Where it reads on a window of the size it was first drawn at, where
    /// it always reads.
    #[must_use]
    pub const fn first(&self) -> AtSize {
        AtSize {
            region: self.drawn.region,
            check: Drawing::First,
        }
    }

    /// Where and with what it reads on a window of `size`: the drawing made
    /// at that size, the first or a fix; else, while its application's
    /// window changes size often, the first drawing scaled to it, with a
    /// state check's pictures redrawn at the scaled region's size. Each call
    /// redraws them, so a caller resolves each size once.
    ///
    /// # Errors
    ///
    /// Returns why it does not read at that size.
    pub fn at(&self, size: Size) -> Result<AtSize, Unread> {
        let (region, drawing) = self.placed(size)?;
        let check = match drawing {
            Some(drawing) => drawing,
            None => match &self.drawn.check {
                Check::State(states) if region.size() != self.drawn.region.size() => {
                    let resized = states.resized(region.size()).ok_or(Unread::TooSmall)?;
                    Drawing::Resized(Box::new(Check::State(resized)))
                },
                Check::Number { .. } | Check::BarFill(_) | Check::Color(_) | Check::State(_) => {
                    Drawing::First
                },
            },
        };
        Ok(AtSize { region, check })
    }

    /// The check that reads it where `at` says.
    #[must_use]
    pub fn check<'value>(&'value self, at: &'value AtSize) -> &'value Check {
        match &at.check {
            Drawing::First => &self.drawn.check,
            Drawing::Fix(position) => self
                .fixes
                .get(*position)
                .and_then(|fix| fix.check.as_ref())
                .unwrap_or(&self.drawn.check),
            Drawing::Resized(check) => check,
        }
    }

    /// The region at `size` and the drawing made there, or `None` for the
    /// first drawing scaled to it.
    fn placed(&self, size: Size) -> Result<(Region, Option<Drawing>), Unread> {
        if size == self.drawn.window_size {
            return Ok((self.drawn.region, Some(Drawing::First)));
        }
        if let Some((position, fix)) = self
            .fixes
            .iter()
            .enumerate()
            .find(|(_, fix)| fix.window_size == size)
        {
            return Ok((fix.region, Some(Drawing::Fix(position))));
        }
        match self.size_changes {
            SizeChangesDef::Rarely => Err(Unread::NotDrawn),
            SizeChangesDef::Often => {
                scaled(self.drawn.region, self.drawn.window_size, size, self.pin)
                    .map(|region| (region, None))
                    .ok_or(Unread::Outside)
            },
        }
    }

    /// Every window size it was drawn at, the first drawing's first.
    pub fn drawn_sizes(&self) -> impl Iterator<Item = Size> {
        std::iter::once(self.drawn.window_size).chain(self.fixes.iter().map(|fix| fix.window_size))
    }
}

impl From<ColorDef> for Rgb {
    fn from(color: ColorDef) -> Self {
        Self {
            red: color.red,
            green: color.green,
            blue: color.blue,
        }
    }
}

/// Rebuilds the value `screen` describes, with the application's
/// `references`, read as its window's `size_changes` say.
///
/// `picture` returns the bytes of a PNG file the check, a fix or a
/// reference names, such as `status-1.png`.
///
/// This is the one place that decides whether a saved value's pictures and
/// regions can be read, so a value that breaks a rule here stops only
/// itself. The rules of the check's own settings are the file's, which
/// [`CheckDef::check`] states and this reports as [`SavedError::Check`].
///
/// # Errors
///
/// Returns a [`SavedError`] when the check breaks a rule of
/// [`CheckDef::check`], a region is empty or does not fit the window size it
/// was drawn at, a picture cannot be read or decoded or a state's picture is
/// not its region's size, a state's name is invalid or two states share a
/// picture, a number's reference is missing or cannot learn from one of its
/// pictures or knows none of the characters the number reads, or the value
/// has too many fixes, two drawings at one size, or a fix whose pictures or
/// reference do not suit its check.
pub fn build(
    screen: &ScreenDef,
    references: &[ReferenceDef],
    size_changes: SizeChangesDef,
    mut picture: impl FnMut(&str) -> io::Result<Vec<u8>>,
) -> Result<SavedValue, SavedError> {
    let (region, window_size) = place(screen.region, screen.window_size)?;
    screen.check.check().map_err(SavedError::Check)?;
    let mut images = state_images(screen.check.states())?;
    let check = match &screen.check {
        CheckDef::Number {
            reference,
            whole,
            characters,
        } => number_check(
            references,
            reference,
            *whole,
            characters.as_deref(),
            &mut picture,
        )?,
        // `CheckDef::check` refused the same colors, the one thing
        // `BarFill::new` refuses.
        CheckDef::BarFill { filled, empty } => Check::BarFill(
            BarFill::new((*filled).into(), (*empty).into())
                .map_err(|_same| SavedError::Check(CheckDefError::SameColors))?,
        ),
        CheckDef::Color { color, tolerance } => Check::Color(ColorMatch {
            color: (*color).into(),
            tolerance: *tolerance,
        }),
        CheckDef::State { states, tolerance } => {
            let images: Vec<&str> = states.iter().map(|state| state.image.as_str()).collect();
            state_match(states, &images, region.size(), *tolerance, &mut picture)?
        },
    };
    if screen.fixes.len() > MAX_FIXES {
        return Err(SavedError::TooManyFixes);
    }
    let mut sizes = BTreeSet::from([(window_size.width, window_size.height)]);
    let fixes = screen
        .fixes
        .iter()
        .map(|fix| {
            let in_fix = |source: SavedError| SavedError::Fix {
                window_size: Size {
                    width: fix.window_size.width,
                    height: fix.window_size.height,
                },
                source: Box::new(source),
            };
            if !sizes.insert((fix.window_size.width, fix.window_size.height)) {
                return Err(in_fix(SavedError::SameSize));
            }
            for image in &fix.images {
                if !images.insert(image) {
                    return Err(in_fix(SavedError::SamePicture(image.clone())));
                }
            }
            build_fix(fix, &screen.check, references, &mut picture).map_err(in_fix)
        })
        .collect::<Result<_, _>>()?;
    Ok(SavedValue {
        drawn: SavedCheck {
            window_size,
            region,
            check,
        },
        fixes,
        pin: screen.pin(),
        size_changes,
    })
}

/// The region `region` of a window of `size`, once it is not empty and fits.
fn place(region: RegionDef, size: SizeDef) -> Result<(Region, Size), SavedError> {
    let window_size = Size {
        width: size.width,
        height: size.height,
    };
    let region = Region::new(region.x, region.y, region.width, region.height)
        .ok_or(SavedError::EmptyRegion)?;
    if region.fits(window_size) {
        Ok((region, window_size))
    } else {
        Err(SavedError::OutsideWindow { window_size })
    }
}

/// A number check that reads with the reference `id` among `references`:
/// with only its glyphs for `characters`, or with all of them for `None`.
/// The glyphs are chosen here, once, so a read matches against no others.
fn number_check(
    references: &[ReferenceDef],
    id: &str,
    whole: bool,
    characters: Option<&str>,
    picture: impl FnMut(&str) -> io::Result<Vec<u8>>,
) -> Result<Check, SavedError> {
    let reference = references
        .iter()
        .find(|known| known.id == id)
        .ok_or_else(|| SavedError::NoReference { id: id.to_owned() })?;
    let reader = reader(reference, picture)?
        .reading(characters)
        .ok_or_else(|| SavedError::NoCharacters {
            id: id.to_owned(),
            characters: characters.unwrap_or_default().to_owned(),
        })?;
    Ok(Check::Number { reader, whole })
}

/// The fix `fix` of a value whose first drawing's check is `check`.
fn build_fix(
    fix: &FixDef,
    check: &CheckDef,
    references: &[ReferenceDef],
    picture: &mut impl FnMut(&str) -> io::Result<Vec<u8>>,
) -> Result<SavedFix, SavedError> {
    let (region, window_size) = place(fix.region, fix.window_size)?;
    let states = check.states();
    if fix.images.len() != states.len() {
        return Err(SavedError::FixPictures {
            found: fix.images.len(),
            states: states.len(),
        });
    }
    let own = match (check, &fix.reference) {
        (
            CheckDef::Number {
                whole, characters, ..
            },
            Some(reference),
        ) => Some(number_check(
            references,
            reference,
            *whole,
            characters.as_deref(),
            &mut *picture,
        )?),
        (CheckDef::State { states, tolerance }, None) => {
            let images: Vec<&str> = fix.images.iter().map(String::as_str).collect();
            Some(state_match(
                states,
                &images,
                region.size(),
                *tolerance,
                picture,
            )?)
        },
        (CheckDef::Number { .. } | CheckDef::BarFill { .. } | CheckDef::Color { .. }, None) => None,
        (CheckDef::BarFill { .. } | CheckDef::Color { .. } | CheckDef::State { .. }, Some(_)) => {
            return Err(SavedError::FixReference);
        },
    };
    Ok(SavedFix {
        window_size,
        region,
        check: own,
    })
}

/// A state check that tells `states` apart by `images`, one picture of
/// `size` for each state in order.
fn state_match(
    states: &[StateDef],
    images: &[&str],
    size: Size,
    tolerance: u8,
    picture: &mut impl FnMut(&str) -> io::Result<Vec<u8>>,
) -> Result<Check, SavedError> {
    let states = states
        .iter()
        .zip(images)
        .map(|(state, image)| {
            let name = StateName::new(&state.name).map_err(SavedError::StateName)?;
            Ok((name, state_picture(image, size, tolerance, picture)?))
        })
        .collect::<Result<_, SavedError>>()?;
    Ok(Check::State(StateMatch::new(states)))
}

/// The file names of `states`' pictures, once no two states use one.
fn state_images(states: &[StateDef]) -> Result<BTreeSet<&str>, SavedError> {
    let mut images = BTreeSet::new();
    for state in states {
        if !images.insert(state.image.as_str()) {
            return Err(SavedError::SamePicture(state.image.clone()));
        }
    }
    Ok(images)
}

/// A reader with the characters `reference` learns from its pictures, which
/// `picture` returns the bytes of. The pictures may be any size: each is
/// text in the reference's font.
///
/// # Errors
///
/// Returns a [`SavedError`] when a picture cannot be read or decoded, or the
/// reader cannot learn from it.
pub fn reader(
    reference: &ReferenceDef,
    mut picture: impl FnMut(&str) -> io::Result<Vec<u8>>,
) -> Result<DigitReader, SavedError> {
    let mut reader = DigitReader::new(reference.ink.into(), reference.tolerance);
    for sample in &reference.samples {
        reader
            .learn(&load(&sample.image, &mut picture)?, &sample.text)
            .map_err(|source| SavedError::Learn {
                name: sample.image.clone(),
                source,
            })?;
    }
    Ok(reader)
}

/// A state's picture `name`, which must be `size`, compared within
/// `tolerance` and only where its mask keeps pixels.
fn state_picture(
    name: &str,
    size: Size,
    tolerance: u8,
    picture: &mut impl FnMut(&str) -> io::Result<Vec<u8>>,
) -> Result<ImageMatch, SavedError> {
    let (saved, mask) = load_masked(name, picture)?;
    if saved.size() != size {
        return Err(SavedError::PictureSize {
            name: name.to_owned(),
            found: saved.size(),
            expected: size,
        });
    }
    // The mask came from the same file, so its size always fits.
    ImageMatch::new(saved, mask.as_ref(), tolerance).map_err(|error| SavedError::Picture {
        name: name.to_owned(),
        source: ImageError::Mask(error),
    })
}

/// The picture `name` and the mask its transparent pixels make, decoded
/// from the bytes `picture` returns for it. Only a state's picture is
/// masked; a reference's samples are read as they look.
fn load_masked(
    name: &str,
    picture: &mut impl FnMut(&str) -> io::Result<Vec<u8>>,
) -> Result<(Image, Option<Mask>), SavedError> {
    let bytes = picture(name).map_err(|source| SavedError::Missing {
        name: name.to_owned(),
        source,
    })?;
    Image::decode_png_masked(&bytes).map_err(|source| SavedError::Picture {
        name: name.to_owned(),
        source,
    })
}

/// The picture `name`, decoded from the bytes `picture` returns for it.
fn load(
    name: &str,
    picture: &mut impl FnMut(&str) -> io::Result<Vec<u8>>,
) -> Result<Image, SavedError> {
    let bytes = picture(name).map_err(|source| SavedError::Missing {
        name: name.to_owned(),
        source,
    })?;
    Image::decode_png(&bytes).map_err(|source| SavedError::Picture {
        name: name.to_owned(),
        source,
    })
}

/// A saved value cannot be read as saved.
#[derive(Debug)]
pub enum SavedError {
    /// The region has no width or no height.
    EmptyRegion,
    /// The region does not fit inside the window size it was drawn at.
    OutsideWindow {
        /// The window size it was drawn at.
        window_size: Size,
    },
    /// A picture the check names cannot be read.
    Missing {
        /// The picture's file name.
        name: String,
        /// Why it cannot be read.
        source: io::Error,
    },
    /// A picture the check names is not a PNG file AppyTappy can read.
    Picture {
        /// The picture's file name.
        name: String,
        /// Why it cannot be decoded.
        source: ImageError,
    },
    /// A picture's size differs from the region's.
    PictureSize {
        /// The picture's file name.
        name: String,
        /// The picture's size.
        found: Size,
        /// The region's size.
        expected: Size,
    },
    /// The check breaks a rule of its own settings.
    Check(CheckDefError),
    /// The value has more than [`MAX_FIXES`] fixes.
    TooManyFixes,
    /// A fix is at the window size of the first drawing or another fix.
    SameSize,
    /// A fix's pictures do not number one for each state of the check.
    FixPictures {
        /// How many it has.
        found: usize,
        /// How many states the check has.
        states: usize,
    },
    /// A fix names a reference, but the check reads no number.
    FixReference,
    /// One of the value's fixes cannot be read.
    Fix {
        /// The window size the fix was drawn at.
        window_size: Size,
        /// Why.
        source: Box<SavedError>,
    },
    /// Two states use this picture.
    SamePicture(String),
    /// A state's name breaks the naming rules.
    StateName(InvalidStateName),
    /// A number reads with a reference the application does not have.
    NoReference {
        /// The reference's ID.
        id: String,
    },
    /// A number reads only characters that its reference knows none of.
    NoCharacters {
        /// The reference's ID.
        id: String,
        /// The characters the number reads.
        characters: String,
    },
    /// A reference cannot learn from one of its pictures.
    Learn {
        /// The picture's file name.
        name: String,
        /// Why it cannot learn from it.
        source: LearnError,
    },
}

impl fmt::Display for SavedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRegion => formatter.write_str("its region is empty"),
            Self::OutsideWindow { window_size } => write!(
                formatter,
                "its region lies outside the {window_size} window it was drawn on"
            ),
            Self::Missing { name, .. } => write!(formatter, "its picture {name} cannot be read"),
            Self::Picture {
                name,
                source: ImageError::TooLarge { size },
            } => write!(
                formatter,
                "its picture {name} is {size}, larger than AppyTappy reads"
            ),
            Self::Picture { name, .. } => {
                write!(
                    formatter,
                    "its picture {name} is not a PNG file it can read"
                )
            },
            Self::PictureSize {
                name,
                found,
                expected,
            } => write!(
                formatter,
                "its picture {name} is {found} but its region is {expected}"
            ),
            Self::Check(error) => write!(formatter, "{error}"),
            Self::TooManyFixes => write!(formatter, "it has more than {MAX_FIXES} fixes"),
            Self::SameSize => formatter.write_str("it is drawn at this size already"),
            Self::FixPictures { found, states } => write!(
                formatter,
                "it has {found} state pictures for the check's {states} states"
            ),
            Self::FixReference => {
                formatter.write_str("it names a reference, but its check reads no number")
            },
            Self::Fix {
                window_size,
                source,
            } => write!(formatter, "at {window_size}, {source}"),
            Self::SamePicture(name) => {
                write!(formatter, "two of its states use the picture {name}")
            },
            Self::StateName(error) => write!(formatter, "its {error}"),
            Self::NoReference { id } => {
                write!(formatter, "its reference {id} is not saved")
            },
            Self::NoCharacters { id, characters } => write!(
                formatter,
                "its reference {id} knows none of the characters it reads, {characters:?}"
            ),
            Self::Learn { name, source } => {
                write!(
                    formatter,
                    "its reference cannot learn from the picture {name}: {source}"
                )
            },
        }
    }
}

impl Error for SavedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Missing { source, .. } => Some(source),
            Self::Picture { source, .. } => Some(source),
            Self::Learn { source, .. } => Some(source),
            Self::Fix { source, .. } => Some(source.as_ref()),
            Self::EmptyRegion
            | Self::OutsideWindow { .. }
            | Self::PictureSize { .. }
            | Self::Check(_)
            | Self::TooManyFixes
            | Self::SameSize
            | Self::FixPictures { .. }
            | Self::FixReference
            | Self::SamePicture(_)
            | Self::StateName(_)
            | Self::NoReference { .. }
            | Self::NoCharacters { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use appytappy_config::app_file::{RegionDef, SampleDef, SizeDef, StateDef};
    use appytappy_config::names::sample_name;
    use appytappy_config::signal::{Number, Value};

    use super::*;
    use crate::screen::digits::drawn::{Pen, Percent, draw};

    const OFTEN: SizeChangesDef = SizeChangesDef::Often;

    const RED: ColorDef = ColorDef {
        red: 255,
        green: 0,
        blue: 0,
    };
    const GRAY: ColorDef = ColorDef {
        red: 40,
        green: 40,
        blue: 40,
    };

    /// A `width` by 2 image of one color.
    fn solid(width: u32, color: ColorDef) -> Image {
        let pixel = [color.blue, color.green, color.red, u8::MAX];
        let pixels = usize::try_from(width.checked_mul(2).expect("small")).expect("small");
        Image::new(Size { width, height: 2 }, pixel.repeat(pixels)).expect("matching length")
    }

    fn screen(check: CheckDef) -> ScreenDef {
        ScreenDef {
            window_size: SizeDef {
                width: 800,
                height: 600,
            },
            region: RegionDef {
                x: 10,
                y: 20,
                width: 4,
                height: 2,
            },
            check,
            pin: None,
            reading_area: false,
            fixes: Vec::new(),
        }
    }

    fn pictures(files: &[(&str, &Image)]) -> BTreeMap<String, Vec<u8>> {
        files
            .iter()
            .map(|(name, image)| ((*name).to_owned(), image.encode_png().expect("encodes")))
            .collect()
    }

    /// `image` as a PNG file whose every pixel is fully transparent.
    fn transparent(image: &Image) -> Vec<u8> {
        let size = image.size();
        let rgba: Vec<u8> = image
            .pixels()
            .iter()
            .flat_map(|&[blue, green, red, _alpha]| [red, green, blue, 0])
            .collect();
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, size.width, size.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut writer| writer.write_image_data(&rgba))
            .expect("encodes");
        bytes
    }

    fn from(files: &BTreeMap<String, Vec<u8>>) -> impl FnMut(&str) -> io::Result<Vec<u8>> {
        |name| {
            files
                .get(name)
                .cloned()
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        }
    }

    #[test]
    fn a_saved_value_keeps_its_window_size_and_region() {
        let built = build(
            &screen(CheckDef::Color {
                color: RED,
                tolerance: 10,
            }),
            &[],
            OFTEN,
            from(&BTreeMap::new()),
        )
        .expect("builds");
        assert_eq!(
            built.drawn.window_size,
            Size {
                width: 800,
                height: 600
            }
        );
        assert_eq!(
            built.drawn.region,
            Region::new(10, 20, 4, 2).expect("not empty")
        );
        assert_eq!(
            built.drawn.check.read(&solid(4, RED)),
            Ok(Value::Flag(true))
        );
        assert_eq!(
            built.drawn.check.read(&solid(4, GRAY)),
            Ok(Value::Flag(false))
        );
    }

    fn state_check(states: &[(&str, &str)]) -> ScreenDef {
        screen(CheckDef::State {
            states: states
                .iter()
                .map(|(name, image)| StateDef {
                    name: (*name).to_owned(),
                    image: (*image).to_owned(),
                })
                .collect(),
            tolerance: 0,
        })
    }

    fn state(name: &str) -> Value {
        Value::State(StateName::new(name).expect("valid"))
    }

    #[test]
    fn a_state_check_ignores_its_pictures_transparent_pixels() {
        let icon = solid(4, RED);
        // Only the left half of each row belongs to the icon.
        let mask = Mask::new(icon.size(), [true, true, false, false].repeat(2)).unwrap();
        let mut files = BTreeMap::new();
        files.insert(
            "icon.png".to_owned(),
            icon.encode_png_masked(&mask).unwrap(),
        );
        let built = build(
            &state_check(&[("Shown", "icon.png")]),
            &[],
            OFTEN,
            from(&files),
        )
        .expect("builds");
        let red = [RED.blue, RED.green, RED.red, u8::MAX];
        let gray = [GRAY.blue, GRAY.green, GRAY.red, u8::MAX];
        let row = [red, red, gray, gray].concat();
        let changed = Image::new(icon.size(), [row.clone(), row].concat()).unwrap();
        assert_eq!(built.drawn.check.read(&changed), Ok(state("Shown")));
        assert_eq!(
            built.drawn.check.read(&solid(4, GRAY)),
            Ok(Value::State(StateName::none()))
        );
    }

    #[test]
    fn a_bar_reads_its_fill_and_refuses_one_color() {
        let bar = build(
            &screen(CheckDef::BarFill {
                filled: RED,
                empty: GRAY,
            }),
            &[],
            OFTEN,
            from(&BTreeMap::new()),
        )
        .expect("builds");
        assert_eq!(
            bar.drawn.check.read(&solid(4, RED)),
            Ok(Value::Number(Number::new(100.0).expect("finite")))
        );
        let same = build(
            &screen(CheckDef::BarFill {
                filled: RED,
                empty: RED,
            }),
            &[],
            OFTEN,
            from(&BTreeMap::new()),
        );
        assert!(
            matches!(same, Err(SavedError::Check(CheckDefError::SameColors))),
            "{same:?}"
        );
    }

    #[test]
    fn a_state_check_reads_the_state_its_region_shows() {
        let white = ColorDef {
            red: 255,
            green: 255,
            blue: 255,
        };
        let files = pictures(&[("red.png", &solid(4, RED)), ("gray.png", &solid(4, GRAY))]);
        let built = build(
            &state_check(&[("Red", "red.png"), ("Gray", "gray.png")]),
            &[],
            OFTEN,
            from(&files),
        )
        .expect("builds");
        assert_eq!(built.drawn.check.read(&solid(4, RED)), Ok(state("Red")));
        assert_eq!(built.drawn.check.read(&solid(4, GRAY)), Ok(state("Gray")));
        assert_eq!(
            built.drawn.check.read(&solid(4, white)),
            Ok(Value::State(StateName::none()))
        );
        let error = build(
            &state_check(&[("none", "red.png")]),
            &[],
            OFTEN,
            from(&files),
        )
        .expect_err("reserved name");
        assert_eq!(
            error.to_string(),
            "its state name \"none\" is \"none\", which stands for no state"
        );
        assert!(error.source().is_none());
    }

    /// "11" as two separate red columns on gray, 4 by 2 pixels drawn
    /// `scale` times larger.
    fn elevens(scale: u32) -> Image {
        let ink = [0, 0, 255, 255];
        let paper = [40, 40, 40, 255];
        let wide = usize::try_from(scale).expect("small");
        let row = [ink, paper, ink, paper]
            .iter()
            .flat_map(|pixel| pixel.repeat(wide))
            .collect::<Vec<u8>>();
        let size = Size {
            width: scale.saturating_mul(4),
            height: scale.saturating_mul(2),
        };
        Image::new(size, row.repeat(wide.saturating_mul(2))).expect("matching length")
    }

    fn counter(text: &str) -> ReferenceDef {
        ReferenceDef {
            id: "counter".to_owned(),
            name: "Counter".to_owned(),
            ink: RED,
            tolerance: 10,
            samples: vec![SampleDef {
                image: sample_name("counter", 1),
                text: text.to_owned(),
            }],
            expects: None,
        }
    }

    #[test]
    fn what_a_reference_expects_changes_nothing_it_learns_or_reads() {
        let files = pictures(&[(&sample_name("counter", 1), &elevens(3))]);
        let number = CheckDef::Number {
            reference: "counter".to_owned(),
            whole: false,
            characters: None,
        };
        let plain = build(
            &screen(number.clone()),
            &[counter("11")],
            OFTEN,
            from(&files),
        )
        .expect("builds");
        // The same glyphs and the same number rules, whatever it expects,
        // even a list no setup app would save.
        for expected in ["0123456789,.%", "11 ", "", &"x".repeat(5000)] {
            let mut expecting = counter("11");
            expecting.expects = Some(expected.to_owned());
            let guided =
                build(&screen(number.clone()), &[expecting], OFTEN, from(&files)).expect("builds");
            assert_eq!(guided.drawn.check, plain.drawn.check, "{expected:?}");
        }
    }

    /// A window of `size` that draws `text` near the middle of its top
    /// edge, as large as the window is tall: digits 20 pixels tall in a
    /// window 300 tall.
    fn window_showing(text: &str, size: Size) -> Image {
        let height = f64::from(size.height) / 15.0;
        let pen = Pen {
            height,
            at: (f64::from(size.width) * 0.376, height * 0.25),
            percent: Percent::Joined,
        };
        draw(text, pen, size)
    }

    #[test]
    fn a_saved_number_reads_in_a_larger_window_that_draws_it_larger() {
        let drawn_at = Size {
            width: 400,
            height: 300,
        };
        let region = Region::new(140, 0, 180, 36).expect("not empty");
        // The reference as the setup app saves it: the region of each
        // labeled screenshot, as a PNG file.
        let labels = ["12.34%", "56.78%", "90%"];
        let mut files = BTreeMap::new();
        let samples = labels
            .iter()
            .zip(1_usize..)
            .map(|(text, number)| {
                let sample = window_showing(text, drawn_at)
                    .crop(region)
                    .expect("inside the window");
                let image = sample_name("percent", number);
                files.insert(image.clone(), sample.encode_png().expect("encodes"));
                SampleDef {
                    image,
                    text: (*text).to_owned(),
                }
            })
            .collect();
        let reference = ReferenceDef {
            id: "percent".to_owned(),
            name: "Percent".to_owned(),
            ink: ColorDef {
                red: 255,
                green: 255,
                blue: 255,
            },
            tolerance: 40,
            samples,
            expects: None,
        };
        let value = ScreenDef {
            window_size: SizeDef {
                width: drawn_at.width,
                height: drawn_at.height,
            },
            region: RegionDef {
                x: region.x(),
                y: region.y(),
                width: region.size().width,
                height: region.size().height,
            },
            check: CheckDef::Number {
                reference: "percent".to_owned(),
                whole: false,
                characters: None,
            },
            pin: None,
            fixes: Vec::new(),
            reading_area: false,
        };
        let built = build(&value, &[reference], OFTEN, from(&files)).expect("builds");
        // The runtime's read: the value placed at the window's size, and
        // its check on that region of the frame.
        for (size, text, shown) in [
            (drawn_at, "74.05%", 74.05),
            (
                Size {
                    width: 600,
                    height: 450,
                },
                "74.05%",
                74.05,
            ),
            (
                Size {
                    width: 500,
                    height: 375,
                },
                "100%",
                100.0,
            ),
        ] {
            let at = built.at(size).expect("reads at this size");
            let frame = window_showing(text, size);
            assert_eq!(
                built
                    .check(&at)
                    .read(&frame.crop(at.region).expect("inside the window")),
                Ok(Value::Number(Number::new(shown).expect("finite"))),
                "{text} in a {size} window"
            );
        }
    }

    #[test]
    fn a_reading_area_changes_nothing_a_value_reads() {
        let files = pictures(&[(&sample_name("counter", 1), &elevens(3))]);
        let number = CheckDef::Number {
            reference: "counter".to_owned(),
            whole: false,
            characters: None,
        };
        let bar = CheckDef::BarFill {
            filled: RED,
            empty: GRAY,
        };
        for check in [number, bar] {
            let plain = build(
                &screen(check.clone()),
                &[counter("11")],
                OFTEN,
                from(&files),
            )
            .expect("builds");
            let mut area = screen(check);
            area.reading_area = true;
            let guided = build(&area, &[counter("11")], OFTEN, from(&files)).expect("builds");
            // The same region, check, pin and sizes, whatever the flag says.
            assert_eq!(guided, plain);
        }
    }

    #[test]
    fn a_number_reads_with_its_reference_at_any_size() {
        // The sample is taller than the region, and its own size.
        let files = pictures(&[(&sample_name("counter", 1), &elevens(3))]);
        let number = CheckDef::Number {
            reference: "counter".to_owned(),
            whole: true,
            characters: None,
        };
        let built = build(
            &screen(number.clone()),
            &[counter("11")],
            OFTEN,
            from(&files),
        )
        .expect("builds");
        assert_eq!(
            built.drawn.check.read(&elevens(1)),
            Ok(Value::Number(Number::new(11.0).expect("finite")))
        );
        assert!(matches!(
            built.drawn.check,
            Check::Number { whole: true, .. }
        ));
        // A sample's transparency means nothing, even when every pixel is
        // transparent.
        let mut clear = BTreeMap::new();
        clear.insert(sample_name("counter", 1), transparent(&elevens(3)));
        let from_clear =
            build(&screen(number), &[counter("11")], OFTEN, from(&clear)).expect("builds");
        assert_eq!(
            from_clear.drawn.check.read(&elevens(1)),
            Ok(Value::Number(Number::new(11.0).expect("finite")))
        );
        let error = build(
            &screen(CheckDef::Number {
                reference: "counter".to_owned(),
                whole: false,
                characters: None,
            }),
            &[counter("111")],
            OFTEN,
            from(&files),
        )
        .expect_err("two characters, not three");
        assert_eq!(
            error.to_string(),
            "its reference cannot learn from the picture ref-counter-1.png: the region shows 2 \
             characters but the text has 3; characters that touch count as one"
        );
        assert!(error.source().is_some());
        let error = build(
            &screen(CheckDef::Number {
                reference: "digits".to_owned(),
                whole: false,
                characters: None,
            }),
            &[counter("11")],
            OFTEN,
            from(&files),
        )
        .expect_err("no such reference");
        assert_eq!(error.to_string(), "its reference digits is not saved");
        assert!(error.source().is_none());
    }

    /// Rings in the reference's color, 16 pixels tall and each as wide as
    /// `widths` says, a blank column apart.
    fn rings(widths: &[usize]) -> Image {
        let line = |fill: &str| {
            let rings: Vec<String> = widths
                .iter()
                .map(|width| format!("#{}#", fill.repeat(width.saturating_sub(2))))
                .collect();
            rings.join(".")
        };
        let (edge, middle) = (line("#"), line("."));
        let mut rows = vec![middle.as_str(); 16];
        rows[0] = &edge;
        rows[15] = &edge;
        let bgra = rows
            .concat()
            .chars()
            .flat_map(|cell| {
                if cell == '#' {
                    [0, 0, 255, 255]
                } else {
                    [40, 40, 40, 255]
                }
            })
            .collect();
        let size = Size {
            width: u32::try_from(edge.len()).expect("small"),
            height: 16,
        };
        Image::new(size, bgra).expect("matching length")
    }

    #[test]
    fn a_number_reads_only_the_characters_it_is_told_to() {
        // The reference knows a letter and the digit that looks like it, a
        // column wider, so each shape matches both and the letter's own
        // glyph is closer.
        let files = pictures(&[
            (&sample_name("counter", 1), &rings(&[10, 11])),
            ("ref-letters-1.png", &rings(&[10])),
        ]);
        let number = |characters: Option<&str>| {
            screen(CheckDef::Number {
                reference: "counter".to_owned(),
                whole: false,
                characters: characters.map(str::to_owned),
            })
        };
        let read = |characters: Option<&str>| {
            build(&number(characters), &[counter("O0")], OFTEN, from(&files))
                .expect("builds")
                .drawn
                .check
                .read(&rings(&[10]))
        };
        let zero = Ok(Value::Number(Number::new(0.0).expect("finite")));
        assert!(read(None).is_err());
        assert!(read(Some("O")).is_err());
        // Characters the reference has yet to learn are passed over.
        assert_eq!(read(Some("0123456789")), zero);
        let error = build(&number(Some("%")), &[counter("O0")], OFTEN, from(&files))
            .expect_err("it knows none of them");
        assert_eq!(
            error.to_string(),
            "its reference counter knows none of the characters it reads, \"%\""
        );
        assert!(error.source().is_none());
        // A fix that reads with its own reference reads the same characters
        // of it.
        let mut fixed = number(Some("0"));
        fixed.fixes = vec![FixDef {
            reference: Some("letters".to_owned()),
            ..fix_at(1000, 500, (5, 5, 2, 2), &[])
        }];
        let mut letters = counter("O");
        letters.id = "letters".to_owned();
        letters.samples[0].image = "ref-letters-1.png".to_owned();
        let error = build(&fixed, &[counter("O0"), letters], OFTEN, from(&files))
            .expect_err("the fix's reference knows no digit");
        assert_eq!(
            error.to_string(),
            "at 1000x500, its reference letters knows none of the characters it reads, \"0\""
        );
    }

    #[test]
    fn broken_pictures_and_regions_say_what_is_wrong() {
        let reference = |image: &str| state_check(&[("Shown", image)]);
        let mut files = pictures(&[("small.png", &solid(3, RED))]);
        files.insert("broken.png".to_owned(), b"not a picture".to_vec());
        // Only a header, which says the picture is one column wider than the
        // largest AppyTappy reads: it is refused before any of it decodes.
        let mut huge = Vec::new();
        png::Encoder::new(&mut huge, 7681, 4320)
            .write_header()
            .expect("a header");
        files.insert("huge.png".to_owned(), huge);
        let problems = ["gone.png", "broken.png", "huge.png", "small.png"].map(|name| {
            let error = build(&reference(name), &[], OFTEN, from(&files)).expect_err("refused");
            (error.to_string(), error.source().is_some())
        });
        assert_eq!(
            problems,
            [
                ("its picture gone.png cannot be read".to_owned(), true),
                (
                    "its picture broken.png is not a PNG file it can read".to_owned(),
                    true
                ),
                (
                    "its picture huge.png is 7681x4320, larger than AppyTappy reads".to_owned(),
                    true
                ),
                (
                    "its picture small.png is 3x2 but its region is 4x2".to_owned(),
                    false
                ),
            ]
        );
        let mut empty = reference("small.png");
        empty.region.width = 0;
        let error = build(&empty, &[], OFTEN, from(&files)).expect_err("empty");
        assert_eq!(error.to_string(), "its region is empty");
        assert!(error.source().is_none());
        let mut outside = reference("small.png");
        outside.region.x = 797;
        let error = build(&outside, &[], OFTEN, from(&files)).expect_err("outside");
        assert_eq!(
            error.to_string(),
            "its region lies outside the 800x600 window it was drawn on"
        );
        assert!(error.source().is_none());
        outside.region.x = 796;
        let edge = build(&outside, &[], OFTEN, from(&files));
        assert!(
            matches!(edge, Err(SavedError::PictureSize { .. })),
            "touching the edge fits, so only the picture is wrong: {edge:?}"
        );
    }

    #[test]
    fn a_state_check_needs_its_file_s_rules_and_each_picture_once() {
        let files = pictures(&[("a.png", &solid(4, RED)), ("b.png", &solid(4, GRAY))]);
        let refused = |states: &[(&str, &str)]| {
            build(&state_check(states), &[], OFTEN, from(&files)).expect_err("refused")
        };
        let same = refused(&[("Paused", "a.png"), ("paused", "b.png")]);
        assert!(
            matches!(
                &same,
                SavedError::Check(CheckDefError::SameStateName(name)) if name == "paused"
            ),
            "{same:?}"
        );
        assert_eq!(same.to_string(), "two of its states are named \"paused\"");
        assert!(matches!(
            refused(&[]),
            SavedError::Check(CheckDefError::NoStates)
        ));
        let refused = |states: &[(&str, &str)]| refused(states).to_string();
        assert_eq!(
            refused(&[("Paused", "a.png"), ("Live", "a.png")]),
            "two of its states use the picture a.png"
        );
        assert!(refused(&[(" ", "a.png")]).starts_with("its state name \" \" is empty"));
        assert!(
            build(
                &state_check(&[("Paused", "a.png"), ("Live", "b.png")]),
                &[],
                OFTEN,
                from(&files)
            )
            .is_ok()
        );
    }

    /// An image of `size` and one color.
    fn filled(size: Size, color: ColorDef) -> Image {
        let pixel = [color.blue, color.green, color.red, u8::MAX];
        let pixels =
            usize::try_from(size.width.checked_mul(size.height).expect("small")).expect("small");
        Image::new(size, pixel.repeat(pixels)).expect("matching length")
    }

    fn size(width: u32, height: u32) -> Size {
        Size { width, height }
    }

    fn fix_at(width: u32, height: u32, region: (u32, u32, u32, u32), images: &[&str]) -> FixDef {
        FixDef {
            window_size: SizeDef { width, height },
            region: RegionDef {
                x: region.0,
                y: region.1,
                width: region.2,
                height: region.3,
            },
            images: images.iter().map(|&image| image.to_owned()).collect(),
            reference: None,
        }
    }

    /// A red-or-gray state check drawn at 800x600 on (10, 20, 4, 2), with
    /// a fix at 1000x500 on (5, 5, 2, 2) whose pictures are swapped, so a
    /// read tells which drawing read it.
    fn red_or_gray() -> (ScreenDef, BTreeMap<String, Vec<u8>>) {
        let mut screen = state_check(&[("Red", "status-1.png"), ("Gray", "status-2.png")]);
        screen.fixes = vec![fix_at(
            1000,
            500,
            (5, 5, 2, 2),
            &["status-1-1.png", "status-1-2.png"],
        )];
        let files = pictures(&[
            ("status-1.png", &solid(4, RED)),
            ("status-2.png", &solid(4, GRAY)),
            ("status-1-1.png", &filled(size(2, 2), GRAY)),
            ("status-1-2.png", &filled(size(2, 2), RED)),
        ]);
        (screen, files)
    }

    #[test]
    fn a_value_reads_its_drawing_at_its_size_and_scales_elsewhere() {
        let (screen, files) = red_or_gray();
        let value = build(&screen, &[], OFTEN, from(&files)).expect("builds");
        // Its center is in the top-left third of the window.
        assert_eq!(value.pin, PinDef::TopLeft);
        let drawn = value.at(size(800, 600)).expect("reads");
        assert_eq!(drawn.region, Region::new(10, 20, 4, 2).expect("not empty"));
        assert!(drawn.is_drawn());
        assert_eq!(value.check(&drawn).read(&solid(4, RED)), Ok(state("Red")));
        // At the fix's size, the fix reads exactly, with its own pictures.
        let fixed = value.at(size(1000, 500)).expect("reads");
        assert_eq!(fixed.region, Region::new(5, 5, 2, 2).expect("not empty"));
        assert!(fixed.is_drawn());
        assert_eq!(
            value.check(&fixed).read(&filled(size(2, 2), RED)),
            Ok(state("Gray"))
        );
        // Twice as tall, the first drawing scales: twice as far from the top
        // left, and its pictures twice the size.
        let doubled = value.at(size(1600, 1200)).expect("reads");
        assert_eq!(
            doubled.region,
            Region::new(20, 40, 8, 4).expect("not empty")
        );
        assert!(!doubled.is_drawn());
        assert_eq!(value.region_at(size(1600, 1200)), Ok(doubled.region));
        let check = value.check(&doubled);
        assert_eq!(check.read(&filled(size(8, 4), GRAY)), Ok(state("Gray")));
        assert_eq!(check.read(&filled(size(8, 4), RED)), Ok(state("Red")));
        // Only wider, the region keeps its size, so its pictures do too.
        let wider = value.at(size(1000, 600)).expect("reads");
        assert!(
            wider.is_drawn(),
            "the first drawing's check, not a redrawn one"
        );
        assert_eq!(value.check(&wider).read(&solid(4, GRAY)), Ok(state("Gray")));
        assert_eq!(
            value.drawn_sizes().collect::<Vec<_>>(),
            [size(800, 600), size(1000, 500)]
        );
    }

    #[test]
    fn a_value_that_cannot_scale_says_why() {
        let (screen, files) = red_or_gray();
        let rarely = build(&screen, &[], SizeChangesDef::Rarely, from(&files)).expect("builds");
        assert_eq!(rarely.at(size(1600, 1200)), Err(Unread::NotDrawn));
        assert_eq!(rarely.region_at(size(1600, 1200)), Err(Unread::NotDrawn));
        assert!(rarely.at(size(1000, 500)).is_ok(), "a fix still reads");
        assert!(rarely.at(size(800, 600)).is_ok());
        let often = build(&screen, &[], OFTEN, from(&files)).expect("builds");
        // Too narrow for the region pinned left at 10 to 14.
        assert_eq!(often.at(size(12, 600)), Err(Unread::Outside));
        assert_eq!(often.region_at(size(12, 600)), Err(Unread::Outside));
        // A quarter as tall scales the 2 rows to 1 but every gray-over-red
        // mask pixel of a sparse picture to nothing.
        let mut sparse = state_check(&[("Dot", "dot.png")]);
        sparse.region.width = 4;
        let icon = solid(4, RED);
        let mask = Mask::new(
            icon.size(),
            vec![true, false, false, false, false, false, false, false],
        )
        .expect("keeps one");
        let mut dotted = BTreeMap::new();
        dotted.insert(
            "dot.png".to_owned(),
            icon.encode_png_masked(&mask).expect("encodes"),
        );
        let dot = build(&sparse, &[], OFTEN, from(&dotted)).expect("builds");
        assert_eq!(dot.at(size(400, 300)), Err(Unread::TooSmall));
        assert_eq!(
            dot.region_at(size(400, 300)),
            Ok(Region::new(5, 10, 2, 1).expect("not empty")),
            "the region alone still fits"
        );
        assert_eq!(
            [Unread::NotDrawn, Unread::Outside, Unread::TooSmall].map(|unread| unread.to_string()),
            [
                "it is read only at the window sizes it was drawn at",
                "scaled to this window size, its region reaches outside the window",
                "scaled to this window size, its states leave no pixel to compare",
            ]
        );
    }

    #[test]
    fn a_number_fix_reads_with_the_reference_learned_at_its_size() {
        let files = pictures(&[
            (&sample_name("counter", 1), &elevens(3)),
            (&sample_name("big", 1), &elevens(2)),
        ]);
        let mut big = counter("11");
        big.id = "big".to_owned();
        // A wider tolerance tells its reader from the counter's.
        big.tolerance = 20;
        big.samples[0].image = sample_name("big", 1);
        let mut screen = screen(CheckDef::Number {
            reference: "counter".to_owned(),
            whole: true,
            characters: None,
        });
        screen.fixes = vec![
            FixDef {
                reference: Some("big".to_owned()),
                ..fix_at(1000, 500, (0, 0, 8, 4), &[])
            },
            fix_at(400, 300, (0, 0, 4, 2), &[]),
        ];
        let references = [counter("11"), big];
        let value = build(&screen, &references, OFTEN, from(&files)).expect("builds");
        let own = value.at(size(1000, 500)).expect("reads");
        assert!(matches!(
            value.check(&own),
            Check::Number { whole: true, .. }
        ));
        assert_ne!(value.check(&own), &value.drawn.check, "its own reference");
        let shared = value.at(size(400, 300)).expect("reads");
        assert_eq!(value.check(&shared), &value.drawn.check);
        assert_eq!(
            value.check(&own).read(&elevens(2)),
            Ok(Value::Number(Number::new(11.0).expect("finite")))
        );
        screen.fixes[0].reference = Some("gone".to_owned());
        let error = build(&screen, &references, OFTEN, from(&files)).expect_err("missing");
        assert_eq!(
            error.to_string(),
            "at 1000x500, its reference gone is not saved"
        );
        assert!(error.source().is_some());
    }

    #[test]
    fn a_value_may_have_as_many_fixes_as_the_most_and_no_more() {
        let mut screen = screen(CheckDef::Color {
            color: RED,
            tolerance: 10,
        });
        let fixes = |count: usize| -> Vec<FixDef> {
            (0..count)
                .map(|n| {
                    fix_at(
                        1000 + u32::try_from(n).expect("small"),
                        500,
                        (0, 0, 1, 1),
                        &[],
                    )
                })
                .collect()
        };
        screen.fixes = fixes(MAX_FIXES);
        let built = build(&screen, &[], OFTEN, from(&BTreeMap::new())).expect("builds");
        assert_eq!(built.fixes.len(), MAX_FIXES);
        screen.fixes = fixes(MAX_FIXES + 1);
        assert_eq!(
            build(&screen, &[], OFTEN, from(&BTreeMap::new()))
                .expect_err("one too many")
                .to_string(),
            format!("it has more than {MAX_FIXES} fixes")
        );
    }

    #[test]
    fn fixes_that_do_not_suit_the_value_say_what_is_wrong() {
        let (states, files) = red_or_gray();
        let refused = |change: &dyn Fn(&mut ScreenDef)| {
            let mut screen = states.clone();
            change(&mut screen);
            build(&screen, &[], OFTEN, from(&files))
                .expect_err("refused")
                .to_string()
        };
        assert_eq!(
            refused(&|screen| screen.fixes[0].window_size = SizeDef {
                width: 800,
                height: 600
            }),
            "at 800x600, it is drawn at this size already"
        );
        assert_eq!(
            refused(&|screen| screen.fixes.push(screen.fixes[0].clone())),
            "at 1000x500, it is drawn at this size already"
        );
        assert_eq!(
            refused(&|screen| {
                screen.fixes = (0..=MAX_FIXES)
                    .map(|n| {
                        fix_at(
                            1000 + u32::try_from(n).expect("small"),
                            500,
                            (0, 0, 1, 1),
                            &[],
                        )
                    })
                    .collect();
            }),
            format!("it has more than {MAX_FIXES} fixes")
        );
        assert_eq!(
            refused(&|screen| {
                screen.fixes[0].images.pop();
            }),
            "at 1000x500, it has 1 state pictures for the check's 2 states"
        );
        assert_eq!(
            refused(&|screen| screen.fixes[0].images[1] = "status-1.png".to_owned()),
            "at 1000x500, two of its states use the picture status-1.png"
        );
        assert_eq!(
            refused(&|screen| screen.fixes[0].region.width = 3),
            "at 1000x500, its picture status-1-1.png is 2x2 but its region is 3x2"
        );
        assert_eq!(
            refused(&|screen| screen.fixes[0].region.x = 999),
            "at 1000x500, its region lies outside the 1000x500 window it was drawn on"
        );
        assert_eq!(
            refused(&|screen| screen.fixes[0].reference = Some("counter".to_owned())),
            "at 1000x500, it names a reference, but its check reads no number"
        );
        let color = screen(CheckDef::Color {
            color: RED,
            tolerance: 1,
        });
        let mut pictured = color.clone();
        pictured.fixes = vec![fix_at(1000, 500, (0, 0, 2, 2), &["x.png"])];
        let error = build(&pictured, &[], OFTEN, from(&files)).expect_err("refused");
        assert_eq!(
            error.to_string(),
            "at 1000x500, it has 1 state pictures for the check's 0 states"
        );
        let mut plain = color;
        plain.fixes = vec![fix_at(1000, 500, (0, 0, 2, 2), &[])];
        let value = build(&plain, &[], OFTEN, from(&files)).expect("builds");
        assert_eq!(
            value.fixes[0].check, None,
            "a color fix reads with the value's own check"
        );
    }
}
