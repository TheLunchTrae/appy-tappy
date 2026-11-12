//! Finding a bar fill check's colors on the session's screenshots, and
//! showing what two colors measure on them.
//!
//! [`detect`] proposes the colors, or says why it proposes none, and
//! [`measure`] reads each screenshot with two colors and marks the pixels
//! each counts. Both look at the screenshots with the window size of the
//! one the region was drawn on, since the region means another spot on a
//! window of another size, and both go through the bar reader the value
//! reads with once it is saved.

use std::iter;

use appytappy_config::app_file::CheckDefError;
use appytappy_config::signal::Number;
use appytappy_sources::screen::check::{BarFill, BarPart};
use appytappy_sources::screen::detect::{Basis, NoBar, detect_bar};
use appytappy_sources::screen::{Image, Region};
use serde::Serialize;

use super::{ColorSpec, RegionSpec, SpecError, to_region};

/// What a bar's colors were found from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) enum BarBasis {
    /// The layout of the screenshot the region was drawn on.
    Layout,
    /// The pixels that change between the screenshots.
    Changes,
    /// Those pixels, of which the colors leave out more than a few.
    RoughChanges,
}

/// What looking for a bar in a region found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum BarDetection {
    /// The bar's two colors.
    Proposed {
        filled: ColorSpec,
        empty: ColorSpec,
        basis: BarBasis,
        /// The screenshots looked at, by index: those with the window size
        /// of the one the region was drawn on.
        screenshots: Vec<usize>,
    },
    /// No bar the check can read, and what the user can do about it.
    Declined { message: String },
}

/// What a screenshot's region reads with two colors.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct BarReading {
    /// The screenshot, by index.
    screenshot: usize,
    /// How full the bar reads, from 0 to 100, or `None` when no pixel of
    /// the region is near either color.
    fill: Option<f64>,
}

/// The pixels of a region two colors count, each as runs of pixels: the
/// index of a run's first pixel and of the pixel after its last, row by row
/// from the top of the region.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct BarMarks {
    /// The pixels that count as filled.
    filled: Vec<[usize; 2]>,
    /// The pixels that count as empty.
    empty: Vec<[usize; 2]>,
}

/// What two colors measure on the session's screenshots.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct BarMeasure {
    /// What each screenshot reads, in the order they were taken.
    readings: Vec<BarReading>,
    /// The pixels counted on the screenshot shown, or `None` when none is
    /// shown or it has another window size.
    marks: Option<BarMarks>,
}

/// What the user can do about a region with no bar to propose.
const fn advice(reason: NoBar) -> &'static str {
    match reason {
        NoBar::Tiny => {
            "The region is too small to find a bar in. Draw it around the whole bar, at least 12 pixels long."
        },
        NoBar::OneColor => {
            "No screenshot of this size shows the bar partly filled: the region is one color. Take a screenshot where the bar is partly filled, then detect again."
        },
        NoBar::AlikeColors => {
            "The region's two main colors are too close to tell apart as filled and empty. If the bar is full or empty here, take a screenshot where it is partly filled. Otherwise pick the colors by hand."
        },
        NoBar::NotABar => {
            "The region does not look like a bar: one color from the left, or from the bottom of a tall region, and another after it. Draw the region closely around the bar, or pick the colors by hand."
        },
        NoBar::Misread => {
            "The bar's two colors also show beside the bar, or part of the bar has neither, so the reading would be off. Draw the region closely around the bar, or pick the colors by hand."
        },
    }
}

/// `region` of each of `screenshots` with the window size of screenshot
/// `drawn_on`, by index.
fn frames(
    screenshots: &[Image],
    region: Region,
    drawn_on: usize,
) -> Result<Vec<(usize, Image)>, SpecError> {
    let size = screenshots
        .get(drawn_on)
        .ok_or(SpecError::NoScreenshot)?
        .size();
    if !region.fits(size) {
        return Err(SpecError::Outside);
    }
    let same_size = screenshots.iter().enumerate();
    Ok(same_size
        .filter(|(_, screenshot)| screenshot.size() == size)
        .filter_map(|(index, screenshot)| Some((index, screenshot.crop(region)?)))
        .collect())
}

/// The colors of the bar in `region`, which was drawn on screenshot
/// `drawn_on`, or why none are proposed.
///
/// # Errors
///
/// Returns [`SpecError::EmptyRegion`], [`SpecError::NoScreenshot`], or
/// [`SpecError::Outside`] when there is no such region to look at.
pub(crate) fn detect(
    screenshots: &[Image],
    region: RegionSpec,
    drawn_on: usize,
) -> Result<BarDetection, SpecError> {
    let frames = frames(screenshots, to_region(region)?, drawn_on)?;
    let indexes = frames.iter().map(|&(index, _)| index).collect();
    let (drawn, others): (Vec<_>, Vec<_>) = frames
        .into_iter()
        .partition(|&(index, _)| index == drawn_on);
    let (_, first) = drawn.first().ok_or(SpecError::NoScreenshot)?;
    let others: Vec<Image> = others.into_iter().map(|(_, image)| image).collect();
    Ok(match detect_bar(first, &others) {
        Ok(proposal) => BarDetection::Proposed {
            filled: proposal.bar.filled().into(),
            empty: proposal.bar.empty().into(),
            basis: match proposal.basis {
                Basis::Layout => BarBasis::Layout,
                Basis::Changes { rough: false } => BarBasis::Changes,
                Basis::Changes { rough: true } => BarBasis::RoughChanges,
            },
            screenshots: indexes,
        },
        Err(reason) => BarDetection::Declined {
            message: advice(reason).to_owned(),
        },
    })
}

/// What a bar of the colors `filled` and `empty` reads in `region` on each
/// screenshot, and which pixels it counts on screenshot `shown`.
///
/// # Errors
///
/// Returns what [`detect`] does, or [`CheckDefError::SameColors`] for two
/// colors that are the same.
pub(crate) fn measure(
    screenshots: &[Image],
    region: RegionSpec,
    drawn_on: usize,
    filled: ColorSpec,
    empty: ColorSpec,
    shown: Option<usize>,
) -> Result<BarMeasure, SpecError> {
    let bar = BarFill::new(filled.into(), empty.into())
        .map_err(|_same| SpecError::Check(CheckDefError::SameColors))?;
    let frames = frames(screenshots, to_region(region)?, drawn_on)?;
    let readings = frames
        .iter()
        .map(|(screenshot, image)| BarReading {
            screenshot: *screenshot,
            fill: bar.percent(image).ok().map(Number::raw),
        })
        .collect();
    let shown = frames.iter().find(|&&(index, _)| Some(index) == shown);
    Ok(BarMeasure {
        readings,
        marks: shown.map(|(_, image)| marks(bar, image)),
    })
}

/// The runs of pixels of `image` that `bar` counts as each part.
fn marks(bar: BarFill, image: &Image) -> BarMarks {
    let mut marks = BarMarks {
        filled: Vec::new(),
        empty: Vec::new(),
    };
    // The run that is open: its part and the index of its first pixel.
    let mut open: Option<(BarPart, usize)> = None;
    // A pixel of no part after the last one closes the last run.
    let parts = bar.parts(image).chain(iter::once(None));
    for (index, part) in parts.enumerate() {
        if open.map(|(part, _)| part) == part {
            continue;
        }
        if let Some((part, start)) = open {
            let runs = match part {
                BarPart::Filled => &mut marks.filled,
                BarPart::Empty => &mut marks.empty,
            };
            runs.push([start, index]);
        }
        open = part.map(|part| (part, index));
    }
    marks
}

#[cfg(test)]
mod tests {
    use appytappy_sources::screen::Size;
    use appytappy_sources::system::Rgb;
    use serde_json::json;

    use super::*;

    const GREEN: Rgb = Rgb {
        red: 40,
        green: 210,
        blue: 60,
    };
    const GRAY: Rgb = Rgb {
        red: 55,
        green: 55,
        blue: 70,
    };
    const WHITE: Rgb = Rgb {
        red: 255,
        green: 255,
        blue: 255,
    };

    /// A screenshot `width` wide and 4 high: white, but for a bar in the
    /// 20 by 2 region at (2, 1), green in its first `filled` columns and
    /// gray after them.
    fn screenshot(width: u32, filled: u32) -> Image {
        let mut bgra = Vec::new();
        for y in 0..4_u32 {
            for x in 0..width {
                let color = match (x, y) {
                    (2..22, 1..3) if x.saturating_sub(2) < filled => GREEN,
                    (2..22, 1..3) => GRAY,
                    _ => WHITE,
                };
                bgra.extend([color.blue, color.green, color.red, 0]);
            }
        }
        Image::new(Size { width, height: 4 }, bgra).expect("four bytes a pixel")
    }

    const BAR: RegionSpec = RegionSpec {
        x: 2,
        y: 1,
        width: 20,
        height: 2,
    };

    fn found(detection: Result<BarDetection, SpecError>) -> serde_json::Value {
        serde_json::to_value(detection.expect("a region to look at")).expect("plain data")
    }

    #[test]
    fn one_screenshot_proposes_its_layouts_colors() {
        let screenshots = [screenshot(30, 5), screenshot(40, 15)];
        assert_eq!(
            found(detect(&screenshots, BAR, 0)),
            json!({
                "kind": "proposed",
                "filled": {"red": 40, "green": 210, "blue": 60},
                "empty": {"red": 55, "green": 55, "blue": 70},
                "basis": "layout",
                "screenshots": [0],
            }),
            "the screenshot of another window size is left out"
        );
    }

    #[test]
    fn screenshots_of_one_size_propose_the_colors_that_change() {
        let screenshots = [
            screenshot(30, 20),
            screenshot(40, 10),
            screenshot(30, 5),
            screenshot(30, 15),
        ];
        // The region was drawn on a full bar, which alone shows one color.
        assert_eq!(
            found(detect(&screenshots, BAR, 0)),
            json!({
                "kind": "proposed",
                "filled": {"red": 40, "green": 210, "blue": 60},
                "empty": {"red": 55, "green": 55, "blue": 70},
                "basis": "changes",
                "screenshots": [0, 2, 3],
            })
        );
        assert_eq!(
            detect(&screenshots[..2], BAR, 0),
            Ok(BarDetection::Declined {
                message: advice(NoBar::OneColor).to_owned()
            })
        );
    }

    #[test]
    fn rough_changes_are_named() {
        // Three of the ten pixels that turn green in the bar's first row,
        // pixels 37 to 39 of the screenshot, turn white instead.
        let mut bgra = screenshot(30, 15).as_bgra().to_vec();
        for byte in &mut bgra[148..160] {
            *byte = 255;
        }
        let odd = Image::new(
            Size {
                width: 30,
                height: 4,
            },
            bgra,
        )
        .expect("the same size");
        let screenshots = [screenshot(30, 5), odd];
        assert_eq!(found(detect(&screenshots, BAR, 0))["basis"], "roughChanges");
    }

    #[test]
    fn the_layout_is_that_of_the_screenshot_the_region_was_drawn_on() {
        // Too few pixels change between these to go by, and only the first
        // shows enough of the filled part.
        let screenshots = [screenshot(30, 3), screenshot(30, 1)];
        assert_eq!(found(detect(&screenshots, BAR, 0))["basis"], "layout");
        assert_eq!(
            detect(&screenshots, BAR, 1),
            Ok(BarDetection::Declined {
                message: advice(NoBar::OneColor).to_owned()
            })
        );
    }

    #[test]
    fn each_reason_tells_the_user_what_to_do() {
        let reasons = [
            (NoBar::Tiny, "at least 12 pixels long"),
            (
                NoBar::OneColor,
                "Take a screenshot where the bar is partly filled, then detect again.",
            ),
            (NoBar::AlikeColors, "too close to tell apart"),
            (NoBar::NotABar, "does not look like a bar"),
            (NoBar::Misread, "also show beside the bar"),
        ];
        for (reason, words) in reasons {
            assert!(advice(reason).contains(words), "{reason:?}");
        }
        let tiny = RegionSpec { width: 11, ..BAR };
        assert_eq!(
            detect(&[screenshot(30, 5)], tiny, 0),
            Ok(BarDetection::Declined {
                message: advice(NoBar::Tiny).to_owned()
            })
        );
    }

    #[test]
    fn a_region_that_cannot_be_looked_at_is_refused() {
        let screenshots = [screenshot(30, 5)];
        let empty = RegionSpec { width: 0, ..BAR };
        assert_eq!(detect(&screenshots, empty, 0), Err(SpecError::EmptyRegion));
        assert_eq!(detect(&screenshots, BAR, 1), Err(SpecError::NoScreenshot));
        let wide = RegionSpec { width: 29, ..BAR };
        assert_eq!(detect(&screenshots, wide, 0), Err(SpecError::Outside));
        let green = ColorSpec::from(GREEN);
        assert_eq!(
            measure(&screenshots, BAR, 0, green, green, None),
            Err(SpecError::Check(CheckDefError::SameColors))
        );
        assert_eq!(
            measure(&screenshots, wide, 0, green, GRAY.into(), None),
            Err(SpecError::Outside)
        );
    }

    #[test]
    fn two_colors_are_read_on_each_screenshot_and_marked_on_the_one_shown() {
        let screenshots = [screenshot(30, 5), screenshot(40, 10), screenshot(30, 15)];
        // One pixel wider and taller than the bar, so it holds white too.
        let region = RegionSpec {
            x: 2,
            y: 1,
            width: 21,
            height: 3,
        };
        let measured =
            measure(&screenshots, region, 0, GREEN.into(), GRAY.into(), Some(2)).expect("a region");
        assert_eq!(
            serde_json::to_value(measured).expect("plain data"),
            json!({
                "readings": [
                    {"screenshot": 0, "fill": 25.0},
                    {"screenshot": 2, "fill": 75.0},
                ],
                // Rows of 21: 15 filled, 5 empty, one white; the last row is white.
                "marks": {
                    "filled": [[0, 15], [21, 36]],
                    "empty": [[15, 20], [36, 41]],
                },
            })
        );
        // The colors the other way around read the rest, and a screenshot
        // of another size is not marked.
        let measured =
            measure(&screenshots, region, 0, GRAY.into(), GREEN.into(), Some(1)).expect("a region");
        assert_eq!(
            serde_json::to_value(measured).expect("plain data"),
            json!({
                "readings": [
                    {"screenshot": 0, "fill": 75.0},
                    {"screenshot": 2, "fill": 25.0},
                ],
                "marks": null,
            })
        );
    }

    #[test]
    fn a_region_with_neither_color_reads_nothing_and_marks_run_to_its_end() {
        let screenshots = [screenshot(30, 5)];
        let corner = RegionSpec {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        };
        let measured =
            measure(&screenshots, corner, 0, GREEN.into(), GRAY.into(), Some(0)).expect("a region");
        assert_eq!(
            serde_json::to_value(measured).expect("plain data"),
            json!({
                "readings": [{"screenshot": 0, "fill": null}],
                "marks": {"filled": [], "empty": []},
            })
        );
        // The last pixel of the region ends a run.
        let measured =
            measure(&screenshots, BAR, 0, GREEN.into(), GRAY.into(), Some(0)).expect("a region");
        assert_eq!(
            serde_json::to_value(measured).expect("plain data")["marks"],
            json!({"filled": [[0, 5], [20, 25]], "empty": [[5, 20], [25, 40]]})
        );
    }
}
