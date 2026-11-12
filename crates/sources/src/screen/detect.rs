//! Proposing a bar fill check's two colors from screenshots of its region.
//!
//! [`BarFill`] counts the pixels near a filled color and near an empty color,
//! wherever they are, so it reads any bar that shows two flat colors.
//! [`detect_bar`] finds those two colors instead of having the user click
//! them, and refuses a region rather than propose colors the reader would
//! read wrongly.
//!
//! It looks for one plain color from the start of the region's long side,
//! the left of a wide region or the bottom of a tall one, and another plain
//! color after it, with nothing between them but an edge and nothing around
//! them but a border. It takes the color at the start as the filled one: the
//! reader has no direction, so that is a convention the user can turn
//! around.
//!
//! A label, an icon, or what lies beside a loosely drawn bar hides that
//! layout in the columns it covers, though the reader leaves its pixels out
//! and reads the bar all the same. So where the whole region shows no bar,
//! the detector looks again over only the lanes that each show the bar from
//! end to end on their own.
//!
//! The colors come from one screenshot's layout. Every other screenshot of
//! the region then has to show the same two colors the same way, at any
//! fill, or neither of them, and on each one [`BarFill::percent`] has to read
//! as full as the layout looks. A bar whose colors change between
//! screenshots is refused, because no two colors read it.

use std::error::Error;
use std::fmt;
use std::iter;

use super::Image;
use super::check::{BarFill, BarPart, distance};
use crate::system::Rgb;

/// The shortest long side, in pixels, that a bar is looked for on.
pub const MIN_LENGTH: usize = 12;

/// The squared distance within which colors are one plain color: a part of
/// a bar stays this near its color along the whole part.
const SAME: u32 = 16 * 16;

/// The squared distance a filled and an empty color need between them.
const APART: u32 = 32 * 32;

/// The most different colors gathered from a region. A bar shows two and a
/// few more for its border and label; more means the region is no bar.
const MAX_CLASSES: usize = 16;

/// The share of a region's long side that the bar's two parts have to
/// cover, in percent. The rest may lie only at the two ends, as a border,
/// a rounded cap, or some background does.
const COVER: usize = 85;

/// A part of a bar is at least one in this many of the long side's columns.
/// It is also how far two screenshots' fills have to differ to show the bar
/// moving.
const PARTS: usize = 20;

/// The fewest columns that make a part of a bar, however short the region.
const MIN_PART: usize = 3;

/// The most columns between the two parts that belong to neither, as an
/// anti-aliased edge does.
const EDGE: usize = 2;

/// The share of a region's lanes, in percent, that have to show the bar
/// from end to end on their own for the detector to go by those lanes
/// alone, past a label or what lies beside the bar.
const CLEAR: usize = 20;

/// How far, in points of fill, the reader may be from the fill a layout
/// shows. Past it the colors are not proposed.
const AGREE: f64 = 5.0;

/// How far, in points of fill, the reader may be from the fill a layout
/// shows before a proposal says that readings can be a few points off.
const ROUGH: f64 = 2.0;

/// A bar's two colors, proposed from screenshots of its region.
///
/// The screenshots are numbered as they were given: 0 is the first, and
/// each one after it counts on from 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarProposal {
    /// The bar the colors make.
    pub bar: BarFill,
    /// The screenshot whose layout gave the colors.
    pub found_on: usize,
    /// The screenshots that show the bar in these colors, full, empty, or
    /// in between, in order. The others show neither color.
    pub shown_on: Vec<usize>,
    /// Whether two screenshots show both parts at different fills, which a
    /// picture in two colors would not.
    pub moves: bool,
    /// Whether the reader is more than a couple of points off the layout on
    /// a screenshot, as a label or a soft edge on the bar causes. It is
    /// never more than a few points off.
    pub rough: bool,
    /// Whether the colors were found over only some of the region's lanes,
    /// the ones that show the bar from end to end, because a label, an
    /// icon, or what lies beside the bar hides it in the others.
    pub around: bool,
}

/// Why no bar is proposed for a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoBar {
    /// The region's long side is shorter than [`MIN_LENGTH`].
    Tiny,
    /// The region shows one color, as a full or an empty bar does.
    OneColor,
    /// The region's two main colors are too close to tell apart.
    AlikeColors,
    /// The region is not one plain color and then another along its long
    /// side.
    NotABar,
    /// The two colors read the region other than its layout looks, as when
    /// something beside the bar has one of them or part of the bar has
    /// another shade.
    Misread,
    /// The colors found on one screenshot do not read another, which shows
    /// one of them but not the bar in both, as a bar that changes color
    /// does.
    Differs {
        /// The screenshot whose layout gave the colors.
        found_on: usize,
        /// The screenshot they do not read.
        image: usize,
    },
}

impl fmt::Display for NoBar {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tiny => formatter.write_str("the region is too short to hold a bar"),
            Self::OneColor => formatter.write_str("the region shows one color"),
            Self::AlikeColors => {
                formatter.write_str("the region's two main colors are too close to tell apart")
            },
            Self::NotABar => formatter.write_str(
                "the region is not one plain color and then another along its long side",
            ),
            Self::Misread => {
                formatter.write_str("the two colors read the region other than its layout looks")
            },
            Self::Differs { found_on, image } => write!(
                formatter,
                "the colors of the bar on screenshot {found_on} do not read screenshot {image}"
            ),
        }
    }
}

impl Error for NoBar {}

/// The filled and the empty color of the bar in a region, from `first`, the
/// region on one screenshot, and `others`, the same region on further
/// screenshots.
///
/// The colors are those of the first screenshot whose layout shows a bar,
/// and they are proposed only when they read every screenshot. An image in
/// `others` of another size than `first` is left out.
///
/// # Errors
///
/// Returns the [`NoBar`] that says why no colors are proposed: why `first`
/// shows no bar when no screenshot does, or which screenshot the colors of
/// another do not read.
pub fn detect_bar(first: &Image, others: &[Image]) -> Result<BarProposal, NoBar> {
    let strip = Strip::new(first);
    if strip.length() < MIN_LENGTH {
        return Err(NoBar::Tiny);
    }
    let images: Vec<(usize, &Image)> = iter::once(first)
        .chain(others)
        .enumerate()
        .filter(|(_, image)| image.size() == first.size())
        .collect();
    let whole = match propose(&images, &vec![true; strip.lanes()]) {
        Ok(proposal) => return Ok(proposal),
        Err(reason) => reason,
    };
    // Whatever else is drawn in the region outvotes the bar in the columns
    // it covers, so look again at only the lanes that show the bar from end
    // to end on their own.
    let clear = match propose(&images, &clear_lanes(&images)) {
        Ok(proposal) => {
            return Ok(BarProposal {
                around: true,
                ..proposal
            });
        },
        Err(reason) => reason,
    };
    // A reason about a bar that was found says more than why the whole
    // region shows none.
    Err(if clear.tells() { clear } else { whole })
}

impl NoBar {
    /// Whether the reason is about a bar that was found, which says more
    /// than why some lanes show none.
    const fn tells(self) -> bool {
        matches!(self, Self::Misread | Self::Differs { .. })
    }
}

/// The lanes that show a bar from end to end on their own, on every one of
/// `images` where at least [`CLEAR`] percent of the lanes do; or no lane
/// when fewer than that are left, or there is no such image. A label, an
/// icon, a border along the bar, and what lies beside the bar are in none
/// of them. An image with fewer such lanes, as one of a full bar or of
/// something else, has no say.
fn clear_lanes(images: &[(usize, &Image)]) -> Vec<bool> {
    let enough = |lanes: &Vec<bool>| {
        let clear = lanes.iter().filter(|&&clear| clear).count();
        covers(clear, lanes.len(), CLEAR)
    };
    let each = images.iter().map(|&(_, image)| {
        let strip = Strip::new(image);
        let clear = (0..strip.lanes()).map(|lane| two_parts(&strip.lane(lane)).is_ok());
        clear.collect::<Vec<bool>>()
    });
    let on_all = each.filter(enough).reduce(|mut all, lanes| {
        for (clear, also) in all.iter_mut().zip(lanes) {
            *clear &= also;
        }
        all
    });
    on_all.filter(enough).unwrap_or_default()
}

/// The bar that the `kept` lanes of `images` show: the colors of the first
/// image whose layout shows one, when they read every image.
fn propose(images: &[(usize, &Image)], kept: &[bool]) -> Result<BarProposal, NoBar> {
    let seen: Vec<(usize, &Image, Vec<Rgb>)> = images
        .iter()
        .map(|&(number, image)| (number, image, Strip::new(image).columns(kept)))
        .collect();
    let mut first_reason = None;
    let found = seen.iter().find_map(|(number, image, columns)| {
        let reason = match from_layout(image, columns) {
            Ok(bar) => return Some((*number, bar)),
            Err(reason) => reason,
        };
        first_reason.get_or_insert(reason);
        None
    });
    // No image at all shows no bar.
    let (found_on, bar) = found.ok_or_else(|| first_reason.unwrap_or(NoBar::NotABar))?;
    let mut shown_on = Vec::new();
    // The fills of the images that show both parts.
    let mut partly: Vec<f64> = Vec::new();
    let mut rough = false;
    for (number, image, columns) in &seen {
        let (shown, off) = match verdict(image, columns, bar) {
            Verdict::Shows { shown, off } => (shown, off),
            Verdict::Absent => continue,
            Verdict::Differs => {
                return Err(NoBar::Differs {
                    found_on,
                    image: *number,
                });
            },
        };
        shown_on.push(*number);
        rough |= off > ROUGH;
        if matches!(shown, Shown::Both { .. }) {
            partly.push(shown.fill());
        }
    }
    let least = partly.iter().copied().fold(f64::INFINITY, f64::min);
    let most = partly.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Ok(BarProposal {
        bar,
        found_on,
        shown_on,
        moves: most - least >= 100.0 / PARTS as f64,
        rough,
        around: false,
    })
}

/// The bar that `columns`, the columns of `image`, lay out as, its two main
/// colors with the one at the start as the filled one, when the reader reads
/// `image` as full as that layout looks.
fn from_layout(image: &Image, columns: &[Rgb]) -> Result<BarFill, NoBar> {
    let bar = two_parts(columns)?;
    match verdict(image, columns, bar) {
        Verdict::Shows { .. } => Ok(bar),
        Verdict::Absent | Verdict::Differs => Err(NoBar::Misread),
    }
}

/// The bar whose two parts `colors` lay out as, from their start to their
/// end: their two main colors, the one at the start as the filled one.
fn two_parts(colors: &[Rgb]) -> Result<BarFill, NoBar> {
    let pair = two_colors(&classes(colors.iter().copied()), part(colors.len()))?;
    let both = |bar: &BarFill| matches!(laid_out(colors, *bar), Some(Shown::Both { .. }));
    [pair, pair.turned()]
        .into_iter()
        .find(both)
        .ok_or(NoBar::NotABar)
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

/// What a region's columns show of a bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shown {
    /// Its filled part and then its empty part, each this many columns.
    Both { filled: usize, empty: usize },
    /// Only its filled part: the bar is full.
    Filled,
    /// Only its empty part: the bar is empty.
    Empty,
}

impl Shown {
    /// How full the bar looks, from 0 to 100.
    fn fill(self) -> f64 {
        match self {
            Self::Both { filled, empty } => {
                filled as f64 * 100.0 / filled.saturating_add(empty) as f64
            },
            Self::Filled => 100.0,
            Self::Empty => 0.0,
        }
    }
}

/// What `columns` show of `bar`, or `None` when they do not lay out as it:
/// its filled part, then at most an edge of columns that belong to neither
/// part, then its empty part. One part alone is a full or an empty bar. The
/// columns before and after belong to neither part, and are no more of the
/// region than a border is.
fn laid_out(columns: &[Rgb], bar: BarFill) -> Option<Shown> {
    let parts: Vec<Option<BarPart>> = columns.iter().map(|&column| belongs(bar, column)).collect();
    let start = parts.iter().position(Option::is_some)?;
    let end = parts.iter().rposition(Option::is_some)?;
    let inner = parts.get(start..=end)?;
    if !covers(inner.len(), parts.len(), COVER) {
        return None;
    }
    let mut runs: Vec<(Option<BarPart>, usize)> = Vec::new();
    for &part in inner {
        match runs.last_mut() {
            Some((last, count)) if *last == part => *count = count.saturating_add(1),
            _ => runs.push((part, 1)),
        }
    }
    match *runs.as_slice() {
        [(Some(BarPart::Filled), _)] => Some(Shown::Filled),
        [(Some(BarPart::Empty), _)] => Some(Shown::Empty),
        [
            (Some(BarPart::Filled), filled),
            (Some(BarPart::Empty), empty),
        ] => Some(Shown::Both { filled, empty }),
        [
            (Some(BarPart::Filled), filled),
            (None, edge),
            (Some(BarPart::Empty), empty),
        ] if edge <= EDGE => Some(Shown::Both { filled, empty }),
        _ => None,
    }
}

/// How one screenshot of a region stands to a bar's two colors.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Verdict {
    /// It shows the bar, and the reader reads it `off` points of fill from
    /// what its layout shows, which is within [`AGREE`].
    Shows { shown: Shown, off: f64 },
    /// It shows neither color, as a screenshot of something else does.
    Absent,
    /// It shows one of the colors on a part's worth of columns, but not
    /// the bar in both, or the reader reads it otherwise than it looks.
    Differs,
}

/// How `image`, whose columns are `columns`, stands to `bar`.
fn verdict(image: &Image, columns: &[Rgb], bar: BarFill) -> Verdict {
    let Some(shown) = laid_out(columns, bar) else {
        let either = columns
            .iter()
            .filter(|&&column| belongs(bar, column).is_some());
        return if either.count() >= part(columns.len()) {
            Verdict::Differs
        } else {
            Verdict::Absent
        };
    };
    let off = bar
        .percent(image)
        .map_or(f64::INFINITY, |reads| (reads.raw() - shown.fill()).abs());
    if off <= AGREE {
        Verdict::Shows { shown, off }
    } else {
        Verdict::Differs
    }
}

/// The part of `bar` a column of the color `color` belongs to: the part the
/// reader counts it as, when it is as near that part's color as one plain
/// color stays. A fill that fades, or a second shade in it, belongs to
/// neither.
fn belongs(bar: BarFill, color: Rgb) -> Option<BarPart> {
    let part = bar.part(color)?;
    let own = match part {
        BarPart::Filled => bar.filled(),
        BarPart::Empty => bar.empty(),
    };
    (distance(color, own) <= SAME).then_some(part)
}

/// The fewest columns of a region `length` long that make a part of a bar.
fn part(length: usize) -> usize {
    length.checked_div(PARTS).unwrap_or_default().max(MIN_PART)
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

    /// The colors of lane `lane`, from the start.
    fn lane(self, lane: usize) -> Vec<Rgb> {
        (0..self.length())
            .filter_map(|column| self.at(column, lane))
            .collect()
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

    /// The proposal of `filled` and `empty` from one screenshot alone.
    fn alone(filled: Rgb, empty: Rgb) -> BarProposal {
        BarProposal {
            bar: BarFill::new(filled, empty).unwrap(),
            found_on: 0,
            shown_on: vec![0],
            moves: false,
            rough: false,
            around: false,
        }
    }

    /// The proposal of green on gray from one screenshot, found over only
    /// the lanes that show the bar from end to end.
    fn around() -> BarProposal {
        BarProposal {
            around: true,
            ..alone(GREEN, GRAY)
        }
    }

    /// The proposal of green on gray that the screenshots `shown_on` show,
    /// two of them at different fills.
    fn moving(found_on: usize, shown_on: &[usize]) -> BarProposal {
        BarProposal {
            found_on,
            shown_on: shown_on.to_vec(),
            moves: true,
            ..alone(GREEN, GRAY)
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
                Ok(alone(GREEN, GRAY)),
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
        assert_eq!(detect_bar(&drained, &[]), Ok(alone(GRAY, GREEN)));
        // A tall region starts at the bottom.
        let tall = picture(8, 100, |_, y| if y >= 70 { GREEN } else { GRAY });
        assert_eq!(detect_bar(&tall, &[]), Ok(alone(GREEN, GRAY)));
        assert!((reads(&tall, &[]) - 30.0).abs() < 1e-9);
        let hanging = picture(8, 100, |_, y| if y < 30 { GREEN } else { GRAY });
        assert_eq!(detect_bar(&hanging, &[]), Ok(alone(GRAY, GREEN)));
        // A square region counts as wide.
        let square = picture(12, 12, |x, _| if x < 6 { GREEN } else { GRAY });
        assert_eq!(detect_bar(&square, &[]), Ok(alone(GREEN, GRAY)));
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
        assert_eq!(detect_bar(&image, &[]), Ok(alone(GREEN, GRAY)));
        // 40 of the bar's 100 columns are filled, and the soft one counts
        // with them.
        assert!((reads(&image, &[]) - 41.0).abs() < 1.0);
        // A border as thick as the bar outvotes it in each column, until
        // only the lanes that show the bar are looked at.
        let framed = picture(100, 10, |x, y| match y {
            3..7 if x < 40 => GREEN,
            3..7 => GRAY,
            _ => WHITE,
        });
        assert_eq!(detect_bar(&framed, &[]), Ok(around()));
    }

    #[test]
    fn an_edge_of_two_columns_may_lie_between_the_parts_and_no_more() {
        let edged = |edge: u32| {
            picture(100, 8, move |x, _| match x {
                0..40 => GREEN,
                _ if x < 40_u32.saturating_add(edge) => WHITE,
                _ => GRAY,
            })
        };
        assert_eq!(detect_bar(&edged(2), &[]), Ok(alone(GREEN, GRAY)));
        assert_eq!(detect_bar(&edged(3), &[]), Err(NoBar::NotABar));
        // The same columns inside a part split it, however few they are.
        let split = picture(100, 8, |x, _| match x {
            20 => WHITE,
            0..40 => GREEN,
            _ => GRAY,
        });
        assert_eq!(detect_bar(&split, &[]), Err(NoBar::NotABar));
    }

    #[test]
    fn what_is_neither_part_may_take_up_the_ends_of_the_region_only_so_far() {
        let margin = |columns: u32| {
            picture(100, 8, move |x, _| match x {
                _ if x < columns => WHITE,
                _ if x < 50 => GREEN,
                _ => GRAY,
            })
        };
        assert_eq!(detect_bar(&margin(15), &[]), Ok(alone(GREEN, GRAY)));
        assert_eq!(detect_bar(&margin(16), &[]), Err(NoBar::NotABar));
        // Rounded caps blend into the background at both ends.
        let capped = picture(100, 8, |x, _| match x {
            0 | 99 => BLACK,
            1 | 98 => red(90),
            2..40 => GREEN,
            _ => GRAY,
        });
        assert_eq!(detect_bar(&capped, &[]), Ok(alone(GREEN, GRAY)));
    }

    #[test]
    fn a_fill_shaded_across_the_bar_is_one_color() {
        let shade = |y: u32| Rgb {
            green: 170_u8.saturating_add(u8::try_from(y).unwrap().saturating_mul(8)),
            ..GREEN
        };
        let image = picture(100, 10, |x, y| if x < 30 { shade(y) } else { GRAY });
        // The lower of the two middle lanes' shades.
        assert_eq!(detect_bar(&image, &[]), Ok(alone(shade(4), GRAY)));
        assert!((reads(&image, &[]) - 30.0).abs() < 1e-9);
    }

    /// A color `share` thousandths of the way from `from` to `to`.
    fn mix(from: Rgb, to: Rgb, share: u32) -> Rgb {
        let channel = |from: u8, to: u8| {
            let part = u32::from(from.abs_diff(to))
                .saturating_mul(share)
                .checked_div(1000)
                .unwrap();
            let part = u8::try_from(part).unwrap();
            if to >= from {
                from.saturating_add(part)
            } else {
                from.saturating_sub(part)
            }
        };
        Rgb {
            red: channel(from.red, to.red),
            green: channel(from.green, to.green),
            blue: channel(from.blue, to.blue),
        }
    }

    /// A bar 200 long whose fill fades from `from` at its start to `to` at
    /// the end of the region, with its first `filled` columns filled.
    fn fading(from: Rgb, to: Rgb, filled: u32) -> Image {
        picture(200, 10, move |x, _| {
            if x < filled {
                mix(from, to, x.saturating_mul(5))
            } else {
                GRAY
            }
        })
    }

    #[test]
    fn a_fill_that_fades_along_the_bar_is_refused() {
        let whole = picture(100, 8, |x, _| Rgb {
            red: 220,
            green: u8::try_from(x).unwrap().saturating_mul(2),
            blue: 30,
        });
        assert_eq!(detect_bar(&whole, &[]), Err(NoBar::NotABar));
        let strong = |filled| {
            let dark = Rgb {
                red: 0,
                green: 90,
                blue: 0,
            };
            let bright = Rgb {
                red: 80,
                green: 255,
                blue: 80,
            };
            fading(dark, bright, filled)
        };
        for filled in [40, 100, 160] {
            assert_eq!(
                detect_bar(&strong(filled), &[]),
                Err(NoBar::NotABar),
                "{filled} columns filled"
            );
        }
        assert_eq!(detect_bar(&strong(40), &[strong(160)]), Err(NoBar::NotABar));
    }

    #[test]
    fn a_fill_that_fades_a_little_is_found_only_where_it_reads_every_screenshot() {
        let mild = |filled| {
            let from = Rgb {
                red: 30,
                green: 180,
                blue: 50,
            };
            let to = Rgb {
                red: 60,
                green: 240,
                blue: 80,
            };
            fading(from, to, filled)
        };
        // The first 40 columns stay within what counts as one plain color.
        let short = detect_bar(&mild(40), &[]).unwrap();
        assert!((short.bar.percent(&mild(40)).unwrap().raw() - 20.0).abs() < 1e-9);
        for filled in [100, 160] {
            assert_eq!(
                detect_bar(&mild(filled), &[]),
                Err(NoBar::NotABar),
                "{filled} columns filled"
            );
        }
        // More of the fade shows on the second screenshot than the first's
        // colors read.
        assert_eq!(
            detect_bar(&mild(40), &[mild(160)]),
            Err(NoBar::Differs {
                found_on: 0,
                image: 1
            })
        );
    }

    #[test]
    fn a_striped_or_a_segmented_fill_is_refused() {
        let dark = Rgb {
            red: 20,
            green: 150,
            blue: 40,
        };
        let striped = |filled: u32| {
            picture(200, 10, move |x, y| {
                match x.saturating_add(y).checked_div(4).unwrap() {
                    _ if x >= filled => GRAY,
                    stripe if stripe.is_multiple_of(2) => GREEN,
                    _ => dark,
                }
            })
        };
        assert_eq!(detect_bar(&striped(50), &[]), Err(NoBar::NotABar));
        assert_eq!(detect_bar(&striped(120), &[]), Err(NoBar::NotABar));
        let segmented = |segments: u32, gap: Rgb| {
            picture(200, 10, move |x, _| match x.checked_div(20).unwrap() {
                _ if x.checked_rem(20).unwrap() >= 18 => gap,
                segment if segment < segments => GREEN,
                _ => GRAY,
            })
        };
        for segments in [3, 5, 8] {
            for gap in [BLACK, GRAY] {
                assert_eq!(
                    detect_bar(&segmented(segments, gap), &[]),
                    Err(NoBar::NotABar),
                    "{segments} segments, gaps of {gap:?}"
                );
            }
        }
    }

    #[test]
    fn colors_closer_than_the_reader_can_split_are_not_proposed() {
        let near = |by: u8| picture(100, 8, move |x, _| if x < 50 { red(100) } else { red(by) });
        assert_eq!(detect_bar(&near(132), &[]), Ok(alone(red(100), red(132))));
        assert_eq!(detect_bar(&near(131), &[]), Err(NoBar::AlikeColors));
    }

    #[test]
    fn a_second_shade_in_a_part_is_one_color_only_when_it_is_near() {
        let shaded = |shade: u8| {
            picture(100, 8, move |x, _| match x {
                0..40 => red(100),
                40..70 => red(shade),
                _ => red(220),
            })
        };
        // Red 116 is as near red 100 as one plain color stays, so the
        // part's color is their average.
        assert_eq!(detect_bar(&shaded(116), &[]), Ok(alone(red(106), red(220))));
        assert!((reads(&shaded(116), &[]) - 70.0).abs() < 1e-9);
        // Red 117 is another color, too near red 100 to be the other part
        // and too far to belong to its part.
        assert_eq!(detect_bar(&shaded(117), &[]), Err(NoBar::NotABar));
    }

    #[test]
    fn a_picture_that_is_not_one_color_then_another_is_not_a_bar() {
        let stripes = picture(100, 8, |x, _| {
            if x.checked_div(6).unwrap().is_multiple_of(2) {
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
        // One column of the other part's color inside a part is enough.
        let stray = picture(100, 8, |x, _| if x < 50 && x != 10 { GREEN } else { GRAY });
        assert_eq!(detect_bar(&stray, &[]), Err(NoBar::NotABar));
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
    fn a_region_shorter_than_twelve_pixels_is_too_small() {
        assert_eq!(detect_bar(&bar(11, 4, 5), &[]), Err(NoBar::Tiny));
        assert_eq!(detect_bar(&bar(12, 4, 6), &[]), Ok(alone(GREEN, GRAY)));
        // The long side counts, whichever it is.
        let tall = picture(4, 11, |_, y| if y < 5 { GRAY } else { GREEN });
        assert_eq!(detect_bar(&tall, &[]), Err(NoBar::Tiny));
    }

    /// A bar of six lanes in a region of `height`, on `background`.
    fn loose(height: u32, background: Rgb, filled: u32) -> Image {
        picture(100, height, move |x, y| match (x, y) {
            (5..95, 12..18) if x < filled.saturating_add(5) => GREEN,
            (5..95, 12..18) => GRAY,
            _ => background,
        })
    }

    #[test]
    fn a_bar_drawn_loosely_on_a_plain_background_is_found() {
        assert_eq!(detect_bar(&loose(30, BLACK, 45), &[]), Ok(around()));
        assert!((reads(&loose(30, BLACK, 45), &[]) - 50.0).abs() < 1e-9);
        // The bar has to be a fifth of the region's lanes: six of 30.
        assert_eq!(detect_bar(&loose(31, BLACK, 45), &[]), Err(NoBar::OneColor));
        // No lane of a full or an empty bar shows two parts.
        assert_eq!(detect_bar(&loose(30, BLACK, 0), &[]), Err(NoBar::OneColor));
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
    }

    /// A bar half full, but for the first of its `lanes` lanes, which has
    /// the empty color throughout.
    fn with_an_empty_lane(lanes: u32) -> Image {
        picture(
            100,
            lanes,
            |x, y| if x < 50 && y > 0 { GREEN } else { GRAY },
        )
    }

    #[test]
    fn the_reader_may_be_a_few_points_off_the_layout_and_the_proposal_says_so() {
        let rough = BarProposal {
            rough: true,
            ..alone(GREEN, GRAY)
        };
        // One lane in 25 is 2 points off, which is not worth a word.
        assert_eq!(
            detect_bar(&with_an_empty_lane(25), &[]),
            Ok(alone(GREEN, GRAY))
        );
        assert_eq!(detect_bar(&with_an_empty_lane(20), &[]), Ok(rough.clone()));
        // Each screenshot is held to that, and one that is off is enough.
        assert_eq!(
            detect_bar(&with_an_empty_lane(20), &[with_an_empty_lane(20)]),
            Ok(BarProposal {
                shown_on: vec![0, 1],
                ..rough
            })
        );
        // One lane in 10 is 5 points off, the most there may be.
        assert_eq!(detect_bar(&with_an_empty_lane(10), &[]), Ok(rough));
        assert_eq!(detect_bar(&with_an_empty_lane(9), &[]), Err(NoBar::Misread));
    }

    #[test]
    fn screenshots_that_show_the_bar_at_different_fills_say_that_it_moves() {
        assert_eq!(
            detect_bar(&bar(100, 8, 30), &[bar(100, 8, 70)]),
            Ok(moving(0, &[0, 1]))
        );
        // A twentieth of the bar is enough of a move, and less is not.
        assert_eq!(
            detect_bar(&bar(100, 8, 30), &[bar(100, 8, 35)]),
            Ok(moving(0, &[0, 1]))
        );
        let still = BarProposal {
            shown_on: vec![0, 1],
            ..alone(GREEN, GRAY)
        };
        assert_eq!(
            detect_bar(&bar(100, 8, 30), &[bar(100, 8, 34)]),
            Ok(still.clone())
        );
        // A full or an empty bar is read, but shows no edge that moved.
        assert_eq!(
            detect_bar(&bar(100, 8, 30), &[bar(100, 8, 100)]),
            Ok(still.clone())
        );
        assert_eq!(detect_bar(&bar(100, 8, 30), &[bar(100, 8, 0)]), Ok(still));
    }

    #[test]
    fn the_colors_come_from_the_first_screenshot_that_shows_both_parts() {
        assert_eq!(
            detect_bar(&bar(100, 8, 100), &[bar(100, 8, 0), bar(100, 8, 40)]),
            Ok(BarProposal {
                found_on: 2,
                shown_on: vec![0, 1, 2],
                ..alone(GREEN, GRAY)
            })
        );
        // Three columns of twenty are a part, and one is not.
        assert_eq!(
            detect_bar(&bar(20, 2, 1), &[bar(20, 2, 3)]),
            Ok(moving(1, &[0, 1]))
        );
        assert_eq!(
            detect_bar(&bar(100, 8, 100), &[bar(100, 8, 0)]),
            Err(NoBar::OneColor),
            "the first screenshot's reason, when none shows both parts"
        );
    }

    /// A health bar 200 long whose fill turns from red to green as it
    /// fills.
    fn health(filled: u32) -> Image {
        let low = Rgb {
            red: 220,
            green: 30,
            blue: 30,
        };
        let high = Rgb {
            red: 30,
            green: 200,
            blue: 30,
        };
        let fill = mix(low, high, filled.saturating_mul(5));
        picture(200, 10, move |x, _| if x < filled { fill } else { GRAY })
    }

    #[test]
    fn a_bar_whose_fill_changes_color_between_screenshots_is_refused() {
        let differs = |image| Err(NoBar::Differs { found_on: 0, image });
        assert_eq!(detect_bar(&health(40), &[health(160)]), differs(1));
        assert_eq!(detect_bar(&health(160), &[health(40)]), differs(1));
        let seven = [160, 150, 165, 155, 170, 145, 40].map(health);
        // The fills near the first one's have a color near enough to its
        // own to be read; the sixth, and the one at a fifth, do not.
        assert_eq!(detect_bar(&seven[0], &seven[1..]), differs(5));
        assert_eq!(detect_bar(&seven[6], &seven[..6]), differs(1));
        // A fill that only brightens, at the same fill or another.
        let bright = Rgb {
            red: 120,
            green: 255,
            blue: 140,
        };
        let pulse =
            |filled: u32| picture(100, 8, move |x, _| if x < filled { bright } else { GRAY });
        assert_eq!(detect_bar(&bar(100, 8, 60), &[pulse(60)]), differs(1));
        assert_eq!(
            detect_bar(&bar(100, 8, 30), &[bar(100, 8, 50), pulse(70)]),
            differs(2)
        );
    }

    #[test]
    fn something_over_part_of_the_bar_is_looked_around_on_every_screenshot() {
        let cream = Rgb {
            red: 255,
            green: 255,
            blue: 225,
        };
        let tooltip = |from: u32| {
            picture(100, 20, move |x, y| match x {
                _ if x >= from && x < from.saturating_add(40) && y < 14 => cream,
                0..50 => GREEN,
                _ => GRAY,
            })
        };
        // Six lanes of twenty stay free of it, and it hides as much of one
        // part as of the other, so the reader still reads a half.
        let found = BarProposal {
            shown_on: vec![0, 1],
            ..around()
        };
        assert_eq!(
            detect_bar(&bar(100, 20, 50), &[tooltip(30)]),
            Ok(found.clone())
        );
        assert_eq!(detect_bar(&tooltip(30), &[bar(100, 20, 50)]), Ok(found));
        // Over the filled part alone it takes the reader too far off.
        assert_eq!(
            detect_bar(&bar(100, 20, 50), &[tooltip(5)]),
            Err(NoBar::Differs {
                found_on: 0,
                image: 1
            })
        );
        // The colors the other way around are not the same bar.
        let turned = picture(100, 8, |x, _| if x < 30 { GRAY } else { GREEN });
        assert_eq!(
            detect_bar(&bar(100, 8, 30), &[turned]),
            Err(NoBar::Differs {
                found_on: 0,
                image: 1
            })
        );
    }

    #[test]
    fn a_screenshot_that_shows_neither_color_is_left_out() {
        let menu = picture(100, 8, |x, _| if x < 60 { BLUE } else { WHITE });
        assert_eq!(
            detect_bar(&bar(100, 8, 30), std::slice::from_ref(&menu)),
            Ok(alone(GREEN, GRAY))
        );
        assert_eq!(
            detect_bar(&bar(100, 8, 30), &[menu, bar(100, 8, 70)]),
            Ok(moving(0, &[0, 2]))
        );
        // Fewer columns of a color than make a part are not the bar.
        let speck =
            |columns: u32| picture(100, 8, move |x, _| if x < columns { GRAY } else { WHITE });
        assert_eq!(
            detect_bar(&bar(100, 8, 30), &[speck(4)]),
            Ok(alone(GREEN, GRAY))
        );
        assert_eq!(
            detect_bar(&bar(100, 8, 30), &[speck(5)]),
            Err(NoBar::Differs {
                found_on: 0,
                image: 1
            })
        );
    }

    #[test]
    fn a_screenshot_of_another_size_is_left_out() {
        let other = picture(50, 8, |_, _| GRAY);
        assert_eq!(
            detect_bar(&bar(100, 8, 30), std::slice::from_ref(&other)),
            Ok(alone(GREEN, GRAY))
        );
        assert_eq!(
            detect_bar(&bar(100, 8, 30), &[other, bar(100, 8, 70)]),
            Ok(moving(0, &[0, 2]))
        );
    }

    #[test]
    fn a_bar_over_a_busy_background_is_found_in_the_lanes_that_show_it() {
        // Four lanes of twenty hold the bar, and every other lane is busy.
        let over_a_scene = |filled: u32| {
            picture(100, 20, move |x, y| match y {
                8..12 if x < filled => GREEN,
                8..12 => GRAY,
                _ if x.is_multiple_of(2) => BLUE,
                _ => WHITE,
            })
        };
        assert_eq!(detect_bar(&over_a_scene(30), &[]), Ok(around()));
        assert_eq!(
            detect_bar(&over_a_scene(30), &[over_a_scene(70)]),
            Ok(BarProposal {
                around: true,
                ..moving(0, &[0, 1])
            })
        );
    }

    #[test]
    fn a_reason_about_a_bar_that_was_found_is_the_one_given() {
        // The whole region shows one color, and the lanes that show a bar
        // show one whose colors also lie beside it.
        let beside = |filled: u32| {
            picture(100, 30, move |x, y| {
                if x < filled && (12..18).contains(&y) {
                    GREEN
                } else {
                    GRAY
                }
            })
        };
        assert_eq!(detect_bar(&beside(30), &[beside(70)]), Err(NoBar::Misread));
        // The whole region shows a bar that another screenshot differs
        // from, which looking at fewer lanes does not change.
        let bright = Rgb {
            red: 120,
            green: 255,
            blue: 140,
        };
        let other = picture(100, 8, |x, _| if x < 70 { bright } else { GRAY });
        assert_eq!(
            detect_bar(&bar(100, 8, 30), &[other]),
            Err(NoBar::Differs {
                found_on: 0,
                image: 1
            })
        );
    }

    /// Whether `text`, drawn in a 3 by 5 font with `scale` pixels to the
    /// dot, covers the pixel (`x`, `y`) of its own box.
    fn glyph(text: &str, scale: u32, x: u32, y: u32) -> bool {
        const FONT: [(char, [&str; 5]); 5] = [
            ('0', ["###", "#.#", "#.#", "#.#", "###"]),
            ('2', ["###", "..#", "###", "#..", "###"]),
            ('5', ["###", "#..", "###", "..#", "###"]),
            ('7', ["###", "..#", "..#", "..#", "..#"]),
            ('%', ["#.#", "..#", ".#.", "#..", "#.#"]),
        ];
        let dot = |pixel: u32| usize::try_from(pixel.checked_div(scale).unwrap()).unwrap();
        let (letter, column) = (
            dot(x).checked_div(4).unwrap(),
            dot(x).checked_rem(4).unwrap(),
        );
        let rows = text.chars().nth(letter).and_then(|letter| {
            let known = FONT.iter().find(|(known, _)| *known == letter);
            known.map(|(_, rows)| rows)
        });
        let row = rows.and_then(|rows| rows.get(dot(y)));
        row.and_then(|row| row.as_bytes().get(column)) == Some(&b'#')
    }

    /// A bar 200 long and `height` high inside a black border one pixel
    /// thick, its first `filled` columns filled, with `text` of three
    /// characters in white at its center, `scale` pixels to the dot.
    fn labeled(filled: u32, height: u32, text: &'static str, scale: u32) -> Image {
        let left = 200_u32
            .saturating_sub(scale.saturating_mul(11))
            .checked_div(2)
            .unwrap();
        let top = height
            .saturating_sub(scale.saturating_mul(5))
            .checked_div(2)
            .unwrap();
        picture(200, height, move |x, y| {
            let inside = x.checked_sub(left).zip(y.checked_sub(top));
            match (x, y) {
                (0 | 199, _) | (_, 0) => BLACK,
                _ if y.saturating_add(1) == height => BLACK,
                _ if inside.is_some_and(|(x, y)| glyph(text, scale, x, y)) => WHITE,
                _ if x < filled => GREEN,
                _ => GRAY,
            }
        })
    }

    /// `image` with each pixel beside a white one blended half into white,
    /// as an anti-aliased label is.
    fn haloed(image: &Image) -> Image {
        let size = image.size();
        let white = |x: Option<u32>, y: Option<u32>| {
            x.zip(y).and_then(|(x, y)| image.pixel(x, y)) == Some(WHITE)
        };
        picture(size.width, size.height, |x, y| {
            let here = image.pixel(x, y).unwrap();
            let beside = white(x.checked_sub(1), Some(y))
                || white(x.checked_add(1), Some(y))
                || white(Some(x), y.checked_sub(1))
                || white(Some(x), y.checked_add(1));
            if here != WHITE && beside {
                mix(here, WHITE, 500)
            } else {
                here
            }
        })
    }

    /// The fills and the labels that say them.
    const LABELS: [(u32, &str, f64); 3] =
        [(50, "25%", 25.0), (100, "50%", 50.0), (150, "75%", 75.0)];

    #[test]
    fn a_bar_is_found_around_a_label_whatever_it_says() {
        // A label 10 high on a bar 20 high, 15 on 24, and 10 on 16.
        for (height, scale) in [(20, 2), (24, 3), (16, 2)] {
            for (filled, text, fill) in LABELS {
                let image = labeled(filled, height, text, scale);
                let proposal = detect_bar(&image, &[]);
                assert!(proposal.is_ok(), "{text} on {height}: {proposal:?}");
                let bar = proposal.unwrap().bar;
                assert_eq!(bar, BarFill::new(GREEN, GRAY).unwrap());
                let reads = bar.percent(&image).unwrap().raw();
                assert!(
                    (reads - fill).abs() < 2.0,
                    "{text} on {height} reads {reads}"
                );
            }
        }
        // The same bar is found with or without going around its label,
        // by where the label's dots fall.
        let around = |text| detect_bar(&labeled(100, 24, text, 3), &[]).unwrap().around;
        assert!(!around("25%"));
        assert!(around("50%"));
    }

    #[test]
    fn a_bar_is_found_around_an_anti_aliased_label() {
        for (filled, text, fill) in LABELS {
            let image = haloed(&labeled(filled, 20, text, 2));
            let proposal = detect_bar(&image, &[]);
            assert!(proposal.is_ok(), "{text}: {proposal:?}");
            let bar = proposal.unwrap().bar;
            assert_eq!(bar, BarFill::new(GREEN, GRAY).unwrap());
            let reads = bar.percent(&image).unwrap().raw();
            assert!((reads - fill).abs() < 2.0, "{text} reads {reads}");
        }
    }

    #[test]
    fn a_bar_is_found_around_an_icon_at_its_center() {
        let icon = |side: u32, filled: u32| {
            let from = 100_u32.saturating_sub(side.checked_div(2).unwrap());
            let top = 10_u32.saturating_sub(side.checked_div(2).unwrap());
            picture(200, 20, move |x, y| {
                match (x.checked_sub(from), y.checked_sub(top)) {
                    (Some(x), Some(y)) if x < side && y < side => WHITE,
                    _ if x < filled => GREEN,
                    _ => GRAY,
                }
            })
        };
        for (side, filled, fill) in [(8, 50, 25.0), (12, 50, 25.0), (12, 150, 75.0)] {
            let image = icon(side, filled);
            let proposal = detect_bar(&image, &[]);
            assert!(proposal.is_ok(), "{side} at {filled}: {proposal:?}");
            let reads = proposal.unwrap().bar.percent(&image).unwrap().raw();
            assert!(
                (reads - fill).abs() < 2.0,
                "{side} at {filled} reads {reads}"
            );
        }
    }

    #[test]
    fn a_label_too_tall_for_its_bar_leaves_too_few_lanes() {
        // A label 10 high on a bar 12 high, in its border, covers every
        // lane of the bar at some column.
        assert_eq!(
            detect_bar(&labeled(100, 12, "50%", 2), &[]),
            Err(NoBar::NotABar)
        );
        // Two lanes of a bar 18 high stay free at each side of a label 10
        // high, which is a fifth of the 20 lanes with the border's.
        let free = |lanes: u32| {
            picture(200, 20, move |x, y| match (x, y) {
                (0 | 199, _) | (_, 0 | 19) => BLACK,
                (60..140, _)
                    if y > lanes && y < 19_u32.saturating_sub(lanes) && x.is_multiple_of(2) =>
                {
                    WHITE
                },
                (0..100, _) => GREEN,
                _ => GRAY,
            })
        };
        assert_eq!(detect_bar(&free(2), &[]), Ok(around()));
        assert_eq!(detect_bar(&free(1), &[]), Err(NoBar::NotABar));
    }

    /// `image`, 200 by 10, under a white block over most of its lanes.
    fn under_a_block(image: &Image) -> Image {
        picture(200, 10, |x, y| match (x, y) {
            (90..110, 2..8) => WHITE,
            _ => image.pixel(x, y).unwrap(),
        })
    }

    #[test]
    fn what_touches_every_lane_is_refused_around_a_label_too() {
        // The block alone is looked around.
        let plain = under_a_block(&bar(200, 10, 50));
        assert_eq!(detect_bar(&plain, &[]), Ok(around()));
        let dark = Rgb {
            red: 20,
            green: 150,
            blue: 40,
        };
        let striped = picture(200, 10, |x, y| {
            match x.saturating_add(y).checked_div(4).unwrap() {
                _ if x >= 50 => GRAY,
                stripe if stripe.is_multiple_of(2) => GREEN,
                _ => dark,
            }
        });
        let segmented = picture(200, 10, |x, _| match x.checked_div(20).unwrap() {
            _ if x.checked_rem(20).unwrap() >= 18 => BLACK,
            segment if segment < 5 => GREEN,
            _ => GRAY,
        });
        let from = Rgb {
            red: 0,
            green: 90,
            blue: 0,
        };
        let to = Rgb {
            red: 80,
            green: 255,
            blue: 80,
        };
        let trail = picture(200, 10, |x, _| match x {
            0..60 => GREEN,
            60..100 => WHITE,
            _ => GRAY,
        });
        for (name, image) in [
            ("striped", striped),
            ("segmented", segmented),
            ("fading", fading(from, to, 100)),
            ("with a trail", trail),
        ] {
            assert_eq!(clear_lanes(&[(0, &image)]), [false; 0], "{name}");
            assert_eq!(
                detect_bar(&under_a_block(&image), &[]),
                Err(NoBar::NotABar),
                "{name}"
            );
        }
        // A bar that changes color is found around the block on the first
        // screenshot, and its colors do not read the second.
        assert_eq!(
            detect_bar(&under_a_block(&health(40)), &[under_a_block(&health(160))]),
            Err(NoBar::Differs {
                found_on: 0,
                image: 1
            })
        );
        // So is a fill that fades a little, once more of it shows.
        let mild = |filled| {
            let from = Rgb {
                red: 30,
                green: 180,
                blue: 50,
            };
            let to = Rgb {
                red: 60,
                green: 240,
                blue: 80,
            };
            under_a_block(&fading(from, to, filled))
        };
        assert_eq!(
            detect_bar(&mild(40), &[mild(160)]),
            Err(NoBar::Differs {
                found_on: 0,
                image: 1
            })
        );
        // The colors beside the bar are still held against the proposal.
        let beside = picture(200, 30, |x, y| match (x, y) {
            (90..110, 12..16) => WHITE,
            (0..90, 8..22) => GREEN,
            _ => GRAY,
        });
        assert_eq!(detect_bar(&beside, &[]), Err(NoBar::Misread));
    }

    #[test]
    fn two_screenshots_with_different_labels_are_both_read() {
        let [first, second] = [labeled(50, 24, "25%", 3), labeled(100, 24, "50%", 3)];
        let proposal = detect_bar(&first, std::slice::from_ref(&second)).unwrap();
        assert_eq!(proposal.bar, BarFill::new(GREEN, GRAY).unwrap());
        assert_eq!(proposal.shown_on, [0, 1]);
        assert!(proposal.moves);
        assert!(proposal.around, "the second label hides the layout");
        for (image, fill) in [(&first, 25.0), (&second, 50.0)] {
            let reads = proposal.bar.percent(image).unwrap().raw();
            assert!((reads - fill).abs() < 2.0, "reads {reads}, drawn at {fill}");
        }
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
        assert_eq!(part(100), 5);
        assert_eq!(part(59), 3);
        assert_eq!(part(12), 3);
        assert!(covers(85, 100, COVER));
        assert!(!covers(84, 100, COVER));
    }

    #[test]
    fn a_column_belongs_to_a_part_within_sixteen_of_its_color() {
        let bar = BarFill::new(red(0), red(100)).unwrap();
        assert_eq!(belongs(bar, red(16)), Some(BarPart::Filled));
        assert_eq!(belongs(bar, red(17)), None);
        assert_eq!(belongs(bar, red(84)), Some(BarPart::Empty));
        assert_eq!(belongs(bar, red(83)), None);
    }

    #[test]
    fn columns_lay_out_as_a_full_an_empty_or_a_partly_filled_bar() {
        let bar = BarFill::new(GREEN, GRAY).unwrap();
        let columns = |filled: usize, empty: usize| {
            let mut columns = vec![WHITE];
            columns.extend(iter::repeat_n(GREEN, filled));
            columns.extend(iter::repeat_n(GRAY, empty));
            columns.push(WHITE);
            columns
        };
        assert_eq!(laid_out(&columns(18, 0), bar), Some(Shown::Filled));
        assert_eq!(laid_out(&columns(0, 18), bar), Some(Shown::Empty));
        let both = Shown::Both {
            filled: 6,
            empty: 12,
        };
        assert_eq!(laid_out(&columns(6, 12), bar), Some(both));
        assert_eq!(laid_out(&columns(6, 12), bar.turned()), None);
        assert_eq!(laid_out(&[WHITE, WHITE], bar), None);
        assert!((both.fill() - 100.0 / 3.0).abs() < 1e-9);
        assert!((Shown::Filled.fill() - 100.0).abs() < 1e-9);
        assert!(Shown::Empty.fill().abs() < 1e-9);
    }

    #[test]
    fn a_lane_is_clear_when_it_shows_the_bar_from_end_to_end_on_its_own() {
        // Lanes: a border, the bar, the bar under a label, a full bar, and
        // the bar with a soft edge and a border at each end.
        let image = picture(100, 5, |x, y| match (x, y) {
            (_, 0) | (0 | 99, 4) => BLACK,
            (45..55, 2) => WHITE,
            (40 | 41, 4) => red(90),
            (_, 3) | (0..40, _) => GREEN,
            _ => GRAY,
        });
        let other = picture(100, 5, |_, _| BLACK);
        assert_eq!(
            clear_lanes(&[(0, &other), (1, &image)]),
            [false, true, false, false, true],
            "an image with no clear lane has no say"
        );
        assert_eq!(clear_lanes(&[(0, &other)]), [false; 0]);
        // Only the lanes that are clear on every image with enough of them.
        let lower = picture(100, 5, |x, y| match (x, y) {
            (45..55, 0 | 1) => WHITE,
            (0..40, _) => GREEN,
            _ => GRAY,
        });
        assert_eq!(
            clear_lanes(&[(0, &image), (1, &lower)]),
            [false, false, false, false, true]
        );
        // Too few when none is clear on both.
        let upper = picture(100, 5, |x, y| match (x, y) {
            (45..55, 1 | 4) => WHITE,
            (0..40, _) => GREEN,
            _ => GRAY,
        });
        assert_eq!(clear_lanes(&[(0, &image), (1, &upper)]), [false; 0]);
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
                "the region is not one plain color and then another along its long side",
            ),
            (
                NoBar::Misread,
                "the two colors read the region other than its layout looks",
            ),
            (
                NoBar::Differs {
                    found_on: 2,
                    image: 0,
                },
                "the colors of the bar on screenshot 2 do not read screenshot 0",
            ),
        ];
        for (reason, text) in reasons {
            assert_eq!(reason.to_string(), text);
        }
    }

    proptest! {
        /// A bar drawn at any two fills that show both parts gets colors
        /// with which the reader reads each fill, to the column.
        #[test]
        fn the_reader_reads_a_detected_bar_at_the_fills_it_was_drawn_at(
            length in 20_u32..160,
            lanes in 1_u32..12,
            fills in [0_u32..1000, 0_u32..1000],
            tall in any::<bool>(),
            filled in any::<[u8; 3]>(),
            empty in any::<[u8; 3]>(),
            noise in proptest::collection::vec(0_u8..=4, 64),
        ) {
            let [filled, empty] = [filled, empty].map(|[red, green, blue]| Rgb { red, green, blue });
            // Far enough apart that the noise below cannot bring the two
            // colors within the distance they need: 48 squared.
            prop_assume!(distance(filled, empty) >= 2304);
            let least = u32::try_from(part(usize::try_from(length).unwrap())).unwrap();
            let most = length.saturating_sub(least);
            // Each pixel is up to 2 off its color in every channel.
            let noisy = |color: Rgb, x: u32, y: u32| {
                let at = usize::try_from(x.wrapping_mul(7).wrapping_add(y.wrapping_mul(13))).unwrap();
                let shift = |channel: u8, turn: usize| {
                    let by = noise[at.wrapping_add(turn).checked_rem(noise.len()).unwrap()];
                    channel.saturating_add(by).saturating_sub(2)
                };
                Rgb { red: shift(color.red, 0), green: shift(color.green, 1), blue: shift(color.blue, 2) }
            };
            // `fill` is in thousandths of the bar.
            let drawn = |fill: u32| {
                let columns = length.saturating_mul(fill).checked_div(1000).unwrap().clamp(least, most);
                let image = if tall {
                    picture(lanes, length, |x, y| {
                        let part = if length.saturating_sub(y) <= columns { filled } else { empty };
                        noisy(part, x, y)
                    })
                } else {
                    picture(length, lanes, |x, y| noisy(if x < columns { filled } else { empty }, x, y))
                };
                (image, f64::from(columns) * 100.0 / f64::from(length))
            };
            let [(first, first_fill), (second, second_fill)] = fills.map(drawn);
            let proposal = detect_bar(&first, std::slice::from_ref(&second));
            prop_assert!(proposal.is_ok(), "{proposal:?}");
            let proposal = proposal.unwrap();
            prop_assert_eq!(&proposal.shown_on, &vec![0, 1]);
            // Squared distances: the noise moves a color by at most 2 in each channel.
            prop_assert!(distance(proposal.bar.filled(), filled) <= 12);
            prop_assert!(distance(proposal.bar.empty(), empty) <= 12);
            for (image, fill) in [(&first, first_fill), (&second, second_fill)] {
                let reads = proposal.bar.percent(image).unwrap().raw();
                prop_assert!((reads - fill).abs() < 1e-9, "reads {reads}, drawn at {fill}");
            }
        }
    }
}
