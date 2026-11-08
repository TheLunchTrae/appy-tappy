//! Where a region drawn at one window size lies at another.
//!
//! Applications draw their interface larger as their window grows taller, so
//! a region scales by one factor, the window's new height over its height
//! when the region was drawn. Its size scales by that factor, and its
//! place keeps its scaled distance from the side of the window it is pinned
//! to: a region pinned left keeps `x · f` from the left edge, one pinned
//! right keeps `(W − x) · f` from the right edge, and one pinned to the
//! center keeps `(x − W / 2) · f` from the middle; the same holds down the
//! window for top, middle and bottom. Each edge rounds to the nearest whole
//! pixel, halves away from zero, and the region's size is the distance
//! between its rounded edges, so a region touching the window's edge still
//! touches it.

use appytappy_config::app_file::{Along, PinDef};

use super::{Region, Size};

/// Where `region`, drawn on a window of `drawn`, lies on a window of `now`
/// when it stays at `pin`, or `None` when it would be empty or reach outside
/// that window.
#[must_use]
pub fn scaled(region: Region, drawn: Size, now: Size, pin: PinDef) -> Option<Region> {
    let factor = Factor {
        new: i128::from(now.height),
        old: i128::from(drawn.height),
    };
    let (x, width) = span(
        pin.horizontal(),
        region.x(),
        region.size().width,
        (drawn.width, now.width),
        factor,
    )?;
    let (y, height) = span(
        pin.vertical(),
        region.y(),
        region.size().height,
        (drawn.height, now.height),
        factor,
    )?;
    let scaled = Region::new(
        u32::try_from(x).ok()?,
        u32::try_from(y).ok()?,
        u32::try_from(width).ok()?,
        u32::try_from(height).ok()?,
    )?;
    scaled.fits(now).then_some(scaled)
}

/// Where a span from `start` that is `length` long, along a side that was
/// `sides.0` long and is now `sides.1`, starts and how long it is. Both its
/// edges are placed and rounded, and its length is their difference, at
/// least 1, so a span that touches its side's end still touches it.
fn span(
    along: Along,
    start: u32,
    length: u32,
    sides: (u32, u32),
    factor: Factor,
) -> Option<(i128, i128)> {
    let (side, new_side) = (i128::from(sides.0), i128::from(sides.1));
    let first = place(along, i128::from(start), side, new_side, factor)?;
    let end = i128::from(start).checked_add(i128::from(length))?;
    let last = place(along, end, side, new_side, factor)?;
    Some((first, last.checked_sub(first)?.max(1)))
}

/// A scale factor, `new / old`.
#[derive(Debug, Clone, Copy)]
struct Factor {
    new: i128,
    old: i128,
}

impl Factor {
    /// `length` scaled and rounded, or `None` when the factor has no `old`.
    fn of(self, length: i128) -> Option<i128> {
        rounded(length.checked_mul(self.new)?, self.old)
    }
}

/// Where an edge at `start` on a side `length` long lies on a side
/// `new_length` long, kept at its scaled distance from `along`.
fn place(
    along: Along,
    start: i128,
    length: i128,
    new_length: i128,
    factor: Factor,
) -> Option<i128> {
    match along {
        Along::Start => factor.of(start),
        // new_length − (length − start) · f, over the factor's `old`.
        Along::End => rounded(
            new_length
                .checked_mul(factor.old)?
                .checked_sub(length.checked_sub(start)?.checked_mul(factor.new)?)?,
            factor.old,
        ),
        // new_length / 2 + (start − length / 2) · f, over twice the `old`.
        Along::Middle => rounded(
            new_length.checked_mul(factor.old)?.checked_add(
                start
                    .checked_mul(2)?
                    .checked_sub(length)?
                    .checked_mul(factor.new)?,
            )?,
            factor.old.checked_mul(2)?,
        ),
    }
}

/// `numerator / denominator` rounded to the nearest whole number, halves
/// away from zero, or `None` for a denominator that is not positive.
fn rounded(numerator: i128, denominator: i128) -> Option<i128> {
    if denominator <= 0 {
        return None;
    }
    let half = numerator.abs().checked_mul(2)?.checked_add(denominator)?;
    let magnitude = half.checked_div(denominator.checked_mul(2)?)?;
    // The sign comes back by multiplying, so zero needs no case of its own.
    magnitude.checked_mul(numerator.signum())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRAWN: Size = Size {
        width: 1920,
        height: 1080,
    };

    fn region(x: u32, y: u32, width: u32, height: u32) -> Region {
        Region::new(x, y, width, height).expect("not empty")
    }

    fn at(pin: PinDef, now: Size) -> Option<Region> {
        // 100 by 40 at (40, 60): near the top-left corner of a 1920 by 1080
        // window, 1880 from its right edge and 1020 from its bottom.
        scaled(region(40, 60, 100, 40), DRAWN, now, pin)
    }

    #[test]
    fn each_pin_keeps_its_scaled_distance_from_its_side() {
        // Wider and taller by a third: f is 4 / 3. Its edges round to 53
        // and 187 across, so it is 134 wide, and 80 to 133 down, so 53
        // high; it is 80 from the top whichever way it is pinned down, since f
        // follows the height.
        let larger = Size {
            width: 3440,
            height: 1440,
        };
        let expected = [
            (PinDef::TopLeft, region(53, 80, 134, 53)),
            // 1720 + (40 − 960) · 4 / 3 = 493.33.
            (PinDef::TopCenter, region(493, 80, 134, 53)),
            // 3440 − 1880 · 4 / 3 = 933.33.
            (PinDef::TopRight, region(933, 80, 134, 53)),
            // 720 + (60 − 540) · 4 / 3 = 80.
            (PinDef::MiddleLeft, region(53, 80, 134, 53)),
            (PinDef::Center, region(493, 80, 134, 53)),
            (PinDef::MiddleRight, region(933, 80, 134, 53)),
            // 1440 − 1020 · 4 / 3 = 80.
            (PinDef::BottomLeft, region(53, 80, 134, 53)),
            (PinDef::BottomCenter, region(493, 80, 134, 53)),
            (PinDef::BottomRight, region(933, 80, 134, 53)),
        ];
        for (pin, region) in expected {
            assert_eq!(at(pin, larger), Some(region), "{pin:?}");
        }
        // Half the height and narrower: f is one half. Pinned left, the
        // region keeps half its distance; pinned to the center or the right,
        // half its distance would put it left of the window.
        let half = Size {
            width: 800,
            height: 540,
        };
        assert_eq!(at(PinDef::TopLeft, half), Some(region(20, 30, 50, 20)));
        assert_eq!(at(PinDef::BottomLeft, half), Some(region(20, 30, 50, 20)));
        assert_eq!(at(PinDef::TopCenter, half), None, "400 − 460 across");
        assert_eq!(at(PinDef::TopRight, half), None, "800 − 940 across");
    }

    #[test]
    fn a_wider_window_moves_only_regions_pinned_away_from_the_left() {
        // Same height, so f is 1: only the width changes where it lies.
        let wide = Size {
            width: 2560,
            height: 1080,
        };
        assert_eq!(at(PinDef::BottomLeft, wide), Some(region(40, 60, 100, 40)));
        assert_eq!(at(PinDef::TopCenter, wide), Some(region(360, 60, 100, 40)));
        assert_eq!(at(PinDef::TopRight, wide), Some(region(680, 60, 100, 40)));
        assert_eq!(at(PinDef::TopLeft, DRAWN), Some(region(40, 60, 100, 40)));
    }

    #[test]
    fn edges_round_to_the_nearest_pixel() {
        assert_eq!(rounded(5, 2), Some(3), "a half rounds up");
        assert_eq!(rounded(-5, 2), Some(-3), "and down below zero");
        assert_eq!(rounded(7, 3), Some(2));
        assert_eq!(rounded(8, 3), Some(3));
        assert_eq!(rounded(-7, 3), Some(-2));
        assert_eq!(rounded(0, 3), Some(0));
        assert_eq!(rounded(1, 0), None);
        assert_eq!(rounded(1, -1), None);
        // 3 · 2 / 3 is exactly 2 wide, and 1 · 2 / 3 rounds up to 1.
        let tiny = scaled(
            region(1, 1, 3, 1),
            Size {
                width: 30,
                height: 3,
            },
            Size {
                width: 30,
                height: 2,
            },
            PinDef::TopLeft,
        );
        assert_eq!(tiny, Some(region(1, 1, 2, 1)));
    }

    #[test]
    fn a_region_touching_its_pinned_edge_still_touches_it_at_a_half_pixel() {
        let half = Size {
            width: 960,
            height: 540,
        };
        // Its left edge is 133.5 from the right edge, which rounds to 134,
        // and so would its 133.5 width, which would end 1 pixel outside.
        let right = region(1653, 0, 267, 41);
        assert_eq!(
            scaled(right, DRAWN, half, PinDef::TopRight),
            Some(region(827, 0, 133, 21))
        );
        // The same down the window, pinned to the bottom.
        let bottom = region(0, 1013, 41, 67);
        assert_eq!(
            scaled(bottom, DRAWN, half, PinDef::BottomLeft),
            Some(region(0, 507, 21, 33))
        );
        // A span whose edges round together keeps a pixel.
        assert_eq!(
            span(
                Along::Start,
                0,
                1,
                (1080, 54),
                Factor { new: 54, old: 1080 }
            ),
            Some((0, 1))
        );
    }

    #[test]
    fn a_region_that_scales_to_nothing_or_past_an_edge_is_not_read() {
        let flat = Size {
            width: 1920,
            height: 0,
        };
        assert_eq!(at(PinDef::TopLeft, flat), None);
        // A 1 pixel line at a twentieth of the height keeps one row and
        // one column.
        let small = Size {
            width: 1920,
            height: 54,
        };
        assert_eq!(
            scaled(region(0, 0, 10, 1), DRAWN, small, PinDef::TopLeft),
            Some(region(0, 0, 1, 1))
        );
        // Touching the right edge fits; one more pixel does not.
        let edge = region(1820, 0, 100, 40);
        assert_eq!(scaled(edge, DRAWN, DRAWN, PinDef::TopRight), Some(edge));
        let narrower = Size {
            width: 1919,
            height: 1080,
        };
        assert_eq!(
            scaled(edge, DRAWN, narrower, PinDef::TopLeft),
            None,
            "pinned left, it keeps its place and passes the edge"
        );
        assert_eq!(
            scaled(edge, DRAWN, narrower, PinDef::TopRight),
            Some(region(1819, 0, 100, 40))
        );
        let shorter = Size {
            width: 1920,
            height: 1079,
        };
        let bottom = region(0, 1040, 100, 40);
        assert_eq!(
            scaled(bottom, DRAWN, shorter, PinDef::BottomLeft),
            Some(region(0, 1039, 100, 40))
        );
    }
}
