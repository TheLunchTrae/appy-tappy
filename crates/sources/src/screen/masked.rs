//! A picture and the pixels of it a comparison takes: the one place that
//! says how far an image is from a saved picture.
//!
//! A state check compares a region with the picture laid over it, and a
//! picture search compares it at every place it fits. Both add up the same
//! difference here, so they agree about which pixels are the same.

use std::ops::Range;

use super::mask::{Mask, MaskError};
use super::{Image, Size};

/// A picture with the runs of its pixels a comparison takes, worked out
/// once, when it is made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MaskedPicture {
    picture: Image,
    runs: Box<[Run]>,
    /// How many red, green, and blue channels a comparison adds up.
    channels: u64,
}

/// A run of compared pixels in one row of the picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Run {
    /// The row it is in.
    pub(super) row: usize,
    /// Its columns.
    pub(super) columns: Range<usize>,
}

impl MaskedPicture {
    /// `picture` compared over the pixels `mask` keeps, or over every pixel
    /// without one.
    ///
    /// # Errors
    ///
    /// Returns [`MaskError::SizeMismatch`] when the mask's size differs from
    /// the picture's.
    pub(super) fn new(picture: Image, mask: Option<&Mask>) -> Result<Self, MaskError> {
        let Some(mask) = mask else {
            return Ok(Self::whole(picture));
        };
        if mask.size() != picture.size() {
            return Err(MaskError::SizeMismatch {
                image: picture.size(),
                mask: mask.size(),
            });
        }
        let width = width_of(&picture);
        let mut runs = Vec::new();
        for (row, kept) in mask.kept().chunks(width).enumerate() {
            let mut column = 0_usize;
            for run in kept.chunk_by(|one, other| one == other) {
                let end = column.saturating_add(run.len());
                if run.first() == Some(&true) {
                    runs.push(Run {
                        row,
                        columns: column..end,
                    });
                }
                column = end;
            }
        }
        Ok(Self::of_runs(picture, runs))
    }

    /// `picture` compared over every pixel. It takes no mask, so it cannot
    /// fail.
    pub(super) fn whole(picture: Image) -> Self {
        let width = width_of(&picture);
        let rows = picture
            .pixels()
            .len()
            .checked_div(width)
            .unwrap_or_default();
        let runs = (0..rows)
            .map(|row| Run {
                row,
                columns: 0..width,
            })
            .collect();
        Self::of_runs(picture, runs)
    }

    fn of_runs(picture: Image, runs: Vec<Run>) -> Self {
        let kept: usize = runs.iter().map(|run| run.columns.len()).sum();
        Self {
            picture,
            runs: runs.into_boxed_slice(),
            // A picture holds at most 8192² pixels, so three channels of
            // each fit a u64.
            channels: u64::try_from(kept).unwrap_or(u64::MAX).saturating_mul(3),
        }
    }

    /// The picture.
    pub(super) const fn picture(&self) -> &Image {
        &self.picture
    }

    /// The picture's size.
    pub(super) const fn size(&self) -> Size {
        self.picture.size()
    }

    /// How many red, green, and blue channels a comparison adds up: three
    /// for each compared pixel.
    pub(super) const fn channels(&self) -> u64 {
        self.channels
    }

    /// The runs of compared pixels, row by row from the top.
    pub(super) fn runs(&self) -> &[Run] {
        &self.runs
    }

    /// The picture's pixels in `run`.
    pub(super) fn pixels(&self, run: &Run) -> &[[u8; 4]] {
        run.row
            .checked_mul(width_of(&self.picture))
            .and_then(|start| {
                self.picture.pixels().get(
                    start.checked_add(run.columns.start)?..start.checked_add(run.columns.end)?,
                )
            })
            .unwrap_or_default()
    }

    /// Whether each pixel is compared, row by row from the top.
    pub(super) fn kept(&self) -> Vec<bool> {
        let width = width_of(&self.picture);
        let mut kept = vec![false; self.picture.pixels().len()];
        for run in &self.runs {
            let start = run.row.saturating_mul(width);
            kept.get_mut(
                start.saturating_add(run.columns.start)..start.saturating_add(run.columns.end),
            )
            .unwrap_or_default()
            .fill(true);
        }
        kept
    }

    /// How far `image` is from the picture laid with its top-left corner at
    /// (`x`, `y`) of it: the summed difference of the compared pixels' red,
    /// green, and blue. It is `None` once the sum passes `limit`, and when
    /// the picture reaches past `image` there.
    pub(super) fn difference(&self, image: &Image, x: usize, y: usize, limit: u64) -> Option<u64> {
        let width = width_of(image);
        let mut total = 0_u64;
        for run in &self.runs {
            let start = y.checked_add(run.row)?.checked_mul(width)?.checked_add(x)?;
            let end = x.checked_add(run.columns.end)?;
            if end > width {
                return None;
            }
            let found = image
                .pixels()
                .get(start.checked_add(run.columns.start)?..start.checked_add(run.columns.end)?)?;
            // A row holds at most 16384 pixels of three channels of 255
            // each, far below u32::MAX.
            let mut row_total = 0_u32;
            for (pixel, expected) in found.iter().zip(self.pixels(run)) {
                for (channel, want) in pixel.iter().zip(expected).take(3) {
                    row_total = row_total.wrapping_add(u32::from(channel.abs_diff(*want)));
                }
            }
            // At most 3 * 8192² channels of 255 each, far below u64::MAX.
            total = total.wrapping_add(u64::from(row_total));
            if total > limit {
                return None;
            }
        }
        Some(total)
    }
}

/// The largest summed difference of `channels` channels that is within
/// `tolerance`: an average of at most `tolerance` a channel.
pub(super) const fn most_within(channels: u64, tolerance: u8) -> u64 {
    channels.saturating_mul(tolerance as u64)
}

/// How many pixels a row of `image` holds. An image is never empty, and its
/// width fits a `usize` wherever its pixels fit in memory.
fn width_of(image: &Image) -> usize {
    usize::try_from(image.size().width).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A gray image drawn from rows of shades.
    fn image(rows: &[&[u8]]) -> Image {
        let width = u32::try_from(rows[0].len()).unwrap();
        let height = u32::try_from(rows.len()).unwrap();
        let bgra = rows
            .iter()
            .flat_map(|row| row.iter())
            .flat_map(|&gray| [gray, gray, gray, 255])
            .collect();
        Image::new(Size { width, height }, bgra).unwrap()
    }

    #[test]
    fn a_mask_becomes_runs_that_stop_at_each_row_end() {
        let picture = image(&[&[1, 2, 3], &[4, 5, 6]]);
        let mask = Mask::new(picture.size(), vec![true, false, true, true, true, false]).unwrap();
        let masked = MaskedPicture::new(picture.clone(), Some(&mask)).unwrap();
        let run = |row, columns| Run { row, columns };
        assert_eq!(masked.runs(), [run(0, 0..1), run(0, 2..3), run(1, 0..2)]);
        assert_eq!(masked.channels(), 12);
        assert_eq!(masked.kept(), mask.kept());
        assert_eq!(
            masked.pixels(&run(1, 0..2)),
            [[4, 4, 4, 255], [5, 5, 5, 255]]
        );

        let whole = MaskedPicture::whole(picture.clone());
        assert_eq!(whole.runs(), [run(0, 0..3), run(1, 0..3)]);
        assert_eq!(whole.channels(), 18);
        assert_eq!(MaskedPicture::new(picture, None).unwrap(), whole);
    }

    #[test]
    fn a_mask_of_another_size_is_refused() {
        let picture = image(&[&[1, 2]]);
        let mask = Mask::new(
            Size {
                width: 1,
                height: 2,
            },
            vec![true, true],
        )
        .unwrap();
        assert_eq!(
            MaskedPicture::new(picture.clone(), Some(&mask)),
            Err(MaskError::SizeMismatch {
                image: picture.size(),
                mask: mask.size(),
            })
        );
    }

    #[test]
    fn the_difference_adds_up_the_compared_channels_at_a_place() {
        let picture = image(&[&[10, 20], &[30, 40]]);
        let mask = Mask::new(picture.size(), vec![true, false, true, true]).unwrap();
        let masked = MaskedPicture::new(picture, Some(&mask)).unwrap();
        let area = image(&[&[0, 0, 0, 0], &[0, 12, 99, 0], &[0, 30, 45, 0]]);
        // At (1, 1): |12 - 10| and |45 - 40| on three channels each; the
        // 99 is under the mask.
        assert_eq!(masked.difference(&area, 1, 1, u64::MAX), Some(21));
        assert_eq!(masked.difference(&area, 1, 1, 21), Some(21));
        assert_eq!(masked.difference(&area, 1, 1, 20), None);
        // At (0, 0): 10, then 30 and |12 - 40|.
        assert_eq!(masked.difference(&area, 0, 0, u64::MAX), Some(204));
        // The first row alone passes the limit.
        assert_eq!(masked.difference(&area, 0, 0, 29), None);
    }

    #[test]
    fn a_place_the_picture_does_not_fit_has_no_difference() {
        let masked = MaskedPicture::whole(image(&[&[0, 0], &[0, 0]]));
        let area = image(&[&[0, 0, 0], &[0, 0, 0], &[0, 0, 0]]);
        assert_eq!(masked.difference(&area, 1, 1, u64::MAX), Some(0));
        // Past the right edge the run would wrap into the next row.
        assert_eq!(masked.difference(&area, 2, 0, u64::MAX), None);
        assert_eq!(masked.difference(&area, 0, 2, u64::MAX), None);
        assert_eq!(masked.difference(&area, usize::MAX, 0, u64::MAX), None);
        assert_eq!(masked.difference(&area, 0, usize::MAX, u64::MAX), None);
    }
}
