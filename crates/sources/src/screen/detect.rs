//! Proposing a bar fill check's two colors from screenshots of its region.
//!
//! [`BarFill`] counts the pixels near a filled color and near an empty color,
//! wherever they are, so it reads any bar that shows two flat colors.
//! [`detect_bar`] finds those two colors instead of having the user click
//! them. It looks for one color from the start of the region's long side,
//! the left of a wide region or the bottom of a tall one, and another color
//! after it, and takes the color at the start as the filled one: the reader
//! has no direction, so that is a convention the user can turn around.
//!
//! One screenshot shows a layout. Several screenshots of one window size
//! show more: the pixels that change between them are the bar's moving
//! part, which no border, label, or background shares. A proposal is checked
//! against the reader before it is made: [`BarFill::percent`] has to read the
//! screenshot as full as the layout looks.

use std::cmp::Ordering;
use std::error::Error;
use std::fmt;
use std::iter;

use super::Image;
use super::check::{BarFill, BarPart, distance};
use crate::system::Rgb;

/// The shortest long side a bar is looked for on, and the fewest pixels
/// that have to change between screenshots to show a bar moving.
const MIN_LENGTH: usize = 12;

/// The squared distance within which colors along a bar are one color.
const SAME: u32 = 16 * 16;

/// The squared distance a filled and an empty color need between them, and
/// that a pixel has to move by to count as changed.
const APART: u32 = 32 * 32;

/// The most different colors gathered from a region. A bar shows two and a
/// few more for its border and label; more means the region is no bar.
const MAX_CLASSES: usize = 16;

/// The share of a region's long side, or of its changed pixels, that the
/// bar's two colors have to cover, in percent.
const COVER: usize = 85;

/// The share of the changed pixels below which the two colors match only
/// part of the bar, in percent.
const CLEAN: usize = 95;

/// How far, in points of fill, the reader may be from the fill the layout
/// shows.
const AGREE: f64 = 5.0;

/// A bar's two colors, proposed from screenshots of its region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarProposal {
    /// The bar the colors make.
    pub bar: BarFill,
    /// What the colors were found from.
    pub basis: Basis,
}

/// What a proposal's colors were found from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    /// The layout of the first screenshot alone, which a picture in two
    /// colors shares with a bar.
    Layout,
    /// The pixels that change between the screenshots.
    Changes {
        /// Whether the colors leave out more than a few of the changed
        /// pixels, as on a bar filled with a gradient.
        rough: bool,
    },
}

/// Why no bar is proposed for a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoBar {
    /// The region's long side is shorter than 12 pixels.
    Tiny,
    /// The region shows one color, as a full or an empty bar does.
    OneColor,
    /// The region's two main colors are too close to tell apart.
    AlikeColors,
    /// The region is not one color and then another along its long side.
    NotABar,
    /// The two colors read the region other than its layout looks, as when
    /// something else in the region has one of them.
    Misread,
}

impl fmt::Display for NoBar {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Tiny => "the region is too short to hold a bar",
            Self::OneColor => "the region shows one color",
            Self::AlikeColors => "the region's two main colors are too close to tell apart",
            Self::NotABar => "the region is not one color and then another along its long side",
            Self::Misread => "the two colors read the region other than its layout looks",
        })
    }
}

impl Error for NoBar {}

/// The filled and the empty color of the bar in a region, from `first`, the
/// region on one screenshot, and `others`, the same region on further
/// screenshots.
///
/// The colors come from the pixels that change between the screenshots
/// when at least 12 change, between two colors, and one screenshot shows
/// both. Otherwise they come from the layout of `first` alone. An image in
/// `others` of another size than `first` is left out.
///
/// # Errors
///
/// Returns the [`NoBar`] that says why `first` shows no bar the reader can
/// read.
pub fn detect_bar(first: &Image, others: &[Image]) -> Result<BarProposal, NoBar> {
    let strip = Strip::new(first);
    if strip.length() < MIN_LENGTH {
        return Err(NoBar::Tiny);
    }
    let others: Vec<&Image> = others
        .iter()
        .filter(|other| other.size() == first.size())
        .collect();
    if let Some(proposal) = from_changes(first, &others) {
        return Ok(proposal);
    }
    let all = vec![true; strip.lanes()];
    let bar = from_layout(strip, &all).or_else(|reason| {
        // Background beside a loosely drawn bar outvotes it in each column,
        // so look again at only the lanes that show more than one color.
        let busy: Vec<bool> = (0..all.len()).map(|lane| !strip.plain(lane)).collect();
        from_layout(strip, &busy).map_err(|again| match again {
            // Those lanes do lay out as a bar, which says more than why
            // the whole region does not.
            NoBar::Misread => again,
            _ => reason,
        })
    })?;
    Ok(BarProposal {
        bar,
        basis: Basis::Layout,
    })
}

/// The bar whose two colors the pixels that change between `first` and
/// `others` show, or `None` when they show no bar.
fn from_changes(first: &Image, others: &[&Image]) -> Option<BarProposal> {
    let mut moving = vec![false; first.pixels().len()];
    for other in others {
        let pairs = first.pixels().iter().zip(other.pixels());
        for (moved, (one, two)) in moving.iter_mut().zip(pairs) {
            *moved |= distance(color(*one), color(*two)) >= APART;
        }
    }
    let frames = || iter::once(first).chain(others.iter().copied());
    let samples = || {
        frames().flat_map(|frame| {
            let moved = frame.pixels().iter().zip(&moving);
            let moved = moved.filter(|&(_, &moved)| moved);
            moved.map(|(pixel, _)| color(*pixel))
        })
    };
    if moving.iter().filter(|&&moved| moved).count() < MIN_LENGTH {
        return None;
    }
    let pair = two_colors(&classes(samples()), 1).ok()?;
    let total = samples().count();
    let matched = samples()
        .filter(|&sample| pair.part(sample).is_some())
        .count();
    if !covers(matched, total, COVER) {
        return None;
    }
    // The bar is in the lanes that hold a changed pixel, whatever is drawn
    // beside it.
    let strip = Strip::new(first);
    let kept: Vec<bool> = (0..strip.lanes())
        .map(|lane| {
            let mut indexes = (0..strip.length()).filter_map(|column| strip.index(column, lane));
            indexes.any(|index| moving.get(index).is_some_and(|&moved| moved))
        })
        .collect();
    // The first screenshot that shows both parts says which is the filled one.
    let bar =
        frames().find_map(|frame| arranged(frame, &Strip::new(frame).columns(&kept), pair).ok())?;
    Some(BarProposal {
        bar,
        basis: Basis::Changes {
            rough: !covers(matched, total, CLEAN),
        },
    })
}

/// The bar the layout of `strip` shows over its `kept` lanes.
fn from_layout(strip: Strip<'_>, kept: &[bool]) -> Result<BarFill, NoBar> {
    let columns = strip.columns(kept);
    let pair = two_colors(&classes(columns.iter().copied()), share(columns.len()))?;
    arranged(strip.image, &columns, pair)
}

/// A bar of the most common color of `classes` and the most common one
/// apart from it, among those with at least `least` colors. Which of the two
/// is the filled one is not settled yet.
fn two_colors(classes: &[Class], least: usize) -> Result<BarFill, NoBar> {
    let mut most_first = classes.iter();
    let most = most_first.next().ok_or(NoBar::NotABar)?;
    let total: usize = classes.iter().map(|class| class.count).sum();
    let mut rest = most_first.filter(|class| class.count >= least).peekable();
    if rest.peek().is_none() {
        return Err(if covers(most.count, total, COVER) {
            NoBar::OneColor
        } else {
            NoBar::NotABar
        });
    }
    rest.find(|class| distance(class.mean(), most.mean()) >= APART)
        .and_then(|other| BarFill::new(most.mean(), other.mean()).ok())
        .ok_or(NoBar::AlikeColors)
}

/// `pair` with its filled color at the start of `columns`, when they lay
/// out as one of the pair's colors and then the other, and the reader reads
/// `image`, the region they are the columns of, as full as that layout
/// looks.
fn arranged(image: &Image, columns: &[Rgb], pair: BarFill) -> Result<BarFill, NoBar> {
    let parts: Vec<BarPart> = columns
        .iter()
        .filter_map(|&column| belongs(pair, column))
        .collect();
    if !covers(parts.len(), columns.len(), COVER) {
        return Err(NoBar::NotABar);
    }
    let filled = parts
        .iter()
        .filter(|&&part| part == BarPart::Filled)
        .count();
    let empty = parts.len().saturating_sub(filled);
    let least = share(columns.len());
    if filled < least || empty < least {
        return Err(NoBar::OneColor);
    }
    // How many columns are out of place when the filled ones come first,
    // and when the empty ones do.
    let out_of_place = |start: BarPart, count: usize| {
        let first = parts.iter().take(count);
        first.filter(|&&part| part != start).count()
    };
    let filled_first = out_of_place(BarPart::Filled, filled);
    let empty_first = out_of_place(BarPart::Empty, empty);
    let (bar, start, misplaced) = match filled_first.cmp(&empty_first) {
        Ordering::Less => (pair, filled, filled_first),
        Ordering::Greater => (pair.turned(), empty, empty_first),
        // Neither order fits better than the other.
        Ordering::Equal => return Err(NoBar::NotABar),
    };
    if misplaced >= least {
        return Err(NoBar::NotABar);
    }
    let looks = start as f64 * 100.0 / parts.len() as f64;
    let agrees = bar
        .percent(image)
        .is_ok_and(|reads| (reads.raw() - looks).abs() <= AGREE);
    if agrees { Ok(bar) } else { Err(NoBar::Misread) }
}

/// The part of `bar` a column of the color `color` belongs to: the part the
/// reader counts it as, when it is within a quarter of the distance between
/// the bar's colors of that part's color. That is half as far as the reader
/// allows, so a gradient that runs from one color to the other is not taken
/// for two parts.
fn belongs(bar: BarFill, color: Rgb) -> Option<BarPart> {
    let part = bar.part(color)?;
    let own = match part {
        BarPart::Filled => bar.filled(),
        BarPart::Empty => bar.empty(),
    };
    // Squared distances, so a quarter of the distance is a sixteenth.
    let near = distance(color, own).saturating_mul(16) <= distance(bar.filled(), bar.empty());
    near.then_some(part)
}

/// The fewest columns of a region `length` long that make a part of a bar:
/// a twentieth of it, and at least 3.
fn share(length: usize) -> usize {
    length.checked_div(20).unwrap_or_default().max(3)
}

/// Whether `part` is at least `percent` percent of `whole`.
fn covers(part: usize, whole: usize, percent: usize) -> bool {
    part.saturating_mul(100) >= whole.saturating_mul(percent)
}

fn color([blue, green, red, _]: [u8; 4]) -> Rgb {
    Rgb { red, green, blue }
}

/// A region's pixels along its long side: columns count from the left of a
/// wide region or from the bottom of a tall one, and lanes run across them.
#[derive(Clone, Copy)]
struct Strip<'a> {
    image: &'a Image,
    tall: bool,
}

impl<'a> Strip<'a> {
    fn new(image: &'a Image) -> Self {
        let size = image.size();
        Self {
            image,
            tall: size.height > size.width,
        }
    }

    /// How many columns the strip has.
    fn length(self) -> usize {
        let size = self.image.size();
        let length = if self.tall { size.height } else { size.width };
        usize::try_from(length).unwrap_or(usize::MAX)
    }

    /// How many lanes the strip has.
    fn lanes(self) -> usize {
        let size = self.image.size();
        let lanes = if self.tall { size.width } else { size.height };
        usize::try_from(lanes).unwrap_or(usize::MAX)
    }

    /// Where the pixel at column `column` of lane `lane`, both inside the
    /// strip, is in the image's pixels.
    fn index(self, column: usize, lane: usize) -> Option<usize> {
        let width = if self.tall {
            self.lanes()
        } else {
            self.length()
        };
        let (x, y) = if self.tall {
            (lane, self.length().checked_sub(1)?.checked_sub(column)?)
        } else {
            (column, lane)
        };
        y.checked_mul(width)?.checked_add(x)
    }

    /// The color at column `column` of lane `lane`.
    fn at(self, column: usize, lane: usize) -> Option<Rgb> {
        let pixel = self.image.pixels().get(self.index(column, lane)?)?;
        Some(color(*pixel))
    }

    /// Each column's median color over the `kept` lanes, from the start.
    fn columns(self, kept: &[bool]) -> Vec<Rgb> {
        (0..self.length())
            .filter_map(|column| {
                let lanes = kept.iter().enumerate().filter(|&(_, &kept)| kept);
                median(lanes.filter_map(|(lane, _)| self.at(column, lane)))
            })
            .collect()
    }

    /// Whether lane `lane` shows one color, but for fewer pixels than make
    /// a part of a bar.
    fn plain(self, lane: usize) -> bool {
        let colors = || (0..self.length()).filter_map(move |column| self.at(column, lane));
        median(colors()).is_none_or(|middle| {
            let off = colors().filter(|&color| distance(color, middle) >= APART);
            off.count() < share(self.length())
        })
    }
}

/// The median of each channel of `colors`, the lower of the middle two for
/// an even number, or `None` for no colors.
fn median(colors: impl Iterator<Item = Rgb>) -> Option<Rgb> {
    let mut counts = [[0_usize; 256]; 3];
    let mut total = 0_usize;
    for color in colors {
        for (channel, value) in counts.iter_mut().zip([color.red, color.green, color.blue]) {
            if let Some(count) = channel.get_mut(usize::from(value)) {
                *count = count.saturating_add(1);
            }
        }
        total = total.saturating_add(1);
    }
    if total == 0 {
        return None;
    }
    let [red, green, blue] = counts.map(|channel| {
        let mut seen = 0_usize;
        let middle = (0..=u8::MAX).zip(channel).find(|&(_, count)| {
            seen = seen.saturating_add(count);
            seen.saturating_mul(2) >= total
        });
        middle.map_or(u8::MAX, |(value, _)| value)
    });
    Some(Rgb { red, green, blue })
}

/// Colors that are alike: those within [`SAME`] of the first of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Class {
    first: Rgb,
    count: usize,
    /// The sums of the colors' red, green, and blue.
    sums: [u64; 3],
}

impl Class {
    /// The average of the class's colors.
    fn mean(&self) -> Rgb {
        let count = u64::try_from(self.count).unwrap_or(u64::MAX);
        let [red, green, blue] = self.sums.map(|sum| {
            let mean = sum.checked_div(count).unwrap_or_default();
            u8::try_from(mean).unwrap_or(u8::MAX)
        });
        Rgb { red, green, blue }
    }
}

/// `colors` gathered into classes, the one with the most colors first and
/// equals in the order they were first seen. Once there are
/// [`MAX_CLASSES`], a color like none of them is left out.
fn classes(colors: impl Iterator<Item = Rgb>) -> Vec<Class> {
    let mut classes: Vec<Class> = Vec::new();
    for color in colors {
        let like = classes
            .iter_mut()
            .find(|class| distance(class.first, color) <= SAME);
        if let Some(class) = like {
            class.count = class.count.saturating_add(1);
            let channels = [color.red, color.green, color.blue];
            for (sum, channel) in class.sums.iter_mut().zip(channels) {
                *sum = sum.saturating_add(u64::from(channel));
            }
        } else if classes.len() < MAX_CLASSES {
            classes.push(Class {
                first: color,
                count: 1,
                sums: [color.red, color.green, color.blue].map(u64::from),
            });
        }
    }
    classes.sort_by_key(|class| std::cmp::Reverse(class.count));
    classes
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::super::Size;
    use super::*;

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
    const BLACK: Rgb = Rgb {
        red: 0,
        green: 0,
        blue: 0,
    };
    const BLUE: Rgb = Rgb {
        red: 0,
        green: 0,
        blue: 255,
    };

    fn red(red: u8) -> Rgb {
        Rgb {
            red,
            green: 0,
            blue: 0,
        }
    }

    /// A picture whose pixel at (`x`, `y`) has the color `paint(x, y)`.
    fn picture(width: u32, height: u32, paint: impl Fn(u32, u32) -> Rgb) -> Image {
        let mut bgra = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let color = paint(x, y);
                bgra.extend([color.blue, color.green, color.red, 255]);
            }
        }
        Image::new(Size { width, height }, bgra).unwrap()
    }

    /// A green bar on gray, `width` by `height`, with its first `filled`
    /// columns filled.
    fn bar(width: u32, height: u32, filled: u32) -> Image {
        picture(width, height, |x, _| if x < filled { GREEN } else { GRAY })
    }

    fn found(filled: Rgb, empty: Rgb, basis: Basis) -> BarProposal {
        BarProposal {
            bar: BarFill::new(filled, empty).unwrap(),
            basis,
        }
    }

    fn reads(image: &Image, others: &[Image]) -> f64 {
        let proposal = detect_bar(image, others).unwrap();
        proposal.bar.percent(image).unwrap().raw()
    }

    #[test]
    fn a_bar_is_found_at_each_fill_that_shows_both_parts() {
        for (filled, percent) in [(5, 5.0), (37, 37.0), (50, 50.0), (95, 95.0)] {
            let image = bar(100, 8, filled);
            assert_eq!(
                detect_bar(&image, &[]),
                Ok(found(GREEN, GRAY, Basis::Layout)),
                "{filled} columns filled"
            );
            assert!((reads(&image, &[]) - percent).abs() < 1e-9);
        }
        // A part under a twentieth of the bar could as well be its border.
        assert_eq!(detect_bar(&bar(100, 8, 4), &[]), Err(NoBar::OneColor));
        assert_eq!(detect_bar(&bar(100, 8, 96), &[]), Err(NoBar::OneColor));
    }

    #[test]
    fn a_full_or_an_empty_bar_is_one_color() {
        assert_eq!(detect_bar(&bar(100, 8, 100), &[]), Err(NoBar::OneColor));
        assert_eq!(detect_bar(&bar(100, 8, 0), &[]), Err(NoBar::OneColor));
        // A border and a label on a full bar do not make a second part.
        let decorated = picture(102, 10, |x, y| {
            let border = !(1..101).contains(&x) || !(1..9).contains(&y);
            let label = (40..60).contains(&x) && (4..6).contains(&y) && x.is_multiple_of(2);
            if border || label { WHITE } else { GREEN }
        });
        assert_eq!(detect_bar(&decorated, &[]), Err(NoBar::OneColor));
    }

    #[test]
    fn the_color_at_the_start_of_the_long_side_is_the_filled_one() {
        // A wide region starts at the left.
        let drained = picture(100, 8, |x, _| if x < 30 { GRAY } else { GREEN });
        assert_eq!(
            detect_bar(&drained, &[]),
            Ok(found(GRAY, GREEN, Basis::Layout))
        );
        // A tall region starts at the bottom.
        let tall = picture(8, 100, |_, y| if y >= 70 { GREEN } else { GRAY });
        assert_eq!(
            detect_bar(&tall, &[]),
            Ok(found(GREEN, GRAY, Basis::Layout))
        );
        assert!((reads(&tall, &[]) - 30.0).abs() < 1e-9);
        let hanging = picture(8, 100, |_, y| if y < 30 { GREEN } else { GRAY });
        assert_eq!(
            detect_bar(&hanging, &[]),
            Ok(found(GRAY, GREEN, Basis::Layout))
        );
        // A square region counts as wide.
        let square = picture(12, 12, |x, _| if x < 6 { GREEN } else { GRAY });
        assert_eq!(
            detect_bar(&square, &[]),
            Ok(found(GREEN, GRAY, Basis::Layout))
        );
    }

    #[test]
    fn a_border_an_anti_aliased_edge_and_a_label_are_left_out() {
        let soft = Rgb {
            red: 45,
            green: 160,
            blue: 63,
        };
        let image = picture(102, 10, |x, y| {
            let border = !(1..101).contains(&x) || !(1..9).contains(&y);
            let label = (30..70).contains(&x) && (4..6).contains(&y) && x.is_multiple_of(3);
            match x {
                _ if border || label => WHITE,
                0..41 => GREEN,
                41 => soft,
                _ => GRAY,
            }
        });
        assert_eq!(
            detect_bar(&image, &[]),
            Ok(found(GREEN, GRAY, Basis::Layout))
        );
        // 40 of the bar's 100 columns are filled, and the soft one counts
        // with them.
        assert!((reads(&image, &[]) - 41.0).abs() < 1.0);
        // A border as thick as the bar outvotes it in each column, until
        // the lanes that are all border are left out.
        let framed = picture(100, 10, |x, y| match y {
            3..7 if x < 40 => GREEN,
            3..7 => GRAY,
            _ => WHITE,
        });
        assert_eq!(
            detect_bar(&framed, &[]),
            Ok(found(GREEN, GRAY, Basis::Layout))
        );
    }

    #[test]
    fn a_fill_shaded_across_the_bar_is_one_color() {
        let shade = |y: u32| Rgb {
            green: 170_u8.saturating_add(u8::try_from(y).unwrap().saturating_mul(8)),
            ..GREEN
        };
        let image = picture(100, 10, |x, y| if x < 30 { shade(y) } else { GRAY });
        // The lower of the two middle lanes' shades.
        assert_eq!(
            detect_bar(&image, &[]),
            Ok(found(shade(4), GRAY, Basis::Layout))
        );
        assert!((reads(&image, &[]) - 30.0).abs() < 1e-9);
    }

    #[test]
    fn a_gradient_along_the_region_is_not_two_parts() {
        let image = picture(100, 8, |x, _| Rgb {
            red: 220,
            green: u8::try_from(x).unwrap().saturating_mul(2),
            blue: 30,
        });
        assert_eq!(detect_bar(&image, &[]), Err(NoBar::NotABar));
    }

    #[test]
    fn colors_closer_than_the_reader_can_split_are_not_proposed() {
        let near = |by: u8| picture(100, 8, move |x, _| if x < 50 { red(100) } else { red(by) });
        assert_eq!(
            detect_bar(&near(132), &[]),
            Ok(found(red(100), red(132), Basis::Layout))
        );
        assert_eq!(detect_bar(&near(131), &[]), Err(NoBar::AlikeColors));
        // A shade too near the main color is passed over for a color far
        // enough apart. Within a quarter of the way to it, here up to red
        // 130, the shade belongs to the main color's part; further off it
        // belongs to neither.
        let shaded = |shade: u8| {
            picture(100, 8, move |x, _| match x {
                0..40 => red(100),
                40..70 => red(shade),
                _ => red(220),
            })
        };
        assert_eq!(
            detect_bar(&shaded(130), &[]),
            Ok(found(red(100), red(220), Basis::Layout))
        );
        assert!((reads(&shaded(130), &[]) - 70.0).abs() < 1e-9);
        assert_eq!(detect_bar(&shaded(131), &[]), Err(NoBar::NotABar));
    }

    #[test]
    fn a_picture_that_is_not_one_color_then_another_is_not_a_bar() {
        let stripes = picture(100, 8, |x, _| {
            if (x / 6).is_multiple_of(2) {
                GREEN
            } else {
                GRAY
            }
        });
        assert_eq!(detect_bar(&stripes, &[]), Err(NoBar::NotABar));
        let island = picture(
            100,
            8,
            |x, _| if (40..60).contains(&x) { GREEN } else { GRAY },
        );
        assert_eq!(detect_bar(&island, &[]), Err(NoBar::NotABar));
        // Too many colors to gather: the seventeenth is left out, so the
        // main color covers less than it needs to.
        let busy = |others: u32| {
            picture(100, 8, move |x, _| {
                if x < others {
                    Rgb {
                        red: 255,
                        green: 0,
                        blue: u8::try_from(x).unwrap().saturating_mul(17),
                    }
                } else {
                    GREEN
                }
            })
        };
        assert_eq!(detect_bar(&busy(15), &[]), Err(NoBar::OneColor));
        assert_eq!(detect_bar(&busy(16), &[]), Err(NoBar::NotABar));
    }

    #[test]
    fn a_few_columns_out_of_place_are_forgiven() {
        let with = |stray: u32| {
            picture(100, 8, move |x, _| {
                if x < 50 && !(10..10_u32.saturating_add(stray)).contains(&x) {
                    GREEN
                } else {
                    GRAY
                }
            })
        };
        assert_eq!(
            detect_bar(&with(4), &[]),
            Ok(found(GREEN, GRAY, Basis::Layout))
        );
        assert_eq!(detect_bar(&with(5), &[]), Err(NoBar::NotABar));
        // Neither order fits better: two filled columns at each end.
        let ends = picture(100, 8, |x, _| match x {
            0 | 1 | 50 | 98 | 99 => GREEN,
            _ => GRAY,
        });
        assert_eq!(detect_bar(&ends, &[]), Err(NoBar::NotABar));
    }

    #[test]
    fn a_region_shorter_than_twelve_pixels_is_too_small() {
        assert_eq!(detect_bar(&bar(11, 4, 5), &[]), Err(NoBar::Tiny));
        assert_eq!(
            detect_bar(&bar(12, 4, 6), &[]),
            Ok(found(GREEN, GRAY, Basis::Layout))
        );
        // The long side counts, whichever it is.
        let tall = picture(4, 11, |_, y| if y < 5 { GRAY } else { GREEN });
        assert_eq!(detect_bar(&tall, &[]), Err(NoBar::Tiny));
    }

    /// A bar of six lanes in a region of thirty, on `background`.
    fn loose(background: Rgb, filled: u32) -> Image {
        picture(100, 30, move |x, y| match (x, y) {
            (5..95, 12..18) if x < filled.saturating_add(5) => GREEN,
            (5..95, 12..18) => GRAY,
            _ => background,
        })
    }

    #[test]
    fn a_bar_drawn_loosely_on_a_plain_background_is_found() {
        assert_eq!(
            detect_bar(&loose(BLACK, 45), &[]),
            Ok(found(GREEN, GRAY, Basis::Layout))
        );
        assert!((reads(&loose(BLACK, 45), &[]) - 50.0).abs() < 1e-9);
        // Nothing to find in the lanes that are not plain either.
        assert_eq!(detect_bar(&loose(BLACK, 0), &[]), Err(NoBar::OneColor));
    }

    #[test]
    fn colors_that_also_show_beside_the_bar_are_not_proposed() {
        // The background has the empty color, so the reader would count it.
        let image = picture(100, 30, |x, y| {
            if x < 45 && (12..18).contains(&y) {
                GREEN
            } else {
                GRAY
            }
        });
        assert_eq!(detect_bar(&image, &[]), Err(NoBar::Misread));
        // One lane in ten is 5 points off, which the reader is allowed.
        let lanes = |extra: u32| {
            picture(
                100,
                10,
                move |x, y| if x < 50 && y >= extra { GREEN } else { GRAY },
            )
        };
        assert_eq!(
            detect_bar(&lanes(1), &[]),
            Ok(found(GREEN, GRAY, Basis::Layout))
        );
        assert_eq!(detect_bar(&lanes(2), &[]), Err(NoBar::Misread));
    }

    #[test]
    fn pixels_that_change_between_screenshots_give_the_colors() {
        let exact = Basis::Changes { rough: false };
        assert_eq!(
            detect_bar(&bar(100, 8, 30), &[bar(100, 8, 70)]),
            Ok(found(GREEN, GRAY, exact))
        );
        assert_eq!(
            detect_bar(&bar(100, 8, 70), &[bar(100, 8, 30)]),
            Ok(found(GREEN, GRAY, exact))
        );
        // A pixel that differs on two other screenshots changed all the same.
        assert_eq!(
            detect_bar(&bar(100, 8, 30), &[bar(100, 8, 70), bar(100, 8, 70)]),
            Ok(found(GREEN, GRAY, exact))
        );
        // The same fill everywhere shows no change, so the layout decides.
        assert_eq!(
            detect_bar(&bar(100, 8, 30), &[bar(100, 8, 30)]),
            Ok(found(GREEN, GRAY, Basis::Layout))
        );
    }

    #[test]
    fn at_least_twelve_pixels_have_to_change() {
        let exact = Basis::Changes { rough: false };
        assert_eq!(
            detect_bar(&bar(100, 1, 40), &[bar(100, 1, 52)]),
            Ok(found(GREEN, GRAY, exact))
        );
        assert_eq!(
            detect_bar(&bar(100, 1, 40), &[bar(100, 1, 51)]),
            Ok(found(GREEN, GRAY, Basis::Layout))
        );
        // A pixel changes when it moves by the distance two colors need.
        let dim = |filled: u32, to: u8| {
            picture(
                100,
                8,
                move |x, _| if x < filled { red(to) } else { red(0) },
            )
        };
        assert_eq!(
            detect_bar(&dim(30, 32), &[dim(70, 32)]),
            Ok(found(red(32), red(0), exact))
        );
        assert_eq!(
            detect_bar(&dim(30, 31), &[dim(70, 31)]),
            Err(NoBar::AlikeColors)
        );
    }

    #[test]
    fn changes_find_a_bar_in_front_of_a_busy_background() {
        let over = |filled: u32| {
            picture(100, 20, move |x, y| match y {
                8..12 if x < filled => GREEN,
                8..12 => GRAY,
                _ if x.is_multiple_of(2) => BLUE,
                _ => WHITE,
            })
        };
        assert_eq!(detect_bar(&over(30), &[]), Err(NoBar::NotABar));
        assert_eq!(
            detect_bar(&over(30), &[over(70)]),
            Ok(found(GREEN, GRAY, Basis::Changes { rough: false }))
        );
    }

    #[test]
    fn changes_need_a_screenshot_that_shows_both_parts() {
        assert_eq!(
            detect_bar(&bar(100, 8, 100), &[bar(100, 8, 0)]),
            Err(NoBar::OneColor)
        );
        // A part of a twentieth is enough, as on one screenshot.
        let exact = Basis::Changes { rough: false };
        assert_eq!(
            detect_bar(&bar(100, 8, 5), &[bar(100, 8, 100)]),
            Ok(found(GREEN, GRAY, exact))
        );
        assert_eq!(
            detect_bar(&bar(100, 8, 95), &[bar(100, 8, 0)]),
            Ok(found(GREEN, GRAY, exact))
        );
        assert_eq!(
            detect_bar(&bar(100, 8, 2), &[bar(100, 8, 100)]),
            Err(NoBar::OneColor),
            "two columns are too few, though the rest of the bar is a part"
        );
        // The same when the small part has the color most changed pixels do.
        assert_eq!(
            detect_bar(&bar(100, 8, 5), &[bar(100, 8, 100), bar(100, 8, 100)]),
            Ok(found(GREEN, GRAY, exact))
        );
        // The second screenshot shows both parts when the first does not.
        assert_eq!(
            detect_bar(&bar(100, 8, 100), &[bar(100, 8, 40)]),
            Ok(found(GREEN, GRAY, exact))
        );
    }

    /// A bar whose changing part also holds `lanes` lanes of something else
    /// that changes, white on one screenshot and black on the other.
    fn flickering(filled: u32, lanes: u32, flicker: Rgb) -> Image {
        picture(100, 20, move |x, y| match x {
            20..80 if y < lanes => flicker,
            _ if x < filled => GREEN,
            _ => GRAY,
        })
    }

    #[test]
    fn changes_in_more_than_two_colors_are_rough_or_no_bar() {
        let pair = |lanes| [flickering(20, lanes, WHITE), flickering(80, lanes, BLACK)];
        let [first, second] = pair(1);
        assert_eq!(
            detect_bar(&first, &[second]),
            Ok(found(GREEN, GRAY, Basis::Changes { rough: false })),
            "a twentieth of the changed pixels is neither color"
        );
        let [first, second] = pair(2);
        assert_eq!(
            detect_bar(&first, &[second]),
            Ok(found(GREEN, GRAY, Basis::Changes { rough: true })),
            "a tenth"
        );
        let [first, second] = pair(4);
        assert_eq!(
            detect_bar(&first, &[second]),
            Ok(found(GREEN, GRAY, Basis::Layout)),
            "a fifth is too much to call the rest a bar's two colors"
        );
    }

    #[test]
    fn a_screenshot_of_another_size_is_left_out() {
        let other = picture(50, 8, |_, _| WHITE);
        assert_eq!(
            detect_bar(&bar(100, 8, 30), std::slice::from_ref(&other)),
            Ok(found(GREEN, GRAY, Basis::Layout))
        );
        assert_eq!(
            detect_bar(&bar(100, 8, 30), &[other, bar(100, 8, 70)]),
            Ok(found(GREEN, GRAY, Basis::Changes { rough: false }))
        );
    }

    #[test]
    fn the_median_is_the_lower_middle_of_each_channel() {
        assert_eq!(median(iter::empty()), None);
        let colors = [red(4), red(1), red(3), red(2)];
        assert_eq!(median(colors.into_iter()), Some(red(2)));
        let mixed = [GREEN, GRAY, WHITE];
        assert_eq!(
            median(mixed.into_iter()),
            Some(Rgb {
                red: 55,
                green: 210,
                blue: 70
            })
        );
    }

    #[test]
    fn classes_gather_colors_within_sixteen_of_their_first() {
        let colors = [red(100), red(200), red(116), red(117), red(210)];
        let found = classes(colors.into_iter());
        assert_eq!(
            found
                .iter()
                .map(|class| (class.first, class.count))
                .collect::<Vec<_>>(),
            [(red(100), 2), (red(200), 2), (red(117), 1)],
            "the most colors first, and equals as first seen"
        );
        assert_eq!(found.first().map(Class::mean), Some(red(108)));
        let many = (0..20_u8).map(|step| Rgb {
            red: (step % 10).saturating_mul(25),
            green: if step < 10 { 0 } else { 200 },
            blue: 0,
        });
        assert_eq!(classes(many).len(), MAX_CLASSES);
    }

    #[test]
    fn a_part_of_a_bar_is_a_twentieth_and_at_least_three_columns() {
        assert_eq!(share(100), 5);
        assert_eq!(share(59), 3);
        assert_eq!(share(12), 3);
        assert!(covers(85, 100, COVER));
        assert!(!covers(84, 100, COVER));
    }

    #[test]
    fn a_column_belongs_to_a_part_within_a_quarter_of_the_colors_distance() {
        let bar = BarFill::new(red(0), red(40)).unwrap();
        assert_eq!(belongs(bar, red(10)), Some(BarPart::Filled));
        assert_eq!(belongs(bar, red(11)), None);
        assert_eq!(belongs(bar, red(30)), Some(BarPart::Empty));
        assert_eq!(belongs(bar, red(29)), None);
    }

    #[test]
    fn a_lane_is_plain_until_a_part_of_it_is_another_color() {
        let image = picture(100, 4, |x, y| match (x, y) {
            (0..4, 0) | (0..5, 1) => GREEN,
            (0..5, 2) => Rgb { red: 86, ..GRAY },
            (0..5, 3) => Rgb { red: 87, ..GRAY },
            _ => GRAY,
        });
        let strip = Strip::new(&image);
        assert!(strip.plain(0), "4 columns are fewer than a part");
        assert!(!strip.plain(1), "5 columns are a part");
        assert!(
            strip.plain(2),
            "a color within 32 of the lane's is the same"
        );
        assert!(!strip.plain(3), "a color 32 from the lane's is another");
    }

    #[test]
    fn reasons_explain_themselves() {
        let reasons = [
            (NoBar::Tiny, "the region is too short to hold a bar"),
            (NoBar::OneColor, "the region shows one color"),
            (
                NoBar::AlikeColors,
                "the region's two main colors are too close to tell apart",
            ),
            (
                NoBar::NotABar,
                "the region is not one color and then another along its long side",
            ),
            (
                NoBar::Misread,
                "the two colors read the region other than its layout looks",
            ),
        ];
        for (reason, text) in reasons {
            assert_eq!(reason.to_string(), text);
        }
    }

    proptest! {
        /// A bar drawn at any fill that shows both parts gets colors with
        /// which the reader reads that fill, to the column.
        #[test]
        fn the_reader_reads_a_detected_bar_at_the_fill_it_was_drawn_at(
            length in 20_u32..160,
            lanes in 1_u32..12,
            fill in 0_u32..1000,
            tall in any::<bool>(),
            filled in any::<[u8; 3]>(),
            empty in any::<[u8; 3]>(),
            noise in proptest::collection::vec(0_u8..=4, 64),
        ) {
            let [filled, empty] = [filled, empty].map(|[red, green, blue]| Rgb { red, green, blue });
            // Far enough apart that the noise below cannot bring the two
            // colors within the distance they need: 48 squared.
            prop_assume!(distance(filled, empty) >= 2304);
            let least = u32::try_from(share(usize::try_from(length).unwrap())).unwrap();
            let most = length.saturating_sub(least);
            // `fill` is in thousandths of the bar.
            let columns = length.saturating_mul(fill).checked_div(1000).unwrap().clamp(least, most);
            // Each pixel is up to 2 off its color in every channel.
            let noisy = |color: Rgb, x: u32, y: u32| {
                let at = usize::try_from(x.wrapping_mul(7).wrapping_add(y.wrapping_mul(13))).unwrap();
                let shift = |channel: u8, turn: usize| {
                    let by = noise[at.wrapping_add(turn).checked_rem(noise.len()).unwrap()];
                    channel.saturating_add(by).saturating_sub(2)
                };
                Rgb { red: shift(color.red, 0), green: shift(color.green, 1), blue: shift(color.blue, 2) }
            };
            let image = if tall {
                picture(lanes, length, |x, y| {
                    let part = if length.saturating_sub(y) <= columns { filled } else { empty };
                    noisy(part, x, y)
                })
            } else {
                picture(length, lanes, |x, y| noisy(if x < columns { filled } else { empty }, x, y))
            };
            let proposal = detect_bar(&image, &[]);
            prop_assert!(proposal.is_ok(), "{proposal:?}");
            let bar = proposal.unwrap().bar;
            // Squared distances: the noise moves a color by at most 2 in each channel.
            prop_assert!(distance(bar.filled(), filled) <= 12);
            prop_assert!(distance(bar.empty(), empty) <= 12);
            let reads = bar.percent(&image).unwrap().raw();
            let drawn = f64::from(columns) * 100.0 / f64::from(length);
            prop_assert!((reads - drawn).abs() < 1e-9, "reads {reads}, drawn at {drawn}");
        }
    }
}
