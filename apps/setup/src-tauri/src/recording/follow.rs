//! How a recording follows its window's size.
//!
//! A recording is started on a screenshot: its region, and the heights of
//! the text a look over the whole window searches for, are in that
//! window's pixels. This module is the one place that says what they are
//! on a window of another size, so the rule changes here alone. It adds no
//! size rule of its own. An application's saved values read at another
//! size as its user answered ([`SizeChangesDef`]), and a recording follows
//! the same answer with the same arithmetic ([`scale`]): where the window
//! changes size often, the region keeps its scaled distance from the side
//! it stays at and a height scales by the same factor, and where it rarely
//! does, nothing is guessed and the recording waits for the size it was
//! drawn at.

use appytappy_config::app_file::{PinDef, RegionDef, SizeChangesDef, SizeDef};
use appytappy_sources::screen::{Region, Size, scale};

/// How a recording follows its window to another size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Follows {
    /// It does not: it records only at the size it was drawn at.
    Drawn,
    /// It scales with the window, its region staying at this side.
    Scaled(PinDef),
}

impl Follows {
    /// How a recording of `region`, drawn on a window of `drawn`, follows
    /// the window of an application whose window changes size as
    /// `size_changes` says. A region that scales stays at `pin`, or at the
    /// side it is nearest when the user chose none, as a saved value's
    /// does.
    pub(crate) fn of(
        size_changes: SizeChangesDef,
        pin: Option<PinDef>,
        region: Region,
        drawn: Size,
    ) -> Self {
        match size_changes {
            SizeChangesDef::Rarely => Self::Drawn,
            SizeChangesDef::Often => Self::Scaled(pin.unwrap_or_else(|| {
                PinDef::guess(
                    RegionDef {
                        x: region.x(),
                        y: region.y(),
                        width: region.size().width,
                        height: region.size().height,
                    },
                    SizeDef {
                        width: drawn.width,
                        height: drawn.height,
                    },
                )
            })),
        }
    }

    /// Where `region`, drawn on a window of `drawn`, is recorded on a
    /// window of `now`, or `None` when it is not recorded there: the
    /// recording does not follow the window to that size, or the scaled
    /// region reaches outside it.
    pub(crate) fn region(self, region: Region, drawn: Size, now: Size) -> Option<Region> {
        match self {
            Self::Drawn => (drawn == now).then_some(region),
            Self::Scaled(pin) => scale::scaled(region, drawn, now, pin),
        }
    }

    /// How tall text `height` pixels tall on a window of `drawn` is on a
    /// window of `now`, or `None` when the recording does not follow the
    /// window there.
    pub(crate) fn height(self, height: u32, drawn: Size, now: Size) -> Option<u32> {
        match self {
            Self::Drawn => (drawn == now).then_some(height),
            Self::Scaled(_) => scale::scaled_length(height, drawn, now),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRAWN: Size = Size {
        width: 200,
        height: 100,
    };

    fn region(x: u32, y: u32, width: u32, height: u32) -> Region {
        Region::new(x, y, width, height).expect("not empty")
    }

    fn tall(width: u32, height: u32) -> Size {
        Size { width, height }
    }

    #[test]
    fn a_window_that_rarely_changes_size_is_recorded_only_at_the_size_drawn() {
        let follows = Follows::of(
            SizeChangesDef::Rarely,
            Some(PinDef::TopLeft),
            region(10, 10, 20, 10),
            DRAWN,
        );
        assert_eq!(follows, Follows::Drawn);
        let place = region(10, 10, 20, 10);
        assert_eq!(follows.region(place, DRAWN, DRAWN), Some(place));
        assert_eq!(follows.region(place, DRAWN, tall(400, 200)), None);
        assert_eq!(follows.region(place, DRAWN, tall(201, 100)), None);
        assert_eq!(follows.height(12, DRAWN, DRAWN), Some(12));
        assert_eq!(follows.height(12, DRAWN, tall(400, 200)), None);
        assert_eq!(follows.height(12, DRAWN, tall(200, 101)), None);
    }

    #[test]
    fn a_window_that_often_changes_size_scales_the_region_and_a_height_by_one_factor() {
        let place = region(160, 10, 20, 10);
        let follows = Follows::of(SizeChangesDef::Often, Some(PinDef::TopRight), place, DRAWN);
        assert_eq!(follows, Follows::Scaled(PinDef::TopRight));
        assert_eq!(follows.region(place, DRAWN, DRAWN), Some(place));
        // Twice the height, and three times the width: the factor is 2,
        // and the region keeps twice its 40 from the right edge.
        let now = tall(600, 200);
        assert_eq!(
            follows.region(place, DRAWN, now),
            Some(region(520, 20, 40, 20))
        );
        assert_eq!(follows.height(12, DRAWN, now), Some(24));
        // Half the height: 13 is 6.5, which rounds up as an edge does.
        assert_eq!(follows.height(13, DRAWN, tall(200, 50)), Some(7));
        // Pinned left, its 160 from the left edge is past a narrow window.
        assert_eq!(
            Follows::Scaled(PinDef::TopLeft).region(place, DRAWN, tall(170, 100)),
            None
        );
    }

    #[test]
    fn a_region_with_no_side_chosen_stays_at_the_side_it_is_nearest() {
        let near = |x, y| Follows::of(SizeChangesDef::Often, None, region(x, y, 20, 10), DRAWN);
        assert_eq!(near(0, 0), Follows::Scaled(PinDef::TopLeft));
        assert_eq!(near(180, 90), Follows::Scaled(PinDef::BottomRight));
        assert_eq!(near(90, 45), Follows::Scaled(PinDef::Center));
    }
}
