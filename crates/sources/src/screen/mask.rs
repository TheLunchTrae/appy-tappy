//! Which pixels of a region a state check compares.
//!
//! A mask leaves out the background behind the thing a state check looks
//! for, so the check still matches when that background changes. The setup
//! app makes one when the user selects the thing with
//! [`select`](super::select), and saves it as the transparent pixels of the
//! check's PNG file.

use std::error::Error;
use std::fmt;
use std::ops::Range;

use super::Size;

/// The pixels of a region a check compares, row by row from the top.
///
/// A mask keeps at least one pixel.
#[derive(Clone, PartialEq, Eq)]
pub struct Mask {
    size: Size,
    kept: Vec<bool>,
}

impl fmt::Debug for Mask {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Mask")
            .field("size", &self.size)
            .field("kept", &self.count())
            .finish_non_exhaustive()
    }
}

impl Mask {
    /// The mask of a `size` region that keeps each pixel whose entry in
    /// `kept` is true.
    ///
    /// # Errors
    ///
    /// Returns [`MaskError::Length`] when `kept` does not hold one entry for
    /// each pixel, and [`MaskError::Empty`] when it keeps no pixel.
    pub fn new(size: Size, kept: Vec<bool>) -> Result<Self, MaskError> {
        if pixels(size) != Some(kept.len()) {
            return Err(MaskError::Length { size });
        }
        if !kept.contains(&true) {
            return Err(MaskError::Empty);
        }
        Ok(Self { size, kept })
    }

    /// The region's size.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// Whether each pixel is kept, row by row from the top.
    #[must_use]
    pub fn kept(&self) -> &[bool] {
        &self.kept
    }

    /// How many pixels it keeps.
    #[must_use]
    pub fn count(&self) -> usize {
        self.kept.iter().filter(|&&kept| kept).count()
    }

    /// Whether it keeps every pixel, so comparing with it is comparing
    /// without one.
    #[must_use]
    pub fn is_full(&self) -> bool {
        !self.kept.contains(&false)
    }

    /// The runs of kept pixels as ranges of pixel indexes, so a check walks
    /// them without testing each pixel.
    #[must_use]
    pub fn spans(&self) -> Vec<Range<usize>> {
        let mut spans = Vec::new();
        let mut start = None;
        for (index, &kept) in self.kept.iter().enumerate() {
            match (kept, start) {
                (true, None) => start = Some(index),
                (false, Some(from)) => {
                    spans.push(from..index);
                    start = None;
                },
                _ => {},
            }
        }
        if let Some(from) = start {
            spans.push(from..self.kept.len());
        }
        spans
    }
}

/// The number of pixels in a region of `size`, if it fits in memory.
pub(crate) fn pixels(size: Size) -> Option<usize> {
    usize::try_from(size.width)
        .ok()?
        .checked_mul(usize::try_from(size.height).ok()?)
}

/// A mask could not be made or used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskError {
    /// The entries did not number one for each pixel of the size.
    Length {
        /// The size the entries should have matched.
        size: Size,
    },
    /// No pixel was kept.
    Empty,
    /// The mask's size differs from the image it goes with.
    SizeMismatch {
        /// The image's size.
        image: Size,
        /// The mask's size.
        mask: Size,
    },
}

impl fmt::Display for MaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Length { size } => {
                write!(formatter, "a {size} mask needs one entry for each pixel")
            },
            Self::Empty => formatter.write_str("the selection leaves no pixel to compare"),
            Self::SizeMismatch { image, mask } => {
                write!(formatter, "the mask is {mask} but its image is {image}")
            },
        }
    }
}

impl Error for MaskError {}

#[cfg(test)]
mod tests {
    use super::*;

    const THREE_BY_TWO: Size = Size {
        width: 3,
        height: 2,
    };

    #[test]
    fn a_mask_needs_one_entry_per_pixel_and_a_kept_pixel() {
        assert_eq!(
            Mask::new(THREE_BY_TWO, vec![true; 5]),
            Err(MaskError::Length { size: THREE_BY_TWO })
        );
        assert_eq!(
            Mask::new(THREE_BY_TWO, vec![false; 6]),
            Err(MaskError::Empty)
        );
        let mask = Mask::new(THREE_BY_TWO, vec![true, false, true, true, true, false]).unwrap();
        assert_eq!(mask.size(), THREE_BY_TWO);
        assert_eq!(mask.count(), 4);
        assert!(!mask.is_full());
        assert!(Mask::new(THREE_BY_TWO, vec![true; 6]).unwrap().is_full());
    }

    #[test]
    fn spans_cover_each_run_of_kept_pixels() {
        let mask = Mask::new(THREE_BY_TWO, vec![true, false, true, true, false, true]).unwrap();
        assert_eq!(mask.spans(), vec![0..1, 2..4, 5..6]);
        let edges = Mask::new(THREE_BY_TWO, vec![false, true, true, true, true, false]).unwrap();
        assert_eq!(edges.spans(), vec![1..5]);
    }

    #[test]
    fn errors_explain_themselves() {
        assert_eq!(
            MaskError::Length { size: THREE_BY_TWO }.to_string(),
            "a 3x2 mask needs one entry for each pixel"
        );
        assert_eq!(
            MaskError::Empty.to_string(),
            "the selection leaves no pixel to compare"
        );
        assert_eq!(
            MaskError::SizeMismatch {
                image: THREE_BY_TWO,
                mask: Size {
                    width: 1,
                    height: 1
                },
            }
            .to_string(),
            "the mask is 1x1 but its image is 3x2"
        );
        assert_eq!(
            format!("{:?}", Mask::new(THREE_BY_TWO, vec![true; 6]).unwrap()),
            "Mask { size: Size { width: 3, height: 2 }, kept: 6, .. }"
        );
    }
}
