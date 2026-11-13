//! A reading's ink as the text reader takes it.

use appytappy_sources::screen::digits::Ink;

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

impl Line {
    /// `ink` as the text reader takes it: black ink on white, cut to the
    /// ink's bounds with a margin of white around them, as text on a page
    /// has. Black on white reads better than the application's own colors,
    /// which can be any pair. `None` without ink.
    pub(crate) fn of(ink: &Ink) -> Option<Self> {
        let size = ink.size();
        let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
        for y in 0..size.height {
            for x in 0..size.width {
                if ink.at(x, y) {
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
        let stride = usize::try_from(width).ok()?;
        let skipped = usize::try_from(margin).ok()?;
        let mut pixels = vec![u8::MAX; stride.checked_mul(usize::try_from(height).ok()?)?];
        for (row, y) in pixels
            .chunks_exact_mut(stride)
            .skip(skipped)
            .zip(top..bottom)
        {
            for (pixel, x) in row.iter_mut().skip(skipped).zip(left..right) {
                if ink.at(x, y) {
                    *pixel = 0;
                }
            }
        }
        Some(Self {
            width,
            height,
            pixels,
        })
    }
}
