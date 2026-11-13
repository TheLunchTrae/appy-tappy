//! Finding more text like one of a reference's boxes: the text elsewhere on
//! the same screenshot in the characters' color and at their size, so the
//! user adds it all at once instead of drawing a box around each.
//!
//! It looks at the ink of the whole screenshot, as the reference's color and
//! tolerance tell ink from background, and joins touching ink pixels into
//! blobs. Blobs side by side on one line, no further apart than the
//! sample's height, make a run of text. A run whose tallest character is as
//! tall as the sample's, within [`SIZE_PERCENT`], is a find. Ink much
//! taller than the sample, such as a panel in the characters' color, is left
//! out, and so is the run inside the sample's own region.
//!
//! A recording of the whole window looks the same way for text of several
//! sizes at once, one for each example the user drew ([`Sizes`]).

use std::collections::HashMap;

use appytappy_sources::screen::digits::DigitReader;
use appytappy_sources::screen::{Image, Region};

use super::ink::{Ink, Line};

/// The most finds one search returns; the rest are counted.
pub(crate) const MAX_FOUND: usize = 12;

/// The most runs of text one look over a whole window takes, nearest the
/// recording's region first.
pub(crate) const MAX_LINES: usize = 32;

/// How far a run's height may be from the sample's and still be the same
/// size, in percent of the sample's.
pub(crate) const SIZE_PERCENT: u32 = 15;

/// One size of text a look over a whole window takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextSize {
    /// How tall its characters are on the frame looked over, in pixels.
    pub(crate) height: u32,
    /// How tall the characters of the example were that it is the size
    /// of, on the screenshot the user drew it on. It is `height` until the
    /// window changes size.
    pub(crate) example: u32,
}

/// The sizes of text a look over a whole window takes, smallest first: one
/// for each example of text the user drew. An example's height is held
/// unless a look for one held before it takes text that tall, within
/// [`SIZE_PERCENT`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sizes(Vec<TextSize>);

impl Sizes {
    /// The one size `height`.
    pub(crate) fn of(height: u32) -> Self {
        Self(vec![TextSize {
            height,
            example: height,
        }])
    }

    /// Adds the example `height`, unless a look for a size it holds takes
    /// text that tall already. A height that no look takes is always
    /// added, so text of every example's height is looked for, whatever
    /// order the examples come in.
    pub(crate) fn add(&mut self, height: u32) {
        self.hold(TextSize {
            height,
            example: height,
        });
    }

    fn hold(&mut self, size: TextSize) {
        if !self
            .0
            .iter()
            .any(|held| same_size(size.height, held.height))
        {
            self.0.push(size);
            self.0.sort_unstable_by_key(|held| held.height);
        }
    }

    /// The sizes `scaled` makes of each height, such as those at another
    /// window size, each still the size of its example. A height it makes
    /// nothing of is left out, and heights that scale to one size are that
    /// one size, of the smallest of their examples.
    pub(crate) fn each(&self, scaled: impl Fn(u32) -> Option<u32>) -> Self {
        let mut sizes = Self(Vec::new());
        for size in &self.0 {
            if let Some(height) = scaled(size.height) {
                sizes.hold(TextSize { height, ..*size });
            }
        }
        sizes
    }

    /// Each height, smallest first.
    pub(crate) fn heights(&self) -> Vec<u32> {
        self.0.iter().map(|size| size.height).collect()
    }
}

/// One run of text a look over a whole window found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Spot {
    /// Where it is on the frame, with a margin around its ink.
    pub(crate) region: Region,
    /// The one of [`Sizes`] its characters have.
    pub(crate) size: TextSize,
}

/// One run of text like the sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Find {
    /// Where it is on the screenshot, with a margin around its ink.
    pub(crate) region: Region,
    /// Its ink for the text reader, or `None` when the margin left none.
    pub(crate) line: Option<Line>,
}

/// What a search found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Search {
    /// The finds, top to bottom and left to right, at most [`MAX_FOUND`].
    pub(crate) finds: Vec<Find>,
    /// Finds past [`MAX_FOUND`].
    pub(crate) more: usize,
    /// Runs in the characters' color of another size.
    pub(crate) other_sizes: usize,
}

/// A rectangle of ink, edges exclusive at the right and bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Bounds {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

impl Bounds {
    const fn height(self) -> u32 {
        self.bottom.saturating_sub(self.top)
    }

    #[cfg(test)]
    const fn width(self) -> u32 {
        self.right.saturating_sub(self.left)
    }

    const fn rows_meet(self, other: Self) -> bool {
        self.top < other.bottom && other.top < self.bottom
    }

    fn meets(self, region: Region) -> bool {
        self.left < region.right()
            && region.x() < self.right
            && self.top < region.bottom()
            && region.y() < self.bottom
    }

    /// Whether all of it lies inside `other`.
    const fn within(self, other: Self) -> bool {
        other.left <= self.left
            && self.right <= other.right
            && other.top <= self.top
            && self.bottom <= other.bottom
    }

    fn centered_in(self, region: Region) -> bool {
        let x = self.left.midpoint(self.right);
        let y = self.top.midpoint(self.bottom);
        (region.x()..region.right()).contains(&x) && (region.y()..region.bottom()).contains(&y)
    }

    fn join(self, other: Self) -> Self {
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }
}

/// Which pixels of a screenshot are ink, one flag a pixel, row by row.
struct Mask {
    ink: Vec<bool>,
    width: u32,
    height: u32,
}

impl Mask {
    fn of(image: &Image, reader: &DigitReader) -> Self {
        let size = image.size();
        Self {
            ink: image
                .pixels()
                .iter()
                .map(|&pixel| reader.is_ink(pixel))
                .collect(),
            width: size.width,
            height: size.height,
        }
    }

    fn index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = u64::from(y)
            .saturating_mul(u64::from(self.width))
            .saturating_add(u64::from(x));
        usize::try_from(index).ok()
    }

    fn at(&self, x: u32, y: u32) -> bool {
        self.index(x, y)
            .and_then(|index| self.ink.get(index))
            .copied()
            .unwrap_or(false)
    }

    /// The height of the band of adjacent inked rows in `region` that holds
    /// the most ink, as the reader takes the line of a region; `None`
    /// without ink.
    fn line_height(&self, region: Region) -> Option<u32> {
        let mut best: Option<(u32, usize)> = None;
        let mut run: Option<(u32, usize)> = None;
        for y in region.y()..=region.bottom() {
            let ink = if y < region.bottom() {
                (region.x()..region.right())
                    .filter(|&x| self.at(x, y))
                    .count()
            } else {
                0
            };
            if ink > 0 {
                let (rows, total) = run.get_or_insert((0, 0));
                *rows = rows.saturating_add(1);
                *total = total.saturating_add(ink);
            } else if let Some(ended) = run.take()
                && best.is_none_or(|(_, most)| ended.1 > most)
            {
                best = Some(ended);
            }
        }
        best.map(|(rows, _)| rows)
    }

    /// Each blob of touching ink, corners included, no taller than `tallest`.
    fn blobs(&self, tallest: u32) -> Vec<Bounds> {
        let mut seen = vec![false; self.ink.len()];
        let mut blobs = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                if let Some(blob) = self.fill(x, y, &mut seen)
                    && blob.height() <= tallest
                {
                    blobs.push(blob);
                }
            }
        }
        blobs
    }

    /// The bounds of the blob of ink at (`x`, `y`), marking its pixels in
    /// `seen`; `None` when that pixel is no ink or was seen already.
    fn fill(&self, x: u32, y: u32, seen: &mut [bool]) -> Option<Bounds> {
        let start = self.index(x, y)?;
        if !self.ink.get(start).copied().unwrap_or(false)
            || std::mem::replace(seen.get_mut(start)?, true)
        {
            return None;
        }
        let mut stack = vec![(x, y)];
        let mut bounds = Bounds {
            left: x,
            top: y,
            right: x.saturating_add(1),
            bottom: y.saturating_add(1),
        };
        // Each pixel is marked as it is pushed, so it comes off the stack at
        // most once and the mask's size bounds the fill.
        for _ in 0..self.ink.len() {
            let Some((px, py)) = stack.pop() else {
                break;
            };
            bounds = bounds.join(Bounds {
                left: px,
                top: py,
                right: px.saturating_add(1),
                bottom: py.saturating_add(1),
            });
            for (nx, ny) in neighbors(px, py) {
                let Some(next) = self.index(nx, ny) else {
                    continue;
                };
                if self.ink.get(next).copied().unwrap_or(false)
                    && let Some(flag) = seen.get_mut(next)
                    && !std::mem::replace(flag, true)
                {
                    stack.push((nx, ny));
                }
            }
        }
        Some(bounds)
    }
}

/// The eight pixels around (`x`, `y`) that do not fall off the top or left
/// edge; the mask drops those past the right and bottom.
fn neighbors(x: u32, y: u32) -> impl Iterator<Item = (u32, u32)> {
    let columns = [x.checked_sub(1), Some(x), x.checked_add(1)];
    let rows = [y.checked_sub(1), Some(y), y.checked_add(1)];
    rows.into_iter()
        .flatten()
        .flat_map(move |ny| columns.into_iter().flatten().map(move |nx| (nx, ny)))
        .filter(move |&(nx, ny)| (nx, ny) != (x, y))
}

/// A run of text: blobs side by side on one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    bounds: Bounds,
    /// The height of its tallest blob: its characters' height, which a
    /// comma or a mark below them does not stretch.
    tallest: u32,
}

impl Run {
    const fn of(blob: Bounds) -> Self {
        Self {
            bounds: blob,
            tallest: blob.height(),
        }
    }

    fn join(self, blob: Bounds) -> Self {
        Self {
            bounds: self.bounds.join(blob),
            tallest: self.tallest.max(blob.height()),
        }
    }
}

/// The bands of `band` rows that rows `top` to `bottom` (exclusive) cross.
fn bands(top: u32, bottom: u32, band: u32) -> std::ops::RangeInclusive<u32> {
    let first = top.checked_div(band).unwrap_or(top);
    let last = bottom
        .saturating_sub(1)
        .max(top)
        .checked_div(band)
        .unwrap_or(bottom);
    first..=last
}

/// Joins `blobs` into runs of text: a blob whose rows meet a run's and that
/// starts no further than `gap` past its right edge joins the first such
/// run. Runs are looked up by the bands of `band` rows they cross, and a run
/// the next blobs start too far right of is dropped from that lookup, so the
/// work grows with the blobs, not with blobs times runs.
fn runs(mut blobs: Vec<Bounds>, gap: u32, band: u32) -> Vec<Run> {
    blobs.sort_by_key(|blob| (blob.left, blob.top));
    let band = band.max(1);
    let mut runs: Vec<Run> = Vec::new();
    let mut open: HashMap<u32, Vec<usize>> = HashMap::new();
    for blob in blobs {
        let mut joined: Option<usize> = None;
        for key in bands(blob.top, blob.bottom, band) {
            let Some(ids) = open.get_mut(&key) else {
                continue;
            };
            // Blobs come left to right, so a run this blob starts too far
            // right of is too far from every later one too.
            ids.retain(|&id| {
                runs.get(id)
                    .is_some_and(|run| blob.left <= run.bounds.right.saturating_add(gap))
            });
            let first = ids
                .iter()
                .copied()
                .filter(|&id| runs.get(id).is_some_and(|run| run.bounds.rows_meet(blob)))
                .min();
            joined = match (joined, first) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
        let joined = joined.and_then(|id| runs.get_mut(id).map(|run| (id, run)));
        let id = if let Some((id, run)) = joined {
            *run = run.join(blob);
            id
        } else {
            runs.push(Run::of(blob));
            runs.len().saturating_sub(1)
        };
        if let Some(run) = runs.get(id) {
            for key in bands(run.bounds.top, run.bounds.bottom, band) {
                let ids = open.entry(key).or_default();
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
    }
    runs
}

/// Whether `height` is the sample's `sample` height, within [`SIZE_PERCENT`]
/// and at least a pixel.
fn same_size(height: u32, sample: u32) -> bool {
    let slack = sample.saturating_mul(SIZE_PERCENT).div_ceil(100).max(1);
    height.abs_diff(sample) <= slack
}

/// The ink of `image`, a find's pixels, for the text reader, as `reader`
/// tells ink from background; `None` when it shows none.
pub(crate) fn ink_line(image: &Image, reader: &DigitReader) -> Option<Line> {
    Ink::of(image, reader).line()
}

/// The height of the characters among `blobs` that `region` shows: that of
/// the tallest blob centered in it, so a taller one the region's edge
/// catches does not count, or else `line`, the region's line of ink. A blob
/// over twice as tall as `line` is no character of it.
fn sample_of(blobs: &[Bounds], region: Region, line: u32) -> u32 {
    let tallest = line.saturating_mul(2);
    blobs
        .iter()
        .filter(|blob| blob.height() <= tallest && blob.centered_in(region))
        .map(|blob| blob.height())
        .max()
        .unwrap_or(line)
}

/// `run` with a margin of a quarter of `sample` around its ink, kept
/// inside the mask; `None` when that leaves no region.
fn with_margin(run: Bounds, sample: u32, mask: &Mask) -> Option<Region> {
    let margin = sample.checked_div(4).unwrap_or(0).max(1);
    let left = run.left.saturating_sub(margin);
    let top = run.top.saturating_sub(margin);
    let right = run.right.saturating_add(margin).min(mask.width);
    let bottom = run.bottom.saturating_add(margin).min(mask.height);
    Region::new(
        left,
        top,
        right.saturating_sub(left),
        bottom.saturating_sub(top),
    )
}

/// How tall the characters are that each of `regions` of `image` shows, in
/// the ink `reader` tells from background: the sizes [`lines`] looks for.
/// `None` for a region that holds no ink or lies outside `image`. The
/// image's ink is read once for all of them.
pub(crate) fn sample_heights(
    image: &Image,
    regions: &[Region],
    reader: &DigitReader,
) -> Vec<Option<u32>> {
    let mask = Mask::of(image, reader);
    let lines: Vec<Option<u32>> = regions
        .iter()
        .map(|&region| {
            mask.line_height(region)
                .filter(|_| region.fits(image.size()))
        })
        .collect();
    let tallest = lines.iter().flatten().copied().max().unwrap_or(0);
    let blobs = mask.blobs(tallest.saturating_mul(2));
    regions
        .iter()
        .zip(lines)
        .map(|(&region, line)| Some(sample_of(&blobs, region, line?)))
        .collect()
}

/// How far the middle of `run` is from the middle of `near`, squared, in
/// doubled pixels so that neither middle is rounded.
fn distance(run: Bounds, near: Region) -> u64 {
    let across = u64::from(run.left).saturating_add(u64::from(run.right));
    let down = u64::from(run.top).saturating_add(u64::from(run.bottom));
    let x = across.abs_diff(u64::from(near.x()).saturating_add(u64::from(near.right())));
    let y = down.abs_diff(u64::from(near.y()).saturating_add(u64::from(near.bottom())));
    x.saturating_mul(x).saturating_add(y.saturating_mul(y))
}

/// Where `image` shows text with characters as tall as one of `sizes`,
/// within [`SIZE_PERCENT`], in the ink `reader` tells from background: each
/// run with a margin around its ink and the size it has, the run nearest
/// the middle of `near` first, and of runs as near the higher one, then the
/// one further left. A recording of the whole window reads these, so it
/// takes the text beside its examples too, wherever the window shows it. It
/// takes the [`MAX_LINES`] nearest, so a window full of text in the
/// characters' color does not crowd out the text around the region.
///
/// Each size is looked for by itself, as [`like`] looks for one: among the
/// ink no taller than twice the size, joined into runs no further apart
/// than the size. Ink that is part of a smaller size's run is left out of
/// the larger sizes' runs, so it is taken once, and text of a larger size
/// beside it is still found.
pub(crate) fn lines(image: &Image, sizes: &Sizes, near: Region, reader: &DigitReader) -> Vec<Spot> {
    let mask = Mask::of(image, reader);
    let largest = sizes.0.last().map_or(0, |size| size.height);
    let blobs = mask.blobs(largest.saturating_mul(2));
    let mut found: Vec<(Bounds, TextSize)> = Vec::new();
    for &size in &sizes.0 {
        let height = size.height;
        let tallest = height.saturating_mul(2);
        // The ink of a smaller size's run is left out before the runs are
        // joined, so text of this size beside it is still a run of its own.
        let inked = blobs
            .iter()
            .copied()
            .filter(|blob| blob.height() <= tallest)
            .filter(|blob| !found.iter().any(|&(taken, _)| blob.within(taken)))
            .collect();
        let lines = runs(inked, height, height)
            .into_iter()
            .filter(|run| same_size(run.tallest, height));
        found.extend(lines.map(|run| (run.bounds, size)));
    }
    found.sort_by_key(|&(run, _)| (distance(run, near), run.top, run.left));
    found
        .into_iter()
        .filter_map(|(run, size)| {
            let region = with_margin(run, size.height, &mask)?;
            Some(Spot { region, size })
        })
        .take(MAX_LINES)
        .collect()
}

/// The text on `image` like the sample in `region`: in the ink `reader`
/// tells from background and with characters as tall as the sample's,
/// leaving out text that meets any of `skip`, such as finds added already.
/// `None` when `region` holds no ink or lies outside `image`.
pub(crate) fn like(
    image: &Image,
    region: Region,
    skip: &[Region],
    reader: &DigitReader,
) -> Option<Search> {
    if !region.fits(image.size()) {
        return None;
    }
    let mask = Mask::of(image, reader);
    let line = mask.line_height(region)?;
    let blobs = mask.blobs(line.saturating_mul(2));
    let sample = sample_of(&blobs, region, line);
    let mut finds = Vec::new();
    let mut other_sizes = 0_usize;
    for run in runs(blobs, sample, sample) {
        if run.bounds.meets(region) || skip.iter().any(|&added| run.bounds.meets(added)) {
            continue;
        }
        if same_size(run.tallest, sample) {
            finds.push(run.bounds);
        } else if run.tallest.saturating_mul(2) >= sample {
            // Specks of ink are not text of another size.
            other_sizes = other_sizes.saturating_add(1);
        }
    }
    finds.sort_by_key(|run| (run.top, run.left));
    let more = finds.len().saturating_sub(MAX_FOUND);
    let finds = finds
        .into_iter()
        .take(MAX_FOUND)
        .filter_map(|run| {
            let region = with_margin(run, sample, &mask)?;
            let line = image.crop(region).and_then(|crop| ink_line(&crop, reader));
            Some(Find { region, line })
        })
        .collect();
    Some(Search {
        finds,
        more,
        other_sizes,
    })
}

#[cfg(test)]
mod tests {
    use appytappy_sources::screen::Size;
    use appytappy_sources::system::Rgb;

    use super::*;

    const INK: [u8; 4] = [0, 0, 255, 0];
    const PAPER: [u8; 4] = [40, 40, 40, 0];

    /// A `width` by `height` screenshot with ink inside each of `boxes`.
    fn screenshot(width: u32, height: u32, boxes: &[(u32, u32, u32, u32)]) -> Image {
        let pixels = (0..height)
            .flat_map(|y| {
                (0..width).flat_map(move |x| {
                    let inked = boxes.iter().any(|&(left, top, w, h)| {
                        (left..left.saturating_add(w)).contains(&x)
                            && (top..top.saturating_add(h)).contains(&y)
                    });
                    if inked { INK } else { PAPER }
                })
            })
            .collect();
        Image::new(Size { width, height }, pixels).expect("matching length")
    }

    fn reader() -> DigitReader {
        DigitReader::new(
            Rgb {
                red: 255,
                green: 0,
                blue: 0,
            },
            10,
        )
    }

    fn region(x: u32, y: u32, width: u32, height: u32) -> Region {
        Region::new(x, y, width, height).expect("not empty")
    }

    /// A run of text `size` tall found in that region.
    fn spot(size: u32, x: u32, y: u32, width: u32, height: u32) -> Spot {
        Spot {
            region: region(x, y, width, height),
            size: TextSize {
                height: size,
                example: size,
            },
        }
    }

    /// The sizes `heights`, added in that order.
    fn sizes(first: u32, more: &[u32]) -> Sizes {
        let mut sizes = Sizes::of(first);
        for &height in more {
            sizes.add(height);
        }
        sizes
    }

    /// Two characters 3 wide and `height` tall, a pixel apart, at (`x`, `y`).
    fn pair(x: u32, y: u32, height: u32) -> [(u32, u32, u32, u32); 2] {
        [(x, y, 3, height), (x.saturating_add(4), y, 3, height)]
    }

    #[test]
    fn text_of_the_same_size_elsewhere_is_found_with_a_margin() {
        let boxes = [pair(2, 2, 8), pair(30, 20, 8)].concat();
        let image = screenshot(60, 40, &boxes);
        let search =
            like(&image, region(0, 0, 12, 12), &[], &reader()).expect("the sample has ink");
        assert_eq!(search.more, 0);
        assert_eq!(search.other_sizes, 0);
        assert_eq!(search.finds.len(), 1);
        let find = &search.finds[0];
        // Ink 30..37 by 20..28, with a margin of a quarter of 8.
        assert_eq!(find.region, region(28, 18, 11, 12));
        let line = find.line.as_ref().expect("the find has ink");
        assert!(line.pixels.contains(&0), "the line carries the ink");
    }

    #[test]
    fn the_lines_of_a_window_are_every_run_of_the_sample_s_size_the_sample_s_too() {
        // The sample, text of its size elsewhere, and text of another size.
        let boxes = [pair(2, 2, 8), pair(30, 20, 8), pair(30, 2, 14)].concat();
        let image = screenshot(60, 40, &boxes);
        let place = region(0, 0, 12, 12);
        // The last holds ink and reaches past the picture's right edge.
        let nowhere = [place, region(0, 30, 10, 10), region(28, 18, 40, 12)];
        assert_eq!(
            sample_heights(&image, &nowhere, &reader()),
            [Some(8), None, None],
            "a place without ink, or past the picture's edge, shows no size"
        );
        assert_eq!(
            lines(&image, &Sizes::of(8), place, &reader()),
            [spot(8, 0, 0, 11, 12), spot(8, 28, 18, 11, 12)],
            "each with a margin, kept inside the picture"
        );
    }

    #[test]
    fn each_region_s_size_is_measured_among_ink_up_to_twice_as_tall_as_its_line() {
        // Four strokes: 8 tall, 40 tall, 20 tall, and 24 tall.
        let strokes = [
            (2, 2, 3, 8),
            (20, 0, 3, 40),
            (30, 10, 3, 20),
            (40, 0, 3, 24),
        ];
        let image = screenshot(60, 44, &strokes);
        let regions = [
            region(0, 0, 8, 12),
            // Ten rows from the middle of the stroke 40 tall, which is over
            // twice as tall as they are.
            region(18, 15, 8, 10),
            // Ten rows from the middle of the stroke 20 tall.
            region(28, 15, 8, 10),
            region(38, 0, 8, 26),
        ];
        assert_eq!(
            sample_heights(&image, &regions, &reader()),
            [Some(8), Some(10), Some(20), Some(24)]
        );
    }

    #[test]
    fn the_lines_nearest_a_place_come_first_and_the_furthest_are_left_out() {
        // The text beside the place comes before the text above it on the
        // picture, which top to bottom would put first.
        let boxes = [pair(30, 2, 8), pair(30, 22, 8)].concat();
        let image = screenshot(60, 40, &boxes);
        assert_eq!(
            lines(&image, &Sizes::of(8), region(2, 22, 8, 8), &reader()),
            [spot(8, 28, 20, 11, 12), spot(8, 28, 0, 11, 12)]
        );
        // A column of 35 runs, 12 pixels apart: three more than one look
        // takes.
        let column: Vec<_> = (0..35_u32)
            .flat_map(|row| pair(2, row.saturating_mul(12).saturating_add(2), 8))
            .collect();
        let tall = screenshot(20, 424, &column);
        let taken = lines(&tall, &Sizes::of(8), region(0, 0, 12, 12), &reader());
        assert_eq!(taken.len(), MAX_LINES);
        let lowest = taken.iter().map(|line| line.region.y()).max();
        assert_eq!(lowest, Some(372), "the three lowest are left out");
    }

    #[test]
    fn sizes_hold_each_height_apart_from_the_others_smallest_first() {
        assert_eq!(sizes(20, &[40, 10, 30]).heights(), [10, 20, 30, 40]);
        // A look for 20 takes text 17 to 23 tall, so those are no other
        // size. 16 and 24 are, though a look for 24 takes text 20 tall too.
        assert_eq!(sizes(20, &[17, 23, 20]).heights(), [20]);
        assert_eq!(sizes(20, &[24, 16]).heights(), [16, 20, 24]);
        // A look for 7 takes text 5 tall, though one for 5 would not take 7.
        assert_eq!(sizes(7, &[5]).heights(), [7]);
        assert_eq!(sizes(5, &[7]).heights(), [5, 7]);
    }

    #[test]
    fn sizes_made_of_each_height_hold_what_is_made_apart_and_leave_out_what_is_not() {
        let held = sizes(8, &[20, 40]);
        let twice = held.each(|height| height.checked_mul(2));
        assert_eq!(twice.heights(), [16, 40, 80]);
        let examples = |sizes: &Sizes| -> Vec<u32> {
            let all = sizes.0.iter();
            all.map(|size| size.example).collect()
        };
        assert_eq!(examples(&twice), [8, 20, 40], "each is of its example");
        // 8 and 20 both come to 2 at a tenth, rounded, so they are one size.
        let tenth = held.each(|height| Some(height.saturating_add(5).checked_div(10)?.max(2)));
        assert_eq!(tenth.heights(), [2, 4]);
        assert_eq!(examples(&tenth), [8, 40], "the smaller example's");
        let some = held.each(|height| (height != 20).then_some(height));
        assert_eq!(some.heights(), [8, 40]);
        assert_eq!(held.each(|_| None).heights(), [0_u32; 0]);
    }

    #[test]
    fn the_lines_of_a_window_are_the_runs_of_each_size_looked_for() {
        let boxes = [pair(2, 2, 8), pair(2, 24, 14), pair(30, 2, 20)].concat();
        let image = screenshot(60, 44, &boxes);
        let place = region(0, 0, 12, 12);
        assert_eq!(
            lines(&image, &sizes(8, &[14]), place, &reader()),
            // A margin of a quarter of each run's own size.
            [spot(8, 0, 0, 11, 12), spot(14, 0, 21, 12, 20)],
            "text 20 tall is neither size"
        );
    }

    #[test]
    fn a_run_two_sizes_take_is_a_line_once_at_the_smaller_size() {
        // 12 is within 15% of both 10 and 13.
        let image = screenshot(60, 40, &pair(4, 4, 12));
        assert_eq!(
            lines(&image, &sizes(10, &[13]), region(0, 0, 12, 12), &reader()),
            [spot(10, 2, 2, 11, 16)]
        );
    }

    #[test]
    fn an_example_just_past_a_held_size_s_reach_is_looked_for_too() {
        // A look for 20 takes text up to 23 tall, so text 24 tall is found
        // only because 24 is held as a size of its own.
        let image = screenshot(60, 40, &pair(4, 4, 24));
        assert_eq!(
            lines(&image, &sizes(20, &[24]), region(0, 0, 12, 12), &reader()),
            [spot(24, 0, 0, 17, 34)]
        );
    }

    #[test]
    fn text_of_a_larger_size_beside_text_of_a_smaller_one_is_a_line_too() {
        // A character 8 tall 11 past text 16 tall: too far to join it for
        // the look for 8, and near enough for the look for 16, whose run it
        // must not take with it.
        let mut boxes = pair(4, 4, 16).to_vec();
        boxes.push((22, 4, 3, 8));
        let image = screenshot(60, 40, &boxes);
        assert_eq!(
            lines(&image, &sizes(8, &[16]), region(0, 0, 12, 12), &reader()),
            [spot(16, 0, 0, 15, 24), spot(8, 20, 2, 7, 12)]
        );
    }

    #[test]
    fn the_nearest_runs_of_all_sizes_together_are_taken() {
        // Text 16 tall in the place, and 33 runs of text 8 tall further off,
        // in 3 rows of 11.
        let mut boxes = pair(2, 2, 16).to_vec();
        for row in [40_u32, 54, 68] {
            for column in 0..11_u32 {
                boxes.extend(pair(column.saturating_mul(20).saturating_add(2), row, 8));
            }
        }
        let image = screenshot(230, 80, &boxes);
        let taken = lines(&image, &sizes(8, &[16]), region(0, 0, 12, 20), &reader());
        assert_eq!(taken.len(), MAX_LINES);
        assert_eq!(taken[0], spot(16, 0, 0, 13, 22), "the nearest is first");
        let small = taken.iter().filter(|spot| spot.size.height == 8).count();
        assert_eq!(small, 31);
        // The run left out is the furthest: the last of the bottom row.
        let last = region(200, 66, 11, 12);
        assert!(taken.iter().all(|spot| spot.region != last));
        assert!(taken.contains(&spot(8, 180, 66, 11, 12)));
    }

    #[test]
    fn each_size_s_runs_join_only_ink_up_to_twice_as_tall_as_it() {
        let place = region(0, 0, 12, 12);
        // Beside text 8 tall, ink 16 tall is part of its run, which is then
        // not 8 tall, even while a look for text 30 tall goes on too.
        let mut boxes = pair(2, 2, 8).to_vec();
        boxes.push((10, 2, 3, 16));
        let image = screenshot(60, 40, &boxes);
        assert_eq!(lines(&image, &sizes(8, &[30]), place, &reader()), []);
        // Ink 17 tall is no part of it, though the look for 30 joins it.
        let mut boxes = pair(2, 2, 8).to_vec();
        boxes.push((10, 2, 3, 17));
        let image = screenshot(60, 40, &boxes);
        assert_eq!(
            lines(&image, &sizes(8, &[30]), place, &reader()),
            [spot(8, 0, 0, 11, 12)]
        );
    }

    #[test]
    fn text_of_another_size_is_counted_and_left_out() {
        let boxes = [pair(2, 2, 8), pair(30, 20, 14), pair(30, 2, 9)].concat();
        let image = screenshot(60, 40, &boxes);
        let search =
            like(&image, region(0, 0, 12, 12), &[], &reader()).expect("the sample has ink");
        // 9 is within 15% of 8 (a slack of 2); 14 is not.
        assert_eq!(search.finds.len(), 1);
        assert_eq!(search.finds[0].region, region(28, 0, 11, 13));
        assert_eq!(search.other_sizes, 1);
    }

    #[test]
    fn specks_and_tall_ink_are_neither_finds_nor_other_sizes() {
        // A 1 by 2 speck, and a 6 by 30 bar more than twice the sample's 8.
        let mut boxes = pair(2, 2, 8).to_vec();
        boxes.extend([(40, 30, 1, 2), (50, 2, 6, 30)]);
        let image = screenshot(60, 40, &boxes);
        let search =
            like(&image, region(0, 0, 12, 12), &[], &reader()).expect("the sample has ink");
        assert_eq!(search.finds, []);
        assert_eq!(search.other_sizes, 0);
    }

    #[test]
    fn characters_further_apart_than_the_sample_is_tall_are_separate_runs() {
        // The sample is 8 tall; the second pair starts 9 past the first's edge.
        let boxes = [pair(2, 2, 8), pair(2, 20, 8), pair(20, 20, 8)].concat();
        let image = screenshot(60, 40, &boxes);
        let search =
            like(&image, region(0, 0, 12, 12), &[], &reader()).expect("the sample has ink");
        let lefts: Vec<u32> = search.finds.iter().map(|find| find.region.x()).collect();
        assert_eq!(lefts, [0, 18]);
    }

    #[test]
    fn finds_go_top_to_bottom_then_left_to_right_and_past_the_most_are_counted() {
        // Fourteen single characters, 4 by 6, on rows two apart.
        let mut boxes = vec![(1, 1, 4, 6)];
        for row in 0..7 {
            for column in 0..2 {
                boxes.push((20 + column * 20, 1 + row * 9, 4, 6));
            }
        }
        let image = screenshot(64, 66, &boxes);
        let search = like(&image, region(0, 0, 6, 8), &[], &reader()).expect("the sample has ink");
        assert_eq!(search.finds.len(), MAX_FOUND);
        assert_eq!(search.more, 14 - MAX_FOUND);
        let first: Vec<(u32, u32)> = search
            .finds
            .iter()
            .take(3)
            .map(|find| (find.region.x(), find.region.y()))
            .collect();
        assert_eq!(first, [(19, 0), (39, 0), (19, 9)]);
    }

    #[test]
    fn a_region_without_ink_or_outside_the_screenshot_finds_nothing() {
        let image = screenshot(20, 20, &pair(10, 10, 6));
        assert_eq!(like(&image, region(0, 0, 5, 5), &[], &reader()), None);
        assert_eq!(like(&image, region(15, 15, 10, 10), &[], &reader()), None);
    }

    #[test]
    fn a_find_at_the_edge_keeps_its_margin_inside_the_screenshot() {
        let boxes = [pair(30, 2, 8), pair(0, 32, 8)].concat();
        let image = screenshot(40, 40, &boxes);
        let search =
            like(&image, region(28, 0, 12, 12), &[], &reader()).expect("the sample has ink");
        assert_eq!(search.finds[0].region, region(0, 30, 9, 10));
    }

    #[test]
    fn a_speck_by_the_sample_does_not_change_its_size() {
        // A one-row speck above the 8-row characters does not stretch them.
        let mut boxes = pair(2, 4, 8).to_vec();
        boxes.push((2, 1, 1, 1));
        boxes.extend(pair(30, 20, 8));
        let image = screenshot(60, 40, &boxes);
        let search =
            like(&image, region(0, 0, 12, 14), &[], &reader()).expect("the sample has ink");
        assert_eq!(search.finds.len(), 1);
    }

    #[test]
    fn a_comma_below_the_characters_does_not_change_their_size() {
        // The find's comma reaches 4 rows below its 8-row characters, a pixel
        // to their right.
        let mut boxes = [pair(2, 2, 8), pair(30, 20, 8)].concat();
        boxes.push((38, 25, 2, 7));
        let image = screenshot(60, 40, &boxes);
        let search =
            like(&image, region(0, 0, 12, 12), &[], &reader()).expect("the sample has ink");
        assert_eq!(search.other_sizes, 0);
        assert_eq!(search.finds.len(), 1);
        assert_eq!(search.finds[0].region, region(28, 18, 14, 16));
    }

    #[test]
    fn text_meeting_a_skipped_region_is_neither_found_nor_counted() {
        // Fourteen single characters, as above; the first twelve finds were
        // added, so a second search offers the last two.
        let mut boxes = vec![(1, 1, 4, 6)];
        for row in 0..7 {
            for column in 0..2 {
                boxes.push((20 + column * 20, 1 + row * 9, 4, 6));
            }
        }
        let image = screenshot(64, 66, &boxes);
        let first = like(&image, region(0, 0, 6, 8), &[], &reader()).expect("the sample has ink");
        let added: Vec<Region> = first.finds.iter().map(|find| find.region).collect();
        let second =
            like(&image, region(0, 0, 6, 8), &added, &reader()).expect("the sample has ink");
        assert_eq!(second.more, 0);
        assert_eq!(second.other_sizes, 0);
        let rest: Vec<(u32, u32)> = second
            .finds
            .iter()
            .map(|find| (find.region.x(), find.region.y()))
            .collect();
        assert_eq!(rest, [(19, 54), (39, 54)]);
    }

    #[test]
    fn runs_join_only_blobs_on_shared_rows_within_the_gap() {
        let blob = |left: u32, top: u32| Bounds {
            left,
            top,
            right: left.saturating_add(3),
            bottom: top.saturating_add(8),
        };
        // Two lines; on the first, a blob 8 past the run joins and one 9 past
        // starts a new run; a blob on the second line is its own run.
        let found = runs(
            vec![blob(0, 0), blob(11, 4), blob(23, 0), blob(0, 30)],
            8,
            8,
        );
        let bounds: Vec<(u32, u32, u32, u32)> = found
            .iter()
            .map(|run| {
                (
                    run.bounds.left,
                    run.bounds.top,
                    run.bounds.right,
                    run.bounds.bottom,
                )
            })
            .collect();
        assert_eq!(bounds, [(0, 0, 14, 12), (0, 30, 3, 38), (23, 0, 26, 8)]);
    }

    #[test]
    fn bounds_measure_and_meet() {
        let bounds = Bounds {
            left: 2,
            top: 3,
            right: 6,
            bottom: 8,
        };
        assert_eq!((bounds.width(), bounds.height()), (4, 5));
        assert!(bounds.meets(region(5, 7, 1, 1)));
        assert!(!bounds.meets(region(6, 3, 1, 1)));
        assert!(!bounds.meets(region(2, 8, 1, 1)));
        assert!(!bounds.meets(region(0, 0, 2, 9)));
        assert!(!bounds.meets(region(0, 0, 9, 3)));
        assert!(bounds.centered_in(region(4, 5, 1, 1)));
        assert!(!bounds.centered_in(region(5, 7, 1, 1)));
        assert!(!bounds.centered_in(region(4, 0, 1, 2)), "only its column");
        assert!(!bounds.centered_in(region(0, 5, 2, 1)), "only its row");
    }

    #[test]
    fn runs_leave_out_blobs_whose_rows_only_touch() {
        let blob = |left: u32, top: u32| Bounds {
            left,
            top,
            right: left.saturating_add(3),
            bottom: top.saturating_add(8),
        };
        // The second blob ends on the row the first starts on, and the third
        // starts on the row the first ends on.
        let found = runs(vec![blob(0, 10), blob(4, 2), blob(4, 18)], 8, 8);
        assert_eq!(found.len(), 3);
    }

    #[test]
    fn the_mask_reads_no_ink_past_its_edges() {
        // Ink only at (0, 1) of a 3 by 2 screenshot: (3, 0) would land on it
        // if the row wrapped.
        let image = screenshot(3, 2, &[(0, 1, 1, 1)]);
        let mask = Mask::of(&image, &reader());
        assert!(mask.at(0, 1));
        assert_eq!(mask.index(3, 0), None);
        assert_eq!(mask.index(0, 2), None);
        assert!(!mask.at(3, 0));
    }

    #[test]
    fn the_line_height_counts_rows_inside_the_region_and_takes_the_first_of_a_tie() {
        // Ink 4 wide on rows 0 and 1, and 2 wide on rows 4 to 7: 8 pixels each.
        let image = screenshot(6, 12, &[(0, 0, 4, 2), (0, 4, 2, 4), (0, 9, 6, 3)]);
        let mask = Mask::of(&image, &reader());
        assert_eq!(mask.line_height(region(0, 0, 6, 8)), Some(2));
        // The region ends inside the ink below it.
        assert_eq!(mask.line_height(region(0, 9, 6, 2)), Some(2));
    }
}
