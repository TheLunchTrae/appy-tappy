//! The colors of an image, each a layer that may hold its text.
//!
//! An application draws one piece of text in a flat color, and may draw
//! the same text in another color a moment later: white while it counts,
//! gold when it is done. So a reader keeps no color. It splits each image
//! into its few colors and asks of each in turn whether its pixels are
//! characters it knows.
//!
//! 1. **Colors.** The image's pixels are counted by color, to the nearest 16
//!    of red, green, and blue. The most common color is the first, and each
//!    next most common one more than [`SAME_COLOR`] from every color so far
//!    is another, up to [`MAX_COLORS`]. A pixel has the color nearest to
//!    its own.
//!
//!    The blend along an anti-aliased edge is no color of its own: a color
//!    that lies between two more common ones, in pixels with others of
//!    their color on few sides, is left out. So each pixel of an edge goes
//!    to the side it is mostly of, and a stroke is as wide in its layer at
//!    any size and at any fraction of a pixel.
//! 2. **Background.** The color most of the image's outermost pixels have
//!    is the background. Its pixels are ink only where pixels of other
//!    colors close them in, as an outline does, so white text with an
//!    outline reads over a white background. What closes them in has to
//!    reach the background itself: the inside of such a text's `0` is
//!    closed in by the outline around its hole, which only the `0`
//!    touches, and is no ink.
//! 3. **Layers.** Every color is a layer: its pixels, but for the image's
//!    outermost ones. One more layer holds the pixels of every color but
//!    the background's, for text that is no one color, such as a gradient.

use std::ops::Range;

use super::super::{Image, Rgb, Size};
use super::ink::{Ink, index};

/// The most colors an image is split into.
const MAX_COLORS: usize = 12;

/// Two colors whose red, green, and blue are each no further apart than
/// this are one color.
const SAME_COLOR: u8 = 40;

/// The bits of each of red, green, and blue that colors are counted by.
const COUNTED_BITS: u32 = 4;

/// How many colors there are to count.
const COUNTED: usize = 1 << (COUNTED_BITS * 3);

/// How many times the pixels of a color that have its color on every side
/// its pixels may be, and the color still be the blend along an edge when
/// it lies between two others. A stroke three pixels wide has more.
const THIN: u32 = 8;

/// How much red, green, or blue must differ between two colors to say
/// how far between them a third lies.
const TELLING: u8 = 48;

/// How far past either of two colors, or apart from channel to channel, a
/// color may lie and still be a blend of the two, in 256ths of the way
/// from one to the other.
const BLEND_SLACK: i32 = 64;

/// How far red, green, and blue may each be from a layer's color in a
/// pixel that is all the layer's.
const PURE: u8 = 6;

/// How many pixels around a pixel on an edge are looked at for the color
/// the layer meets there.
const MEETS: u32 = 2;

/// One layer of an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Layer {
    /// The pixels of one color, by its place among the image's colors.
    Color(u8),
    /// The pixels of every color but the background's.
    Every,
}

/// An image split into its colors.
#[derive(Debug)]
pub(super) struct Layers {
    size: Size,
    /// Each pixel's own color, row by row from the top.
    pixels: Vec<Rgb>,
    /// Each pixel's color, as its place among `colors`, row by row from
    /// the top.
    color: Vec<u8>,
    /// Whether each pixel reaches the image's outermost pixels through
    /// pixels of its own color.
    open: Vec<bool>,
    /// Which color's layer each pixel is ink of, or [`NO_LAYER`] for a
    /// pixel that is ink of none: one of the image's outermost, or
    /// background that reaches them. Of the background's color it holds
    /// every pixel other colors close in, which [`Layers::ink`] narrows.
    inked: Vec<u8>,
    /// The image's colors, the most common first.
    colors: Vec<Rgb>,
    /// The background's place among `colors`.
    background: u8,
}

/// The layer of a pixel that is ink of no layer. An image has fewer colors.
const NO_LAYER: u8 = u8::MAX;
const _: () = assert!(MAX_COLORS < NO_LAYER as usize);

impl Layers {
    /// Splits `image` into its colors.
    pub(super) fn new(image: &Image) -> Self {
        let size = image.size();
        let width = index(size.width).max(1);
        let pixels: Vec<Rgb> = image
            .pixels()
            .iter()
            .map(|&pixel| Rgb::of_pixel(pixel))
            .collect();
        let counted: Vec<u16> = pixels.iter().map(|&pixel| counted(pixel)).collect();
        let mut counts = vec![0_u32; COUNTED];
        // The red, green, and blue of each counted color's pixels, summed.
        let mut sums = vec![[0_u32; 3]; COUNTED];
        for (&pixel, &counted) in pixels.iter().zip(&counted) {
            if let Some(sums) = sums.get_mut(usize::from(counted)) {
                for (sum, channel) in sums.iter_mut().zip([pixel.red, pixel.green, pixel.blue]) {
                    *sum = sum.saturating_add(u32::from(channel));
                }
            }
        }
        // How many pixels of each counted color have it on every side.
        let mut amid = vec![0_u32; COUNTED];
        let rows = || counted.chunks(width);
        let blank: &[u16] = &[];
        let above = std::iter::once(blank).chain(rows());
        let below = rows().skip(1).chain(std::iter::once(blank));
        for ((row, above), below) in rows().zip(above).zip(below) {
            for (x, &color) in row.iter().enumerate() {
                let place = usize::from(color);
                if let Some(count) = counts.get_mut(place) {
                    *count = count.saturating_add(1);
                }
                let beside =
                    |row: &[u16], x: Option<usize>| x.and_then(|x| row.get(x)) == Some(&color);
                if beside(row, x.checked_sub(1))
                    && beside(row, x.checked_add(1))
                    && beside(above, Some(x))
                    && beside(below, Some(x))
                    && let Some(count) = amid.get_mut(place)
                {
                    *count = count.saturating_add(1);
                }
            }
        }
        // The mean color of the pixels counted as `counted`: a flat color
        // itself, where the counted color holds one.
        let mean = |counted: usize| {
            let count = counts.get(counted).copied().unwrap_or(0);
            let channel =
                |sum: u32| u8::try_from(sum.checked_div(count).unwrap_or(0)).unwrap_or(u8::MAX);
            let [red, green, blue] = sums.get(counted).copied().unwrap_or_default();
            Rgb {
                red: channel(red),
                green: channel(green),
                blue: channel(blue),
            }
        };
        let mut common: Vec<(u32, usize)> = counts
            .iter()
            .enumerate()
            .filter(|&(_, &count)| count > 0)
            .map(|(color, &count)| (count, color))
            .collect();
        common.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        // The counted color most of the outermost pixels have. It is what
        // lies behind the image and no blend, however thin: a region as
        // large as a panel is given with one pixel of the window around it.
        let mut around = vec![0_u32; COUNTED];
        for at in outermost(size) {
            if let Some(count) = counted
                .get(at)
                .and_then(|&color| around.get_mut(usize::from(color)))
            {
                *count = count.saturating_add(1);
            }
        }
        let behind = (0..COUNTED)
            .zip(&around)
            .max_by_key(|&(color, &count)| (count, std::cmp::Reverse(color)))
            .map(|(color, _)| color);
        // The most common colors, each with whether it is thin.
        let mut found: Vec<(Rgb, bool)> = Vec::new();
        for &(count, counted) in &common {
            let color = mean(counted);
            let thin = Some(counted) != behind
                && amid
                    .get(counted)
                    .is_some_and(|&amid| amid.saturating_mul(THIN) < count);
            if found.len() < MAX_COLORS.saturating_mul(2)
                && !found.iter().any(|&(one, _)| same(one, color))
            {
                found.push((color, thin));
            }
        }
        // A thin color between two others is the blend along their edge,
        // whichever of the three is the most common: an outline less than
        // two pixels wide has fewer pixels of its own color than of its
        // blends. The least common goes first.
        for place in (0..found.len()).rev() {
            let Some(&(color, thin)) = found.get(place) else {
                continue;
            };
            let others = || {
                found
                    .iter()
                    .enumerate()
                    .filter(move |&(other, _)| other != place)
                    .map(|(_, &(other, _))| other)
            };
            let blend = thin
                && others().enumerate().any(|(skip, one)| {
                    others()
                        .skip(skip.saturating_add(1))
                        .any(|other| between(color, one, other))
                });
            if blend {
                found.remove(place);
            }
        }
        let colors: Vec<Rgb> = found
            .into_iter()
            .map(|(color, _)| color)
            .take(MAX_COLORS)
            .collect();
        // The place of the color nearest each counted color, found once
        // for each that the image has.
        let mut nearest = vec![0_u8; COUNTED];
        for &(_, color) in &common {
            let pixel = mean(color);
            // A blend of two colors goes to the nearer of the two, and not
            // to a third that happens to lie nearer than either: the gray
            // between a white outline and a dark background is no part of
            // red text.
            let blends = |one: Rgb| {
                colors
                    .iter()
                    .any(|&other| other != one && between(pixel, one, other))
            };
            let nearest_of = |blended: bool| {
                (0..=u8::MAX)
                    .zip(&colors)
                    .filter(|&(_, &one)| !blended || blends(one))
                    .min_by_key(|&(_, &one)| apart(one, pixel))
                    .map(|(place, _)| place)
            };
            let place = nearest_of(true).or_else(|| nearest_of(false)).unwrap_or(0);
            if let Some(slot) = nearest.get_mut(color) {
                *slot = place;
            }
        }
        let color: Vec<u8> = counted
            .iter()
            .map(|&counted| nearest.get(usize::from(counted)).copied().unwrap_or(0))
            .collect();
        let mut edge = [0_u32; MAX_COLORS];
        for at in outermost(size) {
            if let Some(count) = color
                .get(at)
                .and_then(|&place| edge.get_mut(usize::from(place)))
            {
                *count = count.saturating_add(1);
            }
        }
        // Of two colors with as many of the outermost pixels, the more
        // common in the image.
        let background = (0..=u8::MAX)
            .zip(edge)
            .max_by_key(|&(place, count)| (count, std::cmp::Reverse(place)))
            .map_or(0, |(place, _)| place);
        let last = (
            width.saturating_sub(1),
            index(size.height).saturating_sub(1),
        );
        let open = open(size, &color);
        let mut inked = Vec::with_capacity(color.len());
        for (y, (colors, open)) in color.chunks(width).zip(open.chunks(width)).enumerate() {
            for (x, (&color, &open)) in colors.iter().zip(open).enumerate() {
                let outermost = x == 0 || y == 0 || x == last.0 || y == last.1;
                inked.push(if outermost || (color == background && open) {
                    NO_LAYER
                } else {
                    color
                });
            }
        }
        Self {
            size,
            pixels,
            color,
            open,
            inked,
            colors,
            background,
        }
    }

    /// The layers that hold ink, each with how many pixels of it, in the
    /// order a read asks them: the colors from the one with the most ink,
    /// the more common of two with as much, and then [`Layer::Every`]. The
    /// background's color is counted with every pixel of it that other
    /// colors close in, which is no fewer than its ink.
    pub(super) fn all(&self) -> Vec<(Layer, u32)> {
        let mut held = [0_u32; MAX_COLORS];
        for &layer in &self.inked {
            if let Some(count) = held.get_mut(usize::from(layer)) {
                *count = count.saturating_add(1);
            }
        }
        let mut colors: Vec<(u32, u8)> = (0..=u8::MAX)
            .zip(held)
            .take(self.colors.len())
            .filter(|&(_, count)| count > 0)
            .map(|(place, count)| (count, place))
            .collect();
        colors.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let every = colors
            .iter()
            .filter(|&&(_, place)| place != self.background)
            .map(|&(count, _)| count)
            .fold(0, u32::saturating_add);
        let mut all: Vec<(Layer, u32)> = colors
            .into_iter()
            .map(|(count, place)| (Layer::Color(place), count))
            .collect();
        if every > 0 {
            all.push((Layer::Every, every));
        }
        all
    }

    /// The ink of `layer`.
    pub(super) fn ink(&self, layer: Layer) -> Ink {
        let background = self.background;
        let inked = match layer {
            Layer::Color(place) if place == background => closed_in(
                index(self.size.width).max(1),
                &self.color,
                &self.open,
                background,
            ),
            Layer::Color(place) => self.inked.iter().map(|&layer| layer == place).collect(),
            Layer::Every => self
                .inked
                .iter()
                .map(|&layer| layer != NO_LAYER && layer != background)
                .collect(),
        };
        Ink::new(inked, self.size)
    }

    /// The color of `layer`, or `None` for [`Layer::Every`], which has
    /// none.
    pub(super) fn color(&self, layer: Layer) -> Option<Rgb> {
        match layer {
            Layer::Color(place) => self.colors.get(usize::from(place)).copied(),
            Layer::Every => None,
        }
    }

    /// How much of the pixel at (`x`, `y`) is `layer`'s, from none of it at
    /// 0 to all of it at 255, where `ink` is the layer's ink. A pixel on
    /// an anti-aliased edge is a blend of the layer's color and what the
    /// layer meets there, among the colors within [`MEETS`] pixels: the
    /// background's and, where one stands apart from both, a third, such
    /// as an outline's. The pixel is the layer's by its share of the
    /// blend that comes closest to its color.
    ///
    /// Only ink and the pixels around it are any of the layer's, and so is
    /// no pixel outside the image.
    pub(super) fn share(&self, layer: Layer, ink: &Ink, x: u32, y: u32) -> u8 {
        let width = index(self.size.width);
        let column = index(x);
        if column >= width {
            return 0;
        }
        let rows = |reach: u32| y.saturating_sub(reach)..=y.saturating_add(reach);
        let columns = |reach: u32| {
            let reach = index(reach);
            column.saturating_sub(reach)..column.saturating_add(reach).saturating_add(1).min(width)
        };
        let beside = ink.at(x, y)
            || rows(1).any(|y| {
                ink.row(y)
                    .get(columns(1))
                    .is_some_and(|inked| inked.contains(&true))
            });
        if !beside {
            return 0;
        }
        // The pixels of row `y` in `columns`, or none outside the image.
        let pixels = |y: u32, columns: Range<usize>| {
            index(y)
                .checked_mul(width)
                .and_then(|start| {
                    let from = start.checked_add(columns.start)?;
                    self.pixels.get(from..start.checked_add(columns.end)?)
                })
                .unwrap_or_default()
        };
        let at = index(y).saturating_mul(width).saturating_add(column);
        let (Some(&pixel), Some(&background)) = (
            self.pixels.get(at),
            self.colors.get(usize::from(self.background)),
        ) else {
            return 0;
        };
        let around = || rows(MEETS).flat_map(|y| pixels(y, columns(MEETS))).copied();
        let place = match layer {
            Layer::Color(place) => place,
            Layer::Every => {
                // A pixel of another color than the background's is the
                // text's by how far it lies from the background toward
                // its color, and one of the background's color by how far
                // toward the color furthest from it nearby.
                if pixel.near(background, PURE) {
                    return 0;
                }
                let nearest = self
                    .color
                    .get(at)
                    .filter(|&&place| place != self.background)
                    .and_then(|&place| self.colors.get(usize::from(place)));
                return nearest.map_or_else(
                    || {
                        let met = around().max_by_key(|&met| squared(met, background));
                        let kept = met.map_or(u8::MAX, |met| part(pixel, background, met));
                        u8::MAX.saturating_sub(kept)
                    },
                    |&nearest| part(pixel, nearest, background),
                );
            },
        };
        let Some(&own) = self.colors.get(usize::from(place)) else {
            return 0;
        };
        if pixel.near(own, PURE) {
            return u8::MAX;
        }
        // All of another color.
        let other = self
            .color
            .get(at)
            .and_then(|&place| self.colors.get(usize::from(place)));
        if other.is_some_and(|&other| pixel.near(other, PURE)) {
            return 0;
        }
        unmixed_near(pixel, own, background, around)
    }

    /// Whether `layer` holds the background's color: the pixels of it
    /// that other colors close in.
    pub(super) fn is_background(&self, layer: Layer) -> bool {
        layer == Layer::Color(self.background)
    }

    /// The background's color.
    pub(super) fn background(&self) -> Option<Rgb> {
        self.colors.get(usize::from(self.background)).copied()
    }

    /// The pixels of every color but that of `own` that pixels of other
    /// colors close in: what else the image shows that could be a
    /// character, such as text in another color. A color that reaches the
    /// image's edge is the scene behind the text, and none of its pixels
    /// there count.
    pub(super) fn others(&self, own: Layer) -> Ink {
        let inked = self
            .color
            .iter()
            .zip(&self.open)
            .map(|(&color, &open)| !open && Layer::Color(color) != own)
            .collect();
        Ink::new(inked, self.size)
    }
}

/// Whether two colors are one color to a reader.
pub(super) fn same(one: Rgb, other: Rgb) -> bool {
    one.near(other, SAME_COLOR)
}

/// Which of the counted colors `color` is.
fn counted(color: Rgb) -> u16 {
    let kept = |channel: u8| u16::from(channel >> (u8::BITS.saturating_sub(COUNTED_BITS)));
    (kept(color.red) << (COUNTED_BITS.saturating_mul(2)))
        | (kept(color.green) << COUNTED_BITS)
        | kept(color.blue)
}

/// Red, green, and blue of `color` less those of `from`.
fn less(color: Rgb, from: Rgb) -> [i32; 3] {
    [
        i32::from(color.red).saturating_sub(i32::from(from.red)),
        i32::from(color.green).saturating_sub(i32::from(from.green)),
        i32::from(color.blue).saturating_sub(i32::from(from.blue)),
    ]
}

/// The sum over red, green, and blue of the products of `a` and `b`.
fn dot(a: [i32; 3], b: [i32; 3]) -> i32 {
    a.into_iter()
        .zip(b)
        .map(|(a, b)| a.saturating_mul(b))
        .fold(0, i32::saturating_add)
}

/// How far apart two colors are, squared.
fn squared(one: Rgb, other: Rgb) -> i32 {
    let apart = less(one, other);
    dot(apart, apart)
}

/// How much of `pixel` is `own`, from 0 to 255, when it is a blend of
/// `own` and `met`: how far it lies from `met` toward `own`. Where `met`
/// is as good as `own`, nothing else is there, and the pixel is all `own`.
fn part(pixel: Rgb, own: Rgb, met: Rgb) -> u8 {
    let toward = less(own, met);
    let whole = dot(toward, toward);
    if whole < i32::from(TELLING).saturating_mul(i32::from(TELLING)) {
        return u8::MAX;
    }
    let part = dot(less(pixel, met), toward)
        .saturating_mul(i32::from(u8::MAX))
        .checked_div(whole)
        .unwrap_or(0);
    u8::try_from(part.clamp(0, i32::from(u8::MAX))).unwrap_or(u8::MAX)
}

/// How much of `pixel` is `own`, from 0 to 255, among the colors `around`
/// it: one of three colors where a third stands apart from `own` and the
/// `background`, as an outline's does, and otherwise one of two, `own`
/// and the color furthest from it.
fn unmixed_near<Around>(pixel: Rgb, own: Rgb, background: Rgb, around: impl Fn() -> Around) -> u8
where
    Around: Iterator<Item = Rgb>,
{
    let along = less(own, background);
    let whole = i64::from(dot(along, along));
    // How far a color lies off the line through `own` and the
    // background: its distance from the line, squared, times `whole`.
    let off_line = |color: Rgb| {
        let to = less(color, background);
        let across = i64::from(dot(along, to));
        i64::from(dot(to, to))
            .saturating_mul(whole)
            .saturating_sub(across.saturating_mul(across))
    };
    around()
        .max_by_key(|&met| off_line(met))
        .and_then(|met| unmixed(pixel, own, met, background))
        .unwrap_or_else(|| {
            around()
                .max_by_key(|&met| squared(met, own))
                .map_or(u8::MAX, |met| part(pixel, own, met))
        })
}

/// How much of `pixel` is `own`, from 0 to 255, when it is a blend of
/// `own`, `met`, and `background`: the three shares that come closest to
/// its color. `None` where `met` lies too near the line through the
/// other two to tell three colors apart.
fn unmixed(pixel: Rgb, own: Rgb, met: Rgb, background: Rgb) -> Option<u8> {
    let (own, met, pixel) = (
        less(own, background),
        less(met, background),
        less(pixel, background),
    );
    let wide = |a: [i32; 3], b: [i32; 3]| i64::from(dot(a, b));
    let (whole, across) = (wide(own, own), wide(own, met));
    let spread = wide(met, met)
        .saturating_mul(whole)
        .saturating_sub(across.saturating_mul(across));
    let telling = i64::from(TELLING).saturating_mul(i64::from(TELLING));
    if spread < telling.saturating_mul(whole) || whole < telling {
        return None;
    }
    let part = wide(own, pixel)
        .saturating_mul(wide(met, met))
        .saturating_sub(wide(met, pixel).saturating_mul(across))
        .saturating_mul(i64::from(u8::MAX))
        .checked_div(spread)?;
    Some(u8::try_from(part.clamp(0, i64::from(u8::MAX))).unwrap_or(u8::MAX))
}

/// Whether `color` is a blend of `one` and `other`: it lies about as far
/// from `one` toward `other` in each of red, green, and blue that tells
/// the two apart, no further than they are, and keeps what they share.
fn between(color: Rgb, one: Rgb, other: Rgb) -> bool {
    let channels = [
        (color.red, one.red, other.red),
        (color.green, one.green, other.green),
        (color.blue, one.blue, other.blue),
    ];
    let (mut least, mut most) = (i32::MAX, i32::MIN);
    for (color, one, other) in channels {
        if one.abs_diff(other) < TELLING {
            // A channel the two share says nothing of how far, but a
            // blend keeps it.
            if color.abs_diff(one) >= TELLING || color.abs_diff(other) >= TELLING {
                return false;
            }
            continue;
        }
        let (color, one, other) = (i32::from(color), i32::from(one), i32::from(other));
        // How far along, in 256ths.
        let along = color
            .saturating_sub(one)
            .saturating_mul(256)
            .checked_div(other.saturating_sub(one))
            .unwrap_or(0);
        least = least.min(along);
        most = most.max(along);
    }
    least >= BLEND_SLACK.saturating_neg()
        && most <= BLEND_SLACK.saturating_add(256)
        && most.saturating_sub(least) <= BLEND_SLACK
}

/// How far apart two colors are: the sum over red, green, and blue.
fn apart(one: Rgb, other: Rgb) -> u16 {
    [
        one.red.abs_diff(other.red),
        one.green.abs_diff(other.green),
        one.blue.abs_diff(other.blue),
    ]
    .into_iter()
    .map(u16::from)
    .sum()
}

/// The places of the outermost pixels of an image of `size`: its first and
/// last rows and columns.
fn outermost(size: Size) -> impl Iterator<Item = usize> {
    let (width, height) = (index(size.width), index(size.height));
    let last_row = height.saturating_sub(1).saturating_mul(width);
    let rows = (0..width).flat_map(move |x| [x, last_row.saturating_add(x)]);
    let columns = (0..height).flat_map(move |y| {
        let start = y.saturating_mul(width);
        [start, start.saturating_add(width.saturating_sub(1))]
    });
    rows.chain(columns)
}

/// Whether each pixel of an image of `size` reaches its outermost pixels
/// through pixels of its own `color`: up, down, left, or right.
fn open(size: Size, color: &[u8]) -> Vec<bool> {
    let mut open = vec![false; color.len()];
    let mut reached: Vec<usize> = Vec::new();
    for at in outermost(size) {
        if let Some(open) = open.get_mut(at)
            && !*open
        {
            *open = true;
            reached.push(at);
        }
    }
    spread(index(size.width).max(1), &mut open, reached, |from, to| {
        color.get(from) == color.get(to)
    });
    open
}

/// Which pixels of the `background` color are closed in by what reaches
/// the background that is `open`: the pixels of other colors that touch
/// what is open, or reach it through pixels of other colors, close in the
/// background-colored pixels they touch, and those the ones they reach
/// through their color. `color` holds each pixel's color in rows `width`
/// long.
fn closed_in(width: usize, color: &[u8], open: &[bool], background: u8) -> Vec<bool> {
    let other = |at: usize| color.get(at).is_some_and(|&color| color != background);
    let mut outside = vec![false; color.len()];
    let mut reached: Vec<usize> = Vec::new();
    for (at, _) in open.iter().enumerate().filter(|&(_, &open)| open) {
        visit(width, at, |next| {
            if other(next)
                && let Some(slot) = outside.get_mut(next)
                && !*slot
            {
                *slot = true;
                reached.push(next);
            }
        });
    }
    spread(width, &mut outside, reached, |_, to| other(to));
    let mut closed = vec![false; color.len()];
    let mut reached: Vec<usize> = Vec::new();
    for (at, _) in outside.iter().enumerate().filter(|&(_, &outside)| outside) {
        visit(width, at, |next| {
            if !other(next)
                && open.get(next) == Some(&false)
                && let Some(slot) = closed.get_mut(next)
                && !*slot
            {
                *slot = true;
                reached.push(next);
            }
        });
    }
    spread(width, &mut closed, reached, |_, to| !other(to));
    closed
}

/// Calls `visit` with `at` and with the places above, below, left, and
/// right of it in rows `width` long. One past the last row is no pixel,
/// which a lookup there says.
fn visit(width: usize, at: usize, mut visit: impl FnMut(usize)) {
    let column = at.rem_euclid(width);
    visit(at);
    if column > 0 {
        visit(at.saturating_sub(1));
    }
    if column.saturating_add(1) < width {
        visit(at.saturating_add(1));
    }
    if let Some(above) = at.checked_sub(width) {
        visit(above);
    }
    visit(at.saturating_add(width));
}

/// Marks in `held` every pixel that the `reached` ones reach, step by step
/// to a neighbor that `passes(from, to)`. The `reached` ones are held
/// already.
fn spread(
    width: usize,
    held: &mut [bool],
    mut reached: Vec<usize>,
    passes: impl Fn(usize, usize) -> bool,
) {
    while let Some(at) = reached.pop() {
        visit(width, at, |next| {
            if let Some(slot) = held.get_mut(next)
                && !*slot
                && passes(at, next)
            {
                *slot = true;
                reached.push(next);
            }
        });
    }
}
