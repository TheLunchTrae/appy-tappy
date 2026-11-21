//! A character's shape, whatever its size and color, and how far it is
//! from another.
//!
//! A character is scaled to a grid by its own bounds, so the same character
//! has the same shape at any size and wherever it lands on the pixel grid.
//! Its bounds and its ink are taken to a fraction of a pixel: a pixel on
//! an anti-aliased edge is the character's by how much of it the character
//! covers ([`Layers::share`]). Each cell holds how much of it is ink, not
//! whether it is. Two shapes are compared by how much ink each has around
//! every cell, in the [`AROUND`] cells each way. An edge that is blurred or
//! a little off moves ink between neighboring cells and leaves about as
//! much around each, while a stroke one character has and the other lacks
//! leaves none there. Two shapes are as far apart as the one place they
//! differ most, however much ink the rest of them share.
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

/// How many cells each way from a cell its ink is summed over: a square
/// of five cells by five, a little wider than a thin font's stroke. A
/// wider one would let such a stroke go missing unseen, and a narrower one
/// would take a blurred edge for a difference.
const AROUND: usize = 2;

/// The cells along each side of that square.
const SQUARE: u32 = 5;
const _: () = assert!(SQUARE as usize == 2 * AROUND + 1);

/// The ink around a cell whose square is all ink.
const ALL_AROUND: u32 = SQUARE * SQUARE * u8::MAX as u32;

/// Which cells of a character's grid are at least half ink, one row of
/// cells in the bits of each `u32`, from the top.
///
/// Two characters with the same cells look alike to a reader, so
/// [`DigitReader::shapes`] names a character's shape by them. Where a
/// character stands in its line is no part of its shape.
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
    /// The ink of each cell and those within [`AROUND`] of it, summed.
    around: [u16; CELLS],
    place: Place,
}

impl std::fmt::Debug for Sample {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Sample")
            .field("shape", &self.shape())
            .field("place", &self.place)
            .finish_non_exhaustive()
    }
}

impl Sample {
    /// The character within `bounds`, standing at `place`, where
    /// `share(x, y)` is how much of the pixel at (`x`, `y`) is the
    /// character's, from 0 to 255.
    pub(super) fn of(share: impl Fn(u32, u32) -> u8, bounds: &Bounds, place: Place) -> Self {
        let cover = cover(share, bounds, place.line);
        Self {
            around: around(&cover),
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
    /// to [`FARTHEST`]: the most that the ink around any one cell differs
    /// between them, in thousandths of a square that is all ink. Two
    /// characters whose places are [unlike](Place::unlike) are
    /// [`FARTHEST`] apart whatever their shapes.
    pub(super) fn distance(&self, other: &Self) -> u16 {
        if self.place.unlike(other.place) {
            return FARTHEST;
        }
        let most = self
            .around
            .iter()
            .zip(&other.around)
            .map(|(&ours, &theirs)| ours.abs_diff(theirs))
            .max()
            .unwrap_or(0);
        let apart = u32::from(most)
            .saturating_mul(u32::from(FARTHEST))
            .checked_div(ALL_AROUND)
            .unwrap_or(0);
        u16::try_from(apart).unwrap_or(FARTHEST).min(FARTHEST)
    }
}

/// The ink of each cell of `cover` and the cells within [`AROUND`] of it,
/// summed. Cells past the grid's edge hold none.
fn around(cover: &[u8; CELLS]) -> [u16; CELLS] {
    let side = GRID as usize;
    // Each cell with those left and right of it, then those sums with the
    // ones above and below.
    let mut across = [0_u16; CELLS];
    for (sums, row) in across.chunks_mut(side).zip(cover.chunks(side)) {
        for (x, sum) in sums.iter_mut().enumerate() {
            let from = x.saturating_sub(AROUND);
            let cells = row
                .iter()
                .skip(from)
                .take(x.saturating_add(AROUND + 1).saturating_sub(from));
            *sum = cells.map(|&cell| u16::from(cell)).sum();
        }
    }
    let mut around = [0_u16; CELLS];
    for (y, sums) in around.chunks_mut(side).enumerate() {
        let from = y.saturating_sub(AROUND);
        let rows = across
            .chunks(side)
            .skip(from)
            .take(y.saturating_add(AROUND + 1).saturating_sub(from));
        for row in rows {
            for (sum, &cell) in sums.iter_mut().zip(row) {
                *sum = sum.saturating_add(cell);
            }
        }
    }
    around
}
const _: () = assert!(
    ALL_AROUND <= u16::MAX as u32,
    "the ink around a cell fits its sum"
);

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

/// Where a character starts and ends along one side, in fractions of a
/// pixel, [`PIXEL`] to one: `inner` holds the pixels that are mostly its
/// own, and `most` how
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
    // How far in the edge is: the first pixel's missing share, less the
    // share of the pixel before it.
    let inset = |first: Option<u32>, before: Option<u32>| {
        (FULL.saturating_sub(share(first)).saturating_mul(PIXEL) / FULL)
            .cast_signed()
            .saturating_sub((share(before).saturating_mul(PIXEL) / FULL).cast_signed())
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
/// the character spans `span`, in fractions of a pixel, and the grid is
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
