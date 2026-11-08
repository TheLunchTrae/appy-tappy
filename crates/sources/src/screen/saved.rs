//! A saved value's check, rebuilt from its application's file.
//!
//! The setup app saves a value as plain data: the window size it was drawn
//! at, its region, and its check, which names a picture or a reference. A
//! reference is the characters of a font, learned from its own pictures. The
//! runtime turns that back into a [`Check`] here. Anything else that reads a
//! saved value should too, so a value reads the same wherever it is read.

use std::error::Error;
use std::fmt;
use std::io;

use appytappy_config::app_file::{CheckDef, ColorDef, ReferenceDef, ScreenDef};

use super::check::{BarFill, Check, ColorMatch, ImageMatch};
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
/// reference names, such as `icon.png`.
///
/// # Errors
///
/// Returns a [`SavedError`] when the region is empty or does not fit the
/// window size it was drawn at, a picture cannot be read or decoded or an
/// image check's picture is not the region's size, the bar's two colors are
/// the same, or a number's reference is missing or cannot learn from one of
/// its pictures.
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
        CheckDef::Image { image, tolerance } => {
            let (saved, mask) = load_masked(image, &mut picture)?;
            if saved.size() != region.size() {
                return Err(SavedError::PictureSize {
                    name: image.clone(),
                    found: saved.size(),
                    expected: region.size(),
                });
            }
            // The mask came from the same file, so its size always fits.
            let check = ImageMatch::new(saved, mask.as_ref(), *tolerance).map_err(|error| {
                SavedError::Picture {
                    name: image.clone(),
                    source: ImageError::Mask(error),
                }
            })?;
            Check::Image(check)
        },
    };
    Ok(SavedCheck {
        window_size,
        region,
        check,
    })
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

/// The picture `name` and the mask its transparent pixels make, decoded
/// from the bytes `picture` returns for it. Only an image check's picture
/// is masked; a reference's samples are read as they look.
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
            | Self::NoReference { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use appytappy_config::app_file::{RegionDef, SampleDef, SizeDef, sample_name};
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

    #[test]
    fn an_image_check_ignores_its_pictures_transparent_pixels() {
        let icon = solid(4, RED);
        // Only the left half of each row belongs to the icon.
        let mask = Mask::new(icon.size(), [true, true, false, false].repeat(2)).unwrap();
        let mut files = BTreeMap::new();
        files.insert(
            "icon.png".to_owned(),
            icon.encode_png_masked(&mask).unwrap(),
        );
        let built = build(
            &screen(CheckDef::Image {
                image: "icon.png".to_owned(),
                tolerance: 0,
            }),
            &[],
            from(&files),
        )
        .expect("builds");
        let red = [RED.blue, RED.green, RED.red, u8::MAX];
        let gray = [GRAY.blue, GRAY.green, GRAY.red, u8::MAX];
        let row = [red, red, gray, gray].concat();
        let changed = Image::new(icon.size(), [row.clone(), row].concat()).unwrap();
        assert_eq!(built.check.read(&changed), Ok(Value::Flag(true)));
        assert_eq!(built.check.read(&solid(4, GRAY)), Ok(Value::Flag(false)));
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
    fn an_image_check_compares_with_its_picture() {
        let files = pictures(&[("icon.png", &solid(4, RED))]);
        let built = build(
            &screen(CheckDef::Image {
                image: "icon.png".to_owned(),
                tolerance: 0,
            }),
            &[],
            from(&files),
        )
        .expect("builds");
        assert_eq!(built.check.read(&solid(4, RED)), Ok(Value::Flag(true)));
        assert_eq!(built.check.read(&solid(4, GRAY)), Ok(Value::Flag(false)));
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
        let reference = |image: &str| {
            screen(CheckDef::Image {
                image: image.to_owned(),
                tolerance: 0,
            })
        };
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
}
