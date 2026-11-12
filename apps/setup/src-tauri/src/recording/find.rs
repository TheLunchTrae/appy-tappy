//! Finding more text like a reference's first sample: the text elsewhere on
//! the same screenshot in the characters' color and at their size, so the
//! user can label it all at once instead of drawing a region around each.
//!
//! It looks at the ink of the whole screenshot, as the reference's color and
//! tolerance tell ink from background, and joins touching ink pixels into
//! blobs. Blobs side by side on one line, no further apart than the
//! sample's height, make a run of text. A run whose tallest character is as
//! tall as the sample's, within [`SIZE_PERCENT`], is a find. Ink much
//! taller than the sample, such as a panel in the characters' color, is left
//! out, and so is the run inside the sample's own region.

use std::collections::HashMap;

use appytappy_sources::screen::digits::DigitReader;
use appytappy_sources::screen::{Image, Region};

use super::ink::{Ink, Line};

/// The most finds one search returns; the rest are counted.
pub(crate) const MAX_FOUND: usize = 12;

/// How far a run's height may be from the sample's and still be the same
/// size, in percent of the sample's.
pub(crate) const SIZE_PERCENT: u32 = 15;

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
        if !self.ink.get(start).copied().unwrap_or(false) {
            return None;
        }
        let flag = seen.get_mut(start)?;
        if *flag {
            return None;
        }
        *flag = true;
        let mut stack = vec![(x, y)];
        let mut bounds = Bounds {
            left: x,
            top: y,
            right: x.saturating_add(1),
            bottom: y.saturating_add(1),
        };
        while let Some((px, py)) = stack.pop() {
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
                    && !*flag
                {
                    *flag = true;
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
    // Measured as each run is: by its tallest character.
    let sample = blobs
        .iter()
        .filter(|blob| blob.meets(region))
        .map(|blob| blob.height())
        .max()
        .unwrap_or(line);
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
    let margin = sample.checked_div(4).unwrap_or(0).max(1);
    let finds = finds
        .into_iter()
        .take(MAX_FOUND)
        .filter_map(|run| {
            let left = run.left.saturating_sub(margin);
            let top = run.top.saturating_sub(margin);
            let right = run.right.saturating_add(margin).min(mask.width);
            let bottom = run.bottom.saturating_add(margin).min(mask.height);
            let region = Region::new(
                left,
                top,
                right.saturating_sub(left),
                bottom.saturating_sub(top),
            )?;
            let line = image
                .crop(region)
                .and_then(|crop| Ink::of(&crop, reader).line());
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
    }
}
