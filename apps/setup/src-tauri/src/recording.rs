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
//! tall as the characters the region showed when it started, or as those of
//! another example the user drew ([`find`]), so text at each size the
//! application draws the characters in is collected. A shape is a character
//! scaled onto one grid, whatever its size, so a shape that shows at two
//! sizes is still kept and counted once.
//! Each such run of text is cropped and counted as the region's own crop
//! is. Looking over a whole frame costs far more than cropping one region,
//! so a thread of its own does it ([`look`]), one frame at a time: the
//! owner sends a frame at most once every [`SCAN_EVERY`], and none while a
//! look is out, and skips the frames between.
//!
//! A recording is drawn on a screenshot of one window size. It follows the
//! window to another size the way [`follow`] says, the one place that rule
//! lives: once frames have kept a new size for a while, its region and the
//! sizes of text it looks for are placed there, or it waits with a problem
//! where it is not recorded at that size. The same grid makes a shape that
//! shows at two window sizes one shape.
//!
//! A digit, a `%`, and a `.` are all shapes the same way: whatever the
//! check's own sampling makes one character is one shape here.

pub(crate) mod character;
pub(crate) mod find;
pub(crate) mod follow;
mod ink;
mod lifecycle;
pub(crate) mod look;
pub(crate) mod looks;
pub(crate) mod ocr;
mod sightings;

use std::time::{Duration, Instant};

use appytappy_config::app_file::{ColorDef, RegionDef, SizeDef};
use appytappy_sources::screen::digits::{DigitReader, ReadError, Shape};
use appytappy_sources::screen::settle::Settle;
use appytappy_sources::screen::{FrameMeaning, Image, Region, Rgb, Size};
use serde::{Deserialize, Serialize};

use crate::ticket::Ticket;

use self::character::CharacterView;
use self::find::{Sizes, TextSize};
use self::follow::{Follows, Unplaced};
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
    /// The size of text a look over the whole window took it for when it
    /// first showed. `None` for a reading of the region itself.
    size: Option<TextSize>,
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

/// Why a window of `size` is not recorded, for the `reason` it is not,
/// when the region was drawn on one of `window_size`.
fn unplaced(reason: Unplaced, window_size: Size, size: Size) -> String {
    let (drawn, now) = (sized(window_size), sized(size));
    match reason {
        Unplaced::NotFollowed => format!(
            "The region was drawn when the window was {drawn}, and it is {now} now. Resize the \
             window to record."
        ),
        Unplaced::Outside => format!(
            "The region was drawn when the window was {drawn}. Scaled to {now}, the window's \
             size now, it reaches outside the window. Resize the window to record."
        ),
        Unplaced::TooSmall => format!(
            "The region was drawn when the window was {drawn}. Scaled to {now}, the window's \
             size now, its text is too small to tell the characters apart. Make the window \
             larger to record."
        ),
    }
}

/// A window's size as a message names it, such as `1280 × 720`.
fn sized(size: Size) -> String {
    format!("{} × {}", size.width, size.height)
}

/// The pixels of `region` on `frame`, a whole window's frame, for a
/// recording of looks, which does not follow its window to another size:
/// its pictures compare pixel for pixel. Otherwise why the frame cannot be
/// recorded: the window is not `window_size`, the size it had when the
/// region was drawn.
fn cut(frame: &Image, region: Region, window_size: Size) -> Result<Image, String> {
    let size = frame.size();
    frame
        .crop(region)
        .filter(|_| size == window_size)
        .ok_or_else(|| unplaced(Unplaced::NotFollowed, window_size, size))
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
    /// The sizes of text it looks for: how tall the characters were that
    /// the region showed when the recording started, and those of each
    /// other example.
    sizes: Sizes,
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
    /// user recorded more, or from before a frame showed the window at
    /// another size than the recording is placed at. Every frame is skipped
    /// until it is answered.
    Abandoned,
}

/// A recording of the readings one region shows.
#[derive(Debug)]
pub(crate) struct Recording {
    /// Its number in the session, so the UI and the text reader tell it
    /// from one it replaced.
    pub(crate) id: Ticket,
    /// The region of the window it records.
    pub(crate) region: Region,
    /// The characters' color.
    ink: Rgb,
    /// How far a pixel may be from `ink` and still be ink.
    tolerance: u8,
    reader: DigitReader,
    /// The window's size when the region was drawn.
    window_size: Size,
    /// How it follows the window to another size.
    follows: Follows,
    /// The window size frames show, settled.
    settle: Settle,
    /// The window size it is placed at, the one frames settled at last, and
    /// where it records there, or why it does not. A recording of the
    /// whole window records near there.
    placed: (Size, Result<Region, Unplaced>),
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
    /// a pixel within `tolerance` of the `ink` color is ink. It `follows`
    /// the window to another size, and is placed at the size it was drawn
    /// at until frames settle at another.
    pub(crate) const fn new(
        id: Ticket,
        region: Region,
        (ink, tolerance): (Rgb, u8),
        (window_size, follows): (Size, Follows),
        started: Instant,
    ) -> Self {
        Self {
            id,
            region,
            ink,
            tolerance,
            reader: DigitReader::new(ink, tolerance),
            window_size,
            follows,
            settle: Settle::new(window_size),
            placed: (window_size, Ok(region)),
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
            sizes: Sizes::of(sample),
            gate: Gate::Idle(None),
            spots: Vec::new(),
        });
        self
    }

    /// Has a recording of the whole window look for text with characters
    /// of each of `heights` too, in pixels: those of the other examples.
    /// The looks from the next one on take it, and what it kept stays. A
    /// recording of the region alone looks for no size.
    pub(crate) fn look_for(&mut self, heights: impl IntoIterator<Item = u32>) {
        if let Looking::Window(scan) = &mut self.looking {
            for height in heights {
                scan.sizes.add(height);
            }
        }
        // A new size may be one that is not looked for at this window size.
        self.put_here(self.placed.0);
    }

    /// Where it looks for characters.
    pub(crate) const fn scope(&self) -> Scope {
        match self.looking {
            Looking::Place => Scope::Place,
            Looking::Window(_) => Scope::Window,
        }
    }

    /// Places it on a window of `size`: where [`Follows`] puts its region
    /// there, or near where for a recording of the whole window, which is
    /// placed only where each size of text it looks for is looked for.
    fn put(&mut self, size: Size) {
        let (follows, drawn) = (self.follows, self.window_size);
        let place = match &self.looking {
            Looking::Place => follows.region(self.region, drawn, size),
            Looking::Window(scan) => scan
                .sizes
                .heights()
                .into_iter()
                .try_for_each(|height| follows.height(height, drawn, size).map(|_| ()))
                .and_then(|()| follows.near(self.region, drawn, size)),
        };
        self.placed = (size, place);
    }

    /// Places it on a window of `size` ([`Self::put`]) and, while it
    /// records, has the reason as its problem when that is nowhere, so a
    /// window that draws no new frame still says why nothing is recorded.
    fn put_here(&mut self, size: Size) {
        self.put(size);
        if let (true, Err(reason)) = (self.life.is_recording(), self.placed.1) {
            self.life
                .set_problem(Some(unplaced(reason, self.window_size, size)));
        }
    }

    /// Places it at the window size frames have settled at by `now`
    /// ([`Settle`]), when they settled at another than it is placed at,
    /// and answers whether it records there, so the newest frame, which
    /// shows that size, can be recorded. Where it does not record, it has
    /// the reason as its problem. A window that stops drawing once it has
    /// its new size sends no frame to settle on, so the owner asks this
    /// whenever it shows the recording, as well as on each frame. The
    /// frame that showed the new size dropped the look that was out
    /// already ([`Self::place`]).
    pub(crate) fn settled(&mut self, now: Instant) -> bool {
        if !self.life.is_recording() {
            return false;
        }
        let Some(size) = self.settle.settled(now) else {
            return false;
        };
        self.put_here(size);
        self.placed.1.is_ok()
    }

    /// Where it records on a frame that arrived at `at` and shows the
    /// window at `size`, or `None` when the frame is not recorded. Each
    /// frame means what it means to every reader of a place on the window
    /// ([`FrameMeaning`]). At the size it is placed at, it records where
    /// [`Self::put`] placed it, or has a problem when that is nowhere. At
    /// another size nothing is recorded until frames have settled there
    /// ([`Self::settled`]), and it is then placed there once, so a drag
    /// across many sizes records none of them. A frame it does not record
    /// forgets the places the last look found text and drops the answer to
    /// a look that is out, since both are of the window as it was.
    fn place(&mut self, size: Size, at: Instant) -> Option<Region> {
        self.settle.saw(size, at);
        self.settled(at);
        let (placed_at, place) = self.placed;
        match (FrameMeaning::of(placed_at, size, place.ok()), place) {
            (FrameMeaning::Readable(region), _) => return Some(region),
            (FrameMeaning::Unreadable(_), Err(reason)) => {
                self.life
                    .set_problem(Some(unplaced(reason, self.window_size, size)));
            },
            (FrameMeaning::Unreadable(_) | FrameMeaning::Resized(_), _) => {},
        }
        self.drop_look();
        None
    }

    /// Forgets the places the last look found text, and drops the answer
    /// to a look that is out: both are of the window as it was.
    fn drop_look(&mut self) {
        if let Looking::Window(scan) = &mut self.looking {
            scan.spots.clear();
            if scan.gate == Gate::Out {
                scan.gate = Gate::Abandoned;
            }
        }
    }

    /// Records `frame`, a whole window's frame that arrived at `at`, in its
    /// region, where that is at the window's size. Answers the reading to
    /// read as text when the frame shows one that is worth keeping, since
    /// it shows a shape no kept reading has, or carries cleanly one that
    /// none carries so. Of several such readings for a shape, the first is
    /// kept. A full recording keeps and counts no more. A recording of the
    /// whole window keeps nothing from a frame here: [`Self::look`] sends
    /// the frame to be looked over.
    pub(crate) fn offer(&mut self, frame: &Image, at: Instant) -> Option<NewReading> {
        if !self.life.is_recording() || matches!(self.looking, Looking::Window(_)) {
            return None;
        }
        let region = self.place(frame.size(), at)?;
        // A region placed at the frame's size lies inside the frame.
        let crop = frame.crop(region)?;
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
        self.keep(crop, shapes.characters(), None, at)
    }

    /// The look to send for `frame`, a whole window's frame that arrived at
    /// `at`, when it records the whole window and a look is due: none is
    /// out, and the last was of a frame at least [`SCAN_EVERY`] before
    /// this one. The look is then out until [`Self::looked`] answers it, so
    /// a look that takes long holds one frame and not a queue of them. A
    /// frame that is not recorded at its size ([`Self::place`]) is not
    /// looked over. The look searches near where the region is at the
    /// frame's size, moved inside the window where it scales outside, for
    /// text of each example's height scaled to that size.
    pub(crate) fn look(&mut self, frame: &Image, at: Instant) -> Option<look::Job> {
        if !self.life.is_recording() {
            return None;
        }
        if !matches!(self.looking, Looking::Window(_)) {
            return None;
        }
        let near = self.place(frame.size(), at)?;
        let Looking::Window(scan) = &mut self.looking else {
            return None;
        };
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
            sizes: scan.sizes.each(|height| {
                let scaled = self.follows.height(height, self.window_size, self.placed.0);
                scaled.ok()
            }),
            near,
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
                self.keep(crop, &shapes, Some(seen.size), at)
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

    /// Goes on recording at `at` after a stop, and `follows` the window as
    /// the user has it now, placed again at the size frames last settled
    /// at. A size frames showed before the stop and had yet to settle at
    /// is forgotten: the window may have had any size since, so a new size
    /// settles from the frames that come now. A look that is still out is
    /// of a frame from before the stop, so its answer is dropped, and so
    /// are the spots a stop by a failure left.
    pub(crate) fn resume(&mut self, follows: Follows, at: Instant) {
        self.life.resume(at);
        self.follows = follows;
        self.settle = Settle::new(self.placed.0);
        self.put_here(self.placed.0);
        self.drop_look();
    }

    fn forget_spots(&mut self) {
        if let Looking::Window(scan) = &mut self.looking {
            scan.spots.clear();
        }
    }

    /// Counts `crop`, which showed `shapes` at `at`, and keeps it when it
    /// is worth keeping; answers it then, for the text reader. `size` is
    /// the size of text a look over the whole window took it for.
    fn keep(
        &mut self,
        crop: Image,
        shapes: &[Shape],
        size: Option<TextSize>,
        at: Instant,
    ) -> Option<NewReading> {
        let shapes = self.sightings.count(shapes)?;
        let kept = self.readings.iter().map(|kept| kept.shapes.as_slice());
        if !self.sightings.worth_keeping(&shapes, kept) {
            if let Some(kept) = self.readings.iter_mut().find(|kept| kept.shapes == shapes) {
                kept.seen = kept.seen.saturating_add(1);
            }
            return None;
        }
        let line = Line::of(&self.reader.ink(&crop))?;
        let index = self.readings.len();
        self.readings.push(Kept {
            characters: character::of(&crop, &self.reader),
            image: crop,
            shapes,
            first: self.life.elapsed(at),
            seen: 1,
            size,
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
            id: self.id.number(),
            recording: self.life.is_recording(),
            elapsed_ms: millis(self.life.elapsed(now)),
            full: self
                .sightings
                .full()
                .map(|full| full_message(full, self.scope())),
            problem: self.life.problem().map(str::to_owned),
            settings: Settings {
                region: self.region.into(),
                ink: self.ink.into(),
                tolerance: self.tolerance,
                window_size: self.window_size.into(),
                scope: self.scope(),
            },
            at: self.placed.0.into(),
            sizes: match &self.looking {
                Looking::Window(scan) => scan.sizes.heights(),
                Looking::Place => Vec::new(),
            },
            spots: match &self.looking {
                Looking::Window(scan) => scan.spots.iter().copied().map(RegionDef::from).collect(),
                Looking::Place => Vec::new(),
            },
            readings: self
                .readings
                .iter()
                .map(|kept| ReadingView {
                    first_ms: millis(kept.first),
                    seen: kept.seen,
                    size: kept.size.map(SizeTaken::of),
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
    /// The window size it is placed at: the size the region was drawn at,
    /// or the one frames settled at since. The region and the sizes of
    /// text are scaled from the one to the other where they differ.
    pub(crate) at: SizeDef,
    /// How tall the characters are that a recording of the whole window
    /// looks for, smallest first, in pixels of the window the region was
    /// drawn on: one size for each example of text that differs from the
    /// others. Empty for a recording of the region alone.
    pub(crate) sizes: Vec<u32>,
    /// Where a recording of the whole window found text like its region's
    /// the last time it looked, on a window of the size it is placed at,
    /// while it records; empty otherwise.
    pub(crate) spots: Vec<RegionDef>,
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
    pub(crate) region: RegionDef,
    /// The characters' color.
    pub(crate) ink: ColorDef,
    /// How far a pixel may be from the characters' color and still be ink.
    pub(crate) tolerance: u8,
    /// The window's size when the region was drawn.
    pub(crate) window_size: SizeDef,
    /// Where it looks for characters.
    pub(crate) scope: Scope,
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
    /// The size of text a recording of the whole window took it for when
    /// it first showed. `None` for a reading of the region itself.
    pub(crate) size: Option<SizeTaken>,
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

/// The size of text a reading was taken for, as the UI shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "RecordedSize"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct SizeTaken {
    /// How tall its characters are in its picture, in pixels: about as
    /// tall as the look searched for at the window's size then.
    pub(crate) height: u32,
    /// The one of the recording's sizes it is: how tall the characters of
    /// that example were on the screenshot it was drawn on. It differs
    /// from `height` when the window had another size.
    pub(crate) example: u32,
}

impl SizeTaken {
    const fn of(size: TextSize) -> Self {
        Self {
            height: size.height,
            example: size.example,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use appytappy_config::app_file::PinDef;
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
        Recording::new(
            Ticket::numbered(1),
            region(),
            (RED, 0),
            (size(), Follows::Drawn),
            started,
        )
    }

    /// A recording of a `width` by `height` window, all of it the region.
    fn recording_of(width: u32, height: u32, started: Instant) -> Recording {
        let region = Region::new(0, 0, width, height).expect("not empty");
        Recording::new(
            Ticket::numbered(1),
            region,
            (RED, 0),
            (Size { width, height }, Follows::Drawn),
            started,
        )
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
        let mut recording = Recording::new(
            Ticket::numbered(1),
            region,
            (RED, 0),
            (tall, Follows::Drawn),
            start,
        );
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

    /// `start` and `millis` more.
    fn after(start: Instant, millis: u64) -> Instant {
        start
            .checked_add(Duration::from_millis(millis))
            .expect("in range")
    }

    /// `character` drawn `times` as large.
    fn larger(times: u32, character: fn(u32, u32) -> bool) -> impl Fn(u32, u32) -> bool {
        move |x, y| {
            character(
                x.checked_div(times).expect("not zero"),
                y.checked_div(times).expect("not zero"),
            )
        }
    }

    #[test]
    fn a_frame_without_ink_keeps_nothing() {
        let start = Instant::now();
        let mut recording = recording(start);
        assert_eq!(recording.offer(&frame(8, 4, |_, _| false), start), None);
        assert_eq!(recording.view(start).problem, None);
        assert!(recording.offer(&frame(8, 4, |_, _| true), start).is_some());
        assert_eq!(recording.readings().len(), 1);
    }

    #[test]
    fn a_recording_that_does_not_follow_its_window_waits_for_the_size_it_was_drawn_at() {
        let start = Instant::now();
        let mut recording = recording(start);
        let wider = frame(9, 4, |_, _| true);
        // A new size says nothing until frames have shown it for a while.
        assert_eq!(recording.offer(&wider, start), None);
        assert_eq!(recording.offer(&wider, after(start, 249)), None);
        assert_eq!(recording.view(after(start, 249)).problem, None);
        assert_eq!(recording.offer(&wider, after(start, 250)), None);
        let view = recording.view(after(start, 250));
        assert_eq!(
            view.problem.as_deref(),
            Some(
                "The region was drawn when the window was 8 × 4, and it is 9 × 4 now. Resize the \
                 window to record."
            )
        );
        assert_eq!(
            view.at,
            SizeDef {
                width: 9,
                height: 4
            }
        );
        assert_eq!(recording.readings().len(), 0);
        // Back at its size, once that has settled, it records again.
        let fits = frame(8, 4, |_, _| true);
        assert_eq!(recording.offer(&fits, after(start, 300)), None);
        assert!(recording.offer(&fits, after(start, 550)).is_some());
        let view = recording.view(after(start, 550));
        assert_eq!(view.problem, None, "a frame it records clears it");
        assert_eq!(
            view.at,
            SizeDef {
                width: 8,
                height: 4
            }
        );
    }

    /// A recording of the 16 by 32 window's left half, which follows the
    /// window to another size from its top-left corner.
    fn following(started: Instant) -> Recording {
        let region = Region::new(0, 0, 8, 32).expect("not empty");
        let size = Size {
            width: 16,
            height: 32,
        };
        Recording::new(
            Ticket::numbered(1),
            region,
            (RED, 0),
            (size, Follows::Scaled(PinDef::TopLeft)),
            started,
        )
    }

    #[test]
    fn a_recording_that_follows_its_window_records_its_scaled_region_once_the_size_settles() {
        let start = Instant::now();
        let mut recording = following(start);
        assert!(recording.offer(&frame(16, 32, zero), start).is_some());
        // The window grows to twice its size by way of another: the 0 shows
        // twice as large in the left half, and an 8 in the right half,
        // outside the scaled region.
        let passing = frame(24, 48, |_, _| true);
        let twice = frame(32, 64, |x, y| {
            larger(2, zero)(x, y) || (x >= 16 && larger(2, eight)(x.saturating_sub(16), y))
        });
        assert_eq!(recording.offer(&passing, after(start, 100)), None);
        assert_eq!(recording.offer(&twice, after(start, 300)), None);
        assert_eq!(
            recording.offer(&twice, after(start, 549)),
            None,
            "the size it passed never settled, and this one has yet to"
        );
        assert_eq!(recording.view(after(start, 549)).problem, None);
        assert_eq!(recording.readings()[0].seen, 1, "nothing is counted yet");
        // Settled, the same shape at another size is the same reading.
        assert_eq!(recording.offer(&twice, after(start, 550)), None);
        assert_eq!(recording.readings()[0].seen, 2);
        let view = recording.view(after(start, 550));
        assert_eq!(view.problem, None);
        assert_eq!(
            view.at,
            SizeDef {
                width: 32,
                height: 64
            }
        );
        // A new shape there is kept from the scaled region alone.
        let eights = frame(32, 64, larger(2, eight));
        let new = recording.offer(&eights, after(start, 600));
        assert_eq!(new.map(|new| new.index), Some(1));
        assert_eq!(
            recording.readings()[1].image.size(),
            Size {
                width: 16,
                height: 64
            }
        );
    }

    #[test]
    fn a_size_settles_when_asked_once_no_frame_comes_to_settle_it() {
        let start = Instant::now();
        // A window that stops drawing at its new size sends one frame.
        let mut recording = following(start);
        let twice = frame(32, 64, larger(2, zero));
        assert_eq!(recording.offer(&twice, start), None);
        assert!(!recording.settled(after(start, 249)), "not yet");
        assert_eq!(recording.view(after(start, 249)).at.width, 16);
        assert!(recording.settled(after(start, 250)), "it records there");
        let view = recording.view(after(start, 250));
        assert_eq!((view.at.width, view.problem), (32, None));
        assert!(!recording.settled(after(start, 300)), "placed once");
        assert!(recording.offer(&twice, after(start, 300)).is_some());
        // Where it does not record, asking says why.
        let mut waits = recording_of(8, 4, start);
        assert_eq!(waits.offer(&frame(9, 4, |_, _| true), start), None);
        assert!(!waits.settled(after(start, 250)));
        let view = waits.view(after(start, 250));
        assert_eq!(view.at.width, 9);
        assert_eq!(
            view.problem.as_deref(),
            Some(
                "The region was drawn when the window was 8 × 4, and it is 9 × 4 now. Resize the \
                 window to record."
            )
        );
        // A stopped recording is placed nowhere new.
        let mut stopped = following(start);
        assert_eq!(stopped.offer(&twice, start), None);
        stopped.stop(after(start, 100));
        assert!(!stopped.settled(after(start, 400)));
        assert_eq!(stopped.view(after(start, 400)).at.width, 16);
    }

    #[test]
    fn recording_more_forgets_a_size_that_had_yet_to_settle_at_the_stop() {
        let start = Instant::now();
        let mut recording = following(start);
        let twice = frame(32, 64, larger(2, zero));
        assert_eq!(recording.offer(&twice, start), None);
        recording.stop(after(start, 100));
        // Five seconds on the window may have had any size, so the frames
        // that come now are the ones a size settles from.
        recording.resume(Follows::Scaled(PinDef::TopLeft), after(start, 5100));
        assert_eq!(recording.offer(&twice, after(start, 5100)), None);
        assert_eq!(recording.view(after(start, 5100)).at.width, 16);
        assert_eq!(recording.offer(&twice, after(start, 5349)), None);
        assert!(recording.offer(&twice, after(start, 5350)).is_some());
    }

    #[test]
    fn a_region_that_scales_past_the_window_s_edge_says_so_until_the_window_fits_it() {
        let start = Instant::now();
        let region = Region::new(8, 0, 8, 32).expect("not empty");
        let drawn = Size {
            width: 16,
            height: 32,
        };
        let follows = Follows::Scaled(PinDef::TopLeft);
        let mut recording = Recording::new(
            Ticket::numbered(1),
            region,
            (RED, 0),
            (drawn, follows),
            start,
        );
        // As tall and narrower: pinned left, the region keeps its place,
        // which is past the right edge.
        let narrow = frame(12, 32, |_, _| true);
        assert_eq!(recording.offer(&narrow, start), None);
        assert_eq!(recording.offer(&narrow, after(start, 250)), None);
        assert_eq!(
            recording.view(after(start, 250)).problem.as_deref(),
            Some(
                "The region was drawn when the window was 16 × 32. Scaled to 12 × 32, the \
                 window's size now, it reaches outside the window. Resize the window to record."
            )
        );
        let wide = frame(20, 32, |x, y| x >= 8 && zero(x.saturating_sub(8), y));
        assert_eq!(recording.offer(&wide, after(start, 300)), None);
        assert!(recording.offer(&wide, after(start, 550)).is_some());
        assert_eq!(recording.view(after(start, 550)).problem, None);
    }

    #[test]
    fn recording_more_follows_the_window_as_the_user_has_it_now() {
        let start = Instant::now();
        let mut recording = recording_of(16, 32, start);
        let twice = frame(32, 64, larger(2, zero));
        assert_eq!(recording.offer(&twice, start), None);
        assert_eq!(recording.offer(&twice, after(start, 250)), None);
        assert!(recording.view(after(start, 250)).problem.is_some());
        recording.stop(after(start, 300));
        // The window is still at the size it settled at, where the
        // recording is placed at once.
        recording.resume(Follows::Scaled(PinDef::TopLeft), after(start, 400));
        assert!(recording.offer(&twice, after(start, 400)).is_some());
        assert_eq!(recording.view(after(start, 400)).problem, None);
        // And back: it waits for the size it was drawn at again.
        recording.stop(after(start, 500));
        recording.resume(Follows::Drawn, after(start, 600));
        assert!(
            recording.view(after(start, 600)).problem.is_some(),
            "it says why at once, before the window draws again"
        );
        assert_eq!(
            recording.offer(&frame(32, 64, larger(2, eight)), after(start, 600)),
            None
        );
        assert!(recording.view(after(start, 600)).problem.is_some());
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
        Recording::new(
            Ticket::numbered(1),
            region,
            (RED, 0),
            (size, Follows::Drawn),
            started,
        )
        .across_window(8)
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
        let spot = |x, y, width, height| RegionDef {
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
    fn a_whole_window_keeps_text_of_each_example_s_size_and_a_shape_once_across_them() {
        let start = Instant::now();
        let mut recording = window_recording(start);
        // The region's stroke, the same stroke twice as large, and a block
        // twice as large as the region's text.
        let shown = blocks(&[STROKE, (20, 20, 6, 16), (44, 20, 12, 16)]);
        assert_eq!(looked_over(&mut recording, &shown, start), Some(1));
        assert_eq!(recording.view(start).sizes, [8]);
        assert_eq!(recording.view(start).spots.len(), 1, "one size is found");
        recording.look_for([16, 17, 8]);
        let later = start + SCAN_EVERY;
        assert_eq!(
            looked_over(&mut recording, &shown, later),
            Some(1),
            "the large stroke shows the shape the small one does"
        );
        let view = recording.view(later);
        assert_eq!(view.sizes, [8, 16], "17 is the size 16 is");
        assert_eq!(view.spots.len(), 3);
        let readings: Vec<(Option<u32>, u32, &[usize])> = view
            .readings
            .iter()
            .map(|reading| {
                let size = reading.size.map(|size| size.example);
                (size, reading.seen, reading.shapes.as_slice())
            })
            .collect();
        assert_eq!(
            readings,
            [(Some(8), 3, &[0][..]), (Some(16), 1, &[1])],
            "the stroke is seen twice at its size and once at the other"
        );
    }

    #[test]
    fn a_recording_of_the_region_alone_looks_for_no_size() {
        let start = Instant::now();
        let mut recording = recording_of(16, 32, start);
        recording.look_for([4]);
        recording
            .offer(&frame(16, 32, zero), start)
            .expect("a new reading");
        let view = recording.view(start);
        assert_eq!(view.sizes, [0_u32; 0]);
        assert_eq!(view.readings[0].size, None);
        assert_eq!(recording.scope(), Scope::Place);
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
        recording.resume(Follows::Drawn, late);
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
        recording.resume(Follows::Drawn, more);
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
        // A look from before that fails is dropped too: the recording goes
        // on, where a failed look of its own would stop it.
        let later = more + SCAN_EVERY;
        let job = recording.look(&blocks(&[BLOCK]), later).expect("due");
        recording.stop(later);
        recording.resume(Follows::Drawn, later);
        assert_eq!(recording.looked(job.at, None, later), []);
        let view = recording.view(later);
        assert!(view.recording, "it records still");
        assert_eq!(view.problem, None);
        assert!(
            recording.look(&blocks(&[BLOCK]), later).is_some(),
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
        assert_eq!(recording.readings().len(), 0);
        assert_eq!(
            looked_over(&mut recording, &blocks(&[BLOCK]), start),
            Some(1),
            "back at its size before the other settled, it looks at once"
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
        recording.resume(Follows::Drawn, start + SCAN_EVERY);
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
        assert_eq!(recording.view(start).spots.len(), 1);
        let wider = frame(61, 40, |x, y| x < 3 && y < 8);
        assert_eq!(looked_over(&mut recording, &wider, after(start, 500)), None);
        let unsettled = recording.view(after(start, 500));
        assert_eq!(unsettled.spots, [], "they are of the window as it was");
        assert_eq!(unsettled.problem, None);
        assert_eq!(looked_over(&mut recording, &wider, after(start, 750)), None);
        let view = recording.view(after(start, 750));
        assert_eq!(
            view.problem.as_deref(),
            Some(
                "The region was drawn when the window was 60 × 40, and it is 61 × 40 now. Resize \
                 the window to record."
            )
        );
        assert_eq!(view.spots, []);
        // Back at its size, once that has settled, it looks again at once,
        // and the problem clears.
        assert_eq!(
            looked_over(&mut recording, &blocks(&[BLOCK]), after(start, 800)),
            None
        );
        assert_eq!(
            looked_over(&mut recording, &blocks(&[BLOCK]), after(start, 1050)),
            Some(1)
        );
        assert_eq!(recording.view(after(start, 1050)).problem, None);
    }

    #[test]
    fn a_whole_window_that_changes_size_is_looked_over_for_the_scaled_sizes_near_the_scaled_region()
    {
        let start = Instant::now();
        let follows = Follows::Scaled(PinDef::TopLeft);
        let region = Region::new(0, 0, 12, 12).expect("not empty");
        let drawn = Size {
            width: 60,
            height: 40,
        };
        let mut recording = Recording::new(
            Ticket::numbered(1),
            region,
            (RED, 0),
            (drawn, follows),
            start,
        )
        .across_window(8);
        recording.look_for([16]);
        assert_eq!(
            looked_over(&mut recording, &blocks(&[STROKE]), start),
            Some(1)
        );
        // Twice the size: a stroke 16 tall is the text that was 8 tall.
        let twice = frame(120, 80, |x, y| (4..10).contains(&x) && (4..20).contains(&y));
        assert!(recording.look(&twice, after(start, 500)).is_none());
        let job = recording
            .look(&twice, after(start, 750))
            .expect("the settled size is looked over at once");
        assert_eq!(job.sizes.heights(), [16, 32]);
        assert_eq!(job.near, Region::new(0, 0, 24, 24).expect("not empty"));
        let new = recording.looked(job.at, Some(look::look(&job)), after(start, 750));
        assert_eq!(
            new,
            [],
            "the same shape at another size is the same reading"
        );
        let view = recording.view(after(start, 750));
        assert_eq!(view.sizes, [8, 16], "as they were drawn");
        assert_eq!(
            view.at,
            SizeDef {
                width: 120,
                height: 80
            }
        );
        // The stroke with a margin of a quarter of 16 pixels, on the window
        // as it is.
        assert_eq!(
            view.spots,
            [RegionDef {
                x: 0,
                y: 0,
                width: 14,
                height: 24
            }]
        );
        assert_eq!(recording.readings()[0].seen, 2);
        // A new shape there is of the example it is as tall as, scaled.
        let block = frame(120, 80, |x, y| {
            (60..72).contains(&x) && (40..56).contains(&y)
        });
        let job = recording
            .look(&block, after(start, 1250))
            .expect("a look is due");
        let new = recording.looked(job.at, Some(look::look(&job)), after(start, 1250));
        assert_eq!(new.len(), 1);
        assert_eq!(
            recording.view(after(start, 1250)).readings[1].size,
            Some(SizeTaken {
                height: 16,
                example: 8
            })
        );
    }

    /// A recording of the whole 60 by 40 window for text 8 pixels tall,
    /// near a region at its right edge, which follows the window to
    /// another size from its top-left corner.
    fn following_window(started: Instant) -> Recording {
        let region = Region::new(48, 0, 12, 12).expect("not empty");
        let drawn = Size {
            width: 60,
            height: 40,
        };
        let follows = Follows::Scaled(PinDef::TopLeft);
        Recording::new(
            Ticket::numbered(1),
            region,
            (RED, 0),
            (drawn, follows),
            started,
        )
        .across_window(8)
    }

    #[test]
    fn a_whole_window_is_looked_over_near_the_edge_its_region_scales_past() {
        let start = Instant::now();
        let mut recording = following_window(start);
        // As tall and half as wide: pinned left, the region keeps its
        // place, which is past the right edge. A look only orders what it
        // finds by how near the region it is, so it is no problem.
        let narrow = frame(30, 40, |x, y| (2..5).contains(&x) && (2..10).contains(&y));
        assert!(recording.look(&narrow, start).is_none());
        let job = recording
            .look(&narrow, after(start, 250))
            .expect("the settled size is looked over");
        assert_eq!(job.near, Region::new(29, 0, 1, 12).expect("not empty"));
        assert_eq!(job.sizes.heights(), [8]);
        let new = recording.looked(job.at, Some(look::look(&job)), after(start, 250));
        assert_eq!(new.len(), 1);
        assert_eq!(recording.view(after(start, 250)).problem, None);
    }

    const TOO_SMALL: &str = "The region was drawn when the window was 60 × 40. Scaled to 30 × 20, \
                             the window's size now, its text is too small to tell the characters \
                             apart. Make the window larger to record.";

    #[test]
    fn a_whole_window_scaled_until_its_text_is_too_small_waits_for_a_larger_one() {
        let start = Instant::now();
        let mut recording = following_window(start);
        recording.look_for([16]);
        // Three quarters the size: text 6 and 12 pixels tall is looked for.
        let smaller = frame(45, 30, |_, _| false);
        assert!(recording.look(&smaller, start).is_none());
        let job = recording.look(&smaller, after(start, 250)).expect("due");
        assert_eq!(job.sizes.heights(), [6, 12]);
        assert_eq!(recording.looked(job.at, Some(Vec::new()), start), []);
        // Half the size: the smaller text would be 4 pixels tall, on fewer
        // rows than tell one character from another.
        let half = frame(30, 20, |_, _| false);
        assert!(recording.look(&half, after(start, 300)).is_none());
        assert!(recording.look(&half, after(start, 800)).is_none());
        let view = recording.view(after(start, 800));
        assert_eq!(view.problem.as_deref(), Some(TOO_SMALL));
        assert_eq!(view.at.width, 30);
        // Back at a size its text can be told apart at, it looks again.
        assert!(recording.look(&smaller, after(start, 900)).is_none());
        assert!(recording.look(&smaller, after(start, 1150)).is_some());
        assert_eq!(recording.view(after(start, 1150)).problem, None);
    }

    #[test]
    fn an_example_too_small_at_the_window_s_size_now_stops_the_looks_when_it_is_added() {
        let start = Instant::now();
        let follows = Follows::Scaled(PinDef::TopLeft);
        let region = Region::new(0, 0, 12, 12).expect("not empty");
        let drawn = Size {
            width: 60,
            height: 40,
        };
        let mut recording = Recording::new(
            Ticket::numbered(1),
            region,
            (RED, 0),
            (drawn, follows),
            start,
        )
        .across_window(16);
        let half = frame(30, 20, |_, _| false);
        assert!(recording.look(&half, start).is_none());
        let job = recording.look(&half, after(start, 250)).expect("due");
        assert_eq!(job.sizes.heights(), [8]);
        assert_eq!(recording.looked(job.at, Some(Vec::new()), start), []);
        recording.look_for([8]);
        assert_eq!(
            recording.view(after(start, 300)).problem.as_deref(),
            Some(TOO_SMALL),
            "it says why at once, before the window draws again"
        );
        assert!(recording.look(&half, after(start, 800)).is_none());
        assert_eq!(
            recording.view(after(start, 800)).problem.as_deref(),
            Some(TOO_SMALL)
        );
    }

    #[test]
    fn a_region_scaled_to_fewer_rows_than_a_character_needs_is_not_recorded() {
        let start = Instant::now();
        let mut recording = following(start);
        // An eighth the size: the region is 4 rows tall.
        let tiny = frame(2, 4, |_, _| true);
        assert_eq!(recording.offer(&tiny, start), None);
        assert_eq!(recording.offer(&tiny, after(start, 250)), None);
        assert_eq!(
            recording.view(after(start, 250)).problem.as_deref(),
            Some(
                "The region was drawn when the window was 16 × 32. Scaled to 2 × 4, the window's \
                 size now, its text is too small to tell the characters apart. Make the window \
                 larger to record."
            )
        );
        assert!(recording.readings().is_empty());
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
        let view = Recording::new(
            Ticket::numbered(1),
            region,
            (RED, 7),
            (size(), Follows::Drawn),
            Instant::now(),
        )
        .view(Instant::now());
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
