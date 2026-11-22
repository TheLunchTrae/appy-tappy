//! A shade over a part of an image, and the image as it shows without it.
//!
//! An application dims what lies behind a panel it opens: a pause menu
//! draws a dark sheet one can see through over the middle of its window. A
//! region the sheet's edge runs through shows its text in two
//! brightnesses, and each character the edge crosses in two colors, so no
//! one color holds a whole character.
//!
//! A black sheet leaves every color under it the same part of itself, in
//! red, green, and blue alike. So where one of an image's colors is such a
//! part of another, and a straight line across or down the image has the
//! darker one on one side and the lighter on the other, the darker side
//! may lie under a shade. It does unless what it shows is what the lighter
//! side shows, as it is: a shade lies over the background too. Its pixels
//! are then made as light again as that part says, and a reader splits
//! the image into its colors as if nothing lay over it. The blend along
//! an edge under the shade is a part of the blend it was, so it comes
//! back as it was too.
//!
//! A reader still asks that every shape matches a glyph, so an image taken
//! for shaded that is not reads as it did or fails.
//!
//! These limits follow:
//!
//! - The sheet is black, or about. One of another color leaves no color
//!   a part of itself.
//! - Its edge is a straight line across or down the image, so the corner
//!   of a sheet, or a round one, is no shade.
//! - Its edge is sharp, or soft over a row on each side.
//! - Under a sheet that leaves a color so little that it shows as the
//!   background does, there is nothing to make light again.

use std::ops::Range;

use super::super::{Rgb, Size};
use super::ink::index;

/// The whole of a color, in the parts a shade leaves of it.
const WHOLE: u32 = 256;

/// The least of a color a shade may leave: a fifth. Under a darker one
/// every color is about black, and any dark color would pass for a
/// lighter one under it.
const DARKEST: u32 = 51;

/// How far red, green, or blue of a color may be from that part of a
/// lighter color's, and the color still be the lighter one under a shade.
const OFF: u8 = 12;

/// How light a pixel is at least, in the lightest of its red, green, and
/// blue, for it to tell how much of a color a shade leaves there.
const FAINT: u8 = 48;

/// One in this many of the pixels that tell where a shade's edge is may
/// lie on the wrong side of it, and each side has at least this many of
/// them.
const STRAY: u32 = 16;

/// The pixels of an image of `size` as they show without the shade over
/// a part of them, or `None` where no shade lies over a part: `pixels`
/// then show as they are. `pixels` are row by row from the top, and
/// `own` holds, for each that is all of one of the image's `colors` and
/// no blend along an edge, that color's place.
pub(super) fn lit(
    pixels: &[Rgb],
    size: Size,
    colors: &[Rgb],
    own: &[Option<u8>],
) -> Option<Vec<Rgb>> {
    let mut pairs = pairs(colors);
    if pairs.is_empty() {
        return None;
    }
    let all = held(
        own.iter().copied().zip(pixels.iter().copied()),
        colors.len(),
    );
    // The pair of colors with the most pixels of both says the most of
    // where a shade is, so it is asked first, and of pairs with as many,
    // the more common colors.
    let fewer = |pair: &Pair| {
        let all = |color: u8| all.get(usize::from(color)).map_or(0, |&(count, _)| count);
        all(pair.light).min(all(pair.dark))
    };
    pairs.sort_by_key(|pair| std::cmp::Reverse(fewer(pair)));
    let width = index(size.width).max(1);
    pairs.into_iter().find_map(|pair| {
        let shade = Shade::over(pixels, own, colors.len(), pair, size)?;
        let lit = |(at, &pixel)| whole(pixel, shade.left(at, width));
        Some((0..).zip(pixels).map(lit).collect())
    })
}

/// How many of `pixels` have each of `colors` colors, by its place, and
/// what those pixels show: the middle one's red, green, and blue, each
/// for itself. A color's place among an image's colors holds the colors
/// near it too, and the middle is what most of its pixels show, where
/// their average is somewhere between. Each pixel comes with its color's
/// place, or with none where it is not counted.
fn held(pixels: impl Iterator<Item = (Option<u8>, Rgb)>, colors: usize) -> Vec<(u32, Rgb)> {
    let mut held: Vec<[Vec<u8>; 3]> = vec![Default::default(); colors];
    for (color, pixel) in pixels {
        if let Some(held) = color.and_then(|color| held.get_mut(usize::from(color))) {
            for (held, channel) in held.iter_mut().zip(channels(pixel)) {
                held.push(channel);
            }
        }
    }
    held.into_iter()
        .map(|mut held| {
            let count = held.each_ref().map(Vec::len)[0];
            let [red, green, blue] = held.each_mut().map(|held| {
                held.sort_unstable();
                held.get(held.len().div_euclid(2)).copied().unwrap_or(0)
            });
            (
                u32::try_from(count).unwrap_or(u32::MAX),
                Rgb { red, green, blue },
            )
        })
        .collect()
}

/// Two of an image's colors, one of which is the other under a shade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pair {
    /// The color outside the shade, by its place among the image's.
    light: u8,
    /// The color it shows as under the shade, by its place.
    dark: u8,
    /// How much of a color the shade leaves, of [`WHOLE`].
    part: u32,
}

/// Each two of `colors` of which one is the other under a shade, in the
/// order of the lighter and then the darker color.
fn pairs(colors: &[Rgb]) -> Vec<Pair> {
    (0..=u8::MAX)
        .zip(colors)
        .flat_map(|(light, &lighter)| {
            (0..=u8::MAX)
                .zip(colors)
                .filter_map(move |(dark, &darker)| {
                    part(lighter, darker).map(|part| Pair { light, dark, part })
                })
        })
        .collect()
}

/// How much of `light` a shade leaves that shows it as `dark`, of
/// [`WHOLE`], or `None` where `dark` is no such part of it: the part that
/// brings red, green, and blue closest, where it is at least [`DARKEST`],
/// less than the whole, and leaves each within [`OFF`].
fn part(light: Rgb, dark: Rgb) -> Option<u32> {
    let sum = |one: Rgb, other: Rgb| {
        channels(one)
            .into_iter()
            .zip(channels(other))
            .map(|(one, other)| u32::from(one).saturating_mul(u32::from(other)))
            .fold(0, u32::saturating_add)
    };
    let all = sum(light, light);
    // Rounded to the nearest part.
    let part = sum(light, dark)
        .saturating_mul(WHOLE)
        .saturating_add(all.div_euclid(2))
        .checked_div(all)?;
    ((DARKEST..WHOLE).contains(&part) && shaded(light, part).near(dark, OFF)).then_some(part)
}

/// Red, green, and blue of `color`.
const fn channels(color: Rgb) -> [u8; 3] {
    [color.red, color.green, color.blue]
}

/// `color` under a shade that leaves `part` of it.
fn shaded(color: Rgb, part: u32) -> Rgb {
    let left = |channel: u8| {
        let left = u32::from(channel).saturating_mul(part).div_euclid(WHOLE);
        u8::try_from(left).unwrap_or(u8::MAX)
    };
    Rgb {
        red: left(color.red),
        green: left(color.green),
        blue: left(color.blue),
    }
}

/// The color that shows as `color` under a shade that leaves `part` of
/// it, as near as a color can be: no lighter than white.
fn whole(color: Rgb, part: u32) -> Rgb {
    let whole = |channel: u8| {
        let whole = u32::from(channel)
            .saturating_mul(WHOLE)
            .checked_div(part)
            .unwrap_or(u32::MAX);
        u8::try_from(whole).unwrap_or(u8::MAX)
    };
    Rgb {
        red: whole(color.red),
        green: whole(color.green),
        blue: whole(color.blue),
    }
}

/// A shade over a part of an image: every row, or every column, from its
/// edge to the image's.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Shade {
    /// Whether its edge runs down the image, so that it lies over
    /// columns, and not across it, over rows.
    down: bool,
    /// How much of a color it leaves in each row, or each column, of
    /// [`WHOLE`]: the whole where it does not lie.
    left: Vec<u32>,
}

impl Shade {
    /// The shade over an image of `size` that shows one color of `pair`
    /// as the other, or `None` where there is none. `pixels` are row by
    /// row from the top, and `own` holds the place, among the image's
    /// `colors` colors, of the color each is all of.
    ///
    /// 1. A straight line across or down the image has the pixels that
    ///    are all the pair's light color on one side and those all its
    ///    dark color on the other ([`Tally::edge`]). Of a line across and
    ///    one down that part them as well, it is the one across.
    /// 2. No more than a few of the pixels on the dark side that are all
    ///    one color show what a color on the light side shows, as it is:
    ///    one in [`STRAY`] of those that show what a color there shows,
    ///    shaded. A background that is the same on both sides lies under
    ///    no shade, and neither does text of two brightnesses on it.
    ///    What only the dark side shows says nothing.
    /// 3. The pair's pixels leave the edge several places it may lie in,
    ///    as where it runs between two characters. It lies where the
    ///    fewest pixels there show a color of the wrong side
    ///    ([`Tally::settled`]).
    /// 4. A shade's edge can be soft, and may lie a row off. So the row
    ///    on each side of it keeps as much as it shows of the row beyond
    ///    it, further from the edge ([`kept`]).
    fn over(
        pixels: &[Rgb],
        own: &[Option<u8>],
        colors: usize,
        pair: Pair,
        size: Size,
    ) -> Option<Self> {
        let width = index(size.width).max(1);
        let lines = (index(size.height), width);
        let (mut rows, mut columns) = (Tally::new(lines.0), Tally::new(lines.1));
        for (at, &own) in (0_usize..).zip(own) {
            for (side, color) in [(Side::Light, pair.light), (Side::Dark, pair.dark)] {
                if own == Some(color) {
                    rows.count(at.div_euclid(width), side);
                    columns.count(at.rem_euclid(width), side);
                }
            }
        }
        let (across, down) = (rows.edge(), columns.edge());
        let is_down = match (&across, &down) {
            (Some(across), Some(down)) => down.wrong < across.wrong,
            (None, Some(_)) => true,
            (_, None) => false,
        };
        let (edge, lines) = if is_down {
            (down?, lines.1)
        } else {
            (across?, lines.0)
        };
        let line = |at: usize| {
            if is_down {
                at.rem_euclid(width)
            } else {
                at.div_euclid(width)
            }
        };
        // What the pixels show that are surely on each side of the edge:
        // before the first place it may lie in, and from the last.
        let places = edge.places.clone();
        let last = places.end.saturating_sub(1);
        let sure = |lines: Range<usize>| {
            let there = (0..)
                .zip(own.iter().copied().zip(pixels.iter().copied()))
                .filter(|&(at, _)| lines.contains(&line(at)));
            held(there.map(|(_, pixel)| pixel), colors)
        };
        let (before, after) = (sure(0..places.start), sure(last..lines));
        let (light, dark) = match edge.shade {
            Lies::Before => (after, before),
            Lies::After => (before, after),
        };
        let shows = |shown: &[(u32, Rgb)], pixel: Rgb| {
            shown
                .iter()
                .any(|&(count, color)| count > 0 && color.near(pixel, OFF))
        };
        // How much the shade leaves, by what the pair's pixels show on
        // each side.
        let of =
            |side: &[(u32, Rgb)], color: u8| side.get(usize::from(color)).map(|&(_, color)| color);
        let part = part(of(&light, pair.light)?, of(&dark, pair.dark)?)?;
        let (mut known, mut against) = (0_u32, 0_u32);
        for &(count, under) in &dark {
            let shades =
                |&(count, color): &(u32, Rgb)| count > 0 && shaded(color, part).near(under, OFF);
            // A color so dark that the shade leaves it about as it is
            // shows the same on both sides, and says nothing.
            let same = |&(count, color): &(u32, Rgb)| {
                count > 0 && color.near(under, OFF) && !shaded(color, part).near(color, OFF)
            };
            if light.iter().any(shades) {
                known = known.saturating_add(count);
            } else if light.iter().any(same) {
                against = against.saturating_add(count);
            }
        }
        if against.saturating_mul(STRAY) >= known {
            return None;
        }
        let mut told = Tally::new(lines);
        for (at, &pixel) in (0_usize..).zip(pixels) {
            let line = line(at);
            if !(places.start..last).contains(&line) {
                continue;
            }
            match (shows(&light, pixel), shows(&dark, pixel)) {
                (true, false) => told.count(line, Side::Light),
                (false, true) => told.count(line, Side::Dark),
                _ => {},
            }
        }
        let over = told.settled(&edge);
        let mut left = vec![WHOLE; lines];
        for left in left.iter_mut().skip(over.start).take(over.len()) {
            *left = part;
        }
        // The row under the shade at its edge with the next one under
        // it, and the row beside the shade with the next one away from
        // it.
        let (under, outside) = match edge.shade {
            Lies::Before => (
                over.end.checked_sub(1).map(|at| (at, at.checked_sub(1))),
                Some((over.end, over.end.checked_add(1))),
            ),
            Lies::After => (
                Some((over.start, over.start.checked_add(1))),
                over.start.checked_sub(1).map(|at| (at, at.checked_sub(1))),
            ),
        };
        let row = |at: usize| -> Vec<Rgb> {
            let (skip, step) = if is_down {
                (at, width)
            } else {
                (at.saturating_mul(width), 1)
            };
            let length = if is_down { usize::MAX } else { width };
            pixels
                .iter()
                .skip(skip)
                .step_by(step)
                .take(length)
                .copied()
                .collect()
        };
        for ((at, beyond), beyond_left) in [(under, part), (outside, WHOLE)]
            .into_iter()
            .filter_map(|(rows, left)| Some((rows?, left)))
        {
            let kept = beyond
                .filter(|&beyond| beyond < lines)
                .and_then(|beyond| kept(&row(at), &row(beyond)));
            if let (Some(kept), Some(left)) = (kept, left.get_mut(at)) {
                *left = kept
                    .saturating_mul(beyond_left)
                    .div_euclid(WHOLE)
                    .clamp(part, WHOLE);
            }
        }
        Some(Self {
            down: is_down,
            left,
        })
    }

    /// How much of a color the shade leaves in the pixel at `at` in rows
    /// `width` long, of [`WHOLE`].
    fn left(&self, at: usize, width: usize) -> u32 {
        let line = if self.down {
            at.rem_euclid(width)
        } else {
            at.div_euclid(width)
        };
        self.left.get(line).copied().unwrap_or(WHOLE)
    }
}

/// How much of the pixels of `beyond`, a row or a column of an image,
/// those of `row` beside it keep, of [`WHOLE`]: for most of them, how
/// light a pixel is against the one beside it, where that one is at
/// least [`FAINT`]. Two rows next to each other show the same thing in
/// most of their pixels, so what differs for most is how much of it a
/// shade leaves. `None` where fewer than half the pixels of `beyond` are
/// that light: the few that are may be the edge of one character.
fn kept(row: &[Rgb], beyond: &[Rgb]) -> Option<u32> {
    let lightest = |color: &Rgb| u32::from(color.red.max(color.green).max(color.blue));
    let mut kept: Vec<u32> = row
        .iter()
        .zip(beyond)
        .filter(|(_, beyond)| lightest(beyond) >= u32::from(FAINT))
        .filter_map(|(row, beyond)| {
            lightest(row)
                .saturating_mul(WHOLE)
                .checked_div(lightest(beyond))
        })
        .collect();
    if kept.len().saturating_mul(2) < beyond.len() {
        return None;
    }
    kept.sort_unstable();
    kept.get(kept.len().div_euclid(2)).copied()
}

/// Which side of a shade's edge a pixel is on, by what it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    /// Outside the shade.
    Light,
    /// Under it.
    Dark,
}

/// Where a shade's edge may lie among an image's rows, or its columns.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Edge {
    /// Which side of the edge the shade is on.
    shade: Lies,
    /// The first rows, or columns, the shade's side may start at, or end
    /// before: next to each other, with as few pixels on the wrong side.
    places: Range<usize>,
    /// How many pixels are on the wrong side.
    wrong: u32,
}

/// Which side of its edge a shade lies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lies {
    /// From the image's first row or column to the edge.
    Before,
    /// From the edge to the last.
    After,
}

/// How many pixels that tell of a shade each row, or each column, of an
/// image holds on each side of its edge.
#[derive(Debug)]
struct Tally {
    light: Vec<u32>,
    dark: Vec<u32>,
}

impl Tally {
    /// No pixels yet in `lines` rows or columns.
    fn new(lines: usize) -> Self {
        Self {
            light: vec![0; lines],
            dark: vec![0; lines],
        }
    }

    /// Counts one pixel of `side` in row or column `line`.
    fn count(&mut self, line: usize, side: Side) {
        let counts = match side {
            Side::Light => &mut self.light,
            Side::Dark => &mut self.dark,
        };
        if let Some(count) = counts.get_mut(line) {
            *count = count.saturating_add(1);
        }
    }

    /// How many pixels are on the wrong side of an edge before each row
    /// or column, and of one after the last, where the shade `lies` on
    /// that side of it: the light ones under the shade, and the dark ones
    /// outside it.
    fn wrong(&self, lies: Lies) -> Vec<(u32, u32)> {
        let sum = |counts: &[u32]| counts.iter().copied().fold(0, u32::saturating_add);
        // Those before the edge that belong after it, and those after it
        // that belong before.
        let (before, after) = match lies {
            Lies::Before => (&self.light, &self.dark),
            Lies::After => (&self.dark, &self.light),
        };
        let (mut early, mut late) = (0_u32, sum(after));
        let mut wrong = vec![(early, late)];
        for (&before, &after) in before.iter().zip(after) {
            early = early.saturating_add(before);
            late = late.saturating_sub(after);
            wrong.push((early, late));
        }
        match lies {
            Lies::Before => wrong,
            Lies::After => wrong
                .into_iter()
                .map(|(dark, light)| (light, dark))
                .collect(),
        }
    }

    /// Where the edge of a shade lies that has the fewest pixels on the
    /// wrong side of it, whichever side the shade is on, or `None` where
    /// more than one in [`STRAY`] of the light pixels or of the dark ones
    /// are, or there are fewer than [`STRAY`] of either. So few are on
    /// the wrong side of one edge only.
    fn edge(&self) -> Option<Edge> {
        let sum = |counts: &[u32]| counts.iter().copied().fold(0, u32::saturating_add);
        let (light, dark) = (sum(&self.light), sum(&self.dark));
        let few = |wrong: u32, all: u32| all >= STRAY && wrong.saturating_mul(STRAY) <= all;
        let best = |shade: Lies| {
            let wrong = self.wrong(shade);
            let total = |&(light, dark): &(u32, u32)| light.saturating_add(dark);
            let fewest = wrong.iter().map(total).min()?;
            let first = wrong.iter().position(|wrong| total(wrong) == fewest)?;
            let run = wrong
                .iter()
                .skip(first)
                .take_while(|wrong| total(wrong) == fewest)
                .count();
            let &(wrong_light, wrong_dark) = wrong.get(first)?;
            (few(wrong_light, light) && few(wrong_dark, dark)).then_some(Edge {
                shade,
                places: first..first.saturating_add(run),
                wrong: fewest,
            })
        };
        best(Lies::After).or_else(|| best(Lies::Before))
    }

    /// The rows, or columns, under a shade with `edge`, which lies at the
    /// one of its places with the fewest of these pixels on the wrong
    /// side, and of those with as few, where the shade is the smallest.
    fn settled(&self, edge: &Edge) -> Range<usize> {
        let wrong = self.wrong(edge.shade);
        let total = |place: usize| {
            wrong
                .get(place)
                .map_or(u32::MAX, |&(light, dark)| light.saturating_add(dark))
        };
        let places = edge.places.clone();
        match edge.shade {
            Lies::Before => 0..places.min_by_key(|&place| total(place)).unwrap_or(0),
            Lies::After => {
                let lines = self.light.len();
                // The last of the places with as few.
                places
                    .rev()
                    .min_by_key(|&place| total(place))
                    .unwrap_or(lines)..lines
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rgb(red: u8, green: u8, blue: u8) -> Rgb {
        Rgb { red, green, blue }
    }

    const fn gray(level: u8) -> Rgb {
        rgb(level, level, level)
    }

    const WHITE: Rgb = gray(255);
    const BLUE: Rgb = rgb(60, 120, 200);

    /// The side of a picture's edge its shade lies on, and the edge's
    /// row or column.
    #[derive(Debug, Clone, Copy)]
    enum Over {
        Below(usize),
        Above(usize),
        RightOf(usize),
        LeftOf(usize),
    }

    impl Over {
        fn covers(self, x: usize, y: usize) -> bool {
            match self {
                Self::Below(edge) => y >= edge,
                Self::Above(edge) => y < edge,
                Self::RightOf(edge) => x >= edge,
                Self::LeftOf(edge) => x < edge,
            }
        }
    }

    /// A picture 16 pixels wide and tall: a block of `ink` from 4 to 12
    /// across and down, on `ground`.
    fn block(ink: Rgb, ground: Rgb) -> Vec<Rgb> {
        (0..256_usize)
            .map(|at| {
                let inside = |at: usize| (4..12).contains(&at);
                if inside(at.rem_euclid(16)) && inside(at.div_euclid(16)) {
                    ink
                } else {
                    ground
                }
            })
            .collect()
    }

    const SIZE: Size = Size {
        width: 16,
        height: 16,
    };

    /// `pixels`, 16 to a row, with what `part(x, y)` leaves of each.
    fn under(pixels: &[Rgb], part: impl Fn(usize, usize) -> u32) -> Vec<Rgb> {
        (0..)
            .zip(pixels)
            .map(|(at, &pixel): (usize, _)| {
                shaded(pixel, part(at.rem_euclid(16), at.div_euclid(16)))
            })
            .collect()
    }

    /// What [`lit`] makes of `pixels`, 16 to a row, each of which is all
    /// of the one of `colors` nearest to it.
    fn lit_of(pixels: &[Rgb], colors: &[Rgb]) -> Option<Vec<Rgb>> {
        let own: Vec<Option<u8>> = pixels
            .iter()
            .map(|&pixel| {
                (0..=u8::MAX)
                    .zip(colors)
                    .find(|&(_, &color)| color == pixel)
            })
            .map(|found| found.map(|(place, _)| place))
            .collect();
        lit(pixels, SIZE, colors, &own)
    }

    /// The colors of `pixels`, each once, in the order they come.
    fn colors_of(pixels: &[Rgb]) -> Vec<Rgb> {
        let mut colors: Vec<Rgb> = Vec::new();
        for &pixel in pixels {
            if !colors.contains(&pixel) {
                colors.push(pixel);
            }
        }
        colors
    }

    /// Whether each of `lit` is within 2 of red, green, and blue of the
    /// one of `plain` in its place: as a color comes back from under a
    /// shade, less what the shade rounded off.
    fn restores(lit: &[Rgb], plain: &[Rgb]) -> bool {
        lit.len() == plain.len()
            && lit
                .iter()
                .zip(plain)
                .all(|(lit, plain)| lit.near(*plain, 2))
    }

    #[test]
    fn a_color_is_another_under_a_shade_that_leaves_a_fifth_of_it_to_all_but_the_whole() {
        // Half of each of red, green, and blue.
        assert_eq!(part(WHITE, gray(127)), Some(127));
        assert_eq!(part(BLUE, rgb(30, 60, 100)), Some(128));
        // A fifth of it, and no less.
        assert_eq!(part(WHITE, gray(51)), Some(51));
        assert_eq!(part(WHITE, gray(50)), None);
        // All but the whole, and never the whole or more.
        assert_eq!(part(gray(200), gray(199)), Some(255));
        assert_eq!(part(gray(200), gray(200)), None);
        assert_eq!(part(gray(127), WHITE), None);
        // The same part of red, green, and blue, within 12 of each.
        assert_eq!(part(rgb(200, 200, 0), rgb(100, 100, 12)), Some(128));
        assert_eq!(part(rgb(200, 200, 0), rgb(100, 100, 13)), None);
        // Black is a part of nothing.
        assert_eq!(part(gray(0), gray(0)), None);
    }

    #[test]
    fn each_two_colors_one_of_which_is_the_other_shaded_are_a_pair() {
        let colors = [BLUE, WHITE, rgb(30, 60, 100), gray(127)];
        assert_eq!(
            pairs(&colors),
            [
                Pair {
                    light: 0,
                    dark: 2,
                    part: 128
                },
                Pair {
                    light: 1,
                    dark: 3,
                    part: 127
                },
            ]
        );
    }

    #[test]
    fn a_shade_leaves_a_part_of_each_color_and_the_whole_comes_back_no_lighter_than_white() {
        assert_eq!(shaded(rgb(255, 100, 3), 128), rgb(127, 50, 1));
        assert_eq!(whole(rgb(127, 50, 1), 128), rgb(254, 100, 2));
        assert_eq!(whole(rgb(200, 50, 1), 128), rgb(255, 100, 2));
        assert_eq!(whole(rgb(200, 50, 1), 256), rgb(200, 50, 1));
    }

    #[test]
    fn what_a_color_s_pixels_show_is_the_middle_one_of_them() {
        let pixels = [
            (Some(1), rgb(10, 0, 50)),
            (Some(1), rgb(30, 0, 40)),
            (None, rgb(200, 200, 200)),
            (Some(1), rgb(20, 90, 60)),
            (Some(0), rgb(7, 8, 9)),
            (Some(5), rgb(1, 1, 1)),
        ];
        assert_eq!(
            held(pixels.into_iter(), 3),
            [(1, rgb(7, 8, 9)), (3, rgb(20, 0, 50)), (0, gray(0))]
        );
    }

    #[test]
    fn a_shade_over_any_side_of_a_picture_is_lifted() {
        let plain = block(WHITE, BLUE);
        for over in [
            Over::Below(7),
            Over::Above(7),
            Over::RightOf(9),
            Over::LeftOf(9),
            // An edge beside the block, which only the ground tells of.
            Over::Below(13),
            Over::LeftOf(2),
        ] {
            let shaded = under(&plain, |x, y| if over.covers(x, y) { 128 } else { 256 });
            let lit = lit_of(&shaded, &colors_of(&shaded));
            assert!(
                lit.as_deref().is_some_and(|lit| restores(lit, &plain)),
                "{over:?}: {lit:?}"
            );
        }
        // No shade, and nothing to lift.
        assert_eq!(lit_of(&plain, &colors_of(&plain)), None);
    }

    #[test]
    fn a_shade_s_edge_between_two_marks_lies_where_the_ground_changes() {
        // The block's color is the one pair: the ground is so dark that
        // the shade leaves it within one color. The block's pixels leave
        // the edge anywhere from 12 to 16 across, and the ground says 14.
        let ground = rgb(20, 40, 70);
        let plain: Vec<Rgb> = block(WHITE, ground)
            .into_iter()
            .zip(0_usize..)
            .map(|(pixel, at)| {
                let far =
                    (14..16).contains(&at.rem_euclid(16)) && (4..12).contains(&at.div_euclid(16));
                if far { WHITE } else { pixel }
            })
            .collect();
        let shaded = under(&plain, |x, _| if x >= 14 { 128 } else { 256 });
        let lit = lit_of(&shaded, &[ground, WHITE, gray(127)]).unwrap();
        assert!(restores(&lit, &plain), "{lit:?}");
    }

    #[test]
    fn a_soft_edge_keeps_what_its_row_shows_of_the_row_beyond() {
        let plain = block(WHITE, BLUE);
        // A row that keeps three quarters between the rows that keep all
        // and those that keep half.
        let soft = |_: usize, y: usize| match y {
            0..7 => 256,
            7 => 192,
            _ => 128,
        };
        let shaded = under(&plain, soft);
        let lit = lit_of(&shaded, &[BLUE, WHITE, rgb(30, 60, 100), gray(127)]).unwrap();
        assert!(restores(&lit, &plain), "{lit:?}");
    }

    #[test]
    fn a_row_keeps_what_most_of_its_pixels_show_of_the_row_beside_it() {
        let row = |levels: [u8; 4]| levels.map(gray);
        // Half of most, whatever one pixel shows.
        assert_eq!(
            kept(&row([50, 50, 200, 50]), &row([100, 100, 100, 100])),
            Some(128)
        );
        // Only pixels as light as 48 beyond tell, and half the row has to.
        assert_eq!(
            kept(&row([24, 24, 9, 9]), &row([48, 48, 47, 47])),
            Some(128)
        );
        assert_eq!(kept(&row([24, 9, 9, 9]), &row([48, 47, 47, 47])), None);
        assert_eq!(kept(&row([0, 0, 0, 0]), &row([0, 0, 0, 0])), None);
        // The lightest of red, green, and blue says how light.
        assert_eq!(kept(&[rgb(0, 30, 5)], &[rgb(60, 0, 10)]), Some(128));
    }

    #[test]
    fn text_in_two_brightnesses_on_one_ground_lies_under_no_shade() {
        // The lower half of the block is half as light, and the ground
        // is the same all over: gray text under white, or a two-tone
        // font.
        let plain = block(WHITE, BLUE);
        let two_tone: Vec<Rgb> = (0_usize..)
            .zip(&plain)
            .map(|(at, &pixel)| {
                if pixel == WHITE && at.div_euclid(16) >= 8 {
                    gray(127)
                } else {
                    pixel
                }
            })
            .collect();
        assert_eq!(lit_of(&two_tone, &[BLUE, WHITE, gray(127)]), None);
        // On a ground so dark that a shade leaves it as it is, the two
        // are one text with a shade over a part.
        let dark = gray(10);
        let on_dark: Vec<Rgb> = two_tone
            .iter()
            .map(|&pixel| if pixel == BLUE { dark } else { pixel })
            .collect();
        let lit = lit_of(&on_dark, &[dark, WHITE, gray(127)]).unwrap();
        // The ground under the shade comes back twice as light too.
        let whole: Vec<Rgb> = (0_usize..)
            .zip(block(WHITE, dark))
            .map(|(at, pixel)| {
                if pixel == dark && at.div_euclid(16) >= 8 {
                    gray(20)
                } else {
                    pixel
                }
            })
            .collect();
        assert!(restores(&lit, &whole), "{lit:?}");
    }

    #[test]
    fn a_few_pixels_that_show_the_light_side_s_ground_lie_under_a_shade() {
        // The shaded half's ground, but for a stripe of the ground as
        // it shows beside the shade: one in 16 of what the shade's
        // side shows shaded is too many, and fewer is not.
        let plain = block(WHITE, BLUE);
        let striped = |stripe: usize| -> Vec<Rgb> {
            let shaded = under(&plain, |_, y| if y >= 8 { 128 } else { 256 });
            (0_usize..)
                .zip(shaded)
                .map(|(at, pixel)| {
                    if at.div_euclid(16) == 15 && at.rem_euclid(16) < stripe {
                        BLUE
                    } else {
                        pixel
                    }
                })
                .collect()
        };
        let colors = [BLUE, WHITE, rgb(30, 60, 100), gray(127)];
        // 128 pixels under the shade, less the stripe's.
        assert!(lit_of(&striped(7), &colors).is_some());
        assert_eq!(lit_of(&striped(8), &colors), None);
    }

    /// A tally of `light` and `dark` pixels in each row.
    fn tally(light: &[u32], dark: &[u32]) -> Tally {
        Tally {
            light: light.to_vec(),
            dark: dark.to_vec(),
        }
    }

    #[test]
    fn an_edge_has_the_light_pixels_on_one_side_and_the_dark_on_the_other() {
        let edge = |light: &[u32], dark: &[u32]| tally(light, dark).edge();
        let found = |shade, places, wrong| {
            Some(Edge {
                shade,
                places,
                wrong,
            })
        };
        assert_eq!(
            edge(&[20, 20, 0, 0], &[0, 0, 20, 20]),
            found(Lies::After, 2..3, 0)
        );
        assert_eq!(
            edge(&[0, 0, 20, 20], &[20, 20, 0, 0]),
            found(Lies::Before, 2..3, 0)
        );
        // Rows that hold neither leave the edge anywhere among them.
        assert_eq!(
            edge(&[20, 0, 0, 0], &[0, 0, 0, 20]),
            found(Lies::After, 1..4, 0)
        );
        assert_eq!(
            edge(&[0, 0, 0, 20], &[20, 0, 0, 0]),
            found(Lies::Before, 1..4, 0)
        );
        // One in 16 of either may lie on the wrong side, and no more.
        assert_eq!(edge(&[30, 2], &[0, 32]), found(Lies::After, 1..2, 2));
        assert_eq!(edge(&[30, 3], &[0, 32]), None);
        assert_eq!(edge(&[32, 0], &[2, 30]), found(Lies::After, 1..2, 2));
        assert_eq!(edge(&[32, 0], &[3, 30]), None);
        // Each side has 16 pixels at least.
        assert_eq!(edge(&[16, 0], &[0, 16]), found(Lies::After, 1..2, 0));
        assert_eq!(edge(&[15, 0], &[0, 16]), None);
        assert_eq!(edge(&[16, 0], &[0, 15]), None);
        // Mixed all over, there is no edge.
        assert_eq!(edge(&[20, 20], &[20, 20]), None);
        assert_eq!(edge(&[], &[]), None);
    }

    #[test]
    fn an_edge_lies_at_the_place_with_the_fewest_pixels_on_the_wrong_side() {
        let edge = |shade| Edge {
            shade,
            places: 1..4,
            wrong: 0,
        };
        // The dark pixels start in the third row.
        let told = tally(&[0, 5, 0, 0], &[0, 0, 5, 0]);
        assert_eq!(told.settled(&edge(Lies::After)), 2..4);
        let told = tally(&[0, 0, 5, 0], &[0, 5, 0, 0]);
        assert_eq!(told.settled(&edge(Lies::Before)), 0..2);
        // Where no pixel tells, the shade is as small as can be.
        let told = tally(&[0; 4], &[0; 4]);
        assert_eq!(told.settled(&edge(Lies::After)), 3..4);
        assert_eq!(told.settled(&edge(Lies::Before)), 0..1);
    }

    #[test]
    fn an_edge_across_comes_before_one_down_that_parts_the_colors_as_well() {
        // The light block is above and left of the dark one, so a line
        // across parts them, and so does one down.
        let corner = |x: usize, y: usize| {
            if x < 8 && y < 8 {
                Some(0)
            } else if x >= 8 && y >= 8 {
                Some(1)
            } else {
                None
            }
        };
        let picture = |color: &dyn Fn(usize, usize) -> Option<u8>| -> (Vec<Rgb>, Vec<Option<u8>>) {
            (0..256_usize)
                .map(|at| {
                    let own = color(at.rem_euclid(16), at.div_euclid(16));
                    let shows = match own {
                        Some(0) => WHITE,
                        Some(_) => gray(127),
                        None => gray(0),
                    };
                    (shows, own)
                })
                .unzip()
        };
        let pair = Pair {
            light: 0,
            dark: 1,
            part: 127,
        };
        let shade = |color: &dyn Fn(usize, usize) -> Option<u8>| {
            let (pixels, own) = picture(color);
            Shade::over(&pixels, &own, 2, pair, SIZE).unwrap()
        };
        assert!(!shade(&corner).down);
        // With a dark pixel among the light rows, only a line down has
        // none on the wrong side.
        let stray = |x: usize, y: usize| {
            if (x, y) == (15, 0) {
                Some(1)
            } else {
                corner(x, y)
            }
        };
        assert!(shade(&stray).down);
        assert_eq!(shade(&stray).left(0, 16), 256);
        assert_eq!(shade(&stray).left(15, 16), 127);
        assert_eq!(shade(&corner).left(15, 16), 256);
        assert_eq!(shade(&corner).left(255, 16), 127);
    }
}
