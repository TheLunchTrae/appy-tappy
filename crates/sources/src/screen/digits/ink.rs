//! Which pixels of an image are ink, and the line of text among them.
//!
//! Every pass over an image's ink reads it here: a read, a lesson, and the
//! setup app's search for text of a size. So they agree about which pixels
//! are ink and about which rows are the line.

use std::ops::Range;

use super::super::{Image, Size};

/// Which pixels of an image are ink, as [`DigitReader::ink`] finds them
/// once.
///
/// [`DigitReader::ink`]: super::DigitReader::ink
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ink {
    /// Whether each pixel is ink, row by row from the top.
    inked: Vec<bool>,
    size: Size,
}

impl Ink {
    /// The ink of `image`: the pixels `is_ink` says are, as
    /// `[blue, green, red, unused]` like [`Image::pixels`] holds them.
    pub(super) fn new(image: &Image, is_ink: impl Fn([u8; 4]) -> bool) -> Self {
        Self {
            inked: image.pixels().iter().map(|&pixel| is_ink(pixel)).collect(),
            size: image.size(),
        }
    }

    /// The size of the image.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// Whether the pixel at (`x`, `y`) is ink. No pixel outside the image
    /// is.
    #[must_use]
    pub fn at(&self, x: u32, y: u32) -> bool {
        x < self.size.width
            && index(y)
                .checked_mul(index(self.size.width))
                .and_then(|start| self.inked.get(start.checked_add(index(x))?))
                .is_some_and(|&inked| inked)
    }

    /// How many pixels of row `y` in `columns` are ink. The columns and
    /// rows outside the image hold none.
    #[must_use]
    pub fn count(&self, columns: Range<u32>, y: u32) -> usize {
        self.row(y)
            .get(index(columns.start)..index(columns.end.min(self.size.width)))
            .unwrap_or_default()
            .iter()
            .filter(|&&inked| inked)
            .count()
    }

    /// Row `y`, or no pixels outside the image.
    fn row(&self, y: u32) -> &[bool] {
        let width = index(self.size.width);
        index(y)
            .checked_mul(width)
            .and_then(|start| self.inked.get(start..start.checked_add(width)?))
            .unwrap_or_default()
    }

    /// The rows of the line of text in `columns` of `rows`: the run of
    /// adjacent inked rows there that holds the most ink, the upper of two
    /// that hold as much, so a stray pixel elsewhere does not stretch the
    /// line. `None` without ink there.
    #[must_use]
    pub fn line(&self, columns: &Range<u32>, rows: Range<u32>) -> Option<Range<u32>> {
        let mut best: Option<(Range<u32>, usize)> = None;
        let mut run: Option<(Range<u32>, usize)> = None;
        let end = rows.end;
        // The row past the last ends a run that reaches the bottom edge.
        for y in rows.chain([end]) {
            let ink = if y < end {
                self.count(columns.clone(), y)
            } else {
                0
            };
            if ink > 0 {
                let (line, total) = run.get_or_insert((y..y, 0));
                line.end = y.saturating_add(1);
                *total = total.saturating_add(ink);
            } else if let Some(ended) = run.take()
                && best.as_ref().is_none_or(|(_, most)| ended.1 > *most)
            {
                best = Some(ended);
            }
        }
        best.map(|(line, _)| line)
    }
}

/// `value` as an index. A pixel's column or row fits a `usize` wherever the
/// image's pixels fit in memory.
fn index(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ink where `rows` have a `#`.
    fn ink(rows: &[&str]) -> Ink {
        let width = u32::try_from(rows[0].len()).unwrap();
        let height = u32::try_from(rows.len()).unwrap();
        Ink {
            inked: rows
                .iter()
                .flat_map(|row| row.bytes())
                .map(|cell| cell == b'#')
                .collect(),
            size: Size { width, height },
        }
    }

    #[test]
    fn ink_is_the_pixels_the_rule_takes() {
        let size = Size {
            width: 2,
            height: 2,
        };
        let image = Image::new(
            size,
            [[9, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [9, 0, 0, 0]].concat(),
        )
        .unwrap();
        let found = Ink::new(&image, |pixel| pixel[0] == 9);
        assert_eq!(found, ink(&["#.", ".#"]));
        assert_eq!(found.size(), size);
    }

    #[test]
    fn no_pixel_outside_the_image_is_ink() {
        // Ink only at (0, 1): (3, 0) would land on it if the row wrapped.
        let ink = ink(&["...", "#.#"]);
        assert!(ink.at(0, 1));
        assert!(!ink.at(1, 1));
        assert!(!ink.at(3, 0));
        assert!(!ink.at(0, 2));
        assert!(!ink.at(u32::MAX, 1));
        assert!(!ink.at(0, u32::MAX));
        assert_eq!(ink.count(0..3, 1), 2);
        assert_eq!(ink.count(1..3, 1), 1);
        assert_eq!(ink.count(0..2, 1), 1);
        assert_eq!(ink.count(0..u32::MAX, 1), 2, "cut at the right edge");
        assert_eq!(ink.count(4..9, 1), 0, "past the right edge");
        assert_eq!(ink.count(0..3, 2), 0);
    }

    #[test]
    fn the_line_is_the_run_of_inked_rows_with_the_most_ink() {
        let ink = ink(&[
            "#.....", //
            "......", //
            "####..", //
            "####..", //
            "......", //
            "##....", //
            "##....", //
            "##....", //
            "##....", //
            "......", //
            "######", //
            "######", //
        ]);
        let all = 0..6;
        assert_eq!(ink.line(&all, 0..12), Some(10..12), "the most ink");
        // Rows 2 to 3 and rows 5 to 8 hold 8 pixels each: the upper one.
        assert_eq!(ink.line(&all, 0..10), Some(2..4));
        assert_eq!(ink.line(&all, 4..10), Some(5..9));
        // Only the ink in the columns counts, and only the rows asked for.
        assert_eq!(ink.line(&(2..6), 0..10), Some(2..4));
        assert_eq!(ink.line(&(0..2), 0..10), Some(5..9));
        assert_eq!(ink.line(&all, 0..2), Some(0..1));
        assert_eq!(ink.line(&all, 10..11), Some(10..11), "cut by the rows");
        assert_eq!(ink.line(&all, 10..40), Some(10..12), "cut by the image");
        assert_eq!(ink.line(&all, 1..2), None);
        assert_eq!(ink.line(&(4..6), 0..10), None);
    }
}
