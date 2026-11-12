//! Recording a value's region for its looks: a screenshot each time the
//! region shows something no screenshot shows yet.
//!
//! A state check needs a screenshot of each look, a bar's colors are picked
//! on screenshots that show it filled and empty, and a color on one that
//! shows it. Catching those moments by hand while using another application
//! is hard, so a recording watches the region instead. A frame is cropped to
//! the region and compared with every look known so far, not only the
//! latest, so a look that comes back is not kept twice. The looks known
//! when a recording starts are the region on each screenshot the session
//! holds, so recording again keeps only what is new. A frame that shows a
//! new look is kept whole, as a screenshot like the ones the user takes, so
//! every tool that works on a screenshot works on it. Only the crop of a
//! frame is compared; the whole frame is copied only when it is kept.
//!
//! A look is the whole region: the comparison is [`ImageMatch`]'s, the one
//! a state check makes with a state's whole picture. It knows nothing of
//! the selections that let a state check ignore a background. So where the
//! background behind the thing changes, most frames count as new looks and
//! the recording soon stops at its limit, and a small change inside a large
//! region may be within the tolerance and never be kept.
//!
//! A comparison is one pass over the region for every look known, on the
//! owner thread, which also answers the UI. After each one the recording
//! rests [`REST`] times as long as the frame took from arriving to compared,
//! and compares no frame until then, so whatever the region's size and the
//! number of looks, comparing takes a bounded share of the thread's time
//! and frames cannot pile up behind it.

use std::time::Instant;

use appytappy_config::app_file::MAX_STATES;
use appytappy_sources::screen::check::ImageMatch;
use appytappy_sources::screen::{Image, Region, Size};
use serde::Serialize;

use crate::screenshots::Screenshots;

use super::{Lifecycle, cut, millis};

/// The most screenshots one recording keeps: a look for each state a state
/// check can tell apart. Each is a whole window's frame held in memory at
/// four bytes a pixel, about 8 MB for a window of 1920 by 1080, so a region
/// that never settles must not keep one for every frame.
pub(crate) const MAX_LOOKS: usize = MAX_STATES;

/// How many times as long as a frame took, from arriving to compared, a
/// recording rests before it compares another. Comparing then takes at most
/// one part in five of the owner thread's time, and less while the thread
/// is behind, since a frame that waited took longer.
pub(crate) const REST: u32 = 4;

/// A recording of the looks one region shows.
#[derive(Debug)]
pub(crate) struct LookRecording {
    /// Its number in the session, so the UI tells it from one it replaced.
    id: u64,
    /// The region of the window it records.
    region: Region,
    /// How far, on average, a crop may be from a look and still be that
    /// look.
    tolerance: u8,
    /// The window's size when the region was drawn; a frame of another size
    /// shows other pixels there.
    window_size: Size,
    /// When it records, and why the latest frame could not be recorded or
    /// why it stopped by itself.
    pub(crate) life: Lifecycle,
    /// Each known look, as a comparison with the region as it shows it: on
    /// the screenshots the session held when it started, then on each frame
    /// it kept.
    looks: Vec<ImageMatch>,
    /// The screenshots it kept, in the order their looks first showed.
    kept: Vec<usize>,
    /// Until when it compares no frame, after its last comparison.
    rests_until: Option<Instant>,
}

impl LookRecording {
    /// Starts recording `region`, drawn on a window of `window_size`, where
    /// a crop within `tolerance` of a look is that look. The looks it knows
    /// already are the region on each of `screenshots` of that window size.
    pub(crate) fn new(
        id: u64,
        region: Region,
        tolerance: u8,
        window_size: Size,
        screenshots: &Screenshots,
        started: Instant,
    ) -> Self {
        let looks = screenshots
            .iter()
            .filter(|screenshot| screenshot.size() == window_size)
            .filter_map(|screenshot| screenshot.crop(region))
            .map(|look| ImageMatch::whole(look, tolerance))
            .collect();
        Self {
            id,
            region,
            tolerance,
            window_size,
            life: Lifecycle::new(started),
            looks,
            kept: Vec::new(),
            rests_until: None,
        }
    }

    /// Records `frame`, a whole window's frame that arrived at `arrived`,
    /// unless it rests: when the frame's region shows a look it does not
    /// know, keeps the frame in `screenshots`, and stops once it has kept
    /// [`MAX_LOOKS`]. `now` reads the time, before the comparison to tell
    /// whether it rests and after it to tell how long the frame took, which
    /// sets how long it rests next.
    pub(crate) fn offer(
        &mut self,
        frame: &Image,
        arrived: Instant,
        now: impl Fn() -> Instant,
        screenshots: &mut Screenshots,
    ) {
        if !self.life.is_recording() {
            return;
        }
        if self.rests_until.is_some_and(|until| now() < until) {
            return;
        }
        let crop = match cut(frame, self.region, self.window_size) {
            Ok(crop) => crop,
            Err(problem) => {
                self.life.set_problem(Some(problem));
                return;
            },
        };
        self.life.set_problem(None);
        // Every crop is of the one region, so none differs in size from a
        // look, which is the only way a comparison fails.
        if !self
            .looks
            .iter()
            .any(|look| look.matches(&crop) == Ok(true))
        {
            self.looks.push(ImageMatch::whole(crop, self.tolerance));
            self.kept.push(screenshots.push(frame.clone()));
            if self.kept.len() >= MAX_LOOKS {
                self.life.fail(
                    arrived,
                    format!(
                        "Recording stopped: it kept {MAX_LOOKS} screenshots, as many as one \
                         recording keeps. Remove the ones you do not need, then start recording \
                         again to keep more."
                    ),
                );
            }
        }
        let compared = now();
        let took = compared.saturating_duration_since(arrived);
        self.rests_until = compared.checked_add(took.saturating_mul(REST));
    }

    /// What the UI shows of it at `now`.
    pub(crate) fn view(&self, now: Instant) -> LookRecordingView {
        LookRecordingView {
            id: self.id,
            recording: self.life.is_recording(),
            elapsed_ms: millis(self.life.elapsed(now)),
            problem: self.life.problem().map(str::to_owned),
            kept: self.kept.clone(),
        }
    }
}

/// A recording of looks as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct LookRecordingView {
    /// Its number in the session.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) id: u64,
    /// Whether it still records.
    pub(crate) recording: bool,
    /// How long it has recorded, or recorded in all once stopped.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) elapsed_ms: u64,
    /// Why the latest frame could not be recorded, or why it stopped by
    /// itself.
    pub(crate) problem: Option<String>,
    /// The indexes of the screenshots it kept, in the order their looks
    /// first showed.
    pub(crate) kept: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// A 2 by 1 frame whose pixels have `shade` for red, green, and blue.
    fn shade(shade: u8) -> Image {
        Image::new(
            Size {
                width: 2,
                height: 1,
            },
            [shade, shade, shade, 0].repeat(2),
        )
        .expect("matching length")
    }

    /// A recording of the whole 2 by 1 frame that knows no look, where
    /// shades at most 10 apart are one look.
    fn recording(started: Instant) -> LookRecording {
        let region = Region::new(0, 0, 2, 1).expect("not empty");
        let size = Size {
            width: 2,
            height: 1,
        };
        LookRecording::new(1, region, 10, size, &Screenshots::new(), started)
    }

    #[test]
    fn it_rests_four_times_as_long_as_a_frame_took_and_compares_none_until_then() {
        let start = Instant::now();
        let after = |millis: u64| start + Duration::from_millis(millis);
        let mut screenshots = Screenshots::new();
        let mut recording = recording(start);
        // The frame arrived at 0 and was compared by 10, so it took 10 and
        // the recording rests until 50.
        recording.offer(&shade(0), start, || after(10), &mut screenshots);
        assert_eq!(recording.view(start).kept, [0]);
        recording.offer(&shade(100), after(20), || after(49), &mut screenshots);
        assert_eq!(
            recording.view(start).kept,
            [0],
            "a frame handled while it rests is not compared"
        );
        // Handled at 50, the same look is new. It arrived at 48, so it took
        // 2 and the recording rests until 58.
        recording.offer(&shade(100), after(48), || after(50), &mut screenshots);
        assert_eq!(recording.view(start).kept, [0, 1]);
        recording.offer(&shade(200), after(57), || after(57), &mut screenshots);
        assert_eq!(recording.view(start).kept, [0, 1]);
        // A frame that shows a known look is compared too, so it also sets
        // the rest: this one took 30, until 180.
        recording.offer(&shade(100), after(30), || after(60), &mut screenshots);
        recording.offer(&shade(200), after(179), || after(179), &mut screenshots);
        assert_eq!(recording.view(start).kept, [0, 1]);
        recording.offer(&shade(200), after(180), || after(180), &mut screenshots);
        assert_eq!(recording.view(start).kept, [0, 1, 2]);
    }

    #[test]
    fn a_frame_of_another_size_costs_no_rest() {
        let start = Instant::now();
        let later = start + Duration::from_secs(1);
        let mut screenshots = Screenshots::new();
        let mut recording = recording(start);
        let wider = Image::new(
            Size {
                width: 3,
                height: 1,
            },
            [0, 0, 0, 0].repeat(3),
        )
        .expect("matching length");
        // It arrived a second ago, but nothing was compared.
        recording.offer(&wider, start, || later, &mut screenshots);
        assert!(recording.view(later).problem.is_some());
        recording.offer(&shade(0), later, || later, &mut screenshots);
        assert_eq!(recording.view(later).kept, [0]);
        assert_eq!(recording.view(later).problem, None);
    }
}
