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
//! A recording of the whole window collects the same way from every place
//! the window shows text like the region's: in the characters' color and as
//! tall as the characters the region showed when it started ([`find`]).
//! Each such run of text is cropped and counted as the region's own crop
//! is. Looking over a whole frame costs far more than cropping one region,
//! so a thread of its own does it ([`look`]), one frame at a time: the
//! owner sends a frame at most once every [`SCAN_EVERY`], and none while a
//! look is out, and skips the frames between.
//!
//! A digit, a `%`, and a `.` are all shapes the same way: whatever the
//! check's own sampling makes one character is one shape here.

pub(crate) mod character;
pub(crate) mod find;
mod ink;
mod lifecycle;
pub(crate) mod look;
pub(crate) mod looks;
pub(crate) mod ocr;
mod sightings;

use std::time::{Duration, Instant};

use appytappy_sources::screen::digits::{DigitReader, ReadError, Shape};
use appytappy_sources::screen::{Image, Region, Size};
use appytappy_sources::system::Rgb;
use serde::{Deserialize, Serialize};

use self::character::CharacterView;
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
    /// Where each of its characters is in the picture, with its shape's
    /// name.
    characters: Vec<CharacterView>,
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

    /// Stops recording at `at`, keeping what it recorded.
    pub(crate) fn stop(&mut self, at: Instant) {
        match self {
            Self::Readings(recording) => recording.stop(at),
            Self::Looks(recording) => recording.life.stop(at),
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

/// Why a frame of a window of `size` cannot be recorded when the region
/// was drawn on one of `window_size`: it shows other pixels there.
fn resized(window_size: Size, size: Size) -> String {
    format!(
        "The region was drawn when the window was {window_size}, and it is {size} now. Resize \
         the window to record."
    )
}

/// The pixels of `region` on `frame`, a whole window's frame, or why the
/// frame cannot be recorded: the window is not `window_size`, the size it
/// had when the region was drawn, so the frame shows other pixels there.
fn cut(frame: &Image, region: Region, window_size: Size) -> Result<Image, String> {
    let size = frame.size();
    frame
        .crop(region)
        .filter(|_| size == window_size)
        .ok_or_else(|| resized(window_size, size))
}

/// The least time between two looks over the whole window. A look reads
/// every pixel of the frame, where a place's crop reads a few hundred, so
/// it is made twice a second rather than on each of the 30 frames: text
/// that stays for half a second is still seen.
pub(crate) const SCAN_EVERY: Duration = Duration::from_millis(500);

/// Where a recording looks for characters, as the UI names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "RecordingScope"))]
#[serde(rename_all = "camelCase")]
pub(crate) enum Scope {
    /// In its region alone.
    Place,
    /// Wherever the window shows text like its region's.
    Window,
}

/// Where a recording looks for characters, and what looking there needs.
#[derive(Debug)]
enum Looking {
    /// In its region alone, on every frame.
    Place,
    /// Wherever the window shows text like its region's.
    Window(Scan),
}

/// Looking over the whole window for text like a region's.
#[derive(Debug)]
struct Scan {
    /// How tall the characters were that the region showed when the
    /// recording started, in pixels: the size of text it looks for.
    sample: u32,
    /// Whether a look is out, and when the last went out.
    gate: Gate,
    /// Where the last look found text, while it records.
    spots: Vec<Region>,
}

/// Whether a frame may go out to be looked over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gate {
    /// No look is out. The last was of the frame that arrived then, so the
    /// frames until [`SCAN_EVERY`] after it are skipped; `None` before the
    /// first.
    Idle(Option<Instant>),
    /// A look is out, and every frame is skipped until it is answered.
    Out,
    /// A look is out whose answer is dropped: its frame is from before the
    /// user recorded more, or from before the window changed size. Every
    /// frame is skipped until it is answered.
    Abandoned,
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
    /// Where it looks for characters.
    looking: Looking,
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
            looking: Looking::Place,
            readings: Vec::new(),
            sightings: Sightings::new(),
        }
    }

    /// The same recording, looking over the whole window for text with
    /// characters `sample` pixels tall, as the region showed when it
    /// started, rather than in the region alone.
    pub(crate) fn across_window(mut self, sample: u32) -> Self {
        self.looking = Looking::Window(Scan {
            sample,
            gate: Gate::Idle(None),
            spots: Vec::new(),
        });
        self
    }

    /// Where it looks for characters.
    pub(crate) const fn scope(&self) -> Scope {
        match self.looking {
            Looking::Place => Scope::Place,
            Looking::Window(_) => Scope::Window,
        }
    }

    /// Records `frame`, a whole window's frame that arrived at `at`, in its
    /// region. Answers the reading to read as text when the frame shows one
    /// that is worth keeping, since it shows a shape no kept reading has,
    /// or carries cleanly one that none carries so. Of several such
    /// readings for a shape, the first is kept. A full recording keeps and
    /// counts no more. A recording of the whole window keeps nothing from
    /// a frame here: [`Self::look`] sends the frame to be looked over.
    pub(crate) fn offer(&mut self, frame: &Image, at: Instant) -> Option<NewReading> {
        if !self.life.is_recording() || matches!(self.looking, Looking::Window(_)) {
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
        self.keep(crop, shapes.characters(), at)
    }

    /// The look to send for `frame`, a whole window's frame that arrived at
    /// `at`, when it records the whole window and a look is due: none is
    /// out, and the last was of a frame at least [`SCAN_EVERY`] before
    /// this one. The look is then out until [`Self::looked`] answers it, so
    /// a look that takes long holds one frame and not a queue of them. A
    /// frame of another size than the window had is a problem and is not
    /// looked over, and the answer to a look that is out then is dropped,
    /// since the places it found text are not where the window shows it.
    pub(crate) fn look(&mut self, frame: &Image, at: Instant) -> Option<look::Job> {
        if !self.life.is_recording() {
            return None;
        }
        let Looking::Window(scan) = &mut self.looking else {
            return None;
        };
        let size = frame.size();
        if size != self.window_size {
            scan.spots.clear();
            if scan.gate == Gate::Out {
                scan.gate = Gate::Abandoned;
            }
            self.life.set_problem(Some(resized(self.window_size, size)));
            return None;
        }
        self.life.set_problem(None);
        match scan.gate {
            Gate::Out | Gate::Abandoned => return None,
            Gate::Idle(Some(last)) if at.saturating_duration_since(last) < SCAN_EVERY => {
                return None;
            },
            Gate::Idle(_) => {},
        }
        scan.gate = Gate::Out;
        Some(look::Job {
            recording: self.id,
            frame: frame.clone(),
            at,
            sample: scan.sample,
            near: self.region,
            reader: self.reader.clone(),
        })
    }

    /// Takes the answer to the look of the frame that arrived at `at`:
    /// counts each run of text it `seen`, and answers each reading to read
    /// as text, as [`Self::offer`] does for the region. The next look may go
    /// out [`SCAN_EVERY`] after `at`. An answer that comes once it stopped
    /// is dropped, and so is the answer to a look from before the user
    /// recorded more or the window changed size, after which the next look
    /// may go out at once. `None` is a look that failed, which stops the
    /// recording at `now` with a reason.
    pub(crate) fn looked(
        &mut self,
        at: Instant,
        seen: Option<Vec<look::Seen>>,
        now: Instant,
    ) -> Vec<NewReading> {
        let Looking::Window(scan) = &mut self.looking else {
            return Vec::new();
        };
        if scan.gate == Gate::Abandoned {
            scan.gate = Gate::Idle(None);
            return Vec::new();
        }
        scan.gate = Gate::Idle(Some(at));
        if !self.life.is_recording() {
            return Vec::new();
        }
        let Some(seen) = seen else {
            self.stop_looking(now);
            return Vec::new();
        };
        scan.spots = seen.iter().map(|seen| seen.spot).collect();
        seen.into_iter()
            .filter_map(|seen| {
                let (crop, shapes) = seen.reading?;
                self.keep(crop, &shapes, at)
            })
            .collect()
    }

    /// Stops recording at `at` because the window can no longer be looked
    /// over, and lets a look go out again once the user records more.
    pub(crate) fn stop_looking(&mut self, at: Instant) {
        if let Looking::Window(scan) = &mut self.looking {
            scan.gate = Gate::Idle(None);
            scan.spots.clear();
        }
        self.life.fail(
            at,
            "Recording stopped: the window could not be looked over. Record more to try again."
                .to_owned(),
        );
    }

    /// Stops recording at `at`; later frames are ignored, and the places
    /// the last look found text are forgotten.
    pub(crate) fn stop(&mut self, at: Instant) {
        self.life.stop(at);
        self.forget_spots();
    }

    /// Goes on recording at `at` after a stop. A look that is still out
    /// is of a frame from before the stop, so its answer is dropped.
    pub(crate) fn resume(&mut self, at: Instant) {
        self.life.resume(at);
        if let Looking::Window(scan) = &mut self.looking {
            // A stop by a failure left the spots, and they are from before
            // it.
            scan.spots.clear();
            if scan.gate == Gate::Out {
                scan.gate = Gate::Abandoned;
            }
        }
    }

    fn forget_spots(&mut self) {
        if let Looking::Window(scan) = &mut self.looking {
            scan.spots.clear();
        }
    }

    /// Counts `crop`, which showed `shapes` at `at`, and keeps it when it
    /// is worth keeping; answers it then, for the text reader.
    fn keep(&mut self, crop: Image, shapes: &[Shape], at: Instant) -> Option<NewReading> {
        let shapes = self.sightings.count(shapes)?;
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
            characters: character::of(&crop, &self.reader),
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
    /// within `tolerance` of `ink`, looking where `scope` says, so going on
    /// with it records the same thing.
    pub(crate) fn records(
        &self,
        region: Region,
        (ink, tolerance): (Rgb, u8),
        window_size: Size,
        scope: Scope,
    ) -> bool {
        self.region == region
            && self.scope() == scope
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
            full: self
                .sightings
                .full()
                .map(|full| full_message(full, self.scope())),
            problem: self.life.problem().map(str::to_owned),
            settings: Settings {
                region: RegionView::of(self.region),
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
                scope: self.scope(),
            },
            spots: match &self.looking {
                Looking::Window(scan) => scan
                    .spots
                    .iter()
                    .map(|&spot| RegionView::of(spot))
                    .collect(),
                Looking::Place => Vec::new(),
            },
            readings: self
                .readings
                .iter()
                .map(|kept| ReadingView {
                    first_ms: millis(kept.first),
                    seen: kept.seen,
                    text: kept.text.clone(),
                    shapes: kept.shapes.clone(),
                    width: kept.image.size().width,
                    characters: kept.characters.clone(),
                })
                .collect(),
            shapes: self.sightings.view(),
        }
    }
}

/// Why a recording that looks where `scope` says is `full`, and what to
/// do about it.
fn full_message(full: Full, scope: Scope) -> String {
    match (full, scope) {
        (Full::Shapes, Scope::Place) => format!(
            "This recording is full: the region showed more than {MAX_SHAPES} different shapes, \
             more than a reference learns. Stop and review what it found. If the region shows \
             more than the value, draw it around the value alone and record again."
        ),
        (Full::Shapes, Scope::Window) => format!(
            "This recording is full: the window showed more than {MAX_SHAPES} different shapes \
             in the characters' color and size, more than a reference learns. Stop and review \
             what it found. If the window shows much other text like the value's, record this \
             place alone instead."
        ),
        (Full::Readings, Scope::Place) => "This recording is full: the region showed more than a \
                                           million different readings, the most one recording \
                                           counts. Stop and review what it found."
            .to_owned(),
        (Full::Readings, Scope::Window) => "This recording is full: the window showed more than \
                                            a million different readings, the most one recording \
                                            counts. Stop and review what it found."
            .to_owned(),
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
    /// Where a recording of the whole window found text like its region's
    /// the last time it looked, while it records; empty otherwise.
    pub(crate) spots: Vec<RegionView>,
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
    /// Where it looks for characters.
    pub(crate) scope: Scope,
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

impl RegionView {
    fn of(region: Region) -> Self {
        Self {
            x: region.x(),
            y: region.y(),
            width: region.size().width,
            height: region.size().height,
        }
    }
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
    /// How wide its picture is, in pixels.
    pub(crate) width: u32,
    /// Where each of its characters is in its picture, left to right, with
    /// a name for its shape that a find's character of the same shape
    /// shares.
    pub(crate) characters: Vec<CharacterView>,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

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
        let reading = &going.readings[0];
        assert_eq!(
            (reading.first_ms, reading.seen, &reading.text),
            (1000, 1, &Text::Pending)
        );
        assert_eq!(reading.shapes, [0]);
        // Its one character covers the two inked columns of its 8.
        assert_eq!(reading.width, 8);
        let columns: Vec<(u32, u32)> = reading
            .characters
            .iter()
            .map(|character| (character.left, character.right))
            .collect();
        assert_eq!(columns, [(0, 2)]);
        assert_eq!(
            recording.view(start).readings[0].text,
            Text::Read("12".to_owned())
        );
    }

    #[test]
    fn it_records_only_the_same_region_ink_window_and_scope() {
        let recording = recording(Instant::now());
        assert!(recording.records(region(), (RED, 0), size(), Scope::Place));
        let other_region = Region::new(1, 0, 7, 4).expect("not empty");
        assert!(!recording.records(other_region, (RED, 0), size(), Scope::Place));
        assert!(!recording.records(region(), (RED, 9), size(), Scope::Place));
        let green = Rgb { green: 1, ..RED };
        assert!(!recording.records(region(), (green, 0), size(), Scope::Place));
        let other_size = Size {
            width: 9,
            height: 4,
        };
        assert!(!recording.records(region(), (RED, 0), other_size, Scope::Place));
        assert!(!recording.records(region(), (RED, 0), size(), Scope::Window));
        let window = recording.across_window(4);
        assert!(window.records(region(), (RED, 0), size(), Scope::Window));
        assert!(!window.records(region(), (RED, 0), size(), Scope::Place));
    }

    /// A 60 by 40 frame with ink inside each of `blocks`, as left, top,
    /// width, and height.
    fn blocks(blocks: &[(u32, u32, u32, u32)]) -> Image {
        frame(60, 40, |x, y| {
            blocks.iter().any(|&(left, top, width, height)| {
                (left..left.saturating_add(width)).contains(&x)
                    && (top..top.saturating_add(height)).contains(&y)
            })
        })
    }

    /// A recording of the whole 60 by 40 window, for text 8 pixels tall
    /// like its region's at the top left.
    fn window_recording(started: Instant) -> Recording {
        let region = Region::new(0, 0, 12, 12).expect("not empty");
        let size = Size {
            width: 60,
            height: 40,
        };
        Recording::new(1, region, (RED, 0), size, started).across_window(8)
    }

    const STROKE: (u32, u32, u32, u32) = (2, 2, 3, 8);
    const BLOCK: (u32, u32, u32, u32) = (30, 20, 6, 8);

    /// Has `recording` look over `frame` as the look's thread would, when
    /// a look is due, and answers how many new readings it showed.
    fn looked_over(recording: &mut Recording, frame: &Image, at: Instant) -> Option<usize> {
        let job = recording.look(frame, at)?;
        Some(recording.looked(job.at, Some(look::look(&job)), at).len())
    }

    #[test]
    fn a_whole_window_keeps_a_reading_for_each_run_of_text_like_the_region_s() {
        let start = Instant::now();
        let mut recording = window_recording(start);
        // A stroke in the region, a block far from it, and a bar too tall
        // to be text of the region's size.
        let shown = blocks(&[STROKE, BLOCK, (50, 2, 3, 20)]);
        assert_eq!(
            recording.offer(&shown, start),
            None,
            "its region alone keeps none"
        );
        let job = recording
            .look(&shown, start)
            .expect("the first frame is looked over");
        let new = recording.looked(job.at, Some(look::look(&job)), start);
        let indexes: Vec<usize> = new.iter().map(|new| new.index).collect();
        assert_eq!(indexes, [0, 1], "one reading for each run, read as text");
        let view = recording.view(start);
        let shapes: Vec<&[usize]> = view
            .readings
            .iter()
            .map(|reading| reading.shapes.as_slice())
            .collect();
        assert_eq!(shapes, [&[0][..], &[1]]);
        // Each run with a margin of a quarter of the 8 pixels around it.
        let spot = |x, y, width, height| RegionView {
            x,
            y,
            width,
            height,
        };
        assert_eq!(view.spots, [spot(0, 0, 7, 12), spot(28, 18, 10, 12)]);
        assert_eq!(view.settings.scope, Scope::Window);
        // The kept picture is the run's own, not the region's.
        assert_eq!(
            recording.readings()[1].image.size(),
            Size {
                width: 10,
                height: 12
            }
        );
    }

    #[test]
    fn a_whole_window_is_looked_over_once_each_interval() {
        let start = Instant::now();
        let mut recording = window_recording(start);
        assert_eq!(
            looked_over(&mut recording, &blocks(&[STROKE]), start),
            Some(1)
        );
        let soon = start + Duration::from_millis(499);
        assert_eq!(
            looked_over(&mut recording, &blocks(&[STROKE, BLOCK]), soon),
            None,
            "a frame before the interval passed is skipped"
        );
        assert_eq!(recording.view(soon).spots.len(), 1, "the last look's");
        assert_eq!(sightings(&recording, soon), [(1, 1)], "and not counted");
        let due = start + SCAN_EVERY;
        assert_eq!(
            looked_over(&mut recording, &blocks(&[STROKE, BLOCK]), due),
            Some(1)
        );
        assert_eq!(sightings(&recording, due), [(2, 1), (1, 1)]);
    }

    #[test]
    fn a_look_that_takes_longer_than_the_interval_holds_one_frame() {
        let start = Instant::now();
        let mut recording = window_recording(start);
        let shown = blocks(&[STROKE]);
        let job = recording
            .look(&shown, start)
            .expect("the first frame goes out");
        // The look is still out long after the interval passed.
        for late in [SCAN_EVERY, 4 * SCAN_EVERY, 60 * SCAN_EVERY] {
            assert!(
                recording.look(&shown, start + late).is_none(),
                "no frame goes out while a look is out"
            );
        }
        let answered = start + 61 * SCAN_EVERY;
        assert_eq!(
            recording
                .looked(job.at, Some(look::look(&job)), answered)
                .len(),
            1
        );
        assert!(
            recording.look(&shown, answered).is_some(),
            "the next frame goes out once it is answered"
        );
    }

    #[test]
    fn a_look_answered_after_a_stop_is_dropped_and_the_next_goes_out_on_record_more() {
        let start = Instant::now();
        let mut recording = window_recording(start);
        assert_eq!(
            looked_over(&mut recording, &blocks(&[STROKE]), start),
            Some(1)
        );
        let job = recording
            .look(&blocks(&[STROKE, BLOCK]), start + SCAN_EVERY)
            .expect("due");
        recording.stop(start + SCAN_EVERY);
        assert_eq!(
            recording.view(start + SCAN_EVERY).spots,
            [],
            "a stop forgets them"
        );
        let late = start + 2 * SCAN_EVERY;
        assert_eq!(recording.looked(job.at, Some(look::look(&job)), late), []);
        assert_eq!(recording.readings().len(), 1, "what it saw is not kept");
        assert_eq!(recording.view(late).spots, []);
        recording.resume(late);
        assert_eq!(
            looked_over(&mut recording, &blocks(&[STROKE, BLOCK]), late),
            Some(1)
        );
    }

    #[test]
    fn a_look_from_before_record_more_is_dropped_when_it_answers_after() {
        let start = Instant::now();
        let mut recording = window_recording(start);
        let job = recording
            .look(&blocks(&[STROKE]), start)
            .expect("the first frame goes out");
        recording.stop(start);
        let more = start + SCAN_EVERY;
        recording.resume(more);
        assert!(
            recording.look(&blocks(&[BLOCK]), more).is_none(),
            "one look is out still"
        );
        assert_eq!(recording.looked(job.at, Some(look::look(&job)), more), []);
        assert_eq!(recording.readings().len(), 0, "what it saw is not kept");
        assert_eq!(recording.view(more).spots, []);
        assert_eq!(
            looked_over(&mut recording, &blocks(&[BLOCK]), more),
            Some(1),
            "and the next frame goes out at once"
        );
    }

    #[test]
    fn a_look_out_when_the_window_changes_size_is_dropped() {
        let start = Instant::now();
        let mut recording = window_recording(start);
        let job = recording
            .look(&blocks(&[STROKE]), start)
            .expect("the first frame goes out");
        let wider = frame(61, 40, |x, y| x < 3 && y < 8);
        assert!(recording.look(&wider, start).is_none());
        assert_eq!(recording.looked(job.at, Some(look::look(&job)), start), []);
        let view = recording.view(start);
        assert_eq!(view.spots, [], "no places from the other size are shown");
        assert!(view.problem.is_some(), "and the problem stands");
        assert_eq!(recording.readings().len(), 0);
        assert_eq!(
            looked_over(&mut recording, &blocks(&[BLOCK]), start),
            Some(1),
            "back at its size it looks at once"
        );
    }

    #[test]
    fn a_look_that_failed_stops_the_recording_and_record_more_looks_again() {
        let start = Instant::now();
        let mut recording = window_recording(start);
        assert_eq!(
            looked_over(&mut recording, &blocks(&[STROKE]), start),
            Some(1)
        );
        let job = recording
            .look(&blocks(&[STROKE]), start + SCAN_EVERY)
            .expect("due");
        assert_eq!(recording.looked(job.at, None, start + SCAN_EVERY), []);
        let view = recording.view(start + SCAN_EVERY);
        assert!(!view.recording);
        assert_eq!(
            view.problem.as_deref(),
            Some(
                "Recording stopped: the window could not be looked over. Record more to try \
                 again."
            )
        );
        assert_eq!(view.spots, []);
        recording.resume(start + SCAN_EVERY);
        assert_eq!(
            looked_over(&mut recording, &blocks(&[BLOCK]), start + SCAN_EVERY),
            Some(1)
        );
    }

    #[test]
    fn a_whole_window_of_another_size_says_so_and_shows_no_spots() {
        let start = Instant::now();
        let mut recording = window_recording(start);
        assert_eq!(
            looked_over(&mut recording, &blocks(&[STROKE]), start),
            Some(1)
        );
        let wider = frame(61, 40, |x, y| x < 3 && y < 8);
        assert_eq!(
            looked_over(&mut recording, &wider, start + SCAN_EVERY),
            None
        );
        let view = recording.view(start + SCAN_EVERY);
        assert_eq!(
            view.problem.as_deref(),
            Some(
                "The region was drawn when the window was 60x40, and it is 61x40 now. Resize the \
                 window to record."
            )
        );
        assert_eq!(view.spots, []);
        // Back at its size it looks again at once, and the problem clears.
        let back = start + SCAN_EVERY + Duration::from_millis(1);
        assert_eq!(
            looked_over(&mut recording, &blocks(&[BLOCK]), back),
            Some(1)
        );
        assert_eq!(recording.view(back).problem, None);
    }

    #[test]
    fn a_full_recording_says_what_showed_too_much() {
        let place = full_message(Full::Shapes, Scope::Place);
        assert!(place.contains("the region showed more than 256 different shapes"));
        assert_eq!(
            full_message(Full::Shapes, Scope::Window),
            "This recording is full: the window showed more than 256 different shapes in the \
             characters' color and size, more than a reference learns. Stop and review what it \
             found. If the window shows much other text like the value's, record this place \
             alone instead."
        );
        assert!(
            full_message(Full::Readings, Scope::Window)
                .contains("the window showed more than a million different readings")
        );
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
                r#""tolerance":7,"windowSize":{"width":8,"height":4},"scope":"place"}"#
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
