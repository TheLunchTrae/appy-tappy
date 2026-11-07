//! A saved value's check, rebuilt from its application's file.
//!
//! The setup app saves a value as plain data: the window size it was drawn
//! at, its region, its check, and the pictures the check learns from. The
//! runtime turns that back into a [`Check`] here. Anything else that reads a
//! saved value should too, so a value reads the same wherever it is read.

use std::error::Error;
use std::fmt;
use std::io;

use appytappy_config::app_file::{CheckDef, ColorDef, ScreenDef};

use super::check::{BarFill, Check, ColorMatch, ReferenceMatch};
use super::digits::{DigitReader, LearnError};
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

/// Rebuilds the check `screen` describes. `picture` returns the bytes of the
/// PNG file the check names, such as `progress-1.png`.
///
/// # Errors
///
/// Returns a [`SavedError`] when the region is empty or does not fit the
/// window size it was drawn at, a picture cannot be
/// read or decoded or is not the region's size, the bar's two colors are the
/// same, or a number cannot learn from one of its pictures.
pub fn build(
    screen: &ScreenDef,
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
    let mut load = |name: &str| {
        let bytes = picture(name).map_err(|source| SavedError::Missing {
            name: name.to_owned(),
            source,
        })?;
        let image = Image::decode_png(&bytes).map_err(|source| SavedError::Picture {
            name: name.to_owned(),
            source,
        })?;
        if image.size() != region.size() {
            return Err(SavedError::PictureSize {
                name: name.to_owned(),
                found: image.size(),
                expected: region.size(),
            });
        }
        Ok(image)
    };
    let check = match &screen.check {
        CheckDef::Number {
            ink,
            tolerance,
            samples,
        } => {
            let mut reader = DigitReader::new((*ink).into(), *tolerance);
            for sample in samples {
                reader
                    .learn(&load(&sample.image)?, &sample.text)
                    .map_err(|source| SavedError::Learn {
                        name: sample.image.clone(),
                        source,
                    })?;
            }
            Check::Number(reader)
        },
        CheckDef::BarFill { filled, empty } => Check::BarFill(
            BarFill::new((*filled).into(), (*empty).into())
                .map_err(|_same| SavedError::SameColors)?,
        ),
        CheckDef::Color { color, tolerance } => Check::Color(ColorMatch {
            color: (*color).into(),
            tolerance: *tolerance,
        }),
        CheckDef::Reference { image, tolerance } => Check::Reference(ReferenceMatch {
            reference: load(image)?,
            tolerance: *tolerance,
        }),
    };
    Ok(SavedCheck {
        window_size,
        region,
        check,
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
    /// A number cannot learn from one of its pictures.
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
            Self::Learn { name, source } => {
                write!(
                    formatter,
                    "it cannot learn from its picture {name}: {source}"
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
            | Self::SameColors => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use appytappy_config::app_file::{RegionDef, SampleDef, SizeDef};
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
    fn a_bar_reads_its_fill_and_refuses_one_color() {
        let bar = build(
            &screen(CheckDef::BarFill {
                filled: RED,
                empty: GRAY,
            }),
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
            from(&BTreeMap::new()),
        );
        assert!(matches!(same, Err(SavedError::SameColors)), "{same:?}");
    }

    #[test]
    fn a_reference_compares_with_its_picture() {
        let files = pictures(&[("icon.png", &solid(4, RED))]);
        let built = build(
            &screen(CheckDef::Reference {
                image: "icon.png".to_owned(),
                tolerance: 0,
            }),
            from(&files),
        )
        .expect("builds");
        assert_eq!(built.check.read(&solid(4, RED)), Ok(Value::Flag(true)));
        assert_eq!(built.check.read(&solid(4, GRAY)), Ok(Value::Flag(false)));
    }

    #[test]
    fn a_number_learns_from_its_samples() {
        // Two separate ink columns read as two characters.
        let ink = [0, 0, 255, 255];
        let paper = [40, 40, 40, 255];
        let row = [ink, paper, ink, paper].concat();
        let picture = Image::new(
            Size {
                width: 4,
                height: 2,
            },
            [row.clone(), row].concat(),
        )
        .expect("matching length");
        let files = pictures(&[("count-1.png", &picture)]);
        let number = CheckDef::Number {
            ink: RED,
            tolerance: 10,
            samples: vec![SampleDef {
                image: "count-1.png".to_owned(),
                text: "11".to_owned(),
            }],
        };
        let built = build(&screen(number), from(&files)).expect("builds");
        assert_eq!(
            built.check.read(&picture),
            Ok(Value::Number(Number::new(11.0).expect("finite")))
        );
        let wrong = CheckDef::Number {
            ink: RED,
            tolerance: 10,
            samples: vec![SampleDef {
                image: "count-1.png".to_owned(),
                text: "1".to_owned(),
            }],
        };
        let error = build(&screen(wrong), from(&files)).expect_err("two characters, not one");
        assert_eq!(
            error.to_string(),
            "it cannot learn from its picture count-1.png: the region shows 2 characters but the \
             text has 1; characters that touch count as one"
        );
        assert!(error.source().is_some());
    }

    #[test]
    fn broken_pictures_and_regions_say_what_is_wrong() {
        let reference = |image: &str| {
            screen(CheckDef::Reference {
                image: image.to_owned(),
                tolerance: 0,
            })
        };
        let mut files = pictures(&[("small.png", &solid(3, RED))]);
        files.insert("broken.png".to_owned(), b"not a picture".to_vec());
        let problems = ["gone.png", "broken.png", "small.png"].map(|name| {
            let error = build(&reference(name), from(&files)).expect_err("refused");
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
        let error = build(&empty, from(&files)).expect_err("empty");
        assert_eq!(error.to_string(), "its region is empty");
        assert!(error.source().is_none());
        let mut outside = reference("small.png");
        outside.region.x = 797;
        let error = build(&outside, from(&files)).expect_err("outside");
        assert_eq!(
            error.to_string(),
            "its region lies outside the 800x600 window it was drawn on"
        );
        assert!(error.source().is_none());
        outside.region.x = 796;
        let edge = build(&outside, from(&files));
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
