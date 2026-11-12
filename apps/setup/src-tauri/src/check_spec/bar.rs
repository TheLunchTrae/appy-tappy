//! Finding a bar fill check's colors on the session's screenshots, and
//! showing what two colors measure on them.
//!
//! [`detect`] proposes the colors, or says why it proposes none, and
//! [`measure`] reads each screenshot with two colors and marks the pixels
//! each counts. Both look at screenshots with the window size of the one
//! the region was drawn on, since the region means another spot on a window
//! of another size: that one and the newest [`MAX_OTHERS`] others, which
//! bounds what a call costs the owner thread. Both go through the bar reader
//! the value reads with once it is saved.

use std::iter;

use appytappy_config::app_file::CheckDefError;
use appytappy_config::signal::Number;
use appytappy_sources::screen::check::{BarFill, BarPart};
use appytappy_sources::screen::detect::{MIN_LENGTH, NoBar, detect_bar};
use appytappy_sources::screen::{Image, Region};
use serde::Serialize;

use super::{ColorSpec, RegionSpec, SpecError, to_region};

/// The most screenshots looked at besides the one the region was drawn on.
/// Each one costs a copy of the region and a few passes over it.
const MAX_OTHERS: usize = 5;

/// The most runs a layer of marks carries. Runs go to the UI as JSON, and a
/// dithered region has a run for every other pixel, so the cap bounds what
/// one answer costs to send and to read. A layer with more carries only its
/// count, and the UI says that it marks none of them.
const MAX_RUNS: usize = 500_000;

/// What looking for a bar in a region found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum BarDetection {
    /// The bar's two colors. Screenshots are named by index.
    #[serde(rename_all = "camelCase")]
    Proposed {
        filled: ColorSpec,
        empty: ColorSpec,
        /// The screenshot whose layout gave the colors.
        found_on: usize,
        /// The screenshots that show the bar in these colors, full, empty,
        /// or in between.
        shown_on: Vec<usize>,
        /// The screenshots looked at that show neither color.
        without: Vec<usize>,
        /// Whether two screenshots show both colors at different fills.
        moves: bool,
        /// Whether the colors read a screenshot a few points off where the
        /// bar ends on it.
        rough: bool,
        /// Whether the colors were found over only the strips of the
        /// region that show the bar from end to end, past a label, an icon,
        /// or what lies beside the bar.
        around: bool,
        /// How many older screenshots of the window size were not looked
        /// at.
        left_out: usize,
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

/// The pixels of one part in a region.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct Layer {
    /// How many pixels count as this part.
    count: usize,
    /// The runs of them in the region, each as the index of its first pixel
    /// and of the pixel after its last, row by row from the top, as a
    /// selection's are. Missing when there are too many runs to draw.
    spans: Option<Vec<[usize; 2]>>,
}

impl Layer {
    /// A layer with no pixel yet.
    const fn new() -> Self {
        Self {
            count: 0,
            spans: Some(Vec::new()),
        }
    }

    /// Adds the run of pixels from `start` up to `end`, which fills the
    /// layer once it holds `most` runs.
    fn push(&mut self, start: usize, end: usize, most: usize) {
        self.count = self.count.saturating_add(end.saturating_sub(start));
        if self.spans.as_ref().is_some_and(|spans| spans.len() >= most) {
            self.spans = None;
        }
        if let Some(spans) = &mut self.spans {
            spans.push([start, end]);
        }
    }
}

/// The pixels of a region that two colors count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct BarMarks {
    /// The pixels that count as filled.
    filled: Layer,
    /// The pixels that count as empty.
    empty: Layer,
}

/// What two colors measure on the session's screenshots.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct BarMeasure {
    /// What each screenshot looked at reads, in the order they were taken.
    readings: Vec<BarReading>,
    /// The pixels counted on the screenshot shown, or `None` when none is
    /// shown or it is not one of those looked at.
    marks: Option<BarMarks>,
    /// How many older screenshots of the window size were not looked at.
    left_out: usize,
}

/// What the user can do about a region with no bar to propose. `drawn_on`
/// is the screenshot the region was drawn on, and `screenshot` gives the
/// screenshot the detector numbered. Screenshots are named as the UI
/// numbers them, from 1.
fn advice(reason: NoBar, drawn_on: usize, screenshot: impl Fn(usize) -> usize) -> String {
    let drawn = drawn_on.saturating_add(1);
    match reason {
        NoBar::Tiny => format!(
            "The region is too small to find a bar in. Draw it around the whole bar, at least {MIN_LENGTH} pixels long."
        ),
        NoBar::OneColor => format!(
            "On screenshot {drawn}, the region is one color, and no other screenshot looked at shows a bar in it. Take a screenshot where the bar is partly filled, then detect again."
        ),
        NoBar::AlikeColors => format!(
            "On screenshot {drawn}, the region's two main colors are too close to tell apart as filled and empty. If the bar is full or empty there, take a screenshot where it is partly filled. Otherwise pick the colors by hand."
        ),
        NoBar::NotABar => format!(
            "On screenshot {drawn}, the region does not look like a bar: one plain color from the left, or from the bottom of a tall region, and another plain color after it. If a label or an icon covers most of the bar's height, draw the region over a strip of the bar above or below it. Otherwise draw the region closely around the bar, or pick the colors by hand."
        ),
        NoBar::Misread { image } => {
            let other = screenshot(image).saturating_add(1);
            format!(
                "On screenshot {other}, the two colors found do not read the region as full as the bar looks: they also show beside the bar, or part of the bar has another shade. Draw the region closely around the bar, or pick the colors by hand."
            )
        },
        NoBar::Uneven { image } => {
            let other = screenshot(image).saturating_add(1);
            format!(
                "On screenshot {other}, something lies over part of the bar, such as a label, an icon, or a shine in another shade. The two colors read that screenshot right, but would read other fills wrong. Draw the region over a strip of the bar that is clear from end to end, or pick the colors by hand."
            )
        },
        NoBar::Differs { found_on, image } => {
            let found = screenshot(found_on).saturating_add(1);
            let other = screenshot(image).saturating_add(1);
            format!(
                "The colors of the bar on screenshot {found} do not read screenshot {other}, which shows one of them but not the bar in both. A bar that changes color as it fills, or from one screenshot to the next, cannot be read with two colors. If a label, an icon, or something else covers most of the bar's height on screenshot {other}, draw the region over a strip of the bar above or below it, or pick the colors by hand."
            )
        },
    }
}

/// A sentence that says `left_out` older screenshots were not looked at,
/// with the space before it, or nothing when none were.
fn left_out_note(left_out: usize) -> String {
    match left_out {
        0 => String::new(),
        1 => " One older screenshot of this size was not looked at.".to_owned(),
        _ => format!(" {left_out} older screenshots of this size were not looked at."),
    }
}

/// `region` of the screenshots looked at, each with its index, in the
/// order they were taken, and how many were left out: of `screenshots`
/// with the window size of screenshot `drawn_on`, that one and the newest
/// `most` others.
fn frames(
    screenshots: &[Image],
    region: Region,
    drawn_on: usize,
    most: usize,
) -> Result<(Vec<(usize, Image)>, usize), SpecError> {
    let size = screenshots
        .get(drawn_on)
        .ok_or(SpecError::NoScreenshot)?
        .size();
    if !region.fits(size) {
        return Err(SpecError::Outside);
    }
    let same_size = screenshots.iter().enumerate();
    let others: Vec<usize> = same_size
        .filter(|&(index, screenshot)| index != drawn_on && screenshot.size() == size)
        .map(|(index, _)| index)
        .collect();
    let left_out = others.len().saturating_sub(most);
    let mut looked_at: Vec<usize> = others.into_iter().skip(left_out).collect();
    looked_at.push(drawn_on);
    looked_at.sort_unstable();
    let frames = looked_at
        .into_iter()
        .filter_map(|index| Some((index, screenshots.get(index)?.crop(region)?)))
        .collect();
    Ok((frames, left_out))
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
    let (frames, left_out) = frames(screenshots, to_region(region)?, drawn_on, MAX_OTHERS)?;
    let (drawn, others): (Vec<_>, Vec<_>) = frames
        .into_iter()
        .partition(|&(index, _)| index == drawn_on);
    let (_, first) = drawn.first().ok_or(SpecError::NoScreenshot)?;
    // The detector numbers the drawn-on screenshot 0 and the others from 1.
    let numbered: Vec<usize> = iter::once(drawn_on)
        .chain(others.iter().map(|&(index, _)| index))
        .collect();
    let screenshot = |number: usize| numbered.get(number).copied().unwrap_or(drawn_on);
    let others: Vec<Image> = others.into_iter().map(|(_, image)| image).collect();
    Ok(match detect_bar(first, &others) {
        Ok(proposal) => {
            let mut shown_on: Vec<usize> = proposal
                .shown_on
                .iter()
                .map(|&shown| screenshot(shown))
                .collect();
            shown_on.sort_unstable();
            let mut without = numbered.clone();
            without.retain(|index| !shown_on.contains(index));
            without.sort_unstable();
            BarDetection::Proposed {
                filled: proposal.bar.filled().into(),
                empty: proposal.bar.empty().into(),
                found_on: screenshot(proposal.found_on),
                shown_on,
                without,
                moves: proposal.moves,
                rough: proposal.rough,
                around: proposal.around,
                left_out,
            }
        },
        Err(reason) => BarDetection::Declined {
            message: advice(reason, drawn_on, screenshot) + &left_out_note(left_out),
        },
    })
}

/// What a bar of the colors `filled` and `empty` reads in `region` on each
/// screenshot looked at, and which pixels it counts on screenshot `shown`.
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
    let (frames, left_out) = frames(screenshots, to_region(region)?, drawn_on, MAX_OTHERS)?;
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
        marks: shown.map(|(_, image)| marks(bar, image, MAX_RUNS)),
        left_out,
    })
}

/// The pixels of `image` that `bar` counts as each part, in layers of at
/// most `most` runs.
fn marks(bar: BarFill, image: &Image, most: usize) -> BarMarks {
    let mut marks = BarMarks {
        filled: Layer::new(),
        empty: Layer::new(),
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
            let layer = match part {
                BarPart::Filled => &mut marks.filled,
                BarPart::Empty => &mut marks.empty,
            };
            layer.push(start, index, most);
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
    /// 20 by 2 region at (2, 1), `fill` in its first `filled` columns and
    /// gray after them.
    fn shot(width: u32, filled: u32, fill: Rgb) -> Image {
        let mut bgra = Vec::new();
        for y in 0..4_u32 {
            for x in 0..width {
                let color = match (x, y) {
                    (2..22, 1..3) if x.saturating_sub(2) < filled => fill,
                    (2..22, 1..3) => GRAY,
                    _ => WHITE,
                };
                bgra.extend([color.blue, color.green, color.red, 0]);
            }
        }
        Image::new(Size { width, height: 4 }, bgra).expect("four bytes a pixel")
    }

    /// A screenshot whose bar is green.
    fn screenshot(width: u32, filled: u32) -> Image {
        shot(width, filled, GREEN)
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

    fn declined(detection: Result<BarDetection, SpecError>) -> String {
        match detection {
            Ok(BarDetection::Declined { message }) => message,
            other => panic!("not declined: {other:?}"),
        }
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
                "foundOn": 0,
                "shownOn": [0],
                "without": [],
                "moves": false,
                "rough": false,
                "around": false,
                "leftOut": 0,
            }),
            "the screenshot of another window size is not looked at"
        );
    }

    #[test]
    fn the_colors_come_from_a_screenshot_that_shows_both_parts_and_read_the_others() {
        let white = shot(30, 0, WHITE);
        let mut screenshots = vec![
            screenshot(30, 20),
            screenshot(40, 10),
            screenshot(30, 5),
            screenshot(30, 15),
        ];
        // Screenshot 4 shows neither color in the region.
        screenshots.push(shot(30, 20, WHITE));
        assert_ne!(screenshots[4], white, "its bar is white, not its track");
        // The region was drawn on a full bar, which alone shows one color.
        assert_eq!(
            found(detect(&screenshots, BAR, 0)),
            json!({
                "kind": "proposed",
                "filled": {"red": 40, "green": 210, "blue": 60},
                "empty": {"red": 55, "green": 55, "blue": 70},
                "foundOn": 2,
                "shownOn": [0, 2, 3],
                "without": [4],
                "moves": true,
                "rough": false,
                "around": false,
                "leftOut": 0,
            })
        );
        assert_eq!(
            declined(detect(&screenshots[..2], BAR, 0)),
            "On screenshot 1, the region is one color, and no other screenshot looked at shows a bar in it. Take a screenshot where the bar is partly filled, then detect again."
        );
        // Drawn on a later screenshot, the others keep their own numbers.
        assert_eq!(found(detect(&screenshots, BAR, 3))["foundOn"], 3);
        assert_eq!(
            found(detect(&screenshots, BAR, 3))["shownOn"],
            json!([0, 2, 3])
        );
    }

    #[test]
    fn a_bar_that_changes_color_is_declined_by_the_screenshots_names() {
        let red = Rgb {
            red: 220,
            green: 30,
            blue: 30,
        };
        let screenshots = [screenshot(30, 15), screenshot(40, 10), shot(30, 5, red)];
        assert_eq!(
            declined(detect(&screenshots, BAR, 2)),
            "The colors of the bar on screenshot 3 do not read screenshot 1, which shows one of them but not the bar in both. A bar that changes color as it fills, or from one screenshot to the next, cannot be read with two colors. If a label, an icon, or something else covers most of the bar's height on screenshot 1, draw the region over a strip of the bar above or below it, or pick the colors by hand."
        );
    }

    #[test]
    fn a_bar_found_around_a_label_says_so() {
        // Three pixels of the bar's first row are black, as a label's are:
        // pixels 44 to 46 of the screenshot, in its empty part. Only the
        // bar's second row shows it from end to end.
        let mut bgra = screenshot(30, 5).as_bgra().to_vec();
        bgra[176..188].fill(0);
        let labeled = Image::new(
            Size {
                width: 30,
                height: 4,
            },
            bgra,
        )
        .expect("the same size");
        let detection = found(detect(&[labeled], BAR, 0));
        assert_eq!(detection["around"], true);
        assert_eq!(
            detection["filled"],
            json!({"red": 40, "green": 210, "blue": 60})
        );
        let strips = "draw the region over a strip of the bar above or below it";
        assert!(advice(NoBar::NotABar, 0, |number| number).contains(strips));
    }

    #[test]
    fn a_bar_with_something_over_part_of_it_is_declined_by_the_screenshots_name() {
        // The first six columns of the bar's first row are white, on the
        // second screenshot: pixels 32 to 37. At the fill it shows, the
        // reader is 4 points off, and a fill that ends after those columns
        // would read 18 for 30.
        let mut bgra = screenshot(30, 15).as_bgra().to_vec();
        bgra[128..152].fill(255);
        let covered = Image::new(
            Size {
                width: 30,
                height: 4,
            },
            bgra,
        )
        .expect("the same size");
        assert_eq!(
            declined(detect(&[screenshot(30, 15), covered], BAR, 0)),
            "On screenshot 2, something lies over part of the bar, such as a label, an icon, or a shine in another shade. The two colors read that screenshot right, but would read other fills wrong. Draw the region over a strip of the bar that is clear from end to end, or pick the colors by hand."
        );
    }

    #[test]
    fn a_reading_a_few_points_off_is_named_rough() {
        // Two of the bar's forty pixels, in different columns of its filled
        // part, are white: pixels 32 and 63 of the screenshot. So the reader
        // is 2.6 points off the half that the layout shows.
        let mut bgra = screenshot(30, 10).as_bgra().to_vec();
        for pixel in [128..132, 252..256] {
            bgra[pixel].fill(255);
        }
        let dented = Image::new(
            Size {
                width: 30,
                height: 4,
            },
            bgra,
        )
        .expect("the same size");
        assert_eq!(found(detect(&[dented], BAR, 0))["rough"], true);
        assert_eq!(found(detect(&[screenshot(30, 10)], BAR, 0))["rough"], false);
    }

    #[test]
    fn each_reason_tells_the_user_what_to_do() {
        let same = |number: usize| number;
        let reasons = [
            (NoBar::Tiny, "at least 12 pixels long"),
            (NoBar::OneColor, "On screenshot 3, the region is one color"),
            (
                NoBar::AlikeColors,
                "On screenshot 3, the region's two main colors are too close",
            ),
            (
                NoBar::NotABar,
                "On screenshot 3, the region does not look like a bar",
            ),
            (
                NoBar::Misread { image: 2 },
                "On screenshot 3, the two colors found do not read the region as full as the bar looks",
            ),
        ];
        for (reason, words) in reasons {
            assert!(advice(reason, 2, same).contains(words), "{reason:?}");
        }
        let tiny = RegionSpec { width: 11, ..BAR };
        assert_eq!(
            declined(detect(&[screenshot(30, 5)], tiny, 0)),
            advice(NoBar::Tiny, 0, same)
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
    fn only_the_newest_screenshots_are_looked_at_beside_the_one_drawn_on() {
        // Ten screenshots of one size, each a column fuller, and one of
        // another size among them.
        let mut screenshots: Vec<Image> = (5..15).map(|filled| screenshot(30, filled)).collect();
        screenshots.insert(4, screenshot(40, 10));
        let (frames, left_out) = frames(
            &screenshots,
            to_region(BAR).expect("a region"),
            1,
            MAX_OTHERS,
        )
        .expect("a region on the screenshot");
        let looked_at: Vec<usize> = frames.iter().map(|&(index, _)| index).collect();
        assert_eq!(looked_at, [1, 6, 7, 8, 9, 10]);
        assert_eq!(left_out, 4);
        let detection = found(detect(&screenshots, BAR, 1));
        assert_eq!(detection["shownOn"], json!([1, 6, 7, 8, 9, 10]));
        assert_eq!(detection["leftOut"], 4);
        let measured =
            measure(&screenshots, BAR, 1, GREEN.into(), GRAY.into(), Some(0)).expect("a region");
        let measured = serde_json::to_value(measured).expect("plain data");
        assert_eq!(measured["leftOut"], 4);
        assert_eq!(
            measured["readings"][0],
            json!({"screenshot": 1, "fill": 30.0})
        );
        assert_eq!(measured["readings"].as_array().map(Vec::len), Some(6));
        assert_eq!(measured["marks"], json!(null), "screenshot 0 is left out");
        // A refusal says so too.
        let full: Vec<Image> = (0..8).map(|_| screenshot(30, 20)).collect();
        assert!(
            declined(detect(&full, BAR, 0))
                .ends_with("detect again. 2 older screenshots of this size were not looked at.")
        );
        assert!(
            declined(detect(&full[..7], BAR, 0))
                .ends_with("detect again. One older screenshot of this size was not looked at.")
        );
        assert!(declined(detect(&full[..6], BAR, 0)).ends_with("detect again."));
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
                    "filled": {"count": 30, "spans": [[0, 15], [21, 36]]},
                    "empty": {"count": 10, "spans": [[15, 20], [36, 41]]},
                },
                "leftOut": 0,
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
                "leftOut": 0,
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
                "marks": {
                    "filled": {"count": 0, "spans": []},
                    "empty": {"count": 0, "spans": []},
                },
                "leftOut": 0,
            })
        );
        // The last pixel of the region ends a run.
        let measured =
            measure(&screenshots, BAR, 0, GREEN.into(), GRAY.into(), Some(0)).expect("a region");
        assert_eq!(
            serde_json::to_value(measured).expect("plain data")["marks"],
            json!({
                "filled": {"count": 10, "spans": [[0, 5], [20, 25]]},
                "empty": {"count": 30, "spans": [[5, 20], [25, 40]]},
            })
        );
    }

    #[test]
    fn a_layer_with_too_many_runs_carries_only_its_count() {
        let bar = BarFill::new(GREEN, GRAY).expect("two colors");
        let image = screenshot(30, 5)
            .crop(to_region(BAR).expect("a region"))
            .expect("inside the screenshot");
        let layers = |most| serde_json::to_value(marks(bar, &image, most)).expect("plain data");
        // Each part has two runs, one in each row.
        assert_eq!(
            layers(2),
            json!({
                "filled": {"count": 10, "spans": [[0, 5], [20, 25]]},
                "empty": {"count": 30, "spans": [[5, 20], [25, 40]]},
            })
        );
        assert_eq!(
            layers(1),
            json!({
                "filled": {"count": 10, "spans": null},
                "empty": {"count": 30, "spans": null},
            })
        );
    }
}
