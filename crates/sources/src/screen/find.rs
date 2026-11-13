//! Finding a picture in an area of a frame, such as a button a click
//! presses wherever it moved to within its panel.
//!
//! A [`PictureSearch`] compares the picture with each place it fits in the
//! area, over the pixels its [`Mask`] keeps, and takes the closest place
//! within its tolerance. Before comparing a place pixel by pixel, it checks
//! a bound from each row's summed channels, which running sums of the area
//! give in a few steps: two rows whose sums differ by more than the
//! tolerance allows cannot match, whatever their pixels are. It also gives
//! up on a place as soon as that place can no longer beat the best one so
//! far.

use std::ops::Range;

use super::mask::{Mask, MaskError};
use super::masked::{MaskedPicture, most_within};
use super::{Image, Size};

/// A picture to look for, with the pixels it compares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PictureSearch {
    picture: MaskedPicture,
    /// The blue, green, and red of each of the picture's runs, each summed,
    /// in the runs' order.
    sums: Box<[[u32; 3]]>,
    tolerance: u8,
}

/// Running sums of each row of an image's blue, green, and red: entry
/// `x` of a row sums the row's first `x` pixels, so a run's sum is two
/// entries apart.
struct RowSums {
    /// Entries per row: one more than the image's width.
    stride: usize,
    sums: Vec<[u32; 3]>,
}

impl RowSums {
    /// The running sums of `image`.
    fn new(image: &Image) -> Option<Self> {
        let width = usize::try_from(image.size().width).ok()?;
        let stride = width.checked_add(1)?;
        let mut sums =
            Vec::with_capacity(stride.checked_mul(usize::try_from(image.size().height).ok()?)?);
        for row in image.pixels().chunks_exact(width) {
            let mut sum = [0_u32; 3];
            sums.push(sum);
            for pixel in row {
                sum = add_channels(sum, *pixel);
                sums.push(sum);
            }
        }
        Some(Self { stride, sums })
    }

    /// The sums of `columns` of row `row`, or `None` when they run past
    /// the image.
    fn run(&self, row: usize, columns: &Range<usize>) -> Option<[u32; 3]> {
        if columns.end >= self.stride {
            return None;
        }
        let start = row.checked_mul(self.stride)?;
        let before = self.sums.get(start.checked_add(columns.start)?)?;
        let through = self.sums.get(start.checked_add(columns.end)?)?;
        let mut sums = [0_u32; 3];
        for ((sum, after), before) in sums.iter_mut().zip(through).zip(before) {
            *sum = after.wrapping_sub(*before);
        }
        Some(sums)
    }
}

/// `sum` with `pixel`'s blue, green, and red added. A row holds at most
/// 16384 pixels of 255, far below `u32::MAX`.
fn add_channels(sum: [u32; 3], pixel: [u8; 4]) -> [u32; 3] {
    let [blue, green, red, _] = pixel;
    let [sum_blue, sum_green, sum_red] = sum;
    [
        sum_blue.wrapping_add(u32::from(blue)),
        sum_green.wrapping_add(u32::from(green)),
        sum_red.wrapping_add(u32::from(red)),
    ]
}

/// Where a picture was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found {
    /// The picture's left edge in the image searched.
    pub x: u32,
    /// The picture's top edge in the image searched.
    pub y: u32,
    /// How alike the place is to the picture, from 0 to 100 percent.
    pub likeness: u8,
}

impl PictureSearch {
    /// A search for `picture`. It compares only the pixels `mask` keeps, or
    /// every pixel without one. `tolerance` is how far, on average, each
    /// compared pixel's red, green, and blue may be from the picture's.
    ///
    /// # Errors
    ///
    /// Returns [`MaskError::SizeMismatch`] when the mask's size differs from
    /// the picture's.
    pub fn new(picture: Image, mask: Option<&Mask>, tolerance: u8) -> Result<Self, MaskError> {
        let picture = MaskedPicture::new(picture, mask)?;
        let sums = picture
            .runs()
            .iter()
            .map(|run| {
                picture
                    .pixels(run)
                    .iter()
                    .fold([0; 3], |sum, pixel| add_channels(sum, *pixel))
            })
            .collect();
        Ok(Self {
            picture,
            sums,
            tolerance,
        })
    }

    /// The picture's size.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.picture.size()
    }

    /// The closest place in `image` where the picture is within its
    /// tolerance, or `None` when it is nowhere, including when the picture
    /// does not fit in `image`. A tie goes to the place found first, going
    /// across each row from the top.
    #[must_use]
    pub fn find(&self, image: &Image) -> Option<Found> {
        let across = image.size().width.checked_sub(self.size().width)?;
        let down = image.size().height.checked_sub(self.size().height)?;
        let mut limit = most_within(self.picture.channels(), self.tolerance);
        let sums = RowSums::new(image)?;
        let mut best = None;
        for y in 0..=down {
            for x in 0..=across {
                let (Ok(column), Ok(row)) = (usize::try_from(x), usize::try_from(y)) else {
                    continue;
                };
                if !self.may_match(&sums, column, row, limit) {
                    continue;
                }
                let Some(total) = self.picture.difference(image, column, row, limit) else {
                    continue;
                };
                best = Some((x, y, total));
                // Only a place closer than this one can take its place.
                let Some(closer) = total.checked_sub(1) else {
                    return best.map(|found| self.found(found));
                };
                limit = closer;
            }
        }
        best.map(|found| self.found(found))
    }

    /// Whether the place at (`x`, `y`) may be within `limit`, by the
    /// difference of each run's summed channels: a lower bound on the
    /// summed difference of its pixels' channels.
    fn may_match(&self, sums: &RowSums, x: usize, y: usize, limit: u64) -> bool {
        let mut bound = 0_u64;
        for (run, wanted) in self.picture.runs().iter().zip(&self.sums) {
            let found = y.checked_add(run.row).and_then(|row| {
                let columns = x.checked_add(run.columns.start)?..x.checked_add(run.columns.end)?;
                sums.run(row, &columns)
            });
            let Some(found) = found else {
                return false;
            };
            for (found, wanted) in found.iter().zip(wanted) {
                bound = bound.wrapping_add(u64::from(found.abs_diff(*wanted)));
            }
            if bound > limit {
                return false;
            }
        }
        true
    }

    /// The place at (`x`, `y`) whose difference is `total`, with its
    /// likeness.
    fn found(&self, (x, y, total): (u32, u32, u64)) -> Found {
        let most = self.picture.channels().saturating_mul(255);
        let unlike = total
            .saturating_mul(100)
            .checked_div(most)
            .unwrap_or_default();
        Found {
            x,
            y,
            likeness: u8::try_from(100_u64.saturating_sub(unlike)).unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An image of `width` by `height` whose pixel at (x, y) is gray
    /// `shade(x, y)`.
    fn image(width: u32, height: u32, shade: impl Fn(u32, u32) -> u8) -> Image {
        let mut bgra = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let gray = shade(x, y);
                bgra.extend([gray, gray, gray, 255]);
            }
        }
        Image::new(Size { width, height }, bgra).unwrap()
    }

    /// The grays of a 3x2 picture, row by row: each pixel differs.
    const GRAYS: [u8; 6] = [20, 60, 100, 140, 180, 220];

    fn picture() -> Image {
        image(3, 2, gray_of)
    }

    /// The picture's gray at (`x`, `y`).
    fn gray_of(x: u32, y: u32) -> u8 {
        let index = y.checked_mul(3).and_then(|row| row.checked_add(x)).unwrap();
        GRAYS[usize::try_from(index).unwrap()]
    }

    /// A 10x6 image of `background` with copies of the picture: each at its
    /// top-left (x, y), every channel raised by its `off`.
    fn area(background: u8, copies: &[(u32, u32, u8)]) -> Image {
        image(10, 6, |x, y| {
            copies
                .iter()
                .find_map(|&(left, top, off)| {
                    let across = x.checked_sub(left).filter(|&across| across < 3)?;
                    let down = y.checked_sub(top).filter(|&down| down < 2)?;
                    Some(gray_of(across, down).saturating_add(off))
                })
                .unwrap_or(background)
        })
    }

    /// Where `search` finds its picture in `image`.
    fn place(search: &PictureSearch, image: &Image) -> Option<(u32, u32)> {
        search.find(image).map(|found| (found.x, found.y))
    }

    #[test]
    fn running_sums_give_each_run_s_channels() {
        let sums = RowSums::new(&picture()).unwrap();
        assert_eq!(sums.run(0, &(0..3)), Some([180; 3]));
        assert_eq!(sums.run(1, &(0..2)), Some([320; 3]));
        assert_eq!(sums.run(0, &(1..3)), Some([160; 3]));
        assert_eq!(sums.run(1, &(2..2)), Some([0; 3]));
        assert_eq!(sums.run(0, &(1..4)), None);
        assert_eq!(sums.run(2, &(0..1)), None);
    }

    #[test]
    fn a_place_whose_row_sums_differ_too_much_cannot_match() {
        let search = PictureSearch::new(picture(), None, 0).unwrap();
        let sums = RowSums::new(&area(0, &[(2, 1, 10)])).unwrap();
        // Each row of three pixels is 30 brighter in each of its three
        // channels: 90 a row, 180 in all.
        assert!(search.may_match(&sums, 2, 1, 180));
        assert!(!search.may_match(&sums, 2, 1, 179));
        assert!(search.may_match(&sums, 2, 1, u64::MAX));
        assert!(!search.may_match(&sums, 8, 1, u64::MAX));
        assert!(!search.may_match(&sums, 2, 5, u64::MAX));
        let top_row = PictureSearch::new(
            picture(),
            Some(
                &Mask::new(
                    picture().size(),
                    vec![true, true, true, false, false, false],
                )
                .unwrap(),
            ),
            0,
        )
        .unwrap();
        assert!(top_row.may_match(&sums, 2, 1, 90));
        assert!(!top_row.may_match(&sums, 2, 1, 89));
    }

    #[test]
    fn finds_the_picture_where_it_is() {
        let search = PictureSearch::new(picture(), None, 0).unwrap();
        assert_eq!(
            search.size(),
            Size {
                width: 3,
                height: 2
            }
        );
        for (x, y) in [(0, 0), (7, 4), (4, 1), (7, 0), (0, 4)] {
            assert_eq!(
                search.find(&area(0, &[(x, y, 0)])),
                Some(Found {
                    x,
                    y,
                    likeness: 100
                }),
                "{x},{y}"
            );
        }
    }

    #[test]
    fn a_place_within_the_tolerance_counts() {
        let strict = PictureSearch::new(picture(), None, 9).unwrap();
        let loose = PictureSearch::new(picture(), None, 10).unwrap();
        let shifted = area(0, &[(5, 2, 10)]);
        assert_eq!(strict.find(&shifted), None);
        let found = loose.find(&shifted).unwrap();
        assert_eq!((found.x, found.y), (5, 2));
        assert_eq!(found.likeness, 97, "10 of 255 off is about 4 percent");
    }

    #[test]
    fn the_closest_place_wins_and_a_tie_goes_to_the_first() {
        let search = PictureSearch::new(picture(), None, 10).unwrap();
        assert_eq!(
            place(&search, &area(0, &[(0, 0, 5), (5, 3, 2)])),
            Some((5, 3))
        );
        assert_eq!(
            place(&search, &area(0, &[(0, 3, 2), (5, 0, 2)])),
            Some((5, 0)),
            "the upper row is searched first"
        );
        assert_eq!(
            place(&search, &area(0, &[(0, 3, 2), (6, 3, 2)])),
            Some((0, 3)),
            "across a row, the left one is first"
        );
        let found = search.find(&area(0, &[(0, 0, 2), (6, 3, 0)])).unwrap();
        assert_eq!((found.x, found.y, found.likeness), (6, 3, 100));
    }

    #[test]
    fn the_mask_leaves_out_the_background() {
        let picture = picture();
        // Only the middle column is compared, so the copy on a white
        // background is found by that column alone.
        let mask = Mask::new(picture.size(), vec![false, true, false, false, true, false]).unwrap();
        let only_middle = image(10, 6, |x, y| match (x, y) {
            (4, 2) => gray_of(1, 0),
            (4, 3) => gray_of(1, 1),
            _ => 255,
        });
        let unmasked = PictureSearch::new(picture.clone(), None, 10).unwrap();
        let masked = PictureSearch::new(picture, Some(&mask), 10).unwrap();
        assert_eq!(unmasked.find(&only_middle), None);
        let found = masked.find(&only_middle).unwrap();
        assert_eq!((found.x, found.y, found.likeness), (3, 2, 100));
    }

    #[test]
    fn each_run_of_a_row_has_its_summed_channels() {
        let picture = picture();
        // The kept run goes from the end of the first row into the second.
        let mask = Mask::new(picture.size(), vec![false, false, true, true, false, false]).unwrap();
        let search = PictureSearch::new(picture.clone(), Some(&mask), 0).unwrap();
        assert_eq!(*search.sums, [[100; 3], [140; 3]]);
        let kept_only = image(10, 6, |x, y| match (x, y) {
            (6, 1) => gray_of(2, 0),
            (4, 2) => gray_of(0, 1),
            _ => 0,
        });
        assert_eq!(place(&search, &kept_only), Some((4, 1)));
        let whole = PictureSearch::new(picture, None, 0).unwrap();
        assert_eq!(*whole.sums, [[180; 3], [540; 3]]);
    }

    #[test]
    fn a_picture_larger_than_the_area_is_nowhere() {
        let search = PictureSearch::new(picture(), None, 255).unwrap();
        assert_eq!(search.find(&image(2, 6, |_, _| 0)), None);
        assert_eq!(search.find(&image(10, 1, |_, _| 0)), None);
        assert_eq!(place(&search, &image(3, 2, |_, _| 0)), Some((0, 0)));
    }

    #[test]
    fn likeness_falls_with_the_difference() {
        let search = PictureSearch::new(image(1, 1, |_, _| 0), None, 255).unwrap();
        let likeness = |gray| search.find(&image(1, 1, |_, _| gray)).unwrap().likeness;
        assert_eq!(likeness(0), 100);
        assert_eq!(likeness(255), 0);
        assert_eq!(likeness(128), 50);
        assert_eq!(likeness(3), 99);
    }

    #[test]
    fn a_mask_of_another_size_is_refused() {
        let mask = Mask::new(
            Size {
                width: 1,
                height: 1,
            },
            vec![true],
        )
        .unwrap();
        assert_eq!(
            PictureSearch::new(picture(), Some(&mask), 0),
            Err(MaskError::SizeMismatch {
                image: Size {
                    width: 3,
                    height: 2
                },
                mask: Size {
                    width: 1,
                    height: 1
                },
            })
        );
    }
}
