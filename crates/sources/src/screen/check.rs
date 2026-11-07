//! The checks that read a value from a region's image.
//!
//! Each check is one pass of integer arithmetic over the region's pixels,
//! with no allocation, so it costs microseconds and gives the same answer on
//! every platform. `docs/screen.md` explains when to use which.

use std::error::Error;
use std::fmt;

use appytappy_config::signal::{Number, Value};

use super::digits::{DigitReader, ReadError};
use super::{Image, Size};
use crate::system::Rgb;

/// A check that reads one value from a region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    /// Reads a number, such as `37` from `37%`.
    Number(DigitReader),
    /// Measures how full a bar is, from 0 to 100.
    BarFill(BarFill),
    /// Whether the region's average color is near a color.
    Color(ColorMatch),
    /// Whether the region looks like a reference image.
    Reference(ReferenceMatch),
}

impl Check {
    /// Reads the check's value from `image`, the region's pixels.
    ///
    /// # Errors
    ///
    /// Returns a [`CheckError`] when the region holds no value this check
    /// can read, such as no recognizable digits.
    pub fn read(&self, image: &Image) -> Result<Value, CheckError> {
        match self {
            Self::Number(reader) => reader
                .read(image)
                .map(Value::Number)
                .map_err(CheckError::Number),
            Self::BarFill(bar) => bar.percent(image).map(Value::Number),
            Self::Color(color) => Ok(Value::Flag(color.matches(image))),
            Self::Reference(reference) => reference.matches(image).map(Value::Flag),
        }
    }
}

/// The squared distance between a pixel's blue, green, and red bytes and a
/// color. It is at most 3 * 255², so it never overflows a `u32`.
fn distance([blue, green, red, _]: [u8; 4], color: Rgb) -> u32 {
    let square = |a: u8, b: u8| u32::from(a.abs_diff(b)).wrapping_mul(u32::from(a.abs_diff(b)));
    square(blue, color.blue)
        .wrapping_add(square(green, color.green))
        .wrapping_add(square(red, color.red))
}

/// A bar's fill: the share of its pixels nearer the filled color than the
/// empty color.
///
/// Pixels far from both colors, such as a border or a label drawn on the bar,
/// are left out: a pixel counts only when it is no farther from the nearer
/// color than the two colors are from each other. Counting pixels instead of
/// finding an edge makes the reading independent of the bar's direction and
/// steady under a little noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarFill {
    filled: Rgb,
    empty: Rgb,
}

impl BarFill {
    /// A bar whose filled part has the color `filled` and whose empty part
    /// has the color `empty`, as the user picks them on a screenshot.
    ///
    /// # Errors
    ///
    /// Returns [`SameColors`] when the two colors are equal.
    pub fn new(filled: Rgb, empty: Rgb) -> Result<Self, SameColors> {
        if filled == empty {
            return Err(SameColors);
        }
        Ok(Self { filled, empty })
    }

    /// The filled share of `image`, from 0 to 100.
    ///
    /// # Errors
    ///
    /// Returns [`CheckError::NoBar`] when no pixel is near either color.
    pub fn percent(self, image: &Image) -> Result<Number, CheckError> {
        let reach = distance(
            [self.empty.blue, self.empty.green, self.empty.red, 0],
            self.filled,
        );
        let mut filled = 0_u64;
        let mut counted = 0_u64;
        for &pixel in image.pixels() {
            let to_filled = distance(pixel, self.filled);
            let to_empty = distance(pixel, self.empty);
            let near = to_filled.min(to_empty) <= reach;
            // Pixel counts fit in a u64, so these additions never wrap.
            filled = filled.wrapping_add(u64::from(near && to_filled < to_empty));
            counted = counted.wrapping_add(u64::from(near));
        }
        if counted == 0 {
            return Err(CheckError::NoBar);
        }
        Number::new(filled as f64 * 100.0 / counted as f64).ok_or(CheckError::NoBar)
    }
}

/// A bar's filled and empty colors were the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SameColors;

impl fmt::Display for SameColors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bar's filled and empty colors must differ")
    }
}

impl Error for SameColors {}

/// Whether a region's average color is within a tolerance of a color, such
/// as a red flash when the player dies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorMatch {
    /// The color to look for.
    pub color: Rgb,
    /// How far each of the average's red, green, and blue may be from the
    /// color's.
    pub tolerance: u8,
}

impl ColorMatch {
    /// Whether the average color of `image` is within the tolerance.
    #[must_use]
    pub fn matches(self, image: &Image) -> bool {
        let mut sums = [0_u64; 3];
        let mut pixels = 0_u64;
        for pixel in image.pixels() {
            for (sum, &channel) in sums.iter_mut().zip(pixel) {
                // At most 8192² pixels of 255 each, far below u64::MAX.
                *sum = sum.wrapping_add(u64::from(channel));
            }
            pixels = pixels.wrapping_add(1);
        }
        let [blue, green, red] = sums.map(|sum| sum.checked_div(pixels).unwrap_or_default());
        let near = |average: u64, target: u8| {
            average.abs_diff(u64::from(target)) <= u64::from(self.tolerance)
        };
        pixels > 0
            && near(blue, self.color.blue)
            && near(green, self.color.green)
            && near(red, self.color.red)
    }
}

/// Whether a region looks like a reference image of the same region, such as
/// a results screen appearing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceMatch {
    /// The region as the setup screenshot showed it.
    pub reference: Image,
    /// How far, on average, each pixel's red, green, and blue may be from the
    /// reference's.
    pub tolerance: u8,
}

impl ReferenceMatch {
    /// Whether `image` is within the tolerance of the reference.
    ///
    /// # Errors
    ///
    /// Returns [`CheckError::SizeMismatch`] when the sizes differ, which
    /// means the region changed after the reference was taken.
    pub fn matches(&self, image: &Image) -> Result<bool, CheckError> {
        if image.size() != self.reference.size() {
            return Err(CheckError::SizeMismatch {
                expected: self.reference.size(),
                found: image.size(),
            });
        }
        let mut difference = 0_u64;
        let mut channels = 0_u64;
        for (pixel, expected) in image.pixels().iter().zip(self.reference.pixels()) {
            for (&found, &wanted) in pixel.iter().zip(expected).take(3) {
                // At most 3 * 8192² channels of 255 each, far below u64::MAX.
                difference = difference.wrapping_add(u64::from(found.abs_diff(wanted)));
            }
            channels = channels.wrapping_add(3);
        }
        Ok(difference <= channels.saturating_mul(u64::from(self.tolerance)))
    }
}

/// A check could not read a value from a region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckError {
    /// No pixel was near the bar's filled or empty color.
    NoBar,
    /// The region's size differs from the reference image's.
    SizeMismatch {
        /// The reference image's size.
        expected: Size,
        /// The region's size.
        found: Size,
    },
    /// The digits could not be read.
    Number(ReadError),
}

impl fmt::Display for CheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBar => formatter
                .write_str("no pixel in the region is near the bar's filled or empty color"),
            Self::SizeMismatch { expected, found } => write!(
                formatter,
                "the region is {found} but its reference image is {expected}"
            ),
            Self::Number(_) => formatter.write_str("the region's number could not be read"),
        }
    }
}

impl Error for CheckError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Number(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, color: Rgb) -> Image {
        let bgra = (0..width)
            .flat_map(|_| [color.blue, color.green, color.red, 255])
            .collect();
        Image::new(Size { width, height: 1 }, bgra).unwrap()
    }

    fn row(colors: &[Rgb]) -> Image {
        let bgra = colors
            .iter()
            .flat_map(|color| [color.blue, color.green, color.red, 255])
            .collect();
        Image::new(
            Size {
                width: u32::try_from(colors.len()).unwrap(),
                height: 1,
            },
            bgra,
        )
        .unwrap()
    }

    const GREEN: Rgb = Rgb {
        red: 40,
        green: 210,
        blue: 60,
    };
    const GRAY: Rgb = Rgb {
        red: 55,
        green: 55,
        blue: 70,
    };
    const WHITE: Rgb = Rgb {
        red: 255,
        green: 255,
        blue: 255,
    };

    #[test]
    fn bar_fill_counts_pixels_nearer_the_filled_color_and_skips_others() {
        let bar = BarFill::new(GREEN, GRAY).unwrap();
        // A white border on each end, three filled pixels, one empty pixel.
        let image = row(&[WHITE, GREEN, GREEN, GREEN, GRAY, WHITE]);
        assert_eq!(bar.percent(&image), Ok(Number::new(75.0).unwrap()));
        assert_eq!(
            Check::BarFill(bar).read(&image),
            Ok(Value::Number(Number::new(75.0).unwrap()))
        );
        assert_eq!(bar.percent(&solid(4, GRAY)), Ok(Number::new(0.0).unwrap()));
        assert_eq!(bar.percent(&solid(4, WHITE)), Err(CheckError::NoBar));
    }

    #[test]
    fn bar_fill_counts_a_pixel_exactly_at_the_colors_distance() {
        let black = Rgb {
            red: 0,
            green: 0,
            blue: 0,
        };
        let ten = Rgb {
            red: 10,
            green: 0,
            blue: 0,
        };
        let bar = BarFill::new(ten, black).unwrap();
        // 20 is exactly as far from 10 as 10 is from 0, so it counts as
        // filled; 21 is farther and is skipped.
        let at_reach = Rgb {
            red: 20,
            green: 0,
            blue: 0,
        };
        let past_reach = Rgb {
            red: 21,
            green: 0,
            blue: 0,
        };
        assert_eq!(
            bar.percent(&row(&[at_reach, black])),
            Ok(Number::new(50.0).unwrap())
        );
        assert_eq!(
            bar.percent(&row(&[past_reach, black])),
            Ok(Number::new(0.0).unwrap())
        );
        // A pixel halfway between the colors counts as empty.
        let halfway = Rgb {
            red: 5,
            green: 0,
            blue: 0,
        };
        assert_eq!(
            bar.percent(&row(&[halfway, ten])),
            Ok(Number::new(50.0).unwrap())
        );
    }

    #[test]
    fn a_bar_needs_two_colors() {
        assert_eq!(BarFill::new(GREEN, GREEN), Err(SameColors));
    }

    #[test]
    fn color_match_compares_each_channel_of_the_average() {
        let red = ColorMatch {
            color: Rgb {
                red: 200,
                green: 20,
                blue: 20,
            },
            tolerance: 10,
        };
        let dark = Rgb {
            red: 190,
            green: 10,
            blue: 30,
        };
        let bright = Rgb {
            red: 211,
            green: 20,
            blue: 20,
        };
        assert!(red.matches(&solid(3, dark)));
        assert!(!red.matches(&solid(3, bright)));
        assert!(!red.matches(&solid(3, Rgb { green: 31, ..dark })));
        assert!(!red.matches(&solid(3, Rgb { blue: 31, ..dark })));
        // The average of 190 and 211 is 200: one pixel past the tolerance
        // does not decide the match.
        assert!(red.matches(&row(&[dark, bright])));
        assert_eq!(
            Check::Color(red).read(&solid(1, dark)),
            Ok(Value::Flag(true))
        );
    }

    #[test]
    fn reference_match_averages_the_difference_over_every_channel() {
        let reference = row(&[GRAY, GRAY]);
        let check = ReferenceMatch {
            reference,
            tolerance: 4,
        };
        // One channel of one pixel off by 24 averages to 4 over 6 channels.
        let close = row(&[GRAY, Rgb { red: 79, ..GRAY }]);
        let far = row(&[GRAY, Rgb { red: 80, ..GRAY }]);
        assert_eq!(check.matches(&close), Ok(true));
        assert_eq!(check.matches(&far), Ok(false));
        assert_eq!(
            Check::Reference(check.clone()).read(&far),
            Ok(Value::Flag(false))
        );
        assert_eq!(
            check.matches(&solid(3, GRAY)),
            Err(CheckError::SizeMismatch {
                expected: Size {
                    width: 2,
                    height: 1
                },
                found: Size {
                    width: 3,
                    height: 1
                },
            })
        );
    }

    #[test]
    fn reference_match_ignores_the_unused_byte() {
        let reference = solid(2, GRAY);
        let mut bgra = reference.as_bgra().to_vec();
        bgra[3] = 0;
        let other = Image::new(reference.size(), bgra).unwrap();
        let check = ReferenceMatch {
            reference,
            tolerance: 0,
        };
        assert_eq!(check.matches(&other), Ok(true));
    }
}
