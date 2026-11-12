//! The different shapes a recording has seen, how often it saw each, and
//! which frames are worth keeping for them.
//!
//! A reference learns one glyph for each different shape, so a shape is
//! what a recording collects. Two shapes are the same only when every cell
//! of their grids is, which is how learning tells glyphs apart. Two grids
//! one cell apart may be one character with a blended edge or two
//! characters, and nothing here can tell which, so both stay and review
//! shows each. The same character is another shape when its line is
//! another height, as beside a `,` that reaches below the digits, and the
//! check needs that shape too.
//!
//! How often a shape showed is counted without keeping a frame: each shape
//! holds two counts, and each different reading costs one fingerprint.
//!
//! A reading is kept for a shape no kept reading shows. That alone would
//! leave a shape with one kept reading whatever stood beside it, and a
//! reading can be learned only when its text names every shape in it. A
//! frame caught while the digit beside a `7` was half drawn holds a shape
//! no character names, so the `7` could not be taught from it. So a kept
//! reading carries a shape cleanly when every other shape in it has shown
//! in another reading too, which a half-drawn digit seldom does, and a
//! shape with no clean carrier gets one more kept reading: the first frame
//! that would be one.

use std::collections::BTreeSet;
use std::hash::{DefaultHasher, Hash, Hasher};

use appytappy_sources::screen::digits::{MAX_GLYPHS, Shape};
use serde::Serialize;

/// The most different shapes one recording holds: as many glyphs as a
/// reference learns, so a recording that showed more could not teach them
/// all. It also bounds the kept readings at two for each shape: a reading
/// is kept either for a shape no kept reading shows or to carry cleanly a
/// shape that none carries so, and a shape stays cleanly carried once it
/// is.
pub(super) const MAX_SHAPES: usize = MAX_GLYPHS;

/// The most different readings one recording counts. A fingerprint costs
/// about 16 bytes in the set, so the bound is about 16 MB. A timer that
/// shows hundredths of a second, and so another reading every frame,
/// reaches it in about ten hours.
const MAX_READINGS: usize = 1 << 20;

/// Why a recording keeps and counts no more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Full {
    /// A frame showed more shapes than fit beside the ones it holds, of
    /// [`MAX_SHAPES`] in all.
    Shapes,
    /// A frame showed another reading than the [`MAX_READINGS`] it
    /// counted.
    Readings,
}

/// One shape and how often it showed.
#[derive(Debug)]
struct Sighted {
    shape: Shape,
    frames: u32,
    readings: u32,
}

/// The different shapes a recording has seen, in the order they first
/// showed.
#[derive(Debug)]
pub(super) struct Sightings {
    shapes: Vec<Sighted>,
    /// A fingerprint of each different reading counted. Two readings with
    /// one fingerprint would count as one, which 64 bits make unlikely
    /// enough to ignore for a count the user reads as evidence.
    readings: BTreeSet<u64>,
    /// Why it takes no more, once a frame did not fit.
    full: Option<Full>,
}

impl Sightings {
    pub(super) const fn new() -> Self {
        Self {
            shapes: Vec::new(),
            readings: BTreeSet::new(),
            full: None,
        }
    }

    /// Why it takes no more, once it is full.
    pub(super) const fn full(&self) -> Option<Full> {
        self.full
    }

    /// Counts one frame that showed `reading`, its characters left to
    /// right. Answers each character's shape as an index into the shapes
    /// seen, so two readings show the same shapes exactly when their
    /// answers are equal.
    ///
    /// Answers `None`, and counts nothing, for a frame that does not fit:
    /// one with more new shapes than [`MAX_SHAPES`] leaves room for, or
    /// with another reading than the [`MAX_READINGS`] counted. That frame
    /// makes it full, and it counts no frame after it.
    pub(super) fn count(&mut self, reading: &[Shape]) -> Option<Vec<usize>> {
        if self.full.is_some() {
            return None;
        }
        // Each character's index once the new shapes have joined, in the
        // order they show.
        let mut new: Vec<&Shape> = Vec::new();
        let characters: Vec<usize> = reading
            .iter()
            .map(|shape| {
                self.index_of(shape).unwrap_or_else(|| {
                    let place = new
                        .iter()
                        .position(|known| *known == shape)
                        .unwrap_or_else(|| {
                            new.push(shape);
                            new.len().saturating_sub(1)
                        });
                    self.shapes.len().saturating_add(place)
                })
            })
            .collect();
        if self.shapes.len().saturating_add(new.len()) > MAX_SHAPES {
            self.full = Some(Full::Shapes);
            return None;
        }
        let fingerprint = fingerprint(&characters);
        if !self.readings.contains(&fingerprint) && self.readings.len() >= MAX_READINGS {
            self.full = Some(Full::Readings);
            return None;
        }
        self.shapes.extend(new.into_iter().map(|shape| Sighted {
            shape: *shape,
            frames: 0,
            readings: 0,
        }));
        let first = self.readings.insert(fingerprint);
        // A reading that shows a shape twice, as `11` does, is one frame
        // and one reading of it.
        let shown: BTreeSet<usize> = characters.iter().copied().collect();
        for index in shown {
            if let Some(sighted) = self.shapes.get_mut(index) {
                sighted.frames = sighted.frames.saturating_add(1);
                sighted.readings = sighted.readings.saturating_add(u32::from(first));
            }
        }
        Some(characters)
    }

    /// Whether a frame that showed `reading`, counted already, is worth
    /// keeping beside the `kept` readings: it shows a shape none of them
    /// shows, or it carries cleanly a shape none of them carries so.
    pub(super) fn worth_keeping<'kept>(
        &self,
        reading: &[usize],
        kept: impl Iterator<Item = &'kept [usize]> + Clone,
    ) -> bool {
        reading.iter().any(|&shape| {
            let mut carriers = kept.clone().filter(|carrier| carrier.contains(&shape));
            let carried = carriers.clone().next().is_some();
            !carried
                || (self.carries_cleanly(reading, shape)
                    && !carriers.any(|carrier| self.carries_cleanly(carrier, shape)))
        })
    }

    /// Whether `reading` carries `shape` cleanly: every other shape in it
    /// has shown in another reading too, so a text is likely to name them
    /// all. It stays so once it is, since the counts only grow.
    fn carries_cleanly(&self, reading: &[usize], shape: usize) -> bool {
        reading.iter().all(|&other| {
            other == shape
                || self
                    .shapes
                    .get(other)
                    .is_some_and(|sighted| sighted.readings >= 2)
        })
    }

    /// Each shape as the UI shows it, in the order they first showed.
    pub(super) fn view(&self) -> Vec<ShapeView> {
        self.shapes
            .iter()
            .map(|sighted| ShapeView {
                frames: sighted.frames,
                readings: sighted.readings,
            })
            .collect()
    }

    fn index_of(&self, shape: &Shape) -> Option<usize> {
        self.shapes
            .iter()
            .position(|sighted| sighted.shape == *shape)
    }
}

/// A number that two readings share only when they show the same shapes in
/// the same order, but for a chance too small to matter.
fn fingerprint(characters: &[usize]) -> u64 {
    let mut hasher = DefaultHasher::new();
    characters.hash(&mut hasher);
    hasher.finish()
}

/// How often one shape showed, as the UI shows it. These are counts of
/// what the recording saw, not a measure of how well the shape reads: a
/// shape that shows often is still whatever the user's label says it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "RecordedShape"))]
pub(crate) struct ShapeView {
    /// How many frames showed it.
    pub(crate) frames: u32,
    /// How many different readings showed it.
    pub(crate) readings: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shape no other `cell` gives.
    fn shape(cell: u32) -> Shape {
        let mut shape = Shape::default();
        shape[0] = cell;
        shape
    }

    fn counts(sightings: &Sightings) -> Vec<(u32, u32)> {
        sightings
            .view()
            .iter()
            .map(|shape| (shape.frames, shape.readings))
            .collect()
    }

    /// Counts a frame that fits.
    fn count(sightings: &mut Sightings, reading: &[Shape]) -> Vec<usize> {
        sightings.count(reading).expect("the frame fits")
    }

    #[test]
    fn a_shape_counts_its_frames_and_its_different_readings() {
        let mut sightings = Sightings::new();
        let (one, two, three) = (shape(1), shape(2), shape(3));
        assert_eq!(count(&mut sightings, &[one, two]), [0, 1]);
        // The same reading again is another frame of each shape, not
        // another reading.
        assert_eq!(count(&mut sightings, &[one, two]), [0, 1]);
        assert_eq!(counts(&sightings), [(2, 1), (2, 1)]);
        // The same shapes in another order are another reading.
        assert_eq!(count(&mut sightings, &[two, one]), [1, 0]);
        assert_eq!(counts(&sightings), [(3, 2), (3, 2)]);
        // A reading that showed before is known among all of them, not
        // only as the latest.
        assert_eq!(count(&mut sightings, &[one, two]), [0, 1]);
        assert_eq!(count(&mut sightings, &[three, two, three]), [2, 1, 2]);
        assert_eq!(counts(&sightings), [(4, 2), (5, 3), (1, 1)]);
    }

    #[test]
    fn a_shape_a_reading_shows_twice_counts_once_a_frame() {
        let mut sightings = Sightings::new();
        assert_eq!(
            count(&mut sightings, &[shape(1), shape(2), shape(1)]),
            [0, 1, 0]
        );
        assert_eq!(counts(&sightings), [(1, 1), (1, 1)]);
    }

    #[test]
    fn shapes_one_cell_apart_stay_apart() {
        let mut sightings = Sightings::new();
        let mut blended = shape(0b0110);
        blended[15] = 1;
        assert_eq!(count(&mut sightings, &[shape(0b0110)]), [0]);
        assert_eq!(count(&mut sightings, &[blended]), [1]);
    }

    #[test]
    fn a_frame_is_worth_keeping_for_a_shape_no_kept_reading_shows() {
        let mut sightings = Sightings::new();
        let (seven, eight) = (shape(7), shape(8));
        let first = count(&mut sightings, &[seven]);
        assert!(sightings.worth_keeping(&first, std::iter::empty()));
        let kept = [first.as_slice()];
        assert!(!sightings.worth_keeping(&first, kept.into_iter()));
        let both = count(&mut sightings, &[seven, eight]);
        assert!(
            sightings.worth_keeping(&both, kept.into_iter()),
            "one shape no kept reading shows is enough"
        );
    }

    #[test]
    fn a_shape_kept_beside_one_seen_nowhere_else_gets_a_clean_carrier() {
        let mut sightings = Sightings::new();
        let (one, seven, half_drawn) = (shape(1), shape(7), shape(99));
        // The first frame with a 7 caught the digit beside it half drawn.
        let caught = count(&mut sightings, &[seven, half_drawn]);
        let alone = count(&mut sightings, &[one]);
        let kept = [caught.as_slice(), alone.as_slice()];
        // The same frame again is the reading already kept.
        assert_eq!(count(&mut sightings, &[seven, half_drawn]), caught);
        assert!(!sightings.worth_keeping(&caught, kept.into_iter()));
        // A 17 holds only shapes it has, and its 1 showed elsewhere, so it
        // carries the 7 as the caught frame does not.
        let clean = count(&mut sightings, &[one, seven]);
        assert!(sightings.worth_keeping(&clean, kept.into_iter()));
        let kept = [caught.as_slice(), alone.as_slice(), clean.as_slice()];
        // With a clean carrier, the 7 needs no more.
        let again = count(&mut sightings, &[seven, one]);
        assert!(!sightings.worth_keeping(&again, kept.into_iter()));
    }

    #[test]
    fn a_kept_reading_carries_cleanly_once_its_other_shapes_show_elsewhere() {
        let mut sightings = Sightings::new();
        let (eight, seven, nine) = (shape(8), shape(7), shape(9));
        let first = count(&mut sightings, &[eight, seven]);
        let kept = [first.as_slice()];
        // The 7 shows in another reading, so the first one carries the 8
        // cleanly, and a later reading with the 8 is not kept for it.
        let second = count(&mut sightings, &[seven, nine]);
        assert!(sightings.worth_keeping(&second, kept.into_iter()));
        let kept = [first.as_slice(), second.as_slice()];
        let third = count(&mut sightings, &[eight, nine]);
        assert!(!sightings.worth_keeping(&third, kept.into_iter()));
    }

    #[test]
    fn a_frame_with_more_new_shapes_than_fit_makes_it_full_and_is_not_counted() {
        let mut sightings = Sightings::new();
        let most = u32::try_from(MAX_SHAPES).expect("fits");
        for cell in 2..most {
            count(&mut sightings, &[shape(cell)]);
        }
        // Two new shapes where one fits.
        let before = counts(&sightings);
        assert_eq!(before.len(), MAX_SHAPES.checked_sub(2).expect("not zero"));
        count(&mut sightings, &[shape(0)]);
        assert_eq!(sightings.full(), None);
        assert_eq!(sightings.count(&[shape(most), shape(1)]), None);
        assert_eq!(sightings.full(), Some(Full::Shapes));
        assert_eq!(sightings.view().len(), MAX_SHAPES.saturating_sub(1));
        // Full, it counts no frame, not even one of shapes it has.
        assert_eq!(sightings.count(&[shape(2)]), None);
        assert_eq!(counts(&sightings).first(), Some(&(1, 1)));
    }

    #[test]
    fn it_holds_as_many_shapes_as_a_reference_learns() {
        let mut sightings = Sightings::new();
        let most = u32::try_from(MAX_SHAPES).expect("fits");
        for cell in 2..most {
            count(&mut sightings, &[shape(cell)]);
        }
        // The same new shape twice in a frame is one shape, so two fit.
        count(&mut sightings, &[shape(0), shape(1), shape(0)]);
        assert_eq!(sightings.view().len(), MAX_SHAPES);
        assert_eq!(sightings.full(), None);
        count(&mut sightings, &[shape(5), shape(1)]);
        assert_eq!(sightings.count(&[shape(most)]), None);
        assert_eq!(sightings.full(), Some(Full::Shapes));
        assert_eq!(sightings.view().len(), MAX_SHAPES);
    }

    #[test]
    fn another_reading_than_the_most_it_counts_makes_it_full() {
        let mut sightings = Sightings::new();
        // The fingerprints of every reading but one.
        sightings
            .readings
            .extend((1_u64..).take(MAX_READINGS.checked_sub(1).expect("not zero")));
        count(&mut sightings, &[shape(1)]);
        assert_eq!(sightings.full(), None);
        // A reading it counted still counts at the bound.
        count(&mut sightings, &[shape(1)]);
        assert_eq!(sightings.full(), None);
        assert_eq!(sightings.count(&[shape(2)]), None);
        assert_eq!(sightings.full(), Some(Full::Readings));
        assert_eq!(counts(&sightings), [(2, 1)], "the frame added no shape");
    }
}
