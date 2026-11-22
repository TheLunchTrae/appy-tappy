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
//!    The blend along an anti-aliased edge is no color of its own. Where
//!    a color lies between two others, each piece of it, the pixels that
//!    touch one another, is a blend when it is thin and has both of those
//!    colors close by. A color that is blends and specks is left out. So each
//!    pixel of an edge goes to the side it is mostly of, and a stroke is
//!    as wide in its layer at any size and at any fraction of a pixel.
//!    Thin text of a color that happens to lie between two others stays
//!    text: the edge of white text on black passes through gray, and only
//!    the pieces along that edge are blends.
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

use std::ops::{BitOr, Range};

use super::super::{Image, Rgb, Size};
use super::ink::{Ink, index};
use super::shade;

/// The most colors an image is split into.
const MAX_COLORS: usize = 12;

/// Two colors whose red, green, and blue are each no further apart than
/// this are one color.
const SAME_COLOR: u8 = 40;

/// The bits of each of red, green, and blue that colors are counted by.
const COUNTED_BITS: u32 = 4;

/// How many colors there are to count.
const COUNTED: usize = 1 << (COUNTED_BITS * 3);

/// How many times the pixels of a piece of a color that have its color on
/// every side its pixels may be, and the piece still be the blend along an
/// edge when its color lies between two others. A stroke three pixels wide
/// has more.
const THIN: u32 = 8;

/// How much red, green, or blue must differ between two colors to say
/// how far between them a third lies.
const TELLING: u8 = 48;

/// How far past either of two colors, or apart from channel to channel, a
/// color may lie and still be a blend of the two, in parts of 256 of the
/// way from one to the other.
const BLEND_SLACK: i32 = 64;

/// How far red, green, and blue may each be from a layer's color in a
/// pixel that is all the layer's.
const PURE: u8 = 6;

/// How many pixels around a pixel on an edge are looked at for the color
/// the layer meets there.
const MEETS: u32 = 2;

/// How many pixels that touch one another the smallest piece of a color
/// has that can be a character: a point two pixels wide.
const SPECK: usize = 4;

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

/// The colors found in an image, before the blends along its edges are
/// told from the colors of their own.
struct Palette {
    /// The most common colors, each with whether it may be a blend.
    found: Vec<(Rgb, bool)>,
    /// Each pixel's place in `found`, row by row from the top, or
    /// [`NO_LAYER`] for a color past the last one found.
    placed: Vec<u8>,
}

impl Palette {
    /// The colors of `pixels`, an image of `size` row by row from the top.
    fn of(pixels: &[Rgb], size: Size) -> Self {
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
        for &color in &counted {
            if let Some(count) = counts.get_mut(usize::from(color)) {
                *count = count.saturating_add(1);
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
        // The most common colors, each with whether it may be a blend.
        let mut found: Vec<(Rgb, bool)> = Vec::new();
        for &(_, counted) in &common {
            let color = mean(counted);
            if found.len() < MAX_COLORS.saturating_mul(2)
                && !found.iter().any(|&(one, _)| same(one, color))
            {
                found.push((color, Some(counted) != behind));
            }
        }
        // The place in `found` of each counted color the image has, and of
        // each pixel.
        let mut spots = vec![NO_LAYER; COUNTED];
        for &(_, counted) in &common {
            let color = mean(counted);
            let place = found.iter().position(|&(one, _)| same(one, color));
            if let (Some(place), Some(slot)) = (place, spots.get_mut(counted)) {
                *slot = u8::try_from(place).unwrap_or(NO_LAYER);
            }
        }
        let placed: Vec<u8> = counted
            .iter()
            .map(|&counted| spots.get(usize::from(counted)).copied().unwrap_or(NO_LAYER))
            .collect();
        Self { found, placed }
    }

    /// The blends among the colors, in rows `width` long.
    fn blends(&self, width: usize) -> Blends {
        blends(&self.found, &self.placed, width)
    }
}

/// The layer of a pixel that is ink of no layer. An image has fewer colors.
const NO_LAYER: u8 = u8::MAX;
const _: () = assert!(MAX_COLORS < NO_LAYER as usize);

impl Layers {
    /// Splits `image` into its colors, as it shows without a shade over
    /// a part of it ([`shade::lit`]).
    pub(super) fn new(image: &Image) -> Self {
        let size = image.size();
        let width = index(size.width).max(1);
        let mut pixels: Vec<Rgb> = image
            .pixels()
            .iter()
            .map(|&pixel| Rgb::of_pixel(pixel))
            .collect();
        let mut palette = Palette::of(&pixels, size);
        let mut blends = palette.blends(width);
        let colors: Vec<Rgb> = palette.found.iter().map(|&(color, _)| color).collect();
        // The color each pixel is all of, where it is no blend.
        let own: Vec<Option<u8>> = palette
            .placed
            .iter()
            .zip(&blends.mixed)
            .map(|(&placed, &mixed)| {
                let gone = blends
                    .gone
                    .get(usize::from(placed))
                    .copied()
                    .unwrap_or(true);
                (!mixed && !gone).then_some(placed)
            })
            .collect();
        if let Some(lit) = shade::lit(&pixels, size, &colors, &own) {
            pixels = lit;
            palette = Palette::of(&pixels, size);
            blends = palette.blends(width);
        }
        let Palette { found, placed } = palette;
        let Blends { gone, mixed } = blends;
        // The place among `colors` of each color found that is one.
        let mut kept = Vec::with_capacity(found.len());
        let mut colors: Vec<Rgb> = Vec::new();
        for (&(color, _), &gone) in found.iter().zip(&gone) {
            let place = (!gone && colors.len() < MAX_COLORS)
                .then(|| u8::try_from(colors.len()).ok())
                .flatten();
            kept.push(place);
            colors.extend(place.map(|_| color));
        }
        // The place of the color nearest `pixel`, of those with a
        // [`bit`] in `among`.
        //
        // A blend of two colors goes to the nearer of the two, and not to
        // a third that happens to lie nearer than either: the gray between
        // a white outline and a dark background is no part of red text.
        let nearest_to = |pixel: Rgb, among: u32| {
            let others = || {
                (0..=u8::MAX)
                    .zip(&colors)
                    .filter(|&(place, _)| among & bit(usize::from(place)) != 0)
            };
            let blends =
                |one: Rgb| others().any(|(_, &other)| other != one && between(pixel, one, other));
            let nearest_of = |blended: bool| {
                others()
                    .filter(|&(_, &one)| !blended || blends(one))
                    .min_by_key(|&(_, &one)| apart(one, pixel))
                    .map(|(place, _)| place)
            };
            nearest_of(true).or_else(|| nearest_of(false))
        };
        // Each pixel's color where it is all of one, and [`NO_LAYER`]
        // where it is a blend.
        let solid: Vec<u8> = placed
            .iter()
            .zip(&mixed)
            .map(|(&placed, &mixed)| {
                kept.get(usize::from(placed))
                    .copied()
                    .flatten()
                    .filter(|_| !mixed)
                    .unwrap_or(NO_LAYER)
            })
            .collect();
        // The colors that lie within no two others, a [`bit`] to each:
        // the lightest of white, gray, and black is one, though it is
        // close enough to the gray to count as a blend a little past it.
        let ends = (0..colors.len())
            .zip(&colors)
            .filter(|&(place, &color)| {
                let others = || {
                    (0..colors.len())
                        .zip(&colors)
                        .filter(move |&(other, _)| other != place)
                };
                !others().any(|(one, &first)| {
                    others().any(|(other, &second)| one < other && within(color, first, second))
                })
            })
            .map(|(place, _)| bit(place))
            .fold(0_u32, BitOr::bitor);
        // A blend goes to the nearest color. One that lies within two
        // others has to touch it to take it: the edge of white on black
        // passes through the color of gray text elsewhere in the image,
        // or of the gray inside a white outline, and is no part of
        // either.
        let color: Vec<u8> = (0..)
            .zip(pixels.iter().zip(&solid))
            .map(|(at, (&pixel, &solid_here))| {
                if solid_here != NO_LAYER {
                    return solid_here;
                }
                nearest_to(pixel, ends | close_by(&solid, width, at, 1)).unwrap_or(0)
            })
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

    /// The layers that hold ink, in the order a read asks them: the colors
    /// from the one with the most ink, the more common of two with as much,
    /// and then [`Layer::Every`]. The background's color is counted with
    /// every pixel of it that other colors close in, which is no fewer than
    /// its ink.
    pub(super) fn all(&self) -> Vec<Layer> {
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
        let every = colors.iter().any(|&(_, place)| place != self.background);
        let mut all: Vec<Layer> = colors
            .into_iter()
            .map(|(_, place)| Layer::Color(place))
            .collect();
        if every {
            all.push(Layer::Every);
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

/// The bit that stands for the color at `place` among those found.
fn bit(place: usize) -> u32 {
    u32::try_from(place)
        .ok()
        .and_then(|place| 1_u32.checked_shl(place))
        .unwrap_or(0)
}

/// Which colors have a pixel within `reach` of the one at `at`, a
/// [`bit`] to each. `placed` holds each pixel's color, in rows `width`
/// long.
fn close_by(placed: &[u8], width: usize, at: usize, reach: usize) -> u32 {
    let column = at.checked_rem(width).unwrap_or(0);
    let from = column.saturating_sub(reach);
    let across = column
        .saturating_add(reach)
        .saturating_add(1)
        .min(width)
        .saturating_sub(from);
    let row = at.saturating_sub(column);
    let first = row.saturating_sub(reach.saturating_mul(width));
    (first..=row.saturating_add(reach.saturating_mul(width)))
        .step_by(width.max(1))
        .filter_map(|row| {
            let start = row.checked_add(from)?;
            placed.get(start..start.checked_add(across)?.min(placed.len()))
        })
        .flatten()
        .fold(0_u32, |met, &color| met | bit(usize::from(color)))
}

/// What the pixels of a color are that are no blends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rest {
    /// None, or pieces of fewer than [`SPECK`] pixels.
    Specks,
    /// A piece large enough to be a character among them.
    Text,
}

/// The blends among the colors found in an image.
struct Blends {
    /// Whether each color found is the blend along an edge, all of it,
    /// and no color of its own.
    gone: Vec<bool>,
    /// Whether each pixel is a blend, though its color is one of the
    /// image's: another color's edge passes through that color.
    mixed: Vec<bool>,
}

/// Which of the colors `found` in an image, each with whether it may be a
/// blend, are the blend along an edge and no color of their own, and
/// which pixels of the rest are blends. `placed` holds each pixel's
/// color, in rows `width` long.
///
/// A color between two others may be the blend along their edge,
/// whichever of the three is the most common: an outline less than two
/// pixels wide has fewer pixels of its own color than of its blends. A
/// blend lies where its two colors meet, so its pixels have both close
/// by, or blends nearer each. Thin text of a color that happens to lie
/// between the background's and that of other text does not, but the
/// edge of that other text passes through its color. So a color's pixels
/// are judged in the pieces they lie in ([`mixed`]), and the color is
/// gone only when it has blends and nothing else but specks, which are
/// blends that lie apart from one of their two colors. The least common
/// color is judged first, and a color that is gone is no end of another.
fn blends(found: &[(Rgb, bool)], placed: &[u8], width: usize) -> Blends {
    let mut blends = Blends {
        gone: vec![false; found.len()],
        mixed: vec![false; placed.len()],
    };
    for (place, &(color, may)) in found.iter().enumerate().rev() {
        if !may {
            continue;
        }
        let (mixed, rest) = mixed(found, &blends.gone, placed, width, place, color);
        if rest == Rest::Specks && !mixed.is_empty() {
            if let Some(gone) = blends.gone.get_mut(place) {
                *gone = true;
            }
            continue;
        }
        for at in mixed {
            if let Some(mixed) = blends.mixed.get_mut(at) {
                *mixed = true;
            }
        }
    }
    blends
}

/// The pixels of `color`, at `place` among `found`, that are a blend of
/// two of the others that are not `gone` themselves, by where each is in
/// `placed`, and what the rest of the color's pixels are. A color that
/// lies between no two of the others has no blends.
///
/// A piece of the color, the pixels that touch one another, is a blend
/// when it is thin, with few pixels that have the color on every side,
/// and a quarter of its pixels have both colors close by. A stroke of
/// text in the color is no blend, and neither is a point with only the
/// background around it.
fn mixed(
    found: &[(Rgb, bool)],
    gone: &[bool],
    placed: &[u8],
    width: usize,
    place: usize,
    color: Rgb,
) -> (Vec<usize>, Rest) {
    let others = || {
        found
            .iter()
            .zip(gone)
            .enumerate()
            .filter(move |&(other, (_, &gone))| other != place && !gone)
            .map(|(other, (&(color, _), _))| (other, color))
    };
    // The colors toward `end`: `end`, and each color between it and this
    // one, which is a blend nearer that end.
    let toward = |(end, end_color): (usize, Rgb)| {
        found
            .iter()
            .enumerate()
            .filter(|&(other, &(other_color, _))| {
                other == end || (other != place && between(other_color, color, end_color))
            })
            .map(|(other, _)| bit(other))
            .fold(0_u32, BitOr::bitor)
    };
    let pairs: Vec<(u32, u32)> = others()
        .enumerate()
        .flat_map(|(skip, one)| {
            others()
                .skip(skip.saturating_add(1))
                .map(move |other| (one, other))
        })
        .filter(|&((_, one), (_, other))| between(color, one, other))
        .map(|(one, other)| (toward(one), toward(other)))
        .collect();
    if pairs.is_empty() {
        return (Vec::new(), Rest::Text);
    }
    let own = |at: usize| {
        placed
            .get(at)
            .is_some_and(|&color| usize::from(color) == place)
    };
    // Whether a pixel has its color on every side: it and its four
    // neighbors.
    let amid = |at: usize| {
        let mut held = 0_u8;
        visit(width, at, |next| {
            held = held.saturating_add(u8::from(own(next)));
        });
        held == 5
    };
    // Whether a pixel has both colors of a pair close by.
    let beside = |at: usize| {
        let met = close_by(placed, width, at, index(MEETS));
        pairs
            .iter()
            .any(|&(one, other)| met & one != 0 && met & other != 0)
    };
    let mut seen = vec![false; placed.len()];
    let (mut mixed, mut rest) = (Vec::new(), Rest::Specks);
    for at in (0..placed.len()).filter(|&at| own(at)) {
        let piece = piece(placed, width, at, &mut seen);
        let inner = piece.iter().filter(|&&at| amid(at)).count();
        let thin = inner.saturating_mul(index(THIN)) < piece.len();
        let both = piece.iter().filter(|&&at| beside(at)).count();
        if thin && both.saturating_mul(4) >= piece.len() {
            mixed.extend(piece);
        } else if piece.len() >= SPECK {
            rest = Rest::Text;
        }
    }
    (mixed, rest)
}

/// The pixels of one color that touch the one at `start` through each
/// other, side to side or corner to corner, by where each is in `placed`,
/// which holds each pixel's color in rows `width` long. None of them is
/// `seen` before, and all are after; a `start` seen before gives none.
fn piece(placed: &[u8], width: usize, start: usize, seen: &mut [bool]) -> Vec<usize> {
    let color = placed.get(start).copied();
    let mut piece = Vec::new();
    let mut next = vec![start];
    while let Some(at) = next.pop() {
        if let Some(seen) = seen.get_mut(at)
            && !*seen
            && placed.get(at).copied() == color
        {
            *seen = true;
        } else {
            continue;
        }
        piece.push(at);
        let column = at.checked_rem(width).unwrap_or(0);
        let columns =
            column.saturating_sub(1)..=column.saturating_add(1).min(width.saturating_sub(1));
        let row = at.saturating_sub(column);
        for row in [row.checked_sub(width), Some(row), row.checked_add(width)]
            .into_iter()
            .flatten()
        {
            next.extend(columns.clone().map(|column| row.saturating_add(column)));
        }
    }
    piece
}

/// Whether two colors are one color to a reader.
pub(super) fn same(one: Rgb, other: Rgb) -> bool {
    one.near(other, SAME_COLOR)
}

/// Which of the counted colors `color` is.
fn counted(color: Rgb) -> u16 {
    let kept = |channel: u8| u16::from(channel >> (u8::BITS.saturating_sub(COUNTED_BITS)));
    // Each channel has bits of its own, so the sum is the three side by
    // side.
    (kept(color.red) << (COUNTED_BITS.saturating_mul(2)))
        .saturating_add(kept(color.green) << COUNTED_BITS)
        .saturating_add(kept(color.blue))
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

/// Whether `color` is a blend of `one` and `other`, or a little past
/// either as the color of an edge's pixel can be: [`blend`] with
/// [`BLEND_SLACK`] past.
fn between(color: Rgb, one: Rgb, other: Rgb) -> bool {
    blend(color, one, other, BLEND_SLACK)
}

/// Whether `color` lies within `one` and `other`, as the color of a whole
/// edge does and the lightest or darkest of three colors does not:
/// [`blend`] with nothing past.
fn within(color: Rgb, one: Rgb, other: Rgb) -> bool {
    blend(color, one, other, 0)
}

/// Whether `color` is a blend of `one` and `other`: it lies about as far
/// from `one` toward `other` in each of red, green, and blue that tells
/// the two apart, no further `past` either than that many parts of 256
/// of the way between them, and keeps what they share.
fn blend(color: Rgb, one: Rgb, other: Rgb, past: i32) -> bool {
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
        // How far along, in parts of 256.
        let along = color
            .saturating_sub(one)
            .saturating_mul(256)
            .checked_div(other.saturating_sub(one))
            .unwrap_or(0);
        least = least.min(along);
        most = most.max(along);
    }
    least >= past.saturating_neg()
        && most <= past.saturating_add(256)
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
///
/// Only a pixel not held yet is reached again, so the loop ends. A pattern
/// says so rather than a test of the slot, which mutation testing could
/// invert into a loop that never ends and grows `reached` until the
/// machine runs out of memory.
fn spread(
    width: usize,
    held: &mut [bool],
    mut reached: Vec<usize>,
    passes: impl Fn(usize, usize) -> bool,
) {
    while let Some(at) = reached.pop() {
        visit(width, at, |next| {
            if let Some(slot @ &mut false) = held.get_mut(next)
                && passes(at, next)
            {
                *slot = true;
                reached.push(next);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rgb(red: u8, green: u8, blue: u8) -> Rgb {
        Rgb { red, green, blue }
    }

    #[test]
    fn a_pixel_at_a_rows_end_has_no_neighbor_in_the_next_row() {
        let neighbors = |at| {
            let mut all = Vec::new();
            visit(3, at, |next| all.push(next));
            all
        };
        // The first and the last pixel of the middle row of three.
        assert_eq!(neighbors(3), [3, 4, 0, 6]);
        assert_eq!(neighbors(5), [5, 4, 2, 8]);
    }

    #[test]
    fn two_colors_that_differ_by_just_enough_in_a_channel_tell_how_far_a_blend_lies() {
        // Red differs by `TELLING` between the two, so a color as far past
        // one of them as a blend may lie is a blend, though it is further
        // from the other than colors that share a channel may be.
        let (one, other) = (rgb(0, 0, 0), rgb(TELLING, 0, 0));
        assert!(between(rgb(60, 0, 0), one, other));
        assert!(!between(rgb(61, 0, 0), one, other));
    }

    #[test]
    fn three_colors_just_far_enough_apart_are_told_apart() {
        // The layer's color and the one it meets are each `TELLING` from
        // the background, in different channels.
        let (own, met, background) = (rgb(TELLING, 0, 0), rgb(0, TELLING, 0), rgb(0, 0, 0));
        assert_eq!(unmixed(own, own, met, background), Some(u8::MAX));
        assert_eq!(unmixed(met, own, met, background), Some(0));
        // A color nearer the background than that, at 47, says nothing.
        assert_eq!(unmixed(own, rgb(47, 0, 0), met, background), None);
        assert_eq!(unmixed(own, own, rgb(0, 47, 0), background), None);
    }

    #[test]
    fn a_piece_of_a_color_between_two_others_is_a_blend_when_thin_with_both_close_by() {
        let (black, gray, white) = (rgb(0, 0, 0), rgb(128, 128, 128), rgb(255, 255, 255));
        let found = [(black, false), (gray, true), (white, true)];
        // The gray pixels of `rows` that are blends, in order, and what
        // is left of the gray.
        let blends = |rows: &[&str]| {
            let placed: Vec<u8> = rows
                .iter()
                .flat_map(|row| row.bytes())
                .map(|cell| match cell {
                    b'b' => 0,
                    b'g' => 1,
                    _ => 2,
                })
                .collect();
            let width = rows.first().map_or(0, |row| row.len());
            let (mut blends, rest) = mixed(&found, &[false; 3], &placed, width, 1, gray);
            blends.sort_unstable();
            (blends, rest)
        };
        // A line a pixel wide between white and black is the edge's blend.
        let line = ["wwwwwwww", "gggggggg", "bbbbbbbb"];
        assert_eq!(blends(&line), ((8..16).collect(), Rest::Specks));
        // So is one with both colors within two pixels of a quarter of it.
        let quarter = ["wwwwwwww", "ggggwbww", "wwwwwwww"];
        assert_eq!(blends(&quarter), ((8..12).collect(), Rest::Specks));
        // Eight pixels with one that has gray on every side are no
        // thinner than a stroke, and stay.
        let cross = ["wwwgwwww", "wggggggw", "bbbgbbbb"];
        assert_eq!(blends(&cross), (Vec::new(), Rest::Text));
        // A pixel with white around it and no black is no blend of the
        // two, and too small to be text.
        let alone = ["wwwww", "wwwww", "wwgww", "wwwww", "wwwww"];
        assert_eq!(blends(&alone), (Vec::new(), Rest::Specks));
    }

    #[test]
    fn a_blend_of_two_colors_just_far_enough_apart_is_shared_between_them() {
        let (own, met) = (rgb(TELLING, 0, 0), rgb(0, 0, 0));
        assert_eq!(part(own, own, met), u8::MAX);
        assert_eq!(part(met, own, met), 0);
        // Colors nearer than that, at 47, are one, and the pixel is all
        // the layer's.
        assert_eq!(part(met, rgb(47, 0, 0), met), u8::MAX);
    }
}
