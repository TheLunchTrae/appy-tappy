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
//! drawn at. It also says why a recording is not placed at a size
//! ([`Unplaced`]), which the recording tells the user.

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
    /// window of `now`, or why it is not recorded there. A scaled region
    /// that reaches outside the window is not, since its pixels are what
    /// is recorded, and neither is one scaled down to fewer rows than
    /// [`MIN_TEXT_HEIGHT`], which holds no text that tall.
    pub(crate) fn region(self, region: Region, drawn: Size, now: Size) -> Result<Region, Unplaced> {
        match self {
            Self::Drawn if drawn == now => Ok(region),
            Self::Drawn => Err(Unplaced::NotFollowed),
            Self::Scaled(pin) => {
                let scaled = scale::scaled(region, drawn, now, pin).ok_or(Unplaced::Outside)?;
                legible(scaled.size().height, region.size().height)?;
                Ok(scaled)
            },
        }
    }

    /// The part of a window of `now` that a look over the whole window
    /// searches near, for a recording whose `region` was drawn on a window
    /// of `drawn`, or why the window is not looked over at that size. The
    /// look only orders what it finds by how near it is, so a region that
    /// scales outside the window is moved and cut to lie inside it rather
    /// than refused.
    pub(crate) fn near(self, region: Region, drawn: Size, now: Size) -> Result<Region, Unplaced> {
        match self {
            Self::Drawn if drawn == now => Ok(region),
            Self::Drawn => Err(Unplaced::NotFollowed),
            Self::Scaled(pin) => {
                scale::scaled_within(region, drawn, now, pin).ok_or(Unplaced::Outside)
            },
        }
    }

    /// How tall text `height` pixels tall on a window of `drawn` is on a
    /// window of `now`, or why text of that size is not looked for there:
    /// the recording does not follow the window to that size, or scaling
    /// made the text shorter than [`MIN_TEXT_HEIGHT`].
    pub(crate) fn height(self, height: u32, drawn: Size, now: Size) -> Result<u32, Unplaced> {
        match self {
            Self::Drawn if drawn == now => Ok(height),
            Self::Drawn => Err(Unplaced::NotFollowed),
            Self::Scaled(_) => {
                let scaled = scale::scaled_length(height, drawn, now).ok_or(Unplaced::Outside)?;
                legible(scaled, height)?;
                Ok(scaled)
            },
        }
    }
}

/// The fewest rows of pixels a character can have and still be told from
/// another: the smallest pixel fonts draw a digit on five rows, and on
/// fewer rows an `8`, a `0`, and a `6` run together into one shape.
pub(crate) const MIN_TEXT_HEIGHT: u32 = 5;

/// Why a recording is not placed on a window of some size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unplaced {
    /// It does not follow its window to that size.
    NotFollowed,
    /// Scaled to that size, its region reaches outside the window, or the
    /// window has no pixels.
    Outside,
    /// Scaled down to that size, its text has fewer rows than
    /// [`MIN_TEXT_HEIGHT`].
    TooSmall,
}

/// Refuses text that scaling made shorter than [`MIN_TEXT_HEIGHT`]:
/// `scaled` rows where it was `drawn` rows. Text the user drew that short
/// is theirs to judge, so only scaling down is refused.
fn legible(scaled: u32, drawn: u32) -> Result<(), Unplaced> {
    if scaled < MIN_TEXT_HEIGHT && scaled < drawn {
        Err(Unplaced::TooSmall)
    } else {
        Ok(())
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
        let not = Err(Unplaced::NotFollowed);
        assert_eq!(follows.region(place, DRAWN, DRAWN), Ok(place));
        assert_eq!(follows.region(place, DRAWN, tall(400, 200)), not);
        assert_eq!(follows.region(place, DRAWN, tall(201, 100)), not);
        assert_eq!(follows.near(place, DRAWN, DRAWN), Ok(place));
        assert_eq!(follows.near(place, DRAWN, tall(400, 200)), not);
        assert_eq!(follows.height(12, DRAWN, DRAWN), Ok(12));
        assert_eq!(
            follows.height(12, DRAWN, tall(400, 200)),
            Err(Unplaced::NotFollowed)
        );
        assert_eq!(
            follows.height(12, DRAWN, tall(200, 101)),
            Err(Unplaced::NotFollowed)
        );
    }

    #[test]
    fn a_window_that_often_changes_size_scales_the_region_and_a_height_by_one_factor() {
        let place = region(160, 10, 20, 10);
        let follows = Follows::of(SizeChangesDef::Often, Some(PinDef::TopRight), place, DRAWN);
        assert_eq!(follows, Follows::Scaled(PinDef::TopRight));
        assert_eq!(follows.region(place, DRAWN, DRAWN), Ok(place));
        // Twice the height, and three times the width: the factor is 2,
        // and the region keeps twice its 40 from the right edge.
        let now = tall(600, 200);
        assert_eq!(
            follows.region(place, DRAWN, now),
            Ok(region(520, 20, 40, 20))
        );
        assert_eq!(follows.near(place, DRAWN, now), Ok(region(520, 20, 40, 20)));
        assert_eq!(follows.height(12, DRAWN, now), Ok(24));
        // Half the height: 13 is 6.5, which rounds up as an edge does.
        assert_eq!(follows.height(13, DRAWN, tall(200, 50)), Ok(7));
    }

    #[test]
    fn a_region_scaled_past_a_window_s_edge_is_recorded_nowhere_and_looked_for_near_the_edge() {
        // Pinned left, its 160 from the left edge is past a narrow window.
        let place = region(160, 10, 20, 10);
        let follows = Follows::Scaled(PinDef::TopLeft);
        let narrow = tall(170, 100);
        assert_eq!(follows.region(place, DRAWN, narrow), Err(Unplaced::Outside));
        assert_eq!(
            follows.near(place, DRAWN, narrow),
            Ok(region(160, 10, 10, 10))
        );
        assert_eq!(
            follows.near(place, DRAWN, tall(100, 100)),
            Ok(region(99, 10, 1, 10))
        );
        // A window with no pixels has no part to look near.
        assert_eq!(
            follows.near(place, DRAWN, tall(0, 100)),
            Err(Unplaced::Outside)
        );
        assert_eq!(
            follows.height(12, tall(200, 0), DRAWN),
            Err(Unplaced::Outside)
        );
    }

    #[test]
    fn text_scaled_down_to_fewer_rows_than_tell_characters_apart_is_refused() {
        let follows = Follows::Scaled(PinDef::TopLeft);
        let half = tall(100, 50);
        assert_eq!(follows.height(10, DRAWN, half), Ok(5));
        assert_eq!(follows.height(8, DRAWN, half), Err(Unplaced::TooSmall));
        // Text the user drew that small is not refused where it is no
        // smaller: at the size drawn, or scaled up.
        assert_eq!(follows.height(4, DRAWN, DRAWN), Ok(4));
        assert_eq!(follows.height(3, DRAWN, tall(266, 133)), Ok(4));
        // A region is refused where it has too few rows to hold such text.
        let place = region(10, 10, 20, 10);
        assert_eq!(follows.region(place, DRAWN, half), Ok(region(5, 5, 10, 5)));
        assert_eq!(
            follows.region(place, DRAWN, tall(80, 40)),
            Err(Unplaced::TooSmall)
        );
        assert_eq!(
            follows.near(place, DRAWN, tall(80, 40)),
            Ok(region(4, 4, 8, 4)),
            "a look's sizes say whether its text is too small"
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
