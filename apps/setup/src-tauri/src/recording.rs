//! Recording a region: each reading it shows while the user uses the
//! application, kept once, for the user to review and label.
//!
//! The preview capture's frames arrive about 30 times a second. Each one is
//! cropped to the region and reduced to its ink, the pixels near the
//! characters' color, which is all a number check sees. A crop whose ink has
//! the shape of a reading already kept is another sighting of that reading.
//! Any other crop with ink is a new reading, and only new readings are read
//! as text, so the text reader runs a few times a recording rather than on
//! every frame. A crop with no ink, such as while a menu covers the region,
//! is skipped.

use std::time::{Duration, Instant};

use appytappy_sources::screen::digits::DigitReader;
use appytappy_sources::screen::{Image, Region, Size};
use serde::Serialize;

/// The most readings one recording keeps. A counter shows a few hundred
/// values at most; past this the region is likely not text in the
/// characters' color, and review would be too long to go through.
pub(crate) const MAX_READINGS: usize = 200;

/// Bits in one word of an [`Ink`] mask.
const WORD_BITS: usize = 64;

/// Which pixels of a crop are ink, one bit per pixel, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Ink {
    words: Vec<u64>,
    count: u32,
    width: usize,
    height: usize,
}

impl Ink {
    /// The ink of `image`, as `reader` tells ink from background.
    fn of(image: &Image, reader: &DigitReader) -> Self {
        let pixels = image.pixels();
        let mut words = vec![0_u64; pixels.len().div_ceil(WORD_BITS)];
        let mut count = 0_u32;
        for (index, &pixel) in pixels.iter().enumerate() {
            if reader.is_ink(pixel)
                && let Some(word) = words.get_mut(index / WORD_BITS)
            {
                *word |= bit(index);
                count = count.saturating_add(1);
            }
        }
        let size = image.size();
        Self {
            words,
            count,
            width: usize::try_from(size.width).unwrap_or(0),
            height: usize::try_from(size.height).unwrap_or(0),
        }
    }

    /// Whether the pixel at (`x`, `y`) is ink.
    fn at(&self, x: usize, y: usize) -> bool {
        let index = y.saturating_mul(self.width).saturating_add(x);
        self.words
            .get(index / WORD_BITS)
            .is_some_and(|word| word & bit(index) != 0)
    }

    /// Whether `other`, a crop of the same size, shows the same characters.
    /// A pixel at a character's edge can fall either side of the tolerance
    /// from one frame to the next, so the two may differ in one pixel, and
    /// one more for each 32 pixels of ink. Two different characters differ
    /// in far more.
    fn same_shape(&self, other: &Self) -> bool {
        let differ = self
            .words
            .iter()
            .zip(&other.words)
            .fold(0_u32, |sum, (mine, theirs)| {
                sum.saturating_add((mine ^ theirs).count_ones())
            });
        differ <= (self.count.max(other.count) / 32).saturating_add(1)
    }

    /// The ink as the text reader takes it: one byte a pixel, black ink on
    /// white, cut to the ink's bounds with a margin of white around them,
    /// as text on a page has. Black on white reads better than the
    /// application's own colors, which can be any pair. `None` without ink.
    fn line(&self) -> Option<Line> {
        let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0, 0);
        for y in 0..self.height {
            for x in 0..self.width {
                if self.at(x, y) {
                    left = left.min(x);
                    top = top.min(y);
                    right = right.max(x.saturating_add(1));
                    bottom = bottom.max(y.saturating_add(1));
                }
            }
        }
        if right <= left {
            return None;
        }
        // Half the ink's height, at least two pixels.
        let margin = (bottom.saturating_sub(top) / 2).max(2);
        let width = right
            .saturating_sub(left)
            .saturating_add(margin.saturating_mul(2));
        let height = bottom
            .saturating_sub(top)
            .saturating_add(margin.saturating_mul(2));
        let mut pixels = vec![u8::MAX; width.saturating_mul(height)];
        for (row, y) in pixels.chunks_exact_mut(width).skip(margin).zip(top..bottom) {
            for (pixel, x) in row.iter_mut().skip(margin).zip(left..right) {
                if self.at(x, y) {
                    *pixel = 0;
                }
            }
        }
        Some(Line {
            width: u32::try_from(width).ok()?,
            height: u32::try_from(height).ok()?,
            pixels,
        })
    }
}

/// The bit for pixel `index` in its word.
fn bit(index: usize) -> u64 {
    u32::try_from(index % WORD_BITS)
        .ok()
        .and_then(|shift| 1_u64.checked_shl(shift))
        .unwrap_or(0)
}

/// One line of text for the text reader: gray pixels, one byte each, row by
/// row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Line {
    /// Its width in pixels.
    pub(crate) width: u32,
    /// Its height in pixels.
    pub(crate) height: u32,
    /// Its pixels: 0 is black, 255 white.
    pub(crate) pixels: Vec<u8>,
}

/// What the text reader made of a reading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    ink: Ink,
    /// When it first showed, from the start of the recording.
    pub(crate) first: Duration,
    /// How many frames showed it.
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

/// A recording of one region.
#[derive(Debug)]
pub(crate) struct Recording {
    /// Its number in the session, so the UI and the text reader tell it
    /// from one it replaced.
    pub(crate) id: u64,
    /// The region of the window it records.
    pub(crate) region: Region,
    reader: DigitReader,
    /// The window's size when the region was drawn; a frame of another size
    /// shows other pixels there.
    window_size: Size,
    /// When it started, moved later by each pause so the time it shows
    /// counts only recording.
    started: Instant,
    /// How long it recorded, once it stopped.
    stopped: Option<Duration>,
    readings: Vec<Kept>,
    /// Why the latest frame could not be recorded, until one can be.
    problem: Option<String>,
}

impl Recording {
    /// Starts recording `region`, drawn on a window of `window_size`, where
    /// ink is what `reader` takes for it.
    pub(crate) const fn new(
        id: u64,
        region: Region,
        reader: DigitReader,
        window_size: Size,
        started: Instant,
    ) -> Self {
        Self {
            id,
            region,
            reader,
            window_size,
            started,
            stopped: None,
            readings: Vec::new(),
            problem: None,
        }
    }

    /// Records `frame`, a whole window's frame that arrived at `at`. Answers
    /// the new reading to read as text, if it showed one.
    pub(crate) fn offer(&mut self, frame: &Image, at: Instant) -> Option<NewReading> {
        if self.stopped.is_some() {
            return None;
        }
        let size = frame.size();
        let crop = frame.crop(self.region).filter(|_| size == self.window_size);
        let Some(crop) = crop else {
            self.problem = Some(format!(
                "The region was drawn when the window was {}, and it is {size} now. Resize the \
                 window to record.",
                self.window_size
            ));
            return None;
        };
        self.problem = None;
        let ink = Ink::of(&crop, &self.reader);
        if ink.count == 0 {
            return None;
        }
        if let Some(known) = self
            .readings
            .iter_mut()
            .find(|known| known.ink.same_shape(&ink))
        {
            known.seen = known.seen.saturating_add(1);
            return None;
        }
        if self.readings.len() >= MAX_READINGS {
            return None;
        }
        let line = ink.line()?;
        let index = self.readings.len();
        self.readings.push(Kept {
            image: crop,
            ink,
            first: at.saturating_duration_since(self.started),
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

    /// Stops recording at `at`; later frames are ignored.
    pub(crate) fn stop(&mut self, at: Instant) {
        if self.stopped.is_none() {
            self.stopped = Some(at.saturating_duration_since(self.started));
        }
    }

    /// Whether it records `region` of a window of `window_size`, telling ink
    /// as `reader` does, so going on with it records the same thing.
    pub(crate) fn records(&self, region: Region, reader: &DigitReader, window_size: Size) -> bool {
        self.region == region && self.reader == *reader && self.window_size == window_size
    }

    /// Goes on recording at `at` after a stop, keeping its readings. The
    /// time it shows goes on from where it stopped.
    pub(crate) fn resume(&mut self, at: Instant) {
        if let Some(recorded) = self.stopped.take() {
            self.started = at.checked_sub(recorded).unwrap_or(at);
        }
    }

    /// How it tells ink from background, with nothing learned yet.
    pub(crate) const fn reader(&self) -> &DigitReader {
        &self.reader
    }

    /// The readings it kept, in the order they first showed.
    pub(crate) fn readings(&self) -> &[Kept] {
        &self.readings
    }

    /// What the UI shows of it at `now`.
    pub(crate) fn view(&self, now: Instant) -> RecordingView {
        RecordingView {
            id: self.id,
            recording: self.stopped.is_none(),
            elapsed_ms: millis(
                self.stopped
                    .unwrap_or_else(|| now.saturating_duration_since(self.started)),
            ),
            full: self.readings.len() >= MAX_READINGS,
            problem: self.problem.clone(),
            readings: self
                .readings
                .iter()
                .map(|kept| ReadingView {
                    first_ms: millis(kept.first),
                    seen: kept.seen,
                    text: kept.text.clone(),
                })
                .collect(),
        }
    }
}

/// Whole milliseconds in `duration`, for the UI.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// A recording as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordingView {
    /// Its number in the session.
    pub(crate) id: u64,
    /// Whether it still records.
    pub(crate) recording: bool,
    /// How long it has recorded, or recorded in all once stopped.
    pub(crate) elapsed_ms: u64,
    /// Whether it holds [`MAX_READINGS`], so it keeps no more.
    pub(crate) full: bool,
    /// Why the latest frame could not be recorded.
    pub(crate) problem: Option<String>,
    /// Its readings, in the order they first showed.
    pub(crate) readings: Vec<ReadingView>,
}

/// One reading as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadingView {
    /// When it first showed, from the start of the recording.
    pub(crate) first_ms: u64,
    /// How many frames showed it.
    pub(crate) seen: u32,
    /// What the text reader made of it.
    pub(crate) text: Text,
}

#[cfg(test)]
mod tests {
    use appytappy_sources::system::Rgb;

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

    fn recording(started: Instant) -> Recording {
        let reader = DigitReader::new(
            Rgb {
                red: 255,
                green: 0,
                blue: 0,
            },
            0,
        );
        Recording::new(1, region(), reader, size(), started)
    }

    #[test]
    fn a_pixel_at_an_edge_is_the_same_reading_and_a_new_column_is_not() {
        let start = Instant::now();
        let mut recording = recording(start);
        let bar = |columns: u32| frame(8, 4, move |x, _| x < columns);
        let first = recording.offer(&bar(2), start).expect("a new reading");
        assert_eq!(first.index, 0);
        // One pixel more is the same reading, seen again.
        let edge = frame(8, 4, |x, y| x < 2 || (x == 2 && y == 0));
        assert_eq!(recording.offer(&edge, start), None);
        // A whole column more is not.
        let second = recording.offer(&bar(3), start).expect("a new reading");
        assert_eq!(second.index, 1);
        // A reading that showed before is found among all of them, not only
        // the latest.
        assert_eq!(recording.offer(&bar(2), start), None);
        let seen: Vec<u32> = recording.readings().iter().map(|kept| kept.seen).collect();
        assert_eq!(seen, [3, 1]);
    }

    #[test]
    fn tall_ink_gets_a_margin_of_half_its_height() {
        let start = Instant::now();
        let reader = DigitReader::new(
            Rgb {
                red: 255,
                green: 0,
                blue: 0,
            },
            0,
        );
        let tall = Size {
            width: 1,
            height: 10,
        };
        let region = Region::new(0, 0, 1, 10).expect("not empty");
        let mut recording = Recording::new(1, region, reader, tall, start);
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
    fn it_keeps_at_most_max_readings() {
        let start = Instant::now();
        let mut recording = recording(start);
        let mut kept = 0_usize;
        // Patterns of the top two rows, a column of two pixels to each bit,
        // so any two differ in more than the pixel an edge may.
        for pattern in 1..=u8::MAX {
            let shown = frame(8, 4, |x, y| y < 2 && pattern & (1 << x) != 0);
            if recording.offer(&shown, start).is_some() {
                kept += 1;
            }
        }
        assert_eq!(kept, MAX_READINGS);
        assert!(recording.view(start).full);
    }

    #[test]
    fn the_time_counts_only_recording_and_text_lands_on_its_reading() {
        let start = Instant::now();
        let second = Duration::from_secs(1);
        let mut recording = recording(start);
        let shown = frame(8, 4, |x, _| x < 2);
        recording.offer(&shown, start + second).expect("new");
        assert_eq!(recording.view(start + 3 * second).elapsed_ms, 3000);
        recording.stop(start + 4 * second);
        recording.stop(start + 5 * second);
        let stopped = recording.view(start + 9 * second);
        assert!(!stopped.recording);
        assert_eq!(stopped.elapsed_ms, 4000);
        assert_eq!(recording.offer(&frame(8, 4, |x, _| x < 3), start), None);
        recording.resume(start + 10 * second);
        recording.resume(start + 11 * second);
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
        let reader = recording.reader().clone();
        assert!(recording.records(region(), &reader, size()));
        let other_region = Region::new(1, 0, 7, 4).expect("not empty");
        assert!(!recording.records(other_region, &reader, size()));
        let other_ink = DigitReader::new(
            Rgb {
                red: 255,
                green: 0,
                blue: 0,
            },
            9,
        );
        assert!(!recording.records(region(), &other_ink, size()));
        let other_size = Size {
            width: 9,
            height: 4,
        };
        assert!(!recording.records(region(), &reader, other_size));
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
