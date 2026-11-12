//! Recording a value's region for its looks: a screenshot each time the
//! region shows something no screenshot shows yet.
//!
//! A state check needs a screenshot of each look, a bar's colors are picked
//! on screenshots that show it filled and empty, and a color on one that
//! shows it. Catching those moments by hand while using another application
//! is hard, so a recording watches the region instead. Each frame is cropped
//! to the region and compared with every look known so far, not only the
//! latest, so a look that comes back is not kept twice. The looks known
//! when a recording starts are the region on each screenshot the session
//! holds, so recording again keeps only what is new. A frame that shows a
//! new look is kept whole, as a screenshot like the ones the user takes, so
//! every tool that works on a screenshot works on it. Only the crop of each
//! frame is compared; the whole frame is copied only when it is kept.
//!
//! Each comparison is one pass over the region for every look known, so a
//! large region that shows many looks costs the owner thread the most.

use std::time::Instant;

use appytappy_config::app_file::MAX_STATES;
use appytappy_sources::screen::{Image, Region, Size};
use serde::Serialize;

use crate::screenshots::Screenshots;

use super::{Lifecycle, cut, millis};

/// The most screenshots one recording keeps: a look for each state a state
/// check can tell apart. Each is a whole window's frame held in memory at
/// four bytes a pixel, about 8 MB for a window of 1920 by 1080, so a region
/// that never settles must not keep one for every frame.
pub(crate) const MAX_LOOKS: usize = MAX_STATES;

/// Whether `shown` is the look `kept` shows: the average difference of
/// their red, green, and blue is at most `tolerance`. A state check compares
/// a region with a state's whole picture by the same rule, so two looks are
/// told apart here exactly when such a check with this tolerance would not
/// read one as the other. Pictures of different sizes are different looks.
pub(crate) fn same_look(kept: &Image, shown: &Image, tolerance: u8) -> bool {
    if kept.size() != shown.size() {
        return false;
    }
    let mut difference = 0_u64;
    for (kept, shown) in kept.pixels().iter().zip(shown.pixels()) {
        for (&kept, &shown) in kept.iter().zip(shown).take(3) {
            // At most 3 * 8192² channels of 255 each, far below u64::MAX.
            difference = difference.saturating_add(u64::from(kept.abs_diff(shown)));
        }
    }
    let channels = u64::try_from(kept.pixels().len())
        .unwrap_or(u64::MAX)
        .saturating_mul(3);
    difference <= channels.saturating_mul(u64::from(tolerance))
}

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
    /// The region as each known look shows it: on the screenshots the
    /// session held when it started, then on each frame it kept.
    looks: Vec<Image>,
    /// The screenshots it kept, in the order their looks first showed.
    kept: Vec<usize>,
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
            .collect();
        Self {
            id,
            region,
            tolerance,
            window_size,
            life: Lifecycle::new(started),
            looks,
            kept: Vec::new(),
        }
    }

    /// Records `frame`, a whole window's frame that arrived at `at`: when
    /// its region shows a look it does not know, keeps the frame in
    /// `screenshots`. Stops once it has kept [`MAX_LOOKS`].
    pub(crate) fn offer(&mut self, frame: &Image, at: Instant, screenshots: &mut Screenshots) {
        if !self.life.is_recording() {
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
        if self
            .looks
            .iter()
            .any(|look| same_look(look, &crop, self.tolerance))
        {
            return;
        }
        self.looks.push(crop);
        self.kept.push(screenshots.push(frame.clone()));
        if self.kept.len() >= MAX_LOOKS {
            self.life.fail(
                at,
                format!(
                    "Recording stopped: it kept {MAX_LOOKS} screenshots, as many as one \
                     recording keeps. Remove the ones you do not need, then start recording \
                     again to keep more."
                ),
            );
        }
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
    use appytappy_sources::screen::check::ImageMatch;

    use super::*;

    /// A `width` by 1 picture of `pixels`, each its blue, green, red, and
    /// unused fourth byte.
    fn picture(pixels: &[[u8; 4]]) -> Image {
        let width = u32::try_from(pixels.len()).expect("a short row");
        Image::new(Size { width, height: 1 }, pixels.concat()).expect("matching length")
    }

    #[test]
    fn a_look_is_the_same_within_the_average_difference_a_state_check_allows() {
        let kept = picture(&[[100, 100, 100, 0], [100, 100, 100, 0]]);
        // What it is compared with, the tolerance, and whether it is the
        // same look. Two pixels are six channels, so a tolerance of 10
        // allows a difference of 60 in all.
        let cases = [
            (picture(&[[100, 100, 100, 0], [100, 100, 100, 0]]), 0, true),
            (picture(&[[101, 100, 100, 0], [100, 100, 100, 0]]), 0, false),
            (picture(&[[160, 100, 100, 0], [100, 100, 100, 0]]), 10, true),
            (picture(&[[120, 80, 110, 0], [90, 100, 100, 0]]), 10, true),
            (
                picture(&[[161, 100, 100, 0], [100, 100, 100, 0]]),
                10,
                false,
            ),
            (picture(&[[120, 80, 110, 0], [90, 100, 101, 0]]), 10, false),
            // The fourth byte is not a color.
            (
                picture(&[[100, 100, 100, 255], [100, 100, 100, 255]]),
                0,
                true,
            ),
            (picture(&[[0, 0, 0, 0], [255, 255, 255, 0]]), 255, true),
        ];
        for (shown, tolerance, same) in cases {
            assert_eq!(
                same_look(&kept, &shown, tolerance),
                same,
                "{shown:?} within {tolerance}"
            );
            let state = ImageMatch::new(kept.clone(), None, tolerance).expect("no mask");
            assert_eq!(
                state.matches(&shown),
                Ok(same),
                "a state check agrees on {shown:?} within {tolerance}"
            );
        }
    }

    #[test]
    fn pictures_of_different_sizes_are_different_looks() {
        let kept = picture(&[[100, 100, 100, 0], [100, 100, 100, 0]]);
        let shorter = picture(&[[100, 100, 100, 0]]);
        assert!(!same_look(&kept, &shorter, 255));
        assert!(!same_look(&shorter, &kept, 255));
    }
}
