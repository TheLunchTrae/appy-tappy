//! A character's shape, whatever its size and color, and how far it is
//! from another.
//!
//! A character is scaled to a grid by its own bounds, so the same character
//! has the same shape at any size and wherever it lands on the pixel grid.
//! Its bounds and its ink are taken to a fraction of a pixel: a pixel on
//! an anti-aliased edge is the character's by how much of it the character
//! covers ([`Layers::share`]). Each cell holds how much of it is ink, not
//! whether it is. Two shapes are as far apart as the ink only one of them
//! has is of the ink either has.
//!
//! [`Layers::share`]: super::layers::Layers::share
//!
//! Scaling by its own bounds makes a `.` as large as a `0`. So a character
//! also carries its [`Place`]: how tall it is and how far down it starts,
//! both against the line it stands in. Two characters that differ in those
//! by more than [`LINE_SLACK`], or one of which is more than twice as wide
//! for its height as the other, are as far apart as shapes can be,
//! whatever their ink.

use std::ops::Range;

/// The grid's rows and columns. A character is scaled until the longer of
/// its width and height fills them, or as far as [`SMALLEST`] lets a small
/// one, and centered.
const GRID: u32 = 32;

/// The grid's cells.
const CELLS: usize = (GRID * GRID) as usize;

/// How much of a cell is ink when all of it is.
const FULL: u64 = u8::MAX as u64;

/// A line's height in the units a character's height and top are held in.
const LINE: u32 = 32;

/// How far two characters' heights, or tops, may differ and their shapes
/// still be compared: a quarter of the line's height. A `.` and a `,` are
/// closer than that, and their shapes tell them apart.
const LINE_SLACK: u8 = 8;

/// As far apart as two shapes can be.
pub(super) const FARTHEST: u16 = 1000;

/// The cells along each side of a block: what one shape has and another
/// does not is summed by block.
const BLOCK: u32 = 4;

/// The blocks along each side of the grid.
const BLOCKS: u32 = GRID / BLOCK;

/// The ink of a window of two blocks by two that is all ink.
const WINDOW: u32 = (2 * BLOCK) * (2 * BLOCK) * u8::MAX as u32;

/// Which cells of a character's grid are at least half ink, one row of
/// cells in the bits of each `u32`, from the top. Two characters with the
/// same cells look alike to a reader, so [`DigitReader::shapes`] names a
/// character's shape by them.
///
/// [`DigitReader::shapes`]: super::DigitReader::shapes
pub type Shape = [u32; GRID as usize];

/// Where a character is in its image: its columns and the rows of them
/// that hold its ink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Bounds {
    pub(super) columns: Range<u32>,
    pub(super) rows: Range<u32>,
}

impl Bounds {
    pub(super) const fn height(&self) -> u32 {
        self.rows.end.saturating_sub(self.rows.start)
    }

    const fn width(&self) -> u32 {
        self.columns.end.saturating_sub(self.columns.start)
    }
}

/// The line a row of characters stands in: what their heights and tops are
/// measured against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Line {
    /// The row its tall characters start at.
    top: u32,
    /// How tall its tall characters are.
    height: u32,
}

impl Line {
    /// The line of `characters`, or `None` without any. Its tall
    /// characters are those at least half as tall as the tallest, so a
    /// `.` or a `-` beside digits does not count. The line is as tall as
    /// the middle one of them, the taller of two, so one bracket that
    /// reaches past the digits does not stretch it, and starts where the
    /// highest of them does.
    pub(super) fn of(characters: &[Bounds]) -> Option<Self> {
        let tallest = characters.iter().map(Bounds::height).max()?;
        let tall = || {
            characters
                .iter()
                .filter(move |bounds| bounds.height().saturating_mul(2) >= tallest)
        };
        let mut heights: Vec<u32> = tall().map(Bounds::height).collect();
        heights.sort_unstable();
        Some(Self {
            top: tall().map(|bounds| bounds.rows.start).min()?,
            height: heights.get(heights.len().div_euclid(2)).copied()?,
        })
    }

    /// The line's height in pixels.
    pub(super) const fn height(self) -> u32 {
        self.height
    }

    /// `pixels` in the units a character's height and top are held in.
    fn units(self, pixels: u32) -> u8 {
        let units = u64::from(pixels)
            .saturating_mul(u64::from(LINE))
            .checked_div(u64::from(self.height))
            .unwrap_or(0);
        u8::try_from(units).unwrap_or(u8::MAX)
    }
}

/// Where a character stands in its line, and its proportions: what tells a
/// `.` from a `0` once both fill the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Place {
    /// Its height, where a line's is [`LINE`].
    height: u8,
    /// How far below the line's top it starts, in the same units.
    top: u8,
    /// Its width and height in pixels.
    size: (u32, u32),
    /// Its line's height in pixels.
    line: u32,
}

impl Place {
    /// The place of the character within `bounds`, standing in `line`.
    pub(super) fn of(bounds: &Bounds, line: Line) -> Self {
        Self {
            height: line.units(bounds.height()),
            top: line.units(bounds.rows.start.saturating_sub(line.top)),
            size: (bounds.width(), bounds.height()),
            line: line.height,
        }
    }

    /// Whether a character here and one at `other` are too unlike for
    /// their shapes to say anything: their heights or tops differ by more
    /// than [`LINE_SLACK`], or one is more than twice as wide for its
    /// height as the other.
    pub(super) fn unlike(self, other: Self) -> bool {
        let wide =
            |one: Self, other: Self| u64::from(one.size.0).saturating_mul(u64::from(other.size.1));
        let (ours, theirs) = (wide(self, other), wide(other, self));
        self.height.abs_diff(other.height) > LINE_SLACK
            || self.top.abs_diff(other.top) > LINE_SLACK
            || ours.max(theirs) > ours.min(theirs).saturating_mul(2)
    }
}

/// One character as a reader compares it: its shape, and its place in its
/// line.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Sample {
    /// How much of each cell is ink, from none at 0 to all of it at 255,
    /// row by row from the top.
    cover: [u8; CELLS],
    /// The most ink of each cell and the eight around it.
    near: [u8; CELLS],
    place: Place,
}

impl std::fmt::Debug for Sample {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Sample")
            .field("shape", &self.shape())
            .field("place", &self.place)
            .finish()
    }
}

impl Sample {
    /// The character within `bounds`, standing at `place`, where
    /// `share(x, y)` is how much of the pixel at (`x`, `y`) is the
    /// character's, from 0 to 255.
    pub(super) fn of(share: impl Fn(u32, u32) -> u8, bounds: &Bounds, place: Place) -> Self {
        let cover = cover(share, bounds, place.line);
        Self {
            near: near(&cover),
            cover,
            place,
        }
    }

    /// Where the character stands in its line.
    pub(super) const fn place(&self) -> Place {
        self.place
    }

    /// The cells that are at least half ink.
    pub(super) fn shape(&self) -> Shape {
        let mut shape = Shape::default();
        for (cells, row) in shape.iter_mut().zip(self.cover.chunks(GRID as usize)) {
            for (column, &cover) in row.iter().enumerate() {
                if u64::from(cover).saturating_mul(2) >= FULL {
                    *cells |= 1 << column;
                }
            }
        }
        shape
    }

    /// How far this character is from `other`, from 0 for the same shape
    /// to [`FARTHEST`]: of the ink either has, the thousandths only one of
    /// them has. Two characters whose places are [unlike](Place::unlike)
    /// are [`FARTHEST`] apart whatever their shapes.
    pub(super) fn distance(&self, other: &Self) -> u16 {
        if self.place.unlike(other.place) {
            return FARTHEST;
        }
        // The ink of each that the other has none of there or beside
        // it, summed over each block of cells.
        let side = GRID as usize;
        let mut blocks = [0_u32; (BLOCKS * BLOCKS) as usize];
        let rows = self
            .cover
            .chunks(side)
            .zip(self.near.chunks(side))
            .zip(other.cover.chunks(side).zip(other.near.chunks(side)));
        for (row, ((ink, near), (other_ink, other_near))) in rows.enumerate() {
            let mut alone = [0_u8; GRID as usize];
            let cells = ink.iter().zip(near).zip(other_ink.iter().zip(other_near));
            for (alone, ((&ink, &near), (&other_ink, &other_near))) in alone.iter_mut().zip(cells) {
                *alone = ink
                    .saturating_sub(other_near)
                    .max(other_ink.saturating_sub(near));
            }
            let across = blocks
                .chunks_mut(BLOCKS as usize)
                .nth(row.div_euclid(BLOCK as usize))
                .unwrap_or_default();
            for (sum, alone) in across.iter_mut().zip(alone.chunks(BLOCK as usize)) {
                let alone: u32 = alone.iter().map(|&alone| u32::from(alone)).sum();
                *sum = sum.saturating_add(alone);
            }
        }
        // The most of it in any two blocks by two.
        let across = BLOCKS as usize;
        let most = blocks
            .chunks(across)
            .zip(blocks.chunks(across).skip(1))
            .flat_map(|(upper, lower)| {
                upper
                    .windows(2)
                    .zip(lower.windows(2))
                    .map(|(upper, lower)| upper.iter().chain(lower).copied().sum::<u32>())
            })
            .max()
            .unwrap_or(0);
        let apart = most
            .saturating_mul(u32::from(FARTHEST))
            .checked_div(WINDOW)
            .unwrap_or(0);
        u16::try_from(apart).unwrap_or(FARTHEST).min(FARTHEST)
    }
}

/// The most ink of each cell of `cover` and the eight cells around it.
fn near(cover: &[u8; CELLS]) -> [u8; CELLS] {
    let side = GRID as usize;
    // The most of each cell and those left and right of it, then of those
    // above and below.
    let mut across = [0_u8; CELLS];
    for (most, row) in across.chunks_mut(side).zip(cover.chunks(side)) {
        for (x, most) in most.iter_mut().enumerate() {
            let from = x.saturating_sub(1);
            let cells = row
                .iter()
                .skip(from)
                .take(x.saturating_add(2).saturating_sub(from));
            *most = cells.copied().max().unwrap_or(0);
        }
    }
    let mut near = [0_u8; CELLS];
    for (y, most) in near.chunks_mut(side).enumerate() {
        let from = y.saturating_sub(1);
        let rows = across
            .chunks(side)
            .skip(from)
            .take(y.saturating_add(2).saturating_sub(from));
        for row in rows {
            for (most, &cell) in most.iter_mut().zip(row) {
                *most = (*most).max(cell);
            }
        }
    }
    near
}

/// How many times the grid's side a line's height is at most: a character
/// smaller than that, such as a `.`, does not fill the grid, so a fraction
/// of a pixel is no more of the grid for it than for a digit.
const SMALLEST: u64 = 2;

/// A pixel's length in the units a character's bounds are held in.
const PIXEL: u64 = 256;

/// The whole of a pixel's share along one side, in the units [`shares`]
/// gives it in.
const WHOLE: u64 = 1 << 13;

/// How much of each grid cell is ink, for the character within `bounds`,
/// where `share(x, y)` is how much of the pixel at (`x`, `y`) is the
/// character's.
///
/// `bounds` holds the pixels that are mostly the character's. The pixels
/// around them may be partly its own, so its edges are taken to a fraction
/// of a pixel: each as far out as the pixels along it are the character's.
fn cover(share: impl Fn(u32, u32) -> u8, bounds: &Bounds, line: u32) -> [u8; CELLS] {
    // The pixels of `bounds` and one more on every side, as far as the
    // image has them.
    let columns = bounds.columns.start.saturating_sub(1)..bounds.columns.end.saturating_add(1);
    let rows = bounds.rows.start.saturating_sub(1)..bounds.rows.end.saturating_add(1);
    let wide = usize::try_from(columns.end.saturating_sub(columns.start)).unwrap_or(usize::MAX);
    let shares: Vec<u8> = rows
        .clone()
        .flat_map(|y| columns.clone().map(move |x| (x, y)))
        .map(|(x, y)| share(x, y))
        .collect();
    // The most any pixel of each column, and of each row, is the
    // character's.
    let mut across = vec![0_u8; wide];
    let mut down = Vec::new();
    for row in shares.chunks(wide.max(1)) {
        down.push(row.iter().copied().max().unwrap_or(0));
        for (most, &share) in across.iter_mut().zip(row) {
            *most = (*most).max(share);
        }
    }
    let (left, right) = edges(&columns, &bounds.columns, &across);
    let (top, bottom) = edges(&rows, &bounds.rows, &down);
    let side = right
        .saturating_sub(left)
        .max(bottom.saturating_sub(top))
        .max(u64::from(line).saturating_mul(PIXEL).div_euclid(SMALLEST))
        .max(1);
    let across = shared(&columns, left..right, side);
    let down = shared(&rows, top..bottom, side);
    let mut inked = [0_u64; CELLS];
    let mut shares_down = down.iter().peekable();
    for (pixel_row, pixels) in shares.chunks(wide.max(1)).enumerate() {
        // The row's ink in each column of cells.
        let mut row = [0_u64; GRID as usize];
        for &(pixel, cell, part) in &across {
            if let (Some(&share), Some(sum)) = (pixels.get(pixel), row.get_mut(cell)) {
                *sum = sum.saturating_add(u64::from(share).saturating_mul(part));
            }
        }
        while let Some(&(_, cell, part)) = shares_down.next_if(|&&(pixel, _, _)| pixel == pixel_row)
        {
            let cells = inked
                .chunks_mut(GRID as usize)
                .nth(cell)
                .unwrap_or_default();
            for (sum, ink) in cells.iter_mut().zip(row) {
                *sum = sum.saturating_add(ink.saturating_mul(part));
            }
        }
    }
    // A cell is `side` by `side` of the units a pixel is `PIXEL` times
    // `GRID` of, and a pixel's whole share is `WHOLE` along each side:
    // the two are the same number, so a cell that is all ink sums to 255
    // times `side` squared.
    let cell = side.saturating_mul(side);
    inked.map(|sum| {
        let cover = sum.checked_div(cell).unwrap_or(0);
        u8::try_from(cover).unwrap_or(u8::MAX)
    })
}
const _: () = assert!(WHOLE == PIXEL * GRID as u64);

/// Where a character starts and ends along one side, in [`PIXEL`]ths of a
/// pixel: `inner` holds the pixels that are mostly its own, and `most` how
/// much the pixels of each column or row of `outer`, which is `inner` and
/// the one before and after it, are its own at most. An edge is as far out
/// as its outermost pixels are the character's.
fn edges(outer: &Range<u32>, inner: &Range<u32>, most: &[u8]) -> (u64, u64) {
    let share = |at: Option<u32>| {
        at.filter(|at| outer.contains(at))
            .and_then(|at| usize::try_from(at.saturating_sub(outer.start)).ok())
            .and_then(|at| most.get(at))
            .map_or(0, |&share| u64::from(share))
    };
    let full = u64::from(u8::MAX);
    // How far in the edge is: the first pixel's missing share, less the
    // share of the pixel before it.
    let inset = |first: Option<u32>, before: Option<u32>| {
        (full.saturating_sub(share(first)).saturating_mul(PIXEL) / full)
            .cast_signed()
            .saturating_sub((share(before).saturating_mul(PIXEL) / full).cast_signed())
    };
    let last = inner.end.checked_sub(1);
    let start = u64::from(inner.start)
        .saturating_mul(PIXEL)
        .saturating_add_signed(inset(Some(inner.start), inner.start.checked_sub(1)));
    let end = u64::from(inner.end)
        .saturating_mul(PIXEL)
        .saturating_add_signed(inset(last, Some(inner.end)).saturating_neg());
    (start, end.max(start.saturating_add(1)))
}

/// How the pixels of `outer` share the grid's cells along one side, where
/// the character spans `span`, in [`PIXEL`]ths of a pixel, and the grid is
/// `side` of them long: each pixel, by its place in `outer`, with each
/// cell it lies over and how much of the pixel's ink the cell holds, of
/// [`WHOLE`], in order. The span is centered on the grid, and a pixel the
/// span cuts has all its ink within the span.
fn shared(outer: &Range<u32>, span: Range<u64>, side: u64) -> Vec<(usize, usize, u64)> {
    let grid = u64::from(GRID);
    let margin = side
        .saturating_sub(span.end.saturating_sub(span.start))
        .saturating_mul(grid)
        .div_euclid(2);
    let mut all = Vec::new();
    for (pixel, at) in outer.clone().enumerate() {
        let from = u64::from(at).saturating_mul(PIXEL).max(span.start);
        let to = u64::from(at)
            .saturating_add(1)
            .saturating_mul(PIXEL)
            .min(span.end);
        let Some(length) = to.checked_sub(from).filter(|&length| length > 0) else {
            continue;
        };
        // In the units a cell is `side` of.
        let start = margin.saturating_add(from.saturating_sub(span.start).saturating_mul(grid));
        let end = start.saturating_add(length.saturating_mul(grid));
        let cells = start.div_euclid(side)..=end.saturating_sub(1).div_euclid(side);
        for (place, cell) in (0_usize..).zip(0..grid) {
            if !cells.contains(&cell) {
                continue;
            }
            let held = end
                .min(cell.saturating_add(1).saturating_mul(side))
                .saturating_sub(start.max(cell.saturating_mul(side)));
            let part = held
                .saturating_mul(WHOLE)
                .checked_div(length.saturating_mul(grid))
                .unwrap_or(0);
            all.push((pixel, place, part));
        }
    }
    all
}
