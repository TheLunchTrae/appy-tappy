//! Recording a region while the user uses the application. A capture
//! records one thing at a time, a [`Recorder`]: the characters a number's
//! region shows, each shape kept once for the user to review and label
//! ([`Recording`]), or the looks a value's region shows, each kept as a
//! screenshot ([`looks`]). Both share when they record and why they stop
//! ([`Lifecycle`]).
//!
//! The preview capture's frames arrive about 30 times a second. For
//! readings, each one is cropped to the region and its characters sampled
//! the way the number check samples them, each scaled onto a small grid of
//! cells set where at least half the pixels are ink. A crop that shows a
//! shape no kept reading has is kept, since a reference learns one glyph
//! for each different shape, and so is one more for a shape whose kept
//! readings all hold a shape seen nowhere else, which no text may be able
//! to name. Any other crop would teach the check nothing new, so it only
//! counts toward how often each of its shapes showed. Only kept readings
//! are read as text, so the text reader runs a few times a recording rather
//! than on every frame. A crop with no ink, such as while a menu covers the
//! region, is skipped.
//!
//! A digit, a `%`, and a `.` are all shapes the same way: whatever the
//! check's own sampling makes one character is one shape here.

pub(crate) mod find;
mod ink;
mod lifecycle;
pub(crate) mod looks;
pub(crate) mod ocr;
mod sightings;

use std::time::{Duration, Instant};

use appytappy_sources::screen::digits::{DigitReader, ReadError};
use appytappy_sources::screen::{Image, Region, Size};
use appytappy_sources::system::Rgb;
use serde::Serialize;

use self::ink::Ink;
pub(crate) use self::ink::Line;
pub(crate) use self::lifecycle::Lifecycle;
use self::looks::LookRecording;
use self::sightings::{Full, MAX_SHAPES, ShapeView, Sightings};

/// What the text reader made of a reading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "RecordedText"))]
#[serde(tag = "kind", content = "text", rename_all = "camelCase")]
pub(crate) enum Text {
    /// Not read yet.
    Pending,
    /// The text it read, which may be empty when it found none.
    Read(String),
    /// It could not read it, and why.
    Failed(String),
}

/// A reading the recording kept.
#[derive(Debug, Clone)]
pub(crate) struct Kept {
    /// The region's pixels when it first showed this reading.
    pub(crate) image: Image,
    /// Each of its characters' shapes, left to right, as an index into the
    /// recording's shapes.
    shapes: Vec<usize>,
    /// When it first showed, from the start of the recording.
    pub(crate) first: Duration,
    /// How many frames showed exactly this reading.
    pub(crate) seen: u32,
    /// What the text reader made of it.
    pub(crate) text: Text,
}

/// A new reading for the text reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewReading {
    /// Its index in the recording.
    pub(crate) index: usize,
    /// Its ink, as the text reader takes it.
    pub(crate) line: Line,
}

/// What a capture records, one thing at a time, so a recording of readings
/// and one of looks never run together.
#[derive(Debug)]
pub(crate) enum Recorder {
    /// The readings a number's region shows, for a reference to learn.
    Readings(Recording),
    /// The looks a value's region shows, each kept as a screenshot.
    Looks(LookRecording),
}

impl Recorder {
    /// What the user does to record again once the window shows, after a
    /// capture failure stopped the recording.
    pub(crate) const fn after_failure(&self) -> &'static str {
        match self {
            Self::Readings(_) => "Record more once the window shows again.",
            Self::Looks(_) => "Start recording again once the window shows.",
        }
    }

    /// When it records and why it stops, which both kinds share.
    pub(crate) const fn life_mut(&mut self) -> &mut Lifecycle {
        match self {
            Self::Readings(recording) => &mut recording.life,
            Self::Looks(recording) => &mut recording.life,
        }
    }
}

/// The pixels of `region` on `frame`, a whole window's frame, or why the
/// frame cannot be recorded: the window is not `window_size`, the size it
/// had when the region was drawn, so the frame shows other pixels there.
fn cut(frame: &Image, region: Region, window_size: Size) -> Result<Image, String> {
    let size = frame.size();
    frame
        .crop(region)
        .filter(|_| size == window_size)
        .ok_or_else(|| {
            format!(
                "The region was drawn when the window was {window_size}, and it is {size} now. \
                 Resize the window to record."
            )
        })
}

/// A recording of the readings one region shows.
#[derive(Debug)]
pub(crate) struct Recording {
    /// Its number in the session, so the UI and the text reader tell it
    /// from one it replaced.
    pub(crate) id: u64,
    /// The region of the window it records.
    pub(crate) region: Region,
    /// The characters' color.
    ink: Rgb,
    /// How far a pixel may be from `ink` and still be ink.
    tolerance: u8,
    reader: DigitReader,
    /// The window's size when the region was drawn; a frame of another size
    /// shows other pixels there.
    window_size: Size,
    /// When it records, and why the latest frame could not be recorded or
    /// why it stopped by itself.
    pub(crate) life: Lifecycle,
    /// The readings it kept, each for a shape none before it showed or
    /// carried cleanly.
    readings: Vec<Kept>,
    /// Every different shape a frame showed, and how often.
    sightings: Sightings,
}

impl Recording {
    /// Starts recording `region`, drawn on a window of `window_size`, where
    /// a pixel within `tolerance` of the `ink` color is ink.
    pub(crate) const fn new(
        id: u64,
        region: Region,
        (ink, tolerance): (Rgb, u8),
        window_size: Size,
        started: Instant,
    ) -> Self {
        Self {
            id,
            region,
            ink,
            tolerance,
            reader: DigitReader::new(ink, tolerance),
            window_size,
            life: Lifecycle::new(started),
            readings: Vec::new(),
            sightings: Sightings::new(),
        }
    }

    /// Records `frame`, a whole window's frame that arrived at `at`. Answers
    /// the reading to read as text, if the frame is worth keeping: it shows
    /// a shape no kept reading has, or carries cleanly one that none
    /// carries so. Of several such frames for a shape, the first is kept. A
    /// full recording keeps and counts no more.
    pub(crate) fn offer(&mut self, frame: &Image, at: Instant) -> Option<NewReading> {
        if !self.life.is_recording() {
            return None;
        }
        let crop = match cut(frame, self.region, self.window_size) {
            Ok(crop) => crop,
            Err(problem) => {
                self.life.set_problem(Some(problem));
                return None;
            },
        };
        let shapes = match self.reader.shapes(&crop) {
            Ok(shapes) => shapes,
            // A crop with none of the characters' color, such as under a
            // menu, is a moment to skip, not a problem.
            Err(ReadError::NoInk) => {
                self.life.set_problem(None);
                return None;
            },
            Err(error) => {
                self.life.set_problem(Some(format!(
                    "Nothing is recorded while {error}. Draw the region around the value alone, \
                     or lower the tolerance."
                )));
                return None;
            },
        };
        self.life.set_problem(None);
        let shapes = self.sightings.count(shapes.characters())?;
        let kept = self.readings.iter().map(|kept| kept.shapes.as_slice());
        if !self.sightings.worth_keeping(&shapes, kept) {
            if let Some(kept) = self.readings.iter_mut().find(|kept| kept.shapes == shapes) {
                kept.seen = kept.seen.saturating_add(1);
            }
            return None;
        }
        let line = Ink::of(&crop, &self.reader).line()?;
        let index = self.readings.len();
        self.readings.push(Kept {
            image: crop,
            shapes,
            first: self.life.elapsed(at),
            seen: 1,
            text: Text::Pending,
        });
        Some(NewReading { index, line })
    }

    /// Keeps what the text reader made of reading `index`.
    pub(crate) fn read(&mut self, index: usize, text: Text) {
        if let Some(kept) = self.readings.get_mut(index) {
            kept.text = text;
        }
    }

    /// Whether it records `region` of a window of `window_size`, with ink
    /// within `tolerance` of `ink`, so going on with it records the same
    /// thing.
    pub(crate) fn records(
        &self,
        region: Region,
        (ink, tolerance): (Rgb, u8),
        window_size: Size,
    ) -> bool {
        self.region == region
            && self.ink == ink
            && self.tolerance == tolerance
            && self.window_size == window_size
    }

    /// The window's size when the region was drawn.
    pub(crate) const fn window_size(&self) -> Size {
        self.window_size
    }

    /// The readings it kept, in the order they first showed.
    pub(crate) fn readings(&self) -> &[Kept] {
        &self.readings
    }

    /// What the UI shows of it at `now`.
    pub(crate) fn view(&self, now: Instant) -> RecordingView {
        RecordingView {
            id: self.id,
            recording: self.life.is_recording(),
            elapsed_ms: millis(self.life.elapsed(now)),
            full: self.sightings.full().map(|full| match full {
                Full::Shapes => format!(
                    "This recording is full: the region showed more than {MAX_SHAPES} different \
                     shapes, more than a reference learns. Stop and review what it found. If the \
                     region shows more than the value, draw it around the value alone and \
                     record again."
                ),
                Full::Readings => "This recording is full: the region showed more than a million \
                                   different readings, the most one recording counts. Stop and \
                                   review what it found."
                    .to_owned(),
            }),
            problem: self.life.problem().map(str::to_owned),
            settings: Settings {
                region: RegionView {
                    x: self.region.x(),
                    y: self.region.y(),
                    width: self.region.size().width,
                    height: self.region.size().height,
                },
                ink: ColorView {
                    red: self.ink.red,
                    green: self.ink.green,
                    blue: self.ink.blue,
                },
                tolerance: self.tolerance,
                window_size: SizeView {
                    width: self.window_size.width,
                    height: self.window_size.height,
                },
            },
            readings: self
                .readings
                .iter()
                .map(|kept| ReadingView {
                    first_ms: millis(kept.first),
                    seen: kept.seen,
                    text: kept.text.clone(),
                    shapes: kept.shapes.clone(),
                })
                .collect(),
            shapes: self.sightings.view(),
        }
    }
}

/// Whole milliseconds in `duration`, for the UI.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// A recording as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordingView {
    /// Its number in the session.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) id: u64,
    /// Whether it still records.
    pub(crate) recording: bool,
    /// How long it has recorded, or recorded in all once stopped.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) elapsed_ms: u64,
    /// Why it keeps and counts no more, once it is full.
    pub(crate) full: Option<String>,
    /// Why the latest frame could not be recorded.
    pub(crate) problem: Option<String>,
    /// What it records, which going on with it needs again.
    pub(crate) settings: Settings,
    /// The readings it kept, in the order they first showed.
    pub(crate) readings: Vec<ReadingView>,
    /// Every different shape its frames showed, in the order they first
    /// showed. Each shows in at least one kept reading.
    pub(crate) shapes: Vec<ShapeView>,
}

/// What a recording records: the region of a window of one size, and what
/// counts as ink there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "RecordingSettings"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct Settings {
    /// The region of the window.
    pub(crate) region: RegionView,
    /// The characters' color.
    pub(crate) ink: ColorView,
    /// How far a pixel may be from the characters' color and still be ink.
    pub(crate) tolerance: u8,
    /// The window's size when the region was drawn.
    pub(crate) window_size: SizeView,
}

/// A window's size as the UI describes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct SizeView {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// A region as the UI describes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct RegionView {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// A color as the UI describes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct ColorView {
    pub(crate) red: u8,
    pub(crate) green: u8,
    pub(crate) blue: u8,
}

/// One reading as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "RecordedReading"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadingView {
    /// When it first showed, from the start of the recording.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) first_ms: u64,
    /// How many frames showed exactly this reading.
    pub(crate) seen: u32,
    /// What the text reader made of it.
    pub(crate) text: Text,
    /// Each of its characters' shapes, left to right, as an index into the
    /// recording's shapes.
    pub(crate) shapes: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use appytappy_sources::screen::digits::Shape;
    use proptest::prelude::*;

    use super::*;

    const INK: [u8; 4] = [0, 0, 255, 0];
    const PAPER: [u8; 4] = [40, 40, 40, 0];

    /// A frame of `width` by `height` whose pixels are ink where `ink`
    /// says so.
    fn frame(width: u32, height: u32, ink: impl Fn(u32, u32) -> bool) -> Image {
        let pixels = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .flat_map(|(x, y)| if ink(x, y) { INK } else { PAPER })
            .collect();
        Image::new(Size { width, height }, pixels).expect("matching length")
    }

    fn region() -> Region {
        Region::new(0, 0, 8, 4).expect("not empty")
    }

    fn size() -> Size {
        Size {
            width: 8,
            height: 4,
        }
    }

    const RED: Rgb = Rgb {
        red: 255,
        green: 0,
        blue: 0,
    };

    fn recording(started: Instant) -> Recording {
        Recording::new(1, region(), (RED, 0), size(), started)
    }

    /// A recording of a `width` by `height` window, all of it the region.
    fn recording_of(width: u32, height: u32, started: Instant) -> Recording {
        let region = Region::new(0, 0, width, height).expect("not empty");
        Recording::new(1, region, (RED, 0), Size { width, height }, started)
    }

    /// A `0` eight pixels wide and 32 tall, its strokes two pixels thick.
    fn zero(x: u32, y: u32) -> bool {
        x < 8 && (!(2..6).contains(&x) || !(2..30).contains(&y))
    }

    /// An `8`: the `0` with a bar across it.
    fn eight(x: u32, y: u32) -> bool {
        zero(x, y) || (x < 8 && (15..17).contains(&y))
    }

    /// A `1`: one stroke two pixels wide and 32 tall.
    fn one(x: u32, _: u32) -> bool {
        x < 2
    }

    /// A frame 48 pixels wide and 32 tall that shows `characters` left to
    /// right, each in a slot 12 pixels wide.
    fn showing(characters: &[fn(u32, u32) -> bool]) -> Image {
        frame(48, 32, |x, y| {
            usize::try_from(x / 12)
                .ok()
                .and_then(|slot| characters.get(slot))
                .is_some_and(|character| character(x % 12, y))
        })
    }

    /// Each shape's frames and different readings.
    fn sightings(recording: &Recording, now: Instant) -> Vec<(u32, u32)> {
        let shapes = recording.view(now).shapes;
        shapes
            .iter()
            .map(|shape| (shape.frames, shape.readings))
            .collect()
    }

    #[test]
    fn a_reading_that_would_teach_nothing_new_is_seen_again() {
        let start = Instant::now();
        let mut recording = recording_of(16, 32, start);
        let first = recording
            .offer(&frame(16, 32, zero), start)
            .expect("a new reading");
        assert_eq!(first.index, 0);
        // A speck inside the 0 fills a quarter of one sampled cell, which
        // stays empty: the same reading.
        let speck = frame(16, 32, |x, y| zero(x, y) || (x, y) == (4, 16));
        assert_eq!(recording.offer(&speck, start), None);
        // A bar across it is a new character.
        assert_eq!(
            recording
                .offer(&frame(16, 32, eight), start)
                .map(|new| new.index),
            Some(1)
        );
        // A reading that showed before is found among all of them, not only
        // the latest.
        assert_eq!(recording.offer(&frame(16, 32, zero), start), None);
        let seen: Vec<u32> = recording.readings().iter().map(|kept| kept.seen).collect();
        assert_eq!(seen, [3, 1]);
        assert_eq!(sightings(&recording, start), [(3, 1), (1, 1)]);
    }

    #[test]
    fn a_reading_is_kept_only_for_a_shape_no_kept_reading_has() {
        let start = Instant::now();
        let mut recording = recording_of(48, 32, start);
        let kept = |recording: &mut Recording, characters: &[fn(u32, u32) -> bool]| {
            recording
                .offer(&showing(characters), start)
                .map(|new| new.index)
        };
        assert_eq!(kept(&mut recording, &[zero]), Some(0));
        assert_eq!(kept(&mut recording, &[eight, zero]), Some(1));
        // Other readings of the shapes it has are counted, not kept.
        assert_eq!(kept(&mut recording, &[zero, eight]), None);
        assert_eq!(kept(&mut recording, &[zero, eight]), None);
        assert_eq!(kept(&mut recording, &[zero, zero, eight]), None);
        // One new shape among known ones keeps the reading.
        assert_eq!(kept(&mut recording, &[eight, one, zero]), Some(2));
        let view = recording.view(start);
        let shapes: Vec<&[usize]> = view
            .readings
            .iter()
            .map(|reading| reading.shapes.as_slice())
            .collect();
        assert_eq!(shapes, [&[0][..], &[1, 0], &[1, 2, 0]]);
        // The 0 showed in six frames and five different readings.
        assert_eq!(sightings(&recording, start), [(6, 5), (5, 4), (1, 1)]);
        let seen: Vec<u32> = view.readings.iter().map(|reading| reading.seen).collect();
        assert_eq!(seen, [1, 1, 1], "each kept reading showed in one frame");
        assert_eq!(kept(&mut recording, &[eight, zero]), None);
        assert_eq!(recording.readings()[1].seen, 2);
    }

    /// The `0` while its right stroke is still being drawn.
    fn half_drawn(x: u32, y: u32) -> bool {
        zero(x, y) && x < 6
    }

    #[test]
    fn a_shape_first_kept_beside_a_half_drawn_one_is_kept_again_on_recording_more() {
        let start = Instant::now();
        let mut recording = recording_of(48, 32, start);
        let kept = |recording: &mut Recording, characters: &[fn(u32, u32) -> bool]| {
            recording
                .offer(&showing(characters), start)
                .map(|new| new.index)
        };
        // The first frame with an 8 caught the digit beside it half drawn,
        // which no text can name, so the 8 cannot be learned from it.
        assert_eq!(kept(&mut recording, &[eight, half_drawn]), Some(0));
        assert_eq!(kept(&mut recording, &[one]), Some(1));
        assert_eq!(kept(&mut recording, &[eight, half_drawn]), None);
        recording.life.stop(start);
        recording.life.resume(start);
        // Recording more, a clean 18 holds only shapes it has seen, and is
        // kept for the 8.
        assert_eq!(kept(&mut recording, &[one, eight]), Some(2));
        assert_eq!(recording.view(start).readings[2].shapes, [2, 0]);
        // The 8 needs no third reading.
        assert_eq!(kept(&mut recording, &[eight, one]), None);
        assert_eq!(kept(&mut recording, &[eight]), None);
    }

    #[test]
    fn a_sign_drawn_in_marks_apart_is_a_shape_for_each_mark() {
        let start = Instant::now();
        let mut recording = recording_of(16, 16, start);
        // A percent sign whose two dots and stroke share no column, so the
        // check's sampling finds three characters.
        let percent = |x: u32, y: u32| {
            (x < 4 && y < 4) || ((6..10).contains(&x) && x + y / 4 == 9) || (x >= 12 && y >= 12)
        };
        assert!(recording.offer(&frame(16, 16, percent), start).is_some());
        assert_eq!(recording.view(start).readings[0].shapes, [0, 1, 2]);
        // It is the same three shapes each time it shows.
        assert_eq!(recording.offer(&frame(16, 16, percent), start), None);
        assert_eq!(sightings(&recording, start), [(2, 1), (2, 1), (2, 1)]);
    }

    /// The rows of the marks a generated frame draws, each three pixels
    /// wide: bit `n` of a row is its pixel in column `n`. They differ in
    /// height, so a mark's shape depends on the marks beside it, as a
    /// digit's does beside a `,`.
    const MARKS: [[u8; 6]; 5] = [
        [7, 5, 5, 7, 0, 0],
        [2, 2, 2, 2, 0, 0],
        [7, 4, 7, 1, 7, 7],
        [0, 0, 0, 3, 0, 0],
        [0, 0, 0, 2, 2, 1],
    ];

    /// A frame 20 pixels wide and 6 tall that shows `marks` left to right,
    /// each in a slot four pixels wide.
    fn marked(marks: &[usize]) -> Image {
        frame(20, 6, |x, y| {
            let row = usize::try_from(x / 4)
                .ok()
                .and_then(|slot| marks.get(slot))
                .and_then(|&mark| MARKS.get(mark))
                .zip(usize::try_from(y).ok())
                .and_then(|(rows, y)| rows.get(y));
            row.is_some_and(|row| row >> (x % 4) & 1 == 1)
        })
    }

    proptest! {
        #[test]
        fn every_shape_shown_is_kept_counted_and_carried_cleanly_where_a_frame_could(
            frames in prop::collection::vec(prop::collection::vec(0..MARKS.len(), 0..5), 0..24),
        ) {
            let start = Instant::now();
            let mut recording = recording_of(20, 6, start);
            let reader = DigitReader::new(RED, 0);
            // How many different readings among `shown` show `shape`.
            let readings_of = |shown: &[Vec<Shape>], shape: &Shape| {
                let with = shown.iter().filter(|reading| reading.contains(shape));
                with.collect::<BTreeSet<_>>().len()
            };
            // Whether `reading` carries `shape` cleanly: its other shapes
            // each show in another of the readings `shown` too.
            let carries = |shown: &[Vec<Shape>], reading: &[Shape], shape: &Shape| {
                reading.contains(shape)
                    && reading
                        .iter()
                        .all(|other| other == shape || readings_of(shown, other) >= 2)
            };
            // Every reading shown, as the check samples it, every different
            // shape in the order it first showed, and the shapes that some
            // frame carried cleanly when it showed.
            let mut shown: Vec<Vec<Shape>> = Vec::new();
            let mut distinct: Vec<Shape> = Vec::new();
            let mut carried: Vec<Shape> = Vec::new();
            for marks in &frames {
                let frame = marked(marks);
                let Ok(shapes) = reader.shapes(&frame) else {
                    prop_assert_eq!(recording.offer(&frame, start), None, "no ink to keep");
                    continue;
                };
                let shapes = shapes.characters();
                let new = shapes.iter().any(|shape| !distinct.contains(shape));
                for shape in shapes {
                    if !distinct.contains(shape) {
                        distinct.push(*shape);
                    }
                }
                let kept = recording.offer(&frame, start).is_some();
                prop_assert!(kept || !new, "a frame with a new shape is kept");
                shown.push(shapes.to_vec());
                for shape in shapes {
                    if carries(&shown, shapes, shape) && !carried.contains(shape) {
                        carried.push(*shape);
                    }
                }
            }
            // Each kept reading's picture is what review learns from.
            let mut kept: Vec<Vec<Shape>> = Vec::new();
            for reading in recording.readings() {
                let shapes = reader.shapes(&reading.image).expect("kept for its ink");
                let indexes: Vec<Option<usize>> = shapes
                    .characters()
                    .iter()
                    .map(|shape| distinct.iter().position(|known| known == shape))
                    .collect();
                let named: Vec<Option<usize>> = reading.shapes.iter().copied().map(Some).collect();
                prop_assert_eq!(named, indexes);
                kept.push(shapes.characters().to_vec());
            }
            // Together the kept readings show every shape, no reading is
            // kept twice, and there are at most two for each shape.
            for shape in &distinct {
                prop_assert!(kept.iter().any(|reading| reading.contains(shape)));
            }
            prop_assert_eq!(kept.iter().collect::<BTreeSet<_>>().len(), kept.len());
            prop_assert!(kept.len() <= distinct.len().saturating_mul(2));
            // A shape that some frame carried cleanly has a kept reading
            // that does, so a shape seen nowhere else beside its first kept
            // reading does not keep it from being taught.
            for shape in &carried {
                prop_assert!(
                    kept.iter().any(|reading| carries(&shown, reading, shape)),
                    "no kept reading carries {shape:?} cleanly among {kept:?}"
                );
            }
            // Each shape's counts are the frames and the different readings
            // that showed it.
            let counted: Vec<(usize, usize)> = distinct
                .iter()
                .map(|shape| {
                    let frames = shown.iter().filter(|reading| reading.contains(shape));
                    (frames.count(), readings_of(&shown, shape))
                })
                .collect();
            let sighted: Vec<(usize, usize)> = sightings(&recording, start)
                .into_iter()
                .map(|(frames, readings)| {
                    (
                        usize::try_from(frames).expect("fits"),
                        usize::try_from(readings).expect("fits"),
                    )
                })
                .collect();
            prop_assert_eq!(sighted, counted);
        }
    }

    #[test]
    fn one_stroke_differs_however_long_the_number() {
        let start = Instant::now();
        let mut recording = recording_of(64, 8, start);
        // Eight solid six-pixel-wide digits: 384 pixels of ink.
        let digits = |x: u32| x % 8 < 6;
        let first = frame(64, 8, |x, _| digits(x));
        assert!(recording.offer(&first, start).is_some());
        // The last digit loses a two by three stroke, six pixels of 384.
        let stroke = frame(64, 8, |x, y| {
            digits(x) && !((58..60).contains(&x) && (2..5).contains(&y))
        });
        assert!(
            recording.offer(&stroke, start).is_some(),
            "a new character is a new reading"
        );
    }

    #[test]
    fn tall_ink_gets_a_margin_of_half_its_height() {
        let start = Instant::now();
        let tall = Size {
            width: 1,
            height: 10,
        };
        let region = Region::new(0, 0, 1, 10).expect("not empty");
        let mut recording = Recording::new(1, region, (RED, 0), tall, start);
        let line = recording
            .offer(&frame(1, 10, |_, _| true), start)
            .expect("a new reading")
            .line;
        // Ten pixels of ink take a margin of five all round.
        assert_eq!((line.width, line.height), (11, 20));
    }

    #[test]
    fn the_text_reader_gets_the_ink_black_on_white_with_a_margin() {
        let start = Instant::now();
        let mut recording = recording(start);
        // Ink at columns 3 and 4 of rows 1 and 2.
        let shown = frame(8, 4, |x, y| (3..5).contains(&x) && (1..3).contains(&y));
        let line = recording.offer(&shown, start).expect("a new reading").line;
        // Two pixels of ink each way, and a margin of two all round.
        assert_eq!((line.width, line.height), (6, 6));
        let black: Vec<usize> = line
            .pixels
            .iter()
            .enumerate()
            .filter(|&(_, &pixel)| pixel == 0)
            .map(|(index, _)| index)
            .collect();
        assert_eq!(black, [14, 15, 20, 21]);
        assert!(
            line.pixels
                .iter()
                .all(|&pixel| pixel == 0 || pixel == u8::MAX)
        );
    }

    #[test]
    fn a_frame_without_ink_or_of_another_size_keeps_nothing() {
        let start = Instant::now();
        let mut recording = recording(start);
        assert_eq!(recording.offer(&frame(8, 4, |_, _| false), start), None);
        assert_eq!(recording.view(start).problem, None);
        assert_eq!(recording.offer(&frame(9, 4, |_, _| true), start), None);
        assert_eq!(
            recording.view(start).problem.as_deref(),
            Some(
                "The region was drawn when the window was 8x4, and it is 9x4 now. Resize the \
                 window to record."
            )
        );
        assert_eq!(recording.offer(&frame(4, 4, |_, _| true), start), None);
        assert!(recording.view(start).problem.is_some(), "too small to crop");
        assert!(recording.offer(&frame(8, 4, |_, _| true), start).is_some());
        assert_eq!(
            recording.view(start).problem,
            None,
            "a fitting frame clears it"
        );
        assert_eq!(recording.readings().len(), 1);
    }

    #[test]
    fn a_region_of_too_many_characters_says_so_until_it_has_none() {
        let start = Instant::now();
        let mut recording = recording_of(60, 4, start);
        // Thirty one-pixel strokes with a gap between each.
        let crowded = frame(60, 4, |x, _| x % 2 == 0);
        assert_eq!(recording.offer(&crowded, start), None);
        assert_eq!(
            recording.view(start).problem.as_deref(),
            Some(
                "Nothing is recorded while the region holds more than 24 characters. Draw the \
                 region around the value alone, or lower the tolerance."
            )
        );
        assert_eq!(recording.offer(&frame(60, 4, |_, _| false), start), None);
        assert_eq!(recording.view(start).problem, None);
        assert!(recording.readings().is_empty());
    }

    #[test]
    fn a_full_recording_keeps_and_counts_no_more_and_says_why() {
        let start = Instant::now();
        let mut recording = recording_of(18, 16, start);
        // One character 16 pixels tall, of each width from 2 to 17 with a
        // pixel cut from each row of its last column in turn, so no two
        // crops have the same shape.
        let notched = |width: u32, row: u32| {
            frame(18, 16, move |x, y| {
                x < width && (x, y) != (width.saturating_sub(1), row)
            })
        };
        for width in 2..18 {
            for row in 0..16 {
                assert!(recording.offer(&notched(width, row), start).is_some());
            }
        }
        assert_eq!(recording.readings().len(), MAX_SHAPES);
        assert_eq!(recording.view(start).full, None, "as many as fit");
        assert_eq!(recording.offer(&notched(2, 0), start), None);
        assert_eq!(recording.readings()[0].seen, 2);
        // One shape more does not fit.
        assert_eq!(recording.offer(&notched(18, 0), start), None);
        assert_eq!(
            recording.view(start).full.as_deref(),
            Some(
                "This recording is full: the region showed more than 256 different shapes, more \
                 than a reference learns. Stop and review what it found. If the region shows \
                 more than the value, draw it around the value alone and record again."
            )
        );
        assert_eq!(recording.offer(&notched(2, 0), start), None);
        // Stopped and recorded more, it is as full as it was.
        recording.life.stop(start);
        assert!(recording.view(start).full.is_some());
        recording.life.resume(start);
        assert!(recording.view(start).full.is_some());
        assert_eq!(recording.offer(&notched(2, 0), start), None);
        assert_eq!(recording.readings().len(), MAX_SHAPES);
        assert_eq!(recording.readings()[0].seen, 2);
        assert_eq!(sightings(&recording, start).len(), MAX_SHAPES);
        assert_eq!(sightings(&recording, start)[0], (2, 1));
    }

    #[test]
    fn the_time_counts_only_recording_and_text_lands_on_its_reading() {
        let start = Instant::now();
        let second = Duration::from_secs(1);
        let mut recording = recording(start);
        let shown = frame(8, 4, |x, _| x < 2);
        recording.offer(&shown, start + second).expect("new");
        assert_eq!(recording.view(start + 3 * second).elapsed_ms, 3000);
        recording.life.stop(start + 4 * second);
        let stopped = recording.view(start + 9 * second);
        assert!(!stopped.recording);
        assert_eq!(stopped.elapsed_ms, 4000);
        assert_eq!(recording.offer(&frame(8, 4, |x, _| x < 3), start), None);
        recording.life.resume(start + 10 * second);
        let going = recording.view(start + 12 * second);
        assert!(going.recording);
        assert_eq!(going.elapsed_ms, 6000);
        recording.read(0, Text::Read("12".to_owned()));
        recording.read(5, Text::Read("lost".to_owned()));
        assert_eq!(
            going.readings[0],
            ReadingView {
                first_ms: 1000,
                seen: 1,
                text: Text::Pending,
                shapes: vec![0],
            }
        );
        assert_eq!(
            recording.view(start).readings[0].text,
            Text::Read("12".to_owned())
        );
    }

    #[test]
    fn it_records_only_the_same_region_ink_and_window() {
        let recording = recording(Instant::now());
        assert!(recording.records(region(), (RED, 0), size()));
        let other_region = Region::new(1, 0, 7, 4).expect("not empty");
        assert!(!recording.records(other_region, (RED, 0), size()));
        assert!(!recording.records(region(), (RED, 9), size()));
        let green = Rgb { green: 1, ..RED };
        assert!(!recording.records(region(), (green, 0), size()));
        let other_size = Size {
            width: 9,
            height: 4,
        };
        assert!(!recording.records(region(), (RED, 0), other_size));
    }

    #[test]
    fn the_ui_sees_what_it_records() {
        let region = Region::new(2, 1, 5, 3).expect("not empty");
        let view = Recording::new(1, region, (RED, 7), size(), Instant::now()).view(Instant::now());
        let json = serde_json::to_string(&view.settings).expect("serializes");
        assert_eq!(
            json,
            concat!(
                r#"{"region":{"x":2,"y":1,"width":5,"height":3},"ink":{"red":255,"green":0,"blue":0},"#,
                r#""tolerance":7,"windowSize":{"width":8,"height":4}}"#
            )
        );
    }

    #[test]
    fn the_ui_sees_text_tagged_by_kind() {
        let json = |text| serde_json::to_string(&text).expect("serializes");
        assert_eq!(json(Text::Pending), r#"{"kind":"pending"}"#);
        assert_eq!(
            json(Text::Read("1".to_owned())),
            r#"{"kind":"read","text":"1"}"#
        );
        assert_eq!(
            json(Text::Failed("x".to_owned())),
            r#"{"kind":"failed","text":"x"}"#
        );
    }
}
