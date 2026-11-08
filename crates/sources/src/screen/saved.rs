//! A saved value's check, rebuilt from its application's file.
//!
//! The setup app saves a value as plain data: the window size it was drawn
//! at, its region, and its check, which names a picture or a reference. A
//! reference is the characters of a font, learned from its own pictures. The
//! runtime turns that back into a [`Check`] here. Anything else that reads a
//! saved value should too, so a value reads the same wherever it is read.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::io;

use appytappy_config::app_file::{
    CheckDef, ColorDef, MAX_STATES, ReferenceDef, ScreenDef, StateDef,
};
use appytappy_config::signal::{InvalidStateName, StateName};

use super::check::{BarFill, Check, ColorMatch, ImageMatch, StateMatch};
use super::digits::{DigitReader, LearnError};
use super::mask::Mask;
use super::{Image, ImageError, Region, Size};
use crate::system::Rgb;

/// A saved value, ready to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedCheck {
    /// The window's size when the region was drawn. The region means
    /// something only while the window has this size.
    pub window_size: Size,
    /// The region of the window's frame the check reads.
    pub region: Region,
    /// The check.
    pub check: Check,
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

/// Rebuilds the check `screen` describes, with the application's
/// `references`. `picture` returns the bytes of a PNG file the check or its
/// reference names, such as `status-1.png`.
///
/// This is the one place that decides whether a saved value can be read: the
/// application's file checks only what the whole file needs, so a value that
/// breaks a rule here stops only itself.
///
/// # Errors
///
/// Returns a [`SavedError`] when the region is empty or does not fit the
/// window size it was drawn at, a picture cannot be read or decoded or a
/// state's picture is not the region's size, a state check has no states or
/// too many, a state's name is invalid or two states share a name or a
/// picture, the bar's two colors are the same, or a number's reference is
/// missing or cannot learn from one of its pictures.
pub fn build(
    screen: &ScreenDef,
    references: &[ReferenceDef],
    mut picture: impl FnMut(&str) -> io::Result<Vec<u8>>,
) -> Result<SavedCheck, SavedError> {
    let region = Region::new(
        screen.region.x,
        screen.region.y,
        screen.region.width,
        screen.region.height,
    )
    .ok_or(SavedError::EmptyRegion)?;
    let window_size = Size {
        width: screen.window_size.width,
        height: screen.window_size.height,
    };
    if !region.fits(window_size) {
        return Err(SavedError::OutsideWindow { window_size });
    }
    let check = match &screen.check {
        CheckDef::Number { reference, whole } => {
            let reference = references
                .iter()
                .find(|known| known.id == *reference)
                .ok_or_else(|| SavedError::NoReference {
                    id: reference.clone(),
                })?;
            Check::Number {
                reader: reader(reference, picture)?,
                whole: *whole,
            }
        },
        CheckDef::BarFill { filled, empty } => Check::BarFill(
            BarFill::new((*filled).into(), (*empty).into())
                .map_err(|_same| SavedError::SameColors)?,
        ),
        CheckDef::Color { color, tolerance } => Check::Color(ColorMatch {
            color: (*color).into(),
            tolerance: *tolerance,
        }),
        CheckDef::State { states, tolerance } => {
            check_states(states)?;
            let states = states
                .iter()
                .map(|state| {
                    let name = StateName::new(&state.name).map_err(SavedError::StateName)?;
                    let picture =
                        state_picture(&state.image, region.size(), *tolerance, &mut picture)?;
                    Ok((name, picture))
                })
                .collect::<Result<_, SavedError>>()?;
            Check::State(StateMatch::new(states))
        },
    };
    Ok(SavedCheck {
        window_size,
        region,
        check,
    })
}

/// Checks a state check's states: at least one and at most [`MAX_STATES`],
/// and no name or picture twice. Names that differ only in case count as the
/// same, since they read alike.
fn check_states(states: &[StateDef]) -> Result<(), SavedError> {
    if states.is_empty() {
        return Err(SavedError::NoStates);
    }
    if states.len() > MAX_STATES {
        return Err(SavedError::TooManyStates);
    }
    let mut names = BTreeSet::new();
    let mut images = BTreeSet::new();
    for state in states {
        if !names.insert(state.name.to_lowercase()) {
            return Err(SavedError::SameStateName(state.name.clone()));
        }
        if !images.insert(state.image.as_str()) {
            return Err(SavedError::SamePicture(state.image.clone()));
        }
    }
    Ok(())
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
    /// The bar's filled and empty colors are the same.
    SameColors,
    /// A state check has no states.
    NoStates,
    /// A state check has more than [`MAX_STATES`] states.
    TooManyStates,
    /// Two states have this name, or names that differ only in case.
    SameStateName(String),
    /// Two states use this picture.
    SamePicture(String),
    /// A state's name breaks the naming rules.
    StateName(InvalidStateName),
    /// A number reads with a reference the application does not have.
    NoReference {
        /// The reference's ID.
        id: String,
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
            Self::SameColors => {
                formatter.write_str("its bar's filled and empty colors are the same")
            },
            Self::NoStates => formatter.write_str("its state check has no state"),
            Self::TooManyStates => write!(
                formatter,
                "its state check has more than {MAX_STATES} states"
            ),
            Self::SameStateName(name) => {
                write!(formatter, "two of its states are named {name:?}")
            },
            Self::SamePicture(name) => {
                write!(formatter, "two of its states use the picture {name}")
            },
            Self::StateName(error) => write!(formatter, "its {error}"),
            Self::NoReference { id } => {
                write!(formatter, "its reference {id} is not saved")
            },
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
            Self::EmptyRegion
            | Self::OutsideWindow { .. }
            | Self::PictureSize { .. }
            | Self::SameColors
            | Self::NoStates
            | Self::TooManyStates
            | Self::SameStateName(_)
            | Self::SamePicture(_)
            | Self::StateName(_)
            | Self::NoReference { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use appytappy_config::app_file::{RegionDef, SampleDef, SizeDef, StateDef, sample_name};
    use appytappy_config::signal::{Number, Value};

    use super::*;

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
            from(&BTreeMap::new()),
        )
        .expect("builds");
        assert_eq!(
            built.window_size,
            Size {
                width: 800,
                height: 600
            }
        );
        assert_eq!(built.region, Region::new(10, 20, 4, 2).expect("not empty"));
        assert_eq!(built.check.read(&solid(4, RED)), Ok(Value::Flag(true)));
        assert_eq!(built.check.read(&solid(4, GRAY)), Ok(Value::Flag(false)));
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
        let built =
            build(&state_check(&[("Shown", "icon.png")]), &[], from(&files)).expect("builds");
        let red = [RED.blue, RED.green, RED.red, u8::MAX];
        let gray = [GRAY.blue, GRAY.green, GRAY.red, u8::MAX];
        let row = [red, red, gray, gray].concat();
        let changed = Image::new(icon.size(), [row.clone(), row].concat()).unwrap();
        assert_eq!(built.check.read(&changed), Ok(state("Shown")));
        assert_eq!(
            built.check.read(&solid(4, GRAY)),
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
            from(&BTreeMap::new()),
        )
        .expect("builds");
        assert_eq!(
            bar.check.read(&solid(4, RED)),
            Ok(Value::Number(Number::new(100.0).expect("finite")))
        );
        let same = build(
            &screen(CheckDef::BarFill {
                filled: RED,
                empty: RED,
            }),
            &[],
            from(&BTreeMap::new()),
        );
        assert!(matches!(same, Err(SavedError::SameColors)), "{same:?}");
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
            from(&files),
        )
        .expect("builds");
        assert_eq!(built.check.read(&solid(4, RED)), Ok(state("Red")));
        assert_eq!(built.check.read(&solid(4, GRAY)), Ok(state("Gray")));
        assert_eq!(
            built.check.read(&solid(4, white)),
            Ok(Value::State(StateName::none()))
        );
        let error = build(&state_check(&[("none", "red.png")]), &[], from(&files))
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
        }
    }

    #[test]
    fn a_number_reads_with_its_reference_at_any_size() {
        // The sample is taller than the region, and its own size.
        let files = pictures(&[(&sample_name("counter", 1), &elevens(3))]);
        let number = CheckDef::Number {
            reference: "counter".to_owned(),
            whole: true,
        };
        let built = build(&screen(number.clone()), &[counter("11")], from(&files)).expect("builds");
        assert_eq!(
            built.check.read(&elevens(1)),
            Ok(Value::Number(Number::new(11.0).expect("finite")))
        );
        assert!(matches!(built.check, Check::Number { whole: true, .. }));
        // A sample's transparency means nothing, even when every pixel is
        // transparent.
        let mut clear = BTreeMap::new();
        clear.insert(sample_name("counter", 1), transparent(&elevens(3)));
        let from_clear = build(&screen(number), &[counter("11")], from(&clear)).expect("builds");
        assert_eq!(
            from_clear.check.read(&elevens(1)),
            Ok(Value::Number(Number::new(11.0).expect("finite")))
        );
        let error = build(
            &screen(CheckDef::Number {
                reference: "counter".to_owned(),
                whole: false,
            }),
            &[counter("1")],
            from(&files),
        )
        .expect_err("two characters, not one");
        assert_eq!(
            error.to_string(),
            "its reference cannot learn from the picture ref-counter-1.png: the region shows 2 \
             characters but the text has 1; characters that touch count as one"
        );
        assert!(error.source().is_some());
        let error = build(
            &screen(CheckDef::Number {
                reference: "digits".to_owned(),
                whole: false,
            }),
            &[counter("11")],
            from(&files),
        )
        .expect_err("no such reference");
        assert_eq!(error.to_string(), "its reference digits is not saved");
        assert!(error.source().is_none());
    }

    #[test]
    fn broken_pictures_and_regions_say_what_is_wrong() {
        let reference = |image: &str| state_check(&[("Shown", image)]);
        let mut files = pictures(&[("small.png", &solid(3, RED))]);
        files.insert("broken.png".to_owned(), b"not a picture".to_vec());
        let problems = ["gone.png", "broken.png", "small.png"].map(|name| {
            let error = build(&reference(name), &[], from(&files)).expect_err("refused");
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
                    "its picture small.png is 3x2 but its region is 4x2".to_owned(),
                    false
                ),
            ]
        );
        let mut empty = reference("small.png");
        empty.region.width = 0;
        let error = build(&empty, &[], from(&files)).expect_err("empty");
        assert_eq!(error.to_string(), "its region is empty");
        assert!(error.source().is_none());
        let mut outside = reference("small.png");
        outside.region.x = 797;
        let error = build(&outside, &[], from(&files)).expect_err("outside");
        assert_eq!(
            error.to_string(),
            "its region lies outside the 800x600 window it was drawn on"
        );
        assert!(error.source().is_none());
        outside.region.x = 796;
        let edge = build(&outside, &[], from(&files));
        assert!(
            matches!(edge, Err(SavedError::PictureSize { .. })),
            "touching the edge fits, so only the picture is wrong: {edge:?}"
        );
        assert_eq!(
            SavedError::SameColors.to_string(),
            "its bar's filled and empty colors are the same"
        );
    }

    #[test]
    fn a_state_check_needs_a_few_states_each_named_and_pictured_once() {
        let files = pictures(&[("a.png", &solid(4, RED)), ("b.png", &solid(4, GRAY))]);
        let refused = |states: &[(&str, &str)]| {
            build(&state_check(states), &[], from(&files))
                .expect_err("refused")
                .to_string()
        };
        assert_eq!(refused(&[]), "its state check has no state");
        let many: Vec<(String, String)> = (0..=MAX_STATES)
            .map(|state| (format!("State {state}"), "a.png".to_owned()))
            .collect();
        let many: Vec<(&str, &str)> = many
            .iter()
            .map(|(name, image)| (name.as_str(), image.as_str()))
            .collect();
        assert_eq!(
            refused(&many),
            format!("its state check has more than {MAX_STATES} states")
        );
        assert_eq!(
            refused(&[("Paused", "a.png"), ("paused", "b.png")]),
            "two of its states are named \"paused\""
        );
        assert_eq!(
            refused(&[("Paused", "a.png"), ("Live", "a.png")]),
            "two of its states use the picture a.png"
        );
        assert!(refused(&[(" ", "a.png")]).starts_with("its state name \" \" is empty"));
        assert!(
            build(
                &state_check(&[("Paused", "a.png"), ("Live", "b.png")]),
                &[],
                from(&files)
            )
            .is_ok()
        );
    }
}
