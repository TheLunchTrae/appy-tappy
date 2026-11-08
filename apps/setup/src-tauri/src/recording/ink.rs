//! A reading's ink: which pixels of a recorded crop are the characters'
//! color, and the same ink as the text reader takes it.

use appytappy_sources::screen::Image;
use appytappy_sources::screen::digits::DigitReader;

/// Bits in one word of an [`Ink`] mask.
const WORD_BITS: usize = 64;

/// Which pixels of a crop are ink, one bit per pixel, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Ink {
    words: Vec<u64>,
    width: usize,
    height: usize,
}

impl Ink {
    /// The ink of `image`, as `reader` tells ink from background.
    pub(super) fn of(image: &Image, reader: &DigitReader) -> Self {
        let pixels = image.pixels();
        let mut words = vec![0_u64; pixels.len().div_ceil(WORD_BITS)];
        for (index, &pixel) in pixels.iter().enumerate() {
            if reader.is_ink(pixel)
                && let Some(word) = words.get_mut(index / WORD_BITS)
            {
                *word |= bit(index);
            }
        }
        let size = image.size();
        Self {
            words,
            width: usize::try_from(size.width).unwrap_or(0),
            height: usize::try_from(size.height).unwrap_or(0),
        }
    }

    /// Whether the pixel at (`x`, `y`) is ink.
    fn at(&self, x: usize, y: usize) -> bool {
        let index = y.saturating_mul(self.width).saturating_add(x);
        self.words
            .get(index / WORD_BITS)
            .is_some_and(|word| word & bit(index) != 0)
    }

    /// The ink as the text reader takes it: one byte a pixel, black ink on
    /// white, cut to the ink's bounds with a margin of white around them,
    /// as text on a page has. Black on white reads better than the
    /// application's own colors, which can be any pair. `None` without ink.
    pub(super) fn line(&self) -> Option<Line> {
        let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0, 0);
        for y in 0..self.height {
            for x in 0..self.width {
                if self.at(x, y) {
                    left = left.min(x);
                    top = top.min(y);
                    right = right.max(x.saturating_add(1));
                    bottom = bottom.max(y.saturating_add(1));
                }
            }
        }
        if right <= left {
            return None;
        }
        // Half the ink's height, at least two pixels.
        let margin = (bottom.saturating_sub(top) / 2).max(2);
        let width = right
            .saturating_sub(left)
            .saturating_add(margin.saturating_mul(2));
        let height = bottom
            .saturating_sub(top)
            .saturating_add(margin.saturating_mul(2));
        let mut pixels = vec![u8::MAX; width.saturating_mul(height)];
        for (row, y) in pixels.chunks_exact_mut(width).skip(margin).zip(top..bottom) {
            for (pixel, x) in row.iter_mut().skip(margin).zip(left..right) {
                if self.at(x, y) {
                    *pixel = 0;
                }
            }
        }
        Some(Line {
            width: u32::try_from(width).ok()?,
            height: u32::try_from(height).ok()?,
            pixels,
        })
    }
}

/// The bit for pixel `index` in its word.
fn bit(index: usize) -> u64 {
    u32::try_from(index % WORD_BITS)
        .ok()
        .and_then(|shift| 1_u64.checked_shl(shift))
        .unwrap_or(0)
}

/// One line of text for the text reader: gray pixels, one byte each, row by
/// row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Line {
    /// Its width in pixels.
    pub(crate) width: u32,
    /// Its height in pixels.
    pub(crate) height: u32,
    /// Its pixels: 0 is black, 255 white.
    pub(crate) pixels: Vec<u8>,
}
