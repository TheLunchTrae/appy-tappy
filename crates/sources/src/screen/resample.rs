//! A state picture and its mask redrawn at another size, for a region that
//! scales with its window.
//!
//! Each axis is redrawn on its own. Shrinking averages the pixels each new
//! pixel covers, so a thin edge still shows as a blend rather than vanishing
//! between samples. Growing takes the nearest pixel, so a sharp edge of an
//! interface stays sharp instead of smearing into its neighbors. A mask's
//! pixel is kept when at least half the pixels it covers are, and a kept
//! pixel's color averages only the kept pixels it covers, so the background
//! a mask leaves out does not tint the edge of the thing.

use std::ops::Range;

use super::{Image, Size};

/// `picture` and the pixels its `kept` mask keeps, row by row, redrawn at
/// `size`, or `None` when `size` is empty or `kept` does not hold one entry
/// for each pixel.
pub(crate) fn resample(picture: &Image, kept: &[bool], size: Size) -> Option<(Image, Vec<bool>)> {
    let from = picture.size();
    let columns = spans(from.width, size.width)?;
    let rows = spans(from.height, size.height)?;
    let width = usize::try_from(from.width).ok()?;
    if Some(kept.len()) != width.checked_mul(usize::try_from(from.height).ok()?) {
        return None;
    }
    let pixels = picture.pixels();
    let mut bgra = Vec::with_capacity(columns.len().checked_mul(rows.len())?.checked_mul(4)?);
    let mut mask = Vec::with_capacity(columns.len().saturating_mul(rows.len()));
    for row in &rows {
        for column in &columns {
            let mut all = Sum::default();
            let mut shown = Sum::default();
            for y in row.clone() {
                let start = y.checked_mul(width)?;
                let line = start.checked_add(column.start)?..start.checked_add(column.end)?;
                let colors = pixels.get(line.clone())?;
                let keeps = kept.get(line)?;
                for (&pixel, &keep) in colors.iter().zip(keeps) {
                    all.add(pixel);
                    shown.add_if(keep, pixel);
                }
            }
            let keep = shown.count.checked_mul(2)? >= all.count;
            let color = if shown.count > 0 { shown } else { all };
            bgra.extend(color.average()?);
            mask.push(keep);
        }
    }
    Some((Image::new(size, bgra).ok()?, mask))
}

/// For each of `to` new pixels along an axis, the old pixels of the `from`
/// it covers: the nearest one when growing, or every one it overlaps when
/// shrinking. `None` when either length is zero.
fn spans(from: u32, to: u32) -> Option<Vec<Range<usize>>> {
    if from == 0 || to == 0 {
        return None;
    }
    let (from, to) = (u64::from(from), u64::from(to));
    (0..to)
        .map(|new| {
            let range = if to >= from {
                // The old pixel under the new one's center.
                let nearest = new
                    .checked_mul(2)?
                    .checked_add(1)?
                    .checked_mul(from)?
                    .checked_div(to.checked_mul(2)?)?;
                nearest..nearest.checked_add(1)?
            } else {
                // Each new pixel covers at least one old one, since `from`
                // is longer.
                let start = new.checked_mul(from)?.checked_div(to)?;
                let end = new.checked_add(1)?.checked_mul(from)?.checked_div(to)?;
                start..end
            };
            Some(usize::try_from(range.start).ok()?..usize::try_from(range.end).ok()?)
        })
        .collect()
}

/// The running total of some pixels' blue, green and red.
#[derive(Debug, Default, Clone, Copy)]
struct Sum {
    channels: [u64; 3],
    count: u64,
}

impl Sum {
    fn add(&mut self, pixel: [u8; 4]) {
        for (total, channel) in self.channels.iter_mut().zip(pixel) {
            *total = total.saturating_add(u64::from(channel));
        }
        self.count = self.count.saturating_add(1);
    }

    /// Adds `pixel` when `keep` says to.
    fn add_if(&mut self, keep: bool, pixel: [u8; 4]) {
        if keep {
            self.add(pixel);
        }
    }

    /// The average pixel, rounded, opaque.
    fn average(self) -> Option<[u8; 4]> {
        let half = self.count.checked_div(2)?;
        let mut pixel = [0, 0, 0, u8::MAX];
        for (out, total) in pixel.iter_mut().zip(self.channels) {
            *out = u8::try_from(total.checked_add(half)?.checked_div(self.count)?).ok()?;
        }
        Some(pixel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `width` by 1 image whose pixels have these blue channels.
    fn line(blues: &[u8]) -> Image {
        let width = u32::try_from(blues.len()).expect("short");
        Image::new(
            Size { width, height: 1 },
            blues
                .iter()
                .flat_map(|&blue| [blue, 0, 0, u8::MAX])
                .collect(),
        )
        .expect("matching length")
    }

    fn blues(image: &Image) -> Vec<u8> {
        image.pixels().iter().map(|pixel| pixel[0]).collect()
    }

    fn size(width: u32, height: u32) -> Size {
        Size { width, height }
    }

    /// `picture` redrawn at `size`, every pixel kept.
    fn whole(picture: &Image, size: Size) -> Image {
        let kept = vec![true; picture.pixels().len()];
        let (image, mask) = resample(picture, &kept, size).expect("resamples");
        assert!(mask.iter().all(|&kept| kept), "a whole mask stays whole");
        image
    }

    #[test]
    fn shrinking_averages_and_growing_takes_the_nearest_pixel() {
        let edge = line(&[0, 0, 255, 255]);
        assert_eq!(blues(&whole(&edge, size(2, 1))), [0, 255]);
        // Three into two: the middle pixel's old pixels are split, so each
        // new one averages the old pixels it starts in.
        assert_eq!(blues(&whole(&line(&[0, 100, 200]), size(2, 1))), [0, 150]);
        // A blend at the edge rounds to the nearest level.
        assert_eq!(blues(&whole(&line(&[0, 255]), size(1, 1))), [128]);
        // Growing keeps the edge sharp: each new pixel copies one old one.
        assert_eq!(
            blues(&whole(&edge, size(8, 1))),
            [0, 0, 0, 0, 255, 255, 255, 255]
        );
        assert_eq!(
            blues(&whole(&line(&[0, 255]), size(3, 1))),
            [0, 255, 255],
            "the middle center is on the second"
        );
        assert_eq!(whole(&edge, edge.size()), edge);
    }

    #[test]
    fn rows_resample_like_columns() {
        let column = Image::new(
            size(1, 4),
            [0, 0, 255, 255]
                .iter()
                .flat_map(|&blue| [blue, 0, 0, u8::MAX])
                .collect(),
        )
        .expect("matching length");
        assert_eq!(blues(&whole(&column, size(1, 2))), [0, 255]);
        assert_eq!(blues(&whole(&column, size(2, 2))), [0, 0, 255, 255]);
    }

    #[test]
    fn a_mask_keeps_what_half_its_pixels_keep_and_colors_from_them() {
        let picture = line(&[10, 200, 30, 50]);
        let kept = [false, true, false, false];
        let (half, mask) = resample(&picture, &kept, size(2, 1)).expect("resamples");
        // The first new pixel covers one kept pixel of two, so it is kept and
        // takes the kept one's color; the second covers none.
        assert_eq!(mask, [true, false]);
        assert_eq!(blues(&half), [200, 40]);
        let (grown, mask) = resample(&picture, &kept, size(8, 1)).expect("resamples");
        assert_eq!(mask, [false, false, true, true, false, false, false, false]);
        assert_eq!(blues(&grown), [10, 10, 200, 200, 30, 30, 50, 50]);
        let (_, sparse) = resample(
            &line(&[0, 0, 0, 0]),
            &[true, false, false, false],
            size(1, 1),
        )
        .expect("resamples");
        assert_eq!(sparse, [false], "one kept pixel of four is too few");
    }

    #[test]
    fn nothing_resamples_to_or_from_no_pixels() {
        let picture = line(&[1, 2]);
        assert!(resample(&picture, &[true, true], size(0, 1)).is_none());
        assert!(resample(&picture, &[true, true], size(1, 0)).is_none());
        assert!(resample(&picture, &[true], size(1, 1)).is_none());
        assert_eq!(spans(0, 3), None);
        assert_eq!(spans(3, 0), None);
    }
}
