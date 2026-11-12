//! The different shapes a recording has seen, and how often it saw each.
//!
//! A reference learns one glyph for each different shape, so a shape is
//! what a recording collects: a reading is worth keeping only when it shows
//! a shape no kept reading has. Two shapes are the same only when every
//! cell of their grids is, which is how learning tells glyphs apart. Two
//! grids one cell apart may be one character with a blended edge or two
//! characters, and nothing here can tell which, so both stay and review
//! shows each. The same character is another shape when its line is
//! another height, as beside a `,` that reaches below the digits, and the
//! check needs that shape too.
//!
//! How often a shape showed is counted without keeping a frame: each shape
//! holds two counts, and each different reading costs one fingerprint.

use std::collections::BTreeSet;
use std::hash::{DefaultHasher, Hash, Hasher};

use appytappy_sources::screen::digits::{MAX_GLYPHS, Shape};
use serde::Serialize;

/// The most different shapes one recording looks for: as many glyphs as a
/// reference learns, so a recording that found more could not teach them
/// all. Each kept reading shows at least one shape no reading before it
/// has, so this also bounds the kept readings, which cost the most: a kept
/// reading holds its crop at four bytes a pixel, so a region 300 pixels
/// wide and 60 tall costs 72 KB a reading and 18 MB at the bound.
pub(super) const MAX_SHAPES: usize = MAX_GLYPHS;

/// The most different readings one recording counts. A fingerprint costs
/// about 16 bytes in the set, so the bound is about 16 MB. A timer that
/// shows hundredths of a second, and so another reading every frame,
/// reaches it in nine hours.
pub(super) const MAX_READINGS: usize = 1 << 20;

/// Why a recording keeps and counts no more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Full {
    /// It found [`MAX_SHAPES`] different shapes.
    Shapes,
    /// It counted [`MAX_READINGS`] different readings.
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
}

impl Sightings {
    pub(super) const fn new() -> Self {
        Self {
            shapes: Vec::new(),
            readings: BTreeSet::new(),
        }
    }

    /// Why it takes no more, once it is full.
    pub(super) fn full(&self) -> Option<Full> {
        if self.shapes.len() >= MAX_SHAPES {
            Some(Full::Shapes)
        } else if self.readings.len() >= MAX_READINGS {
            Some(Full::Readings)
        } else {
            None
        }
    }

    /// Whether `reading`, a frame's characters left to right, shows a shape
    /// no counted reading has.
    pub(super) fn shows_new(&self, reading: &[Shape]) -> bool {
        reading.iter().any(|shape| self.index_of(shape).is_none())
    }

    /// Counts one frame that showed `reading`. Answers each of its
    /// characters' shapes, left to right, as an index into the shapes seen,
    /// so two readings show the same shapes exactly when their answers are
    /// equal.
    pub(super) fn count(&mut self, reading: &[Shape]) -> Vec<usize> {
        let characters: Vec<usize> = reading.iter().map(|shape| self.index(shape)).collect();
        let first = self.readings.insert(fingerprint(&characters));
        // A reading that shows a shape twice, as `11` does, is one frame
        // and one reading of it.
        let shown: BTreeSet<usize> = characters.iter().copied().collect();
        for index in shown {
            if let Some(sighted) = self.shapes.get_mut(index) {
                sighted.frames = sighted.frames.saturating_add(1);
                sighted.readings = sighted.readings.saturating_add(u32::from(first));
            }
        }
        characters
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

    /// The index of `shape`, which joins the shapes seen if it is new.
    fn index(&mut self, shape: &Shape) -> usize {
        self.index_of(shape).unwrap_or_else(|| {
            self.shapes.push(Sighted {
                shape: *shape,
                frames: 0,
                readings: 0,
            });
            self.shapes.len().saturating_sub(1)
        })
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

    #[test]
    fn a_shape_counts_its_frames_and_its_different_readings() {
        let mut sightings = Sightings::new();
        let (one, two, three) = (shape(1), shape(2), shape(3));
        assert!(sightings.shows_new(&[one, two]));
        assert_eq!(sightings.count(&[one, two]), [0, 1]);
        assert!(!sightings.shows_new(&[two, one]));
        assert!(sightings.shows_new(&[two, three]), "one new shape is enough");
        // The same reading again is another frame of each shape, not
        // another reading.
        assert_eq!(sightings.count(&[one, two]), [0, 1]);
        assert_eq!(counts(&sightings), [(2, 1), (2, 1)]);
        // The same shapes in another order are another reading.
        assert_eq!(sightings.count(&[two, one]), [1, 0]);
        assert_eq!(counts(&sightings), [(3, 2), (3, 2)]);
        // A reading that showed before is known among all of them, not
        // only as the latest.
        assert_eq!(sightings.count(&[one, two]), [0, 1]);
        assert_eq!(sightings.count(&[three]), [2]);
        assert_eq!(counts(&sightings), [(4, 2), (4, 2), (1, 1)]);
    }

    #[test]
    fn a_shape_a_reading_shows_twice_counts_once_a_frame() {
        let mut sightings = Sightings::new();
        assert_eq!(sightings.count(&[shape(1), shape(2), shape(1)]), [0, 1, 0]);
        assert_eq!(counts(&sightings), [(1, 1), (1, 1)]);
    }

    #[test]
    fn shapes_one_cell_apart_stay_apart() {
        let mut sightings = Sightings::new();
        let mut blended = shape(0b0110);
        blended[15] = 1;
        assert_eq!(sightings.count(&[shape(0b0110)]), [0]);
        assert!(sightings.shows_new(&[blended]));
        assert_eq!(sightings.count(&[blended]), [1]);
    }

    #[test]
    fn it_is_full_at_as_many_shapes_as_a_reference_learns() {
        let mut sightings = Sightings::new();
        for cell in 1..u32::try_from(MAX_SHAPES).expect("fits") {
            sightings.count(&[shape(cell)]);
        }
        assert_eq!(sightings.full(), None);
        sightings.count(&[shape(0)]);
        assert_eq!(sightings.full(), Some(Full::Shapes));
    }

    #[test]
    fn it_is_full_at_the_most_readings_it_counts() {
        let mut sightings = Sightings::new();
        // The fingerprints of every reading but one.
        sightings
            .readings
            .extend((1_u64..).take(MAX_READINGS.checked_sub(1).expect("not zero")));
        assert_eq!(sightings.full(), None);
        sightings.count(&[shape(1)]);
        assert_eq!(sightings.full(), Some(Full::Readings));
    }
}
