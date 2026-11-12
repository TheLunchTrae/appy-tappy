//! Recording a region while the user uses the application. A capture
//! records one thing at a time, a [`Recorder`]: the readings a number's
//! region shows, each kept once for the user to review and label
//! ([`Recording`]), or the looks a value's region shows, each kept as a
//! screenshot ([`looks`]). Both share when they record and why they stop
//! ([`Lifecycle`]).
//!
//! The preview capture's frames arrive about 30 times a second. For
//! readings, each one is cropped to the region and its characters sampled
//! the way the number check samples them, each scaled onto a small grid of
//! cells set where at least half the pixels are ink. A crop whose
//! characters have the same shapes as a reading already kept is another
//! sighting of that reading: it would teach the check nothing new. Any other
//! crop with ink is a new reading, and only new readings are read as text,
//! so the text reader runs a few times a recording rather than on every
//! frame. A crop with no ink, such as while a menu covers the region, is
//! skipped.

pub(crate) mod find;
mod ink;
mod lifecycle;
pub(crate) mod looks;
pub(crate) mod ocr;

use std::time::{Duration, Instant};

use appytappy_sources::screen::digits::{DigitReader, ReadError, Shapes};
use appytappy_sources::screen::{Image, Region, Size};
use appytappy_sources::system::Rgb;
use serde::Serialize;

use self::ink::{Ink, Line};
pub(crate) use self::lifecycle::Lifecycle;
use self::looks::LookRecording;

/// The most readings one recording keeps. A counter shows a few hundred
/// values at most; past this the region is likely not text in the
/// characters' color, and review would be too long to go through.
pub(crate) const MAX_READINGS: usize = 200;

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
    /// Its characters' shapes.
    shapes: Shapes,
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
    readings: Vec<Kept>,
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
        }
    }

    /// Records `frame`, a whole window's frame that arrived at `at`. Answers
    /// the new reading to read as text, if it showed one.
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
        if let Some(known) = self
            .readings
            .iter_mut()
            .find(|known| known.shapes == shapes)
        {
            known.seen = known.seen.saturating_add(1);
            return None;
        }
        if self.readings.len() >= MAX_READINGS {
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
            full: self.readings.len() >= MAX_READINGS,
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
    /// Whether it holds [`MAX_READINGS`], so it keeps no more.
    pub(crate) full: bool,
    /// Why the latest frame could not be recorded.
    pub(crate) problem: Option<String>,
    /// What it records, which going on with it needs again.
    pub(crate) settings: Settings,
    /// Its readings, in the order they first showed.
    pub(crate) readings: Vec<ReadingView>,
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
    /// How many frames showed it.
    pub(crate) seen: u32,
    /// What the text reader made of it.
    pub(crate) text: Text,
}

#[cfg(test)]
mod tests {
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
        let eight = frame(16, 32, |x, y| {
            zero(x, y) || (x < 8 && (15..17).contains(&y))
        });
        assert_eq!(recording.offer(&eight, start).map(|new| new.index), Some(1));
        // A reading that showed before is found among all of them, not only
        // the latest.
        assert_eq!(recording.offer(&frame(16, 32, zero), start), None);
        let seen: Vec<u32> = recording.readings().iter().map(|kept| kept.seen).collect();
        assert_eq!(seen, [3, 1]);
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
    fn it_keeps_at_most_max_readings() {
        let start = Instant::now();
        let mut recording = recording_of(33, 16, start);
        let mut kept = 0_usize;
        // Two characters 16 pixels tall, of every pair of widths from 1 to
        // 16, so no two crops have the same shapes.
        for first in 1..=16 {
            for second in 1..=16 {
                let shown = frame(33, 16, |x, _| {
                    x < first || (first + 1..first + 1 + second).contains(&x)
                });
                if recording.offer(&shown, start).is_some() {
                    kept += 1;
                }
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
