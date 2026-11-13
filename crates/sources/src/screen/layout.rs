//! Where a capture puts the regions it copies out of a frame.
//!
//! A capture of several values copies each value's region into one packed
//! image, stacked top to bottom at its left edge, so it moves only the pixels
//! the values need. When the regions overlap enough that one rectangle around
//! them all is smaller, or the stack would be too tall for a texture, the
//! capture copies that rectangle instead.

use super::{Region, Size};

/// The tallest packed image: the tallest texture that every Direct3D 11
/// device makes, down to feature level 10.
const TALLEST: u32 = 8_192;

/// One copy from a frame into the packed image, whose left edge it starts
/// at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// The part of the frame copied.
    pub from: Region,
    /// The row of the packed image where it goes.
    pub y: u32,
}

/// What a capture copies out of each frame for several regions, and where
/// each region then sits in the image it lends its handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The packed image's size.
    size: Size,
    /// The copies that fill it.
    copies: Box<[Placement]>,
    /// Each region given, in order, as it sits in the packed image, or
    /// `None` where none was given.
    slots: Box<[Option<Region>]>,
}

impl Layout {
    /// The layout for one region.
    #[must_use]
    pub fn one(region: Region) -> Self {
        Self {
            size: region.size(),
            copies: Box::new([Placement { from: region, y: 0 }]),
            slots: Box::new([Region::whole(region.size())]),
        }
    }

    /// The layout for `regions`, where `None` stands for a slot with no
    /// region, such as a value drawn at another window size. It is `None`
    /// when no slot has a region.
    #[must_use]
    pub fn new(regions: impl IntoIterator<Item = Option<Region>>) -> Option<Self> {
        let regions: Vec<Option<Region>> = regions.into_iter().collect();
        let bounds = regions.iter().flatten().copied().reduce(union)?;
        Some(
            Self::stacked(&regions)
                .filter(|stacked| area(stacked.size) < area(bounds.size()))
                .unwrap_or_else(|| Self::bounded(&regions, bounds)),
        )
    }

    /// Each distinct region stacked top to bottom, or `None` when the stack
    /// would be too tall.
    fn stacked(regions: &[Option<Region>]) -> Option<Self> {
        let mut copies: Vec<Placement> = Vec::new();
        let mut height = 0_u32;
        let mut width = 0_u32;
        let mut slots = Vec::with_capacity(regions.len());
        for region in regions {
            let Some(region) = *region else {
                slots.push(None);
                continue;
            };
            let y = if let Some(copy) = copies.iter().find(|copy| copy.from == region) {
                copy.y
            } else {
                let y = height;
                copies.push(Placement { from: region, y });
                height = height.checked_add(region.size().height)?;
                width = width.max(region.size().width);
                y
            };
            let size = region.size();
            slots.push(Region::new(0, y, size.width, size.height));
        }
        (height <= TALLEST).then(|| Self {
            size: Size { width, height },
            copies: copies.into(),
            slots: slots.into(),
        })
    }

    /// The one rectangle around every region.
    fn bounded(regions: &[Option<Region>], bounds: Region) -> Self {
        Self {
            size: bounds.size(),
            copies: Box::new([Placement { from: bounds, y: 0 }]),
            slots: regions
                .iter()
                .map(|region| region.and_then(|region| within(region, bounds)))
                .collect(),
        }
    }

    /// The packed image's size.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// The copies that fill the packed image.
    #[must_use]
    pub fn copies(&self) -> &[Placement] {
        &self.copies
    }

    /// Where the region in slot `index` sits in the packed image, or `None`
    /// for a slot without one.
    #[must_use]
    pub fn packed(&self, index: usize) -> Option<Region> {
        self.slots.get(index).copied().flatten()
    }
}

/// The smallest region that holds both.
fn union(one: Region, other: Region) -> Region {
    let left = one.x().min(other.x());
    let top = one.y().min(other.y());
    let right = one.right().max(other.right());
    let bottom = one.bottom().max(other.bottom());
    // Both regions are inside the `u32` range, so their union is too, and it
    // is no smaller than either.
    Region::new(
        left,
        top,
        right.saturating_sub(left),
        bottom.saturating_sub(top),
    )
    .unwrap_or(one)
}

/// `region`, measured from the corner of `bounds`.
fn within(region: Region, bounds: Region) -> Option<Region> {
    let size = region.size();
    Region::new(
        region.x().checked_sub(bounds.x())?,
        region.y().checked_sub(bounds.y())?,
        size.width,
        size.height,
    )
}

/// The pixels in an image of `size`.
fn area(size: Size) -> u64 {
    u64::from(size.width).saturating_mul(u64::from(size.height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(x: u32, y: u32, width: u32, height: u32) -> Region {
        Region::new(x, y, width, height).unwrap()
    }

    #[test]
    fn far_apart_regions_are_stacked_at_the_left_edge() {
        let layout = Layout::new([
            Some(region(0, 0, 10, 4)),
            None,
            Some(region(2000, 1000, 6, 3)),
        ])
        .unwrap();
        assert_eq!(
            layout.size(),
            Size {
                width: 10,
                height: 7
            }
        );
        assert_eq!(
            layout.copies(),
            [
                Placement {
                    from: region(0, 0, 10, 4),
                    y: 0
                },
                Placement {
                    from: region(2000, 1000, 6, 3),
                    y: 4
                },
            ]
        );
        assert_eq!(layout.packed(0), Some(region(0, 0, 10, 4)));
        assert_eq!(layout.packed(1), None);
        assert_eq!(layout.packed(2), Some(region(0, 4, 6, 3)));
        assert_eq!(layout.packed(3), None);
    }

    #[test]
    fn the_same_region_twice_is_copied_once() {
        let layout = Layout::new([Some(region(5, 5, 4, 4)), Some(region(5, 5, 4, 4))]).unwrap();
        assert_eq!(layout.copies().len(), 1);
        assert_eq!(layout.packed(1), layout.packed(0));
    }

    #[test]
    fn overlapping_regions_copy_the_rectangle_around_them() {
        let layout =
            Layout::new([Some(region(10, 10, 20, 20)), Some(region(12, 12, 18, 18))]).unwrap();
        assert_eq!(
            layout.copies(),
            [Placement {
                from: region(10, 10, 20, 20),
                y: 0
            }]
        );
        assert_eq!(layout.packed(1), Some(region(2, 2, 18, 18)));
    }

    #[test]
    fn a_stack_too_tall_for_a_texture_copies_the_rectangle_instead() {
        let tall = (0..5).map(|column| Some(region(column * 1000, 0, 1, 4000)));
        let layout = Layout::new(tall).unwrap();
        assert_eq!(layout.copies().len(), 1);
        assert_eq!(
            layout.size(),
            Size {
                width: 4001,
                height: 4000
            }
        );
    }

    #[test]
    fn a_stack_no_smaller_than_the_rectangle_copies_the_rectangle() {
        // Side by side, the stack is 2x4 and the rectangle 4x2.
        let layout = Layout::new([Some(region(0, 0, 2, 2)), Some(region(2, 0, 2, 2))]).unwrap();
        assert_eq!(
            layout.copies(),
            [Placement {
                from: region(0, 0, 4, 2),
                y: 0
            }]
        );
    }

    #[test]
    fn a_stack_as_tall_as_a_texture_can_be_is_kept() {
        let half = TALLEST / 2;
        let layout =
            Layout::new([Some(region(0, 0, 1, half)), Some(region(100, 0, 1, half))]).unwrap();
        assert_eq!(layout.copies().len(), 2);
        assert_eq!(layout.size().height, TALLEST);
        let taller = Layout::new([
            Some(region(0, 0, 1, half)),
            Some(region(100, 0, 1, half + 1)),
        ])
        .unwrap();
        assert_eq!(taller.copies().len(), 1, "one row too tall");
    }

    #[test]
    fn no_region_makes_no_layout() {
        assert_eq!(Layout::new([None, None]), None);
    }

    #[test]
    fn one_region_is_the_whole_packed_image() {
        let layout = Layout::one(region(7, 9, 3, 2));
        assert_eq!(layout.packed(0), Some(region(0, 0, 3, 2)));
        assert_eq!(
            layout.size(),
            Size {
                width: 3,
                height: 2
            }
        );
    }
}
