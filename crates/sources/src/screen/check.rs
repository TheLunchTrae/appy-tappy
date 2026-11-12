//! The checks that read a value from a region's image.
//!
//! Each check is one pass of integer arithmetic over the region's pixels,
//! with no allocation, so it costs microseconds and gives the same answer on
//! every platform. `wiki/Screen-regions.md` explains when to use which.

use std::error::Error;
use std::fmt;
use std::ops::Range;

use appytappy_config::signal::{Number, StateName, Value};

use super::digits::{DigitReader, ReadError};
use super::mask::{Mask, MaskError};
use super::resample::resample;
use super::{Image, Size};
use crate::system::Rgb;

/// A check that reads one value from a region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    /// Reads a number, such as `37` from `37%`.
    Number {
        /// The reader with the reference's characters.
        reader: DigitReader,
        /// Whether only the whole part counts, so `37.5%` reads as 37.
        whole: bool,
    },
    /// Measures how full a bar is, from 0 to 100.
    BarFill(BarFill),
    /// Whether the region's average color is near a color.
    Color(ColorMatch),
    /// Which of a few saved pictures of it the region looks like.
    State(StateMatch),
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
            Self::Number { reader, whole } => reader
                .read(image, *whole)
                .map(Value::Number)
                .map_err(CheckError::Number),
            Self::BarFill(bar) => bar.percent(image).map(Value::Number),
            Self::Color(color) => Ok(Value::Flag(color.matches(image))),
            Self::State(states) => states.read(image).map(Value::State),
        }
    }
}

/// The squared distance between two colors. It is at most 3 * 255², so it
/// never overflows a `u32`.
pub(super) fn distance(one: Rgb, other: Rgb) -> u32 {
    let square = |a: u8, b: u8| u32::from(a.abs_diff(b)).wrapping_mul(u32::from(a.abs_diff(b)));
    square(one.blue, other.blue)
        .wrapping_add(square(one.green, other.green))
        .wrapping_add(square(one.red, other.red))
}

/// A bar's fill: the share of its pixels nearer the filled color than the
/// empty color.
///
/// Pixels far from both colors, such as a border or a label drawn on the bar,
/// are left out: a pixel counts only when it is within half the distance
/// between the two colors of the nearer one. Counting pixels instead of
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

    /// The color of the filled part.
    #[must_use]
    pub const fn filled(self) -> Rgb {
        self.filled
    }

    /// The color of the empty part.
    #[must_use]
    pub const fn empty(self) -> Rgb {
        self.empty
    }

    /// This bar with its two colors the other way around.
    #[must_use]
    pub const fn turned(self) -> Self {
        Self {
            filled: self.empty,
            empty: self.filled,
        }
    }

    /// What `pixel` counts as, as `[blue, green, red, unused]` like
    /// [`Image::pixels`] holds it: the nearer of the two colors when it is
    /// within half their distance of it, the empty color on a tie, and
    /// neither otherwise.
    #[must_use]
    pub fn classify(self, [blue, green, red, _]: [u8; 4]) -> BarPixel {
        let color = Rgb { red, green, blue };
        let to_filled = distance(color, self.filled);
        let to_empty = distance(color, self.empty);
        // Squared distances, so half the distance is a quarter of the
        // distance between the two colors.
        if to_filled.min(to_empty).saturating_mul(4) > distance(self.filled, self.empty) {
            BarPixel::Neither
        } else if to_filled < to_empty {
            BarPixel::Filled
        } else {
            BarPixel::Empty
        }
    }

    /// The filled share of `image`, from 0 to 100.
    ///
    /// # Errors
    ///
    /// Returns [`CheckError::NoBar`] when no pixel is near either color.
    pub fn percent(self, image: &Image) -> Result<Number, CheckError> {
        let mut filled = 0_u64;
        let mut counted = 0_u64;
        for &pixel in image.pixels() {
            let class = self.classify(pixel);
            // Pixel counts fit in a u64, so these additions never wrap.
            filled = filled.wrapping_add(u64::from(class == BarPixel::Filled));
            counted = counted.wrapping_add(u64::from(class != BarPixel::Neither));
        }
        if counted == 0 {
            return Err(CheckError::NoBar);
        }
        Number::new(filled as f64 * 100.0 / counted as f64).ok_or(CheckError::NoBar)
    }
}

/// What one pixel of a bar counts as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarPixel {
    /// Part of the bar's filled share.
    Filled,
    /// Part of the bar's empty share.
    Empty,
    /// Far from both colors, such as a border or a label, and left out.
    Neither,
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
/// as a panel turning red.
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
        self.near(average(image))
    }

    /// Whether each of `color`'s red, green, and blue is within the
    /// tolerance of the looked-for color's.
    #[must_use]
    pub fn near(self, color: Rgb) -> bool {
        self.farthest_channel(color) <= self.tolerance
    }

    /// How far `color` is from the looked-for color: the largest difference
    /// among their red, green, and blue, which the tolerance bounds.
    #[must_use]
    pub fn farthest_channel(self, color: Rgb) -> u8 {
        color
            .red
            .abs_diff(self.color.red)
            .max(color.green.abs_diff(self.color.green))
            .max(color.blue.abs_diff(self.color.blue))
    }
}

/// The average color of `image`: the mean of each of its pixels' red, green,
/// and blue, rounded down.
#[must_use]
pub fn average(image: &Image) -> Rgb {
    let mut sums = [0_u64; 3];
    let mut pixels = 0_u64;
    for pixel in image.pixels() {
        for (sum, &channel) in sums.iter_mut().zip(pixel) {
            // At most 8192² pixels of 255 each, far below u64::MAX.
            *sum = sum.wrapping_add(u64::from(channel));
        }
        pixels = pixels.wrapping_add(1);
    }
    // A mean of bytes is a byte, and an image is never empty.
    let [blue, green, red] = sums.map(|sum| {
        sum.checked_div(pixels)
            .and_then(|mean| u8::try_from(mean).ok())
            .unwrap_or_default()
    });
    Rgb { red, green, blue }
}

/// Whether a region looks like a saved picture of the same region, such as
/// a results screen appearing.
///
/// A [`Mask`] limits the comparison to the pixels of the thing itself, so a
/// changing background behind it does not count. The runs of compared pixels
/// are worked out once, when the check is made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageMatch {
    picture: Image,
    spans: Box<[Range<usize>]>,
    tolerance: u8,
}

impl ImageMatch {
    /// A check that compares a region with `picture`, the region as a
    /// setup screenshot showed it. It compares only the pixels `mask` keeps,
    /// or every pixel without one. `tolerance` is how far, on average, each
    /// compared pixel's red, green, and blue may be from the picture's.
    ///
    /// # Errors
    ///
    /// Returns [`MaskError::SizeMismatch`] when the mask's size differs from
    /// the picture's.
    pub fn new(picture: Image, mask: Option<&Mask>, tolerance: u8) -> Result<Self, MaskError> {
        let spans = match mask {
            Some(mask) if mask.size() != picture.size() => {
                return Err(MaskError::SizeMismatch {
                    image: picture.size(),
                    mask: mask.size(),
                });
            },
            Some(mask) => mask.spans(),
            None => return Ok(Self::whole(picture, tolerance)),
        };
        Ok(Self {
            picture,
            spans: spans.into_boxed_slice(),
            tolerance,
        })
    }

    /// A check that compares every pixel of a region with `picture`. It
    /// takes no mask, so it cannot fail.
    #[must_use]
    pub fn whole(picture: Image, tolerance: u8) -> Self {
        let spans = std::iter::once(0..picture.pixels().len()).collect();
        Self {
            picture,
            spans,
            tolerance,
        }
    }

    /// The region as the setup screenshot showed it.
    #[must_use]
    pub const fn picture(&self) -> &Image {
        &self.picture
    }

    /// This check for a region of `size`, with its picture and mask redrawn,
    /// or `None` when `size` is empty or the redrawn mask keeps no pixel.
    ///
    /// Shrinking averages the pixels each new pixel covers, and growing
    /// takes the nearest one. A pixel of the mask is kept when at least half
    /// the pixels it covers are, and takes the color of only those.
    #[must_use]
    pub fn resized(&self, size: Size) -> Option<Self> {
        let mut kept = vec![false; self.picture.pixels().len()];
        for span in &self.spans {
            kept.get_mut(span.clone()).unwrap_or_default().fill(true);
        }
        let (picture, kept) = resample(&self.picture, &kept, size)?;
        let mask = Mask::new(size, kept).ok()?;
        Self::new(picture, Some(&mask), self.tolerance).ok()
    }

    /// Whether `image` is within the tolerance of the picture over the
    /// compared pixels.
    ///
    /// # Errors
    ///
    /// Returns [`CheckError::SizeMismatch`] when the sizes differ, which
    /// means the region changed after the picture was taken.
    pub fn matches(&self, image: &Image) -> Result<bool, CheckError> {
        self.difference(image)
            .map(|difference| difference.within(self.tolerance))
    }

    /// How far `image` is from the picture over the compared pixels.
    ///
    /// # Errors
    ///
    /// Returns [`CheckError::SizeMismatch`] when the sizes differ.
    fn difference(&self, image: &Image) -> Result<Difference, CheckError> {
        if image.size() != self.picture.size() {
            return Err(CheckError::SizeMismatch {
                expected: self.picture.size(),
                found: image.size(),
            });
        }
        let mut difference = 0_u64;
        let mut channels = 0_u64;
        for span in &self.spans {
            let found = image.pixels().get(span.clone()).unwrap_or_default();
            let wanted = self.picture.pixels().get(span.clone()).unwrap_or_default();
            for (pixel, expected) in found.iter().zip(wanted) {
                for (&found, &wanted) in pixel.iter().zip(expected).take(3) {
                    // At most 3 * 8192² channels of 255 each, far below u64::MAX.
                    difference = difference.wrapping_add(u64::from(found.abs_diff(wanted)));
                }
                channels = channels.wrapping_add(3);
            }
        }
        Ok(Difference {
            total: difference,
            channels,
        })
    }
}

/// How far a region is from a picture: the summed difference of the
/// compared red, green, and blue channels, and how many there were.
#[derive(Debug, Clone, Copy)]
struct Difference {
    total: u64,
    channels: u64,
}

impl Difference {
    /// Whether the average difference per channel is at most `tolerance`.
    fn within(self, tolerance: u8) -> bool {
        self.total <= self.channels.saturating_mul(u64::from(tolerance))
    }

    /// Whether the average difference is smaller than `other`'s. Both totals
    /// and channel counts are below 2^40, so the products fit a `u128`.
    fn closer_than(self, other: Self) -> bool {
        u128::from(self.total).wrapping_mul(u128::from(other.channels))
            < u128::from(other.total).wrapping_mul(u128::from(self.channels))
    }
}

/// Which of a few named pictures a region looks like, such as a corner that
/// shows `Paused` or `Live`.
///
/// Each state is an [`ImageMatch`] with its own picture and mask. The region
/// reads as the state whose picture it is closest to on average, among those
/// within the tolerance, or [`StateName::none`] when it is within none of
/// them. A tie goes to the earlier state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateMatch {
    states: Box<[(StateName, ImageMatch)]>,
    none: StateName,
}

impl StateMatch {
    /// A check that tells `states` apart, each a name and its picture.
    #[must_use]
    pub fn new(states: Vec<(StateName, ImageMatch)>) -> Self {
        Self {
            states: states.into_boxed_slice(),
            none: StateName::none(),
        }
    }

    /// This check for a region of `size`, each state's picture redrawn with
    /// [`ImageMatch::resized`], or `None` when one of them cannot be.
    #[must_use]
    pub fn resized(&self, size: Size) -> Option<Self> {
        let states = self
            .states
            .iter()
            .map(|(name, picture)| Some((name.clone(), picture.resized(size)?)))
            .collect::<Option<Vec<_>>>()?;
        Some(Self::new(states))
    }

    /// The state `image` shows, or [`StateName::none`].
    ///
    /// # Errors
    ///
    /// Returns [`CheckError::SizeMismatch`] when `image` is not the size of
    /// a state's picture, which means the region changed after the pictures
    /// were taken.
    pub fn read(&self, image: &Image) -> Result<StateName, CheckError> {
        let mut best: Option<(&StateName, Difference)> = None;
        for (name, picture) in &self.states {
            let difference = picture.difference(image)?;
            let closer = best.is_none_or(|(_, closest)| difference.closer_than(closest));
            if difference.within(picture.tolerance) && closer {
                best = Some((name, difference));
            }
        }
        Ok(best.map_or(&self.none, |(name, _)| name).clone())
    }
}

/// A check could not read a value from a region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckError {
    /// No pixel was near the bar's filled or empty color.
    NoBar,
    /// The region's size differs from the saved picture's.
    SizeMismatch {
        /// The saved picture's size.
        expected: Size,
        /// The region's size.
        found: Size,
    },
    /// The number could not be read, and at which stage of the read.
    Number(ReadError),
}

impl CheckError {
    /// What went wrong, without what this read saw: a region that keeps
    /// failing the same way keeps this text while what it shows changes, so
    /// a log can say it once. [`ReadError::stage`] for a number.
    #[must_use]
    pub fn stage(&self) -> String {
        match self {
            Self::Number(error) => error.stage().to_owned(),
            Self::NoBar | Self::SizeMismatch { .. } => self.to_string(),
        }
    }
}

impl fmt::Display for CheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBar => formatter
                .write_str("no pixel in the region is near the bar's filled or empty color"),
            Self::SizeMismatch { expected, found } => write!(
                formatter,
                "the region is {found} but its saved picture is {expected}"
            ),
            // The read's own error says which stage failed, in full.
            Self::Number(error) => write!(formatter, "{error}"),
        }
    }
}

/// A number's error shows as its read's error, which has no cause of its
/// own, so no variant has a source to give.
impl Error for CheckError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_explain_themselves() {
        let size = Size {
            width: 2,
            height: 1,
        };
        assert_eq!(
            SameColors.to_string(),
            "a bar's filled and empty colors must differ"
        );
        assert_eq!(
            CheckError::NoBar.to_string(),
            "no pixel in the region is near the bar's filled or empty color"
        );
        let mismatch = CheckError::SizeMismatch {
            expected: size,
            found: Size {
                width: 3,
                height: 1,
            },
        };
        assert_eq!(
            mismatch.to_string(),
            "the region is 3x1 but its saved picture is 2x1"
        );
        assert_eq!(mismatch.stage(), mismatch.to_string());
        assert_eq!(CheckError::NoBar.stage(), CheckError::NoBar.to_string());
        // A number's error is its read's, so the reason shows wherever the
        // check's error does.
        let read = DigitReader::new(WHITE, 0)
            .read(&solid(1, WHITE), false)
            .expect_err("no glyph learned");
        let number = CheckError::Number(read.clone());
        assert_eq!(number.to_string(), read.to_string());
        assert_eq!(number.stage(), read.stage());
        assert_ne!(number.stage(), number.to_string());
    }

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

    /// Rows of colors, each as long as the first, as an image.
    fn rows(rows: &[&[Rgb]]) -> Image {
        let bgra = rows
            .iter()
            .flat_map(|row| row.iter())
            .flat_map(|color| [color.blue, color.green, color.red, 255])
            .collect();
        Image::new(
            Size {
                width: u32::try_from(rows[0].len()).unwrap(),
                height: u32::try_from(rows.len()).unwrap(),
            },
            bgra,
        )
        .unwrap()
    }

    #[test]
    fn a_bar_reads_the_same_fill_wherever_it_sits_in_a_larger_region() {
        let bar = BarFill::new(GREEN, GRAY).unwrap();
        // Three filled pixels and one empty one, in a region of white: at
        // its top left, in its middle, and at its bottom right.
        let blank = [WHITE; 8];
        let left = [GREEN, GREEN, GREEN, GRAY, WHITE, WHITE, WHITE, WHITE];
        let middle = [WHITE, WHITE, GREEN, GREEN, GREEN, GRAY, WHITE, WHITE];
        let right = [WHITE, WHITE, WHITE, WHITE, GREEN, GREEN, GREEN, GRAY];
        for region in [
            [&left, &blank, &blank],
            [&blank, &middle, &blank],
            [&blank, &blank, &right],
        ] {
            let region: Vec<&[Rgb]> = region.iter().map(|row| row.as_slice()).collect();
            assert_eq!(bar.percent(&rows(&region)), Ok(Number::new(75.0).unwrap()));
        }
        // Another pixel of the empty color in the region counts with the
        // bar's.
        let stray = [GRAY, WHITE, WHITE, WHITE, WHITE, WHITE, WHITE, WHITE];
        assert_eq!(
            bar.percent(&rows(&[&blank, &middle, &stray])),
            Ok(Number::new(60.0).unwrap())
        );
    }

    #[test]
    fn bar_fill_counts_a_pixel_exactly_at_half_the_colors_distance() {
        let red = |red| Rgb {
            red,
            green: 0,
            blue: 0,
        };
        let bar = BarFill::new(red(10), red(0)).unwrap();
        // 15 is half the colors' distance from 10, so it counts as filled;
        // 16 is farther and is skipped.
        assert_eq!(
            bar.percent(&row(&[red(15), red(0)])),
            Ok(Number::new(50.0).unwrap())
        );
        assert_eq!(
            bar.percent(&row(&[red(16), red(0)])),
            Ok(Number::new(0.0).unwrap())
        );
    }

    #[test]
    fn bar_fill_counts_a_pixel_midway_between_the_colors_as_empty() {
        let red = |red| Rgb {
            red,
            green: 0,
            blue: 0,
        };
        let bar = BarFill::new(red(10), red(0)).unwrap();
        assert_eq!(bar.percent(&row(&[red(5)])), Ok(Number::new(0.0).unwrap()));
    }

    #[test]
    fn a_bar_pixel_is_filled_empty_or_neither() {
        let bar = BarFill::new(GREEN, GRAY).unwrap();
        let pixel = |color: Rgb| [color.blue, color.green, color.red, 255];
        assert_eq!(bar.classify(pixel(GREEN)), BarPixel::Filled);
        assert_eq!(bar.classify(pixel(GRAY)), BarPixel::Empty);
        assert_eq!(bar.classify(pixel(WHITE)), BarPixel::Neither);
    }

    #[test]
    fn bar_fill_skips_a_dark_border() {
        let black = Rgb {
            red: 0,
            green: 0,
            blue: 0,
        };
        let bar = BarFill::new(GREEN, GRAY).unwrap();
        let image = row(&[black, black, GREEN, GREEN, GRAY, GRAY, black, black]);
        assert_eq!(bar.percent(&image), Ok(Number::new(50.0).unwrap()));
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
    fn a_color_is_as_far_as_its_farthest_channel() {
        let red = ColorMatch {
            color: Rgb {
                red: 200,
                green: 20,
                blue: 20,
            },
            tolerance: 10,
        };
        let color = |red, green, blue| Rgb { red, green, blue };
        assert_eq!(red.farthest_channel(color(197, 25, 20)), 5);
        assert_eq!(red.farthest_channel(color(200, 20, 13)), 7);
        assert_eq!(red.farthest_channel(color(209, 20, 20)), 9);
        // Exactly the tolerance is near; one more is not.
        assert!(red.near(color(210, 10, 30)));
        assert!(!red.near(color(210, 9, 30)));
    }

    #[test]
    fn the_average_color_is_each_channels_mean_rounded_down() {
        let dark = Rgb {
            red: 10,
            green: 0,
            blue: 255,
        };
        let light = Rgb {
            red: 13,
            green: 1,
            blue: 255,
        };
        assert_eq!(
            average(&row(&[dark, light, light])),
            Rgb {
                red: 12,
                green: 0,
                blue: 255
            }
        );
    }

    #[test]
    fn image_match_averages_the_difference_over_every_channel() {
        let picture = row(&[GRAY, GRAY]);
        let check = ImageMatch::new(picture, None, 4).unwrap();
        // One channel of one pixel off by 24 averages to 4 over 6 channels.
        let close = row(&[GRAY, Rgb { red: 79, ..GRAY }]);
        let far = row(&[GRAY, Rgb { red: 80, ..GRAY }]);
        assert_eq!(check.matches(&close), Ok(true));
        assert_eq!(check.matches(&far), Ok(false));
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

    fn state(name: &str) -> StateName {
        StateName::new(name).unwrap()
    }

    #[test]
    fn a_state_check_reads_the_closest_state_within_the_tolerance() {
        let paused = ImageMatch::new(row(&[GRAY, GRAY]), None, 10).unwrap();
        let live = ImageMatch::new(row(&[GREEN, GREEN]), None, 10).unwrap();
        let states = StateMatch::new(vec![(state("Paused"), paused), (state("Live"), live)]);
        let check = Check::State(states.clone());
        assert_eq!(
            check.read(&row(&[GRAY, GRAY])),
            Ok(Value::State(state("Paused")))
        );
        assert_eq!(
            check.read(&row(&[GREEN, Rgb { red: 20, ..GREEN }])),
            Ok(Value::State(state("Live")))
        );
        // Far from both: no state.
        assert_eq!(
            check.read(&row(&[WHITE, WHITE])),
            Ok(Value::State(StateName::none()))
        );
        assert_eq!(
            states.read(&solid(3, GRAY)),
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
    fn a_state_check_picks_the_closer_of_two_states_within_the_tolerance() {
        let near = Rgb { red: 70, ..GRAY };
        let dim = ImageMatch::new(row(&[GRAY, GRAY]), None, 20).unwrap();
        let bright = ImageMatch::new(row(&[near, near]), None, 20).unwrap();
        let states = StateMatch::new(vec![(state("Dim"), dim), (state("Bright"), bright)]);
        // Red 68 is 2 from Bright's 70 and 13 from Dim's 55. Both are within
        // 20 on average, and the closer one wins in either order.
        let probe = Rgb { red: 68, ..GRAY };
        assert_eq!(states.read(&row(&[probe, probe])), Ok(state("Bright")));
        let reversed = StateMatch::new(states.states.iter().rev().cloned().collect());
        assert_eq!(reversed.read(&row(&[probe, probe])), Ok(state("Bright")));
    }

    #[test]
    fn a_state_check_breaks_a_tie_for_the_earlier_state() {
        let picture = || ImageMatch::new(row(&[GRAY]), None, 5).unwrap();
        let states = StateMatch::new(vec![
            (state("First"), picture()),
            (state("Second"), picture()),
        ]);
        assert_eq!(states.read(&row(&[GRAY])), Ok(state("First")));
    }

    #[test]
    fn a_state_check_compares_states_by_their_average_difference() {
        // Masked to one pixel, "Small" is off by 6 over 3 channels: 2 each.
        // Unmasked, "Wide" is off by 9 over 6 channels: 1.5 each, so it is
        // closer though its total is larger.
        let picture = row(&[GRAY, GRAY]);
        let mask = Mask::new(picture.size(), vec![true, false]).unwrap();
        let small = ImageMatch::new(picture.clone(), Some(&mask), 5).unwrap();
        let wide = ImageMatch::new(picture, None, 5).unwrap();
        let image = row(&[
            Rgb {
                red: GRAY.red.wrapping_add(6),
                ..GRAY
            },
            Rgb {
                red: GRAY.red.wrapping_add(3),
                ..GRAY
            },
        ]);
        let states = StateMatch::new(vec![
            (state("Small"), small.clone()),
            (state("Wide"), wide.clone()),
        ]);
        assert_eq!(states.read(&image), Ok(state("Wide")));
        let reversed = StateMatch::new(vec![(state("Wide"), wide), (state("Small"), small)]);
        assert_eq!(reversed.read(&image), Ok(state("Wide")));
    }

    #[test]
    fn image_match_ignores_the_unused_byte() {
        let picture = solid(2, GRAY);
        let mut bgra = picture.as_bgra().to_vec();
        bgra[3] = 0;
        let other = Image::new(picture.size(), bgra).unwrap();
        let check = ImageMatch::new(picture, None, 0).unwrap();
        assert_eq!(check.matches(&other), Ok(true));
    }

    #[test]
    fn image_match_compares_only_the_pixels_its_mask_keeps() {
        let picture = row(&[WHITE, GREEN, GREEN, WHITE]);
        let size = picture.size();
        let mask = Mask::new(size, vec![false, true, true, false]).unwrap();
        let check = ImageMatch::new(picture.clone(), Some(&mask), 4).unwrap();
        // The background on both ends changed completely; the thing did not.
        assert_eq!(check.matches(&row(&[GRAY, GREEN, GREEN, GRAY])), Ok(true));
        // One kept channel off by 24 averages to 4 over the 6 kept channels.
        let close = row(&[GRAY, GREEN, Rgb { red: 64, ..GREEN }, GRAY]);
        let far = row(&[WHITE, GREEN, Rgb { red: 65, ..GREEN }, WHITE]);
        assert_eq!(check.matches(&close), Ok(true));
        assert_eq!(check.matches(&far), Ok(false));
        let unmasked = ImageMatch::new(picture.clone(), None, 4).unwrap();
        assert_eq!(
            unmasked.matches(&row(&[GRAY, GREEN, GREEN, GRAY])),
            Ok(false)
        );
        assert_eq!(check.picture(), &picture);
    }

    #[test]
    fn a_resized_match_keeps_its_mask_and_tolerance() {
        let picture = row(&[WHITE, GREEN, GREEN, WHITE]);
        let mask = Mask::new(picture.size(), vec![false, true, true, false]).unwrap();
        let check = ImageMatch::new(picture, Some(&mask), 4).unwrap();
        let wide = Size {
            width: 8,
            height: 1,
        };
        let resized = check.resized(wide).expect("keeps pixels");
        assert_eq!(resized.picture().size(), wide);
        // The background left out at both ends stays out at twice the size.
        let changed = row(&[GRAY, GRAY, GREEN, GREEN, GREEN, GREEN, GRAY, GRAY]);
        assert_eq!(resized.matches(&changed), Ok(true));
        let off = Rgb { red: 64, ..GREEN };
        let close = row(&[GRAY, GRAY, GREEN, GREEN, GREEN, off, GRAY, GRAY]);
        assert_eq!(
            resized.matches(&close),
            Ok(true),
            "24 over 12 channels is 2"
        );
        let thing = row(&[GRAY, GRAY, GRAY, GREEN, GREEN, GREEN, GRAY, GRAY]);
        assert_eq!(resized.matches(&thing), Ok(false));
        // A one pixel region keeps only half the thing it would cover.
        let one = Size {
            width: 1,
            height: 1,
        };
        let shrunk = check.resized(one).expect("half is enough");
        assert_eq!(shrunk.matches(&solid(1, GREEN)), Ok(true));
        let edge = ImageMatch::new(
            row(&[WHITE, WHITE, WHITE, GREEN]),
            Some(
                &Mask::new(
                    Size {
                        width: 4,
                        height: 1,
                    },
                    vec![false, false, false, true],
                )
                .unwrap(),
            ),
            0,
        )
        .unwrap();
        assert!(
            edge.resized(one).is_none(),
            "a quarter of a pixel keeps none"
        );
        let states = StateMatch::new(vec![(StateName::new("Shown").unwrap(), check)]);
        let resized = states.resized(wide).expect("keeps pixels");
        assert_eq!(resized.read(&changed), Ok(StateName::new("Shown").unwrap()));
        let none = StateMatch::new(vec![(StateName::new("Edge").unwrap(), edge)]);
        assert!(none.resized(one).is_none());
    }

    #[test]
    fn image_match_refuses_a_mask_of_another_size() {
        let mask = Mask::new(
            Size {
                width: 3,
                height: 1,
            },
            vec![true; 3],
        )
        .unwrap();
        assert_eq!(
            ImageMatch::new(solid(2, GRAY), Some(&mask), 0),
            Err(MaskError::SizeMismatch {
                image: Size {
                    width: 2,
                    height: 1
                },
                mask: mask.size(),
            })
        );
    }
}
