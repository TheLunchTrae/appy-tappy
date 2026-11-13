//! Selecting the thing a state check looks for on a screenshot.
//!
//! The setup app offers two ways to select it instead of drawing its box
//! exactly. [`snap`] shrinks a rough box to the thing inside it, and
//! [`grow`] spreads from a clicked pixel over similar colors, like a magic
//! wand. Either gives a [`Selection`]: the box around the thing and a
//! [`Mask`] of its pixels, which the check compares while it ignores the
//! background around them.
//!
//! Colors are compared per channel: two colors are alike when each of their
//! red, green, and blue differ by at most a tolerance, as [`Rgb::near`] says.

use std::collections::HashMap;

use super::mask::{Mask, pixels};
use super::{Image, Region, Rgb, Size};

/// The most background colors [`snap`] collects from a box's edge. A plain
/// background, even a gradient, has far fewer; more means the edge is not
/// plain, and further colors would only slow the snap down.
const MAX_BACKGROUND_COLORS: usize = 256;

/// A thing selected on a screenshot: its box and which pixels in the box
/// belong to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    region: Region,
    mask: Mask,
}

impl Selection {
    /// The box around the selected pixels, in the screenshot's pixels.
    #[must_use]
    pub const fn region(&self) -> Region {
        self.region
    }

    /// Which pixels of [`Selection::region`] are selected.
    #[must_use]
    pub const fn mask(&self) -> &Mask {
        &self.mask
    }

    /// Whether the pixel at (`x`, `y`) of the screenshot is selected.
    #[must_use]
    pub fn contains(&self, x: u32, y: u32) -> bool {
        let (Some(column), Some(row)) = (
            x.checked_sub(self.region.x()),
            y.checked_sub(self.region.y()),
        ) else {
            return false;
        };
        let size = self.region.size();
        // A row past the bottom gives an index past the end, which `get`
        // refuses; a column past the right would wrap into the next row.
        column < size.width
            && index(column, row, size)
                .and_then(|index| self.mask.kept().get(index))
                .is_some_and(|&kept| kept)
    }

    /// The pixels selected here or in `other`.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let left = self.region.x().min(other.region.x());
        let top = self.region.y().min(other.region.y());
        let right = self.region.right().max(other.region.right());
        let bottom = self.region.bottom().max(other.region.bottom());
        // Both regions are non-empty, so their union is too.
        Region::new(
            left,
            top,
            right.saturating_sub(left),
            bottom.saturating_sub(top),
        )
        .and_then(|region| trim(region, |x, y| self.contains(x, y) || other.contains(x, y)))
        .unwrap_or_else(|| self.clone())
    }

    /// The pixels selected here but not in `other`, or `None` when `other`
    /// covers them all.
    #[must_use]
    pub fn without(&self, other: &Self) -> Option<Self> {
        trim(self.region, |x, y| {
            self.contains(x, y) && !other.contains(x, y)
        })
    }

    /// The selected pixels of `region`, which may differ from the
    /// selection's own box, as after the user adjusts the box by hand. It is
    /// `None` when no pixel of `region` is selected.
    #[must_use]
    pub fn mask_in(&self, region: Region) -> Option<Mask> {
        let kept = points(region).map(|(x, y)| self.contains(x, y)).collect();
        Mask::new(region.size(), kept).ok()
    }
}

/// The thing inside `rough`, a box the user dragged around it on `image`
/// with some background on every side.
///
/// The colors along the box's edge are the background, the first 256
/// different ones. Every pixel within `tolerance` of one of them is left
/// out, including background showing between and inside the thing's parts,
/// such as inside the letter O. It is `None` when `rough` reaches past the
/// image or holds only background.
#[must_use]
pub fn snap(image: &Image, rough: Region, tolerance: u8) -> Option<Selection> {
    if !rough.fits(image.size()) {
        return None;
    }
    let mut background: Vec<Rgb> = Vec::new();
    for color in edge(rough).filter_map(|(x, y)| image.pixel(x, y)) {
        if background.len() < MAX_BACKGROUND_COLORS && !background.contains(&color) {
            background.push(color);
        }
    }
    // A screenshot repeats its colors, so each is compared with the
    // background once.
    let mut known: HashMap<Rgb, bool> = HashMap::new();
    let kept: Vec<bool> = points(rough)
        .map(|(x, y)| {
            image.pixel(x, y).is_some_and(|color| {
                *known
                    .entry(color)
                    .or_insert_with(|| !background.iter().any(|edge| edge.near(color, tolerance)))
            })
        })
        .collect();
    trim(rough, |x, y| {
        x.checked_sub(rough.x())
            .zip(y.checked_sub(rough.y()))
            .and_then(|(column, row)| index(column, row, rough.size()))
            .and_then(|index| kept.get(index))
            .is_some_and(|&kept| kept)
    })
}

/// The thing at (`x`, `y`) on `image`, grown like a magic wand.
///
/// It takes the pixels connected to the point, side by side, whose colors
/// are within `tolerance` of its color, and fills any holes they enclose. It
/// is `None` when the point is outside the image.
#[must_use]
pub fn grow(image: &Image, x: u32, y: u32, tolerance: u8) -> Option<Selection> {
    let size = image.size();
    let seed = image.pixel(x, y)?;
    let mut filled = vec![false; pixels(size)?];
    let start = index(x, y, size)?;
    *filled.get_mut(start)? = true;
    let mut pending = vec![(x, y)];
    while let Some((x, y)) = pending.pop() {
        for (x, y) in neighbors(x, y, size) {
            let Some(at) = index(x, y, size) else {
                continue;
            };
            let near = image
                .pixel(x, y)
                .is_some_and(|color| seed.near(color, tolerance));
            if near && let Some(slot) = filled.get_mut(at).filter(|slot| !**slot) {
                *slot = true;
                pending.push((x, y));
            }
        }
    }
    let whole = Region::whole(size)?;
    let grown = trim(whole, |x, y| {
        index(x, y, size)
            .and_then(|at| filled.get(at))
            .is_some_and(|&kept| kept)
    })?;
    Some(fill_holes(&grown))
}

/// `selection` with every unselected pixel that cannot reach the edge of
/// its box through other unselected pixels selected too.
fn fill_holes(selection: &Selection) -> Selection {
    let region = selection.region;
    let size = region.size();
    let mut outside = vec![false; pixels(size).unwrap_or_default()];
    let mut pending = Vec::new();
    for (x, y) in edge(region) {
        let (column, row) = (x.saturating_sub(region.x()), y.saturating_sub(region.y()));
        if !selection.contains(x, y)
            && let Some(slot) = index(column, row, size).and_then(|at| outside.get_mut(at))
            && !*slot
        {
            *slot = true;
            pending.push((column, row));
        }
    }
    while let Some((column, row)) = pending.pop() {
        for (column, row) in neighbors(column, row, size) {
            let x = region.x().saturating_add(column);
            let y = region.y().saturating_add(row);
            if !selection.contains(x, y)
                && let Some(slot) = index(column, row, size).and_then(|at| outside.get_mut(at))
                && !*slot
            {
                *slot = true;
                pending.push((column, row));
            }
        }
    }
    let kept = outside.iter().map(|&outside| !outside).collect();
    Mask::new(size, kept).map_or_else(
        |_error| selection.clone(),
        |mask| Selection { region, mask },
    )
}

/// The smallest selection holding the pixels of `region` that `selected`
/// picks, or `None` when it picks none.
fn trim(region: Region, selected: impl Fn(u32, u32) -> bool) -> Option<Selection> {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for (x, y) in points(region) {
        if selected(x, y) {
            bounds = Some(bounds.map_or((x, y, x, y), |(left, top, right, bottom)| {
                (left.min(x), top.min(y), right.max(x), bottom.max(y))
            }));
        }
    }
    let (left, top, right, bottom) = bounds?;
    let width = right.checked_sub(left)?.checked_add(1)?;
    let height = bottom.checked_sub(top)?.checked_add(1)?;
    let region = Region::new(left, top, width, height)?;
    let kept = points(region).map(|(x, y)| selected(x, y)).collect();
    let mask = Mask::new(region.size(), kept).ok()?;
    Some(Selection { region, mask })
}

/// The points of `region`, row by row from the top.
fn points(region: Region) -> impl Iterator<Item = (u32, u32)> {
    (region.y()..region.bottom())
        .flat_map(move |y| (region.x()..region.right()).map(move |x| (x, y)))
}

/// The points along the edge of `region`; corners may repeat.
fn edge(region: Region) -> impl Iterator<Item = (u32, u32)> {
    let last_column = region.right().saturating_sub(1);
    let last_row = region.bottom().saturating_sub(1);
    let across = (region.x()..region.right()).flat_map(move |x| [(x, region.y()), (x, last_row)]);
    let down = (region.y()..region.bottom()).flat_map(move |y| [(region.x(), y), (last_column, y)]);
    across.chain(down)
}

/// The points beside (`x`, `y`) to the left, right, top, and bottom that lie
/// inside a picture of `size`.
fn neighbors(x: u32, y: u32, size: Size) -> impl Iterator<Item = (u32, u32)> {
    [
        x.checked_sub(1).map(|x| (x, y)),
        x.checked_add(1).filter(|&x| x < size.width).map(|x| (x, y)),
        y.checked_sub(1).map(|y| (x, y)),
        y.checked_add(1)
            .filter(|&y| y < size.height)
            .map(|y| (x, y)),
    ]
    .into_iter()
    .flatten()
}

/// The index of pixel (`x`, `y`) in a picture of `size`, row by row.
fn index(x: u32, y: u32, size: Size) -> Option<usize> {
    usize::try_from(y)
        .ok()?
        .checked_mul(usize::try_from(size.width).ok()?)?
        .checked_add(usize::try_from(x).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKY: [u8; 3] = [80, 120, 200];
    const GOLD: [u8; 3] = [240, 190, 70];
    const STAR: [u8; 3] = [255, 250, 230];

    /// A picture drawn from rows of characters: `.` is sky, `#` gold, `*` a
    /// star's white, and `~` a slightly lighter sky.
    fn picture(rows: &[&str]) -> Image {
        let width = u32::try_from(rows[0].len()).unwrap();
        let height = u32::try_from(rows.len()).unwrap();
        let bgra = rows
            .iter()
            .flat_map(|row| row.chars())
            .flat_map(|character| {
                let [red, green, blue] = match character {
                    '#' => GOLD,
                    '*' => STAR,
                    '~' => [90, 130, 210],
                    _ => SKY,
                };
                [blue, green, red, 255]
            })
            .collect();
        Image::new(Size { width, height }, bgra).unwrap()
    }

    /// The selection as rows of `#` for selected and `.` for not, over its
    /// own box.
    fn drawn(selection: &Selection) -> Vec<String> {
        let region = selection.region();
        (region.y()..region.bottom())
            .map(|y| {
                (region.x()..region.right())
                    .map(|x| if selection.contains(x, y) { '#' } else { '.' })
                    .collect()
            })
            .collect()
    }

    const SHIELD: &[&str] = &[
        "..........",
        "..~.......",
        "...####...",
        "...#**#...",
        "...####.~.",
        "....##....",
        "..........",
    ];

    #[test]
    fn snap_shrinks_a_rough_box_to_the_thing_inside() {
        let image = picture(SHIELD);
        let rough = Region::new(1, 1, 8, 6).unwrap();
        let selection = snap(&image, rough, 24).unwrap();
        assert_eq!(selection.region(), Region::new(3, 2, 4, 4).unwrap());
        assert_eq!(drawn(&selection), ["####", "####", "####", ".##."]);
    }

    #[test]
    fn snap_leaves_out_background_inside_the_thing() {
        let image = picture(&[".......", ".#####.", ".#...#.", ".#####.", "......."]);
        let selection = snap(&image, Region::whole(image.size()).unwrap(), 8).unwrap();
        assert_eq!(drawn(&selection), ["#####", "#...#", "#####"]);
    }

    #[test]
    fn snap_needs_something_besides_background_inside_the_box() {
        let image = picture(SHIELD);
        assert_eq!(snap(&image, Region::new(0, 0, 3, 2).unwrap(), 24), None);
        // A tight tolerance keeps the lighter sky as part of the thing, and
        // the gold on the box's edge counts as background.
        let strict = snap(&image, Region::new(1, 0, 3, 3).unwrap(), 4).unwrap();
        assert_eq!(strict.region(), Region::new(2, 1, 1, 1).unwrap());
        assert_eq!(snap(&image, Region::new(8, 0, 3, 3).unwrap(), 24), None);
    }

    #[test]
    fn snap_takes_the_first_256_different_edge_colors_as_background() {
        // The top row has 300 different colors and the bottom row repeats
        // the first. The edge is read across both rows together, so the
        // repeats must not use up the 256.
        let shade = |i: u32| {
            [
                u8::try_from(i % 256).unwrap(),
                u8::try_from(i / 256).unwrap(),
                0,
            ]
        };
        let (width, height) = (300, 3);
        let mut bgra = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let [red, green, blue] = match (x, y) {
                    (x, 0) => shade(x),
                    (150, 1) => shade(256),
                    (151, 1) => shade(200),
                    _ => shade(0),
                };
                bgra.extend_from_slice(&[blue, green, red, 255]);
            }
        }
        let image = Image::new(Size { width, height }, bgra).unwrap();
        let selection = snap(&image, Region::whole(image.size()).unwrap(), 0).unwrap();
        // Colors past the first 256 are the thing, on the edge or inside;
        // the 201st is background.
        assert_eq!(selection.region(), Region::new(150, 0, 150, 2).unwrap());
        assert!(selection.contains(150, 1));
        assert!(!selection.contains(151, 1));
        assert!(selection.contains(256, 0));
        assert!(!selection.contains(255, 0));
    }

    #[test]
    fn neighbors_stay_inside_the_picture() {
        let size = Size {
            width: 3,
            height: 2,
        };
        assert_eq!(
            neighbors(1, 0, size).collect::<Vec<_>>(),
            [(0, 0), (2, 0), (1, 1)]
        );
        // The bottom right corner has nothing to its right or below.
        assert_eq!(neighbors(2, 1, size).collect::<Vec<_>>(), [(1, 1), (2, 0)]);
    }

    #[test]
    fn grow_spreads_over_similar_colors_and_fills_holes() {
        let image = picture(SHIELD);
        let selection = grow(&image, 3, 2, 24).unwrap();
        assert_eq!(selection.region(), Region::new(3, 2, 4, 4).unwrap());
        // The white star is enclosed by gold, so it is filled in.
        assert_eq!(drawn(&selection), ["####", "####", "####", ".##."]);
        assert_eq!(selection.mask().count(), 14);
        assert_eq!(grow(&image, 10, 0, 24), None);
    }

    #[test]
    fn grow_stops_at_colors_past_the_tolerance() {
        let image = picture(SHIELD);
        let star = grow(&image, 4, 3, 24).unwrap();
        assert_eq!(star.region(), Region::new(4, 3, 2, 1).unwrap());
        // A wide tolerance takes the sky, which reaches every edge, and
        // fills in the shield it encloses.
        let sky = grow(&image, 0, 0, 30).unwrap();
        assert_eq!(sky.region(), Region::whole(image.size()).unwrap());
        assert!(sky.contains(3, 2));
    }

    #[test]
    fn grow_leaves_a_gap_open_to_the_edge_unfilled() {
        let image = picture(&[".....", ".###.", ".#.#.", ".#.#.", "....."]);
        let selection = grow(&image, 1, 1, 8).unwrap();
        assert_eq!(drawn(&selection), ["###", "#.#", "#.#"]);
    }

    #[test]
    fn selections_combine() {
        let image = picture(SHIELD);
        let shield = grow(&image, 3, 2, 24).unwrap();
        let speck = grow(&image, 8, 4, 4).unwrap();
        assert_eq!(speck.region(), Region::new(8, 4, 1, 1).unwrap());
        let both = shield.union(&speck);
        assert_eq!(both.region(), Region::new(3, 2, 6, 4).unwrap());
        assert!(both.contains(8, 4) && both.contains(3, 2) && !both.contains(7, 4));
        assert_eq!(both.without(&speck), Some(shield.clone()));
        let star = grow(&image, 4, 3, 24).unwrap();
        let ring = shield.without(&star).unwrap();
        assert_eq!(ring.region(), shield.region());
        assert!(!ring.contains(4, 3) && ring.contains(3, 3));
        assert_eq!(star.without(&shield), None);
    }

    #[test]
    fn a_selection_masks_any_box_it_overlaps() {
        let image = picture(SHIELD);
        let shield = grow(&image, 3, 2, 24).unwrap();
        let wider = Region::new(2, 2, 3, 1).unwrap();
        assert_eq!(shield.mask_in(wider).unwrap().kept(), [false, true, true]);
        assert_eq!(shield.mask_in(Region::new(0, 0, 2, 2).unwrap()), None);
        assert!(!shield.contains(2, 5) && !shield.contains(3, 6) && !shield.contains(7, 2));
    }
}
