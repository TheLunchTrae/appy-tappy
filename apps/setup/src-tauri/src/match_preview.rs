//! Which pixels of a screenshot a color setting takes, for the UI to tint
//! while the user picks the color and moves its tolerance.
//!
//! Each rule asks the function its reader asks: a reference's characters
//! [`DigitReader::is_ink`], a bar [`BarFill::classify`], and a color check
//! [`Rgb::near`]. So the tint shows what the reader will take, at the
//! tolerance's edge too.
//!
//! A color check compares its region's average color, not each pixel, so its
//! answer carries that average, how far it is, and what the check reads on
//! the screenshot beside the pixels that are near on their own.

use appytappy_config::app_file::CheckDefError;
use appytappy_sources::screen::check::{BarFill, BarPixel, ColorMatch, average};
use appytappy_sources::screen::digits::DigitReader;
use appytappy_sources::screen::{Image, Region, Rgb};
use serde::{Deserialize, Serialize};

use crate::check_spec::{ColorSpec, RegionSpec, SpecError};

/// The most runs a layer carries. Runs go to the UI as JSON, and a picture
/// dithered at a tolerance's edge has a run for every other pixel, so the cap
/// bounds what one answer costs to send and to read. A layer with more
/// carries only its count, and the UI says that it tints none of them.
const MAX_RUNS: usize = 500_000;

/// What makes a pixel match, as the UI describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum MatchRule {
    /// A reference's characters: each of a pixel's red, green, and blue is
    /// within `tolerance` of `ink`'s.
    Ink { ink: ColorSpec, tolerance: u8 },
    /// A bar's fill: a pixel is filled, empty, or neither.
    BarFill { filled: ColorSpec, empty: ColorSpec },
    /// A color check: a pixel on its own is within `tolerance` of `color`.
    Color { color: ColorSpec, tolerance: u8 },
}

/// The pixels of one kind in the rectangle asked about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct Layer {
    /// How many pixels are of this kind.
    count: usize,
    /// The runs of them in the rectangle, each as the index of its first
    /// pixel and of the pixel after its last, row by row from the top, as a
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

    /// Adds the run of pixels from `start` up to `end`.
    fn push(&mut self, start: usize, end: usize) {
        self.count = self.count.saturating_add(end.saturating_sub(start));
        if self
            .spans
            .as_ref()
            .is_some_and(|spans| spans.len() >= MAX_RUNS)
        {
            self.spans = None;
        }
        if let Some(spans) = &mut self.spans {
            spans.push([start, end]);
        }
    }
}

/// What a rule takes of a rectangle of a screenshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum MatchView {
    /// The pixels a reference counts as its characters.
    Ink { ink: Layer },
    /// The pixels a bar counts as filled and as empty; the rest are neither.
    BarFill { filled: Layer, empty: Layer },
    /// The pixels within a color check's tolerance on their own, and what
    /// the check itself compares: the rectangle's average color.
    Color {
        /// The pixels that are near the color on their own.
        near: Layer,
        /// The rectangle's average color.
        average: ColorSpec,
        /// The largest difference between the average's red, green, and
        /// blue and the color's, which the tolerance bounds.
        distance: u8,
        /// Whether the check reads yes on this rectangle.
        matches: bool,
    },
}

/// What `rule` takes of `area` on `image`.
///
/// # Errors
///
/// Returns [`SpecError::EmptyRegion`] for an empty `area`,
/// [`SpecError::Outside`] for one that reaches past the image, and
/// [`SpecError::Check`] for a bar whose two colors are the same.
pub(crate) fn find(
    image: &Image,
    area: RegionSpec,
    rule: MatchRule,
) -> Result<MatchView, SpecError> {
    let area = Region::try_from(area).map_err(|_empty| SpecError::EmptyRegion)?;
    // The one place that decides whether the rectangle lies on the picture.
    let cropped = image.crop(area).ok_or(SpecError::Outside)?;
    let pixels = cropped.pixels().iter().copied();
    Ok(match rule {
        MatchRule::Ink { ink, tolerance } => {
            let reader = DigitReader::new(ink.into(), tolerance);
            let [ink] = layers(pixels.map(|pixel| reader.is_ink(pixel).then_some(0)));
            MatchView::Ink { ink }
        },
        MatchRule::BarFill { filled, empty } => {
            let bar = BarFill::new(filled.into(), empty.into())
                .map_err(|_same| SpecError::Check(CheckDefError::SameColors))?;
            let [filled, empty] = layers(pixels.map(|pixel| match bar.classify(pixel) {
                BarPixel::Filled => Some(0),
                BarPixel::Empty => Some(1),
                BarPixel::Neither => None,
            }));
            MatchView::BarFill { filled, empty }
        },
        MatchRule::Color { color, tolerance } => {
            let check = ColorMatch {
                color: color.into(),
                tolerance,
            };
            let [near] = layers(pixels.map(|[blue, green, red, _]| {
                Rgb { red, green, blue }
                    .near(check.color, check.tolerance)
                    .then_some(0)
            }));
            let average = average(&cropped);
            MatchView::Color {
                near,
                average: average.into(),
                distance: average.farthest_channel(check.color),
                matches: check.matches(&cropped),
            }
        },
    })
}

/// The `N` layers of a rectangle whose pixels, row by row from the top, are
/// each of the layer `kinds` gives, or of none.
fn layers<const N: usize>(kinds: impl Iterator<Item = Option<usize>>) -> [Layer; N] {
    let mut layers = [const { Layer::new() }; N];
    // The layer and the first pixel of the run being read.
    let mut open: Option<(usize, usize)> = None;
    let mut close = |open: Option<(usize, usize)>, end: usize| {
        if let Some((layer, start)) = open
            && let Some(layer) = layers.get_mut(layer)
        {
            layer.push(start, end);
        }
    };
    let mut length = 0_usize;
    for (index, kind) in kinds.enumerate() {
        if open.map(|(layer, _)| layer) != kind {
            close(open, index);
            open = kind.map(|layer| (layer, index));
        }
        length = index.saturating_add(1);
    }
    close(open, length);
    layers
}

#[cfg(test)]
mod tests {
    use appytappy_config::signal::Number;
    use appytappy_sources::screen::Size;
    use appytappy_sources::screen::check::CheckError;
    use appytappy_sources::screen::digits::ReadError;

    use super::*;

    fn color(red: u8, green: u8, blue: u8) -> ColorSpec {
        Rgb { red, green, blue }.into()
    }

    /// A picture whose pixels are `rows` of reds, each with no green or
    /// blue.
    fn reds(rows: &[&[u8]]) -> Image {
        let size = Size {
            width: u32::try_from(rows[0].len()).expect("small"),
            height: u32::try_from(rows.len()).expect("small"),
        };
        let bgra = rows
            .iter()
            .flat_map(|row| row.iter())
            .flat_map(|&red| [0, 0, red, 255])
            .collect();
        Image::new(size, bgra).expect("matching length")
    }

    fn area(x: u32, y: u32, width: u32, height: u32) -> RegionSpec {
        RegionSpec {
            x,
            y,
            width,
            height,
        }
    }

    fn whole(image: &Image) -> RegionSpec {
        area(0, 0, image.size().width, image.size().height)
    }

    fn layer(count: usize, spans: &[[usize; 2]]) -> Layer {
        Layer {
            count,
            spans: Some(spans.to_vec()),
        }
    }

    /// Red 200, give or take 10 on each channel.
    fn ink() -> MatchRule {
        MatchRule::Ink {
            ink: color(200, 0, 0),
            tolerance: 10,
        }
    }

    #[test]
    fn runs_cover_each_kind_row_by_row() {
        // A 4x2 picture: ink, ink, not, ink / ink, not, not, ink.
        let image = reds(&[&[200, 205, 0, 195], &[200, 0, 0, 200]]);
        assert_eq!(
            find(&image, whole(&image), ink()),
            Ok(MatchView::Ink {
                // A run goes on from the end of a row into the next.
                ink: layer(5, &[[0, 2], [3, 5], [7, 8]])
            })
        );
        // Only the middle columns: the indexes count within the rectangle.
        assert_eq!(
            find(&image, area(1, 0, 2, 2), ink()),
            Ok(MatchView::Ink {
                ink: layer(1, &[[0, 1]])
            })
        );
        assert_eq!(
            find(&image, area(2, 1, 2, 1), ink()),
            Ok(MatchView::Ink {
                ink: layer(1, &[[1, 2]])
            })
        );
    }

    #[test]
    fn no_match_and_every_match_are_answers() {
        let image = reds(&[&[200, 200], &[200, 200]]);
        assert_eq!(
            find(&image, whole(&image), ink()),
            Ok(MatchView::Ink {
                ink: layer(4, &[[0, 4]])
            })
        );
        let none = MatchRule::Ink {
            ink: color(0, 200, 0),
            tolerance: 10,
        };
        assert_eq!(
            find(&image, whole(&image), none),
            Ok(MatchView::Ink { ink: layer(0, &[]) })
        );
    }

    #[test]
    fn a_bar_answers_its_filled_and_empty_pixels_apart() {
        let rule = MatchRule::BarFill {
            filled: color(200, 0, 0),
            empty: color(100, 0, 0),
        };
        // Filled, filled, empty, neither (black is farther from both than
        // half their distance), empty, filled.
        let image = reds(&[&[200, 190, 100, 0, 110, 200]]);
        assert_eq!(
            find(&image, whole(&image), rule),
            Ok(MatchView::BarFill {
                filled: layer(3, &[[0, 2], [5, 6]]),
                empty: layer(2, &[[2, 3], [4, 5]]),
            })
        );
        let same = MatchRule::BarFill {
            filled: color(1, 2, 3),
            empty: color(1, 2, 3),
        };
        assert_eq!(
            find(&image, whole(&image), same),
            Err(SpecError::Check(CheckDefError::SameColors))
        );
    }

    #[test]
    fn a_color_answers_its_average_beside_the_near_pixels() {
        let rule = MatchRule::Color {
            color: color(200, 0, 0),
            tolerance: 10,
        };
        // 190 and 211 average to 200: the check reads yes though one pixel
        // alone is past the tolerance.
        let image = reds(&[&[190, 211]]);
        assert_eq!(
            find(&image, whole(&image), rule),
            Ok(MatchView::Color {
                near: layer(1, &[[0, 1]]),
                average: color(200, 0, 0),
                distance: 0,
                matches: true,
            })
        );
        // The average is of the rectangle asked about, not of the picture.
        assert_eq!(
            find(&image, area(1, 0, 1, 1), rule),
            Ok(MatchView::Color {
                near: layer(0, &[]),
                average: color(211, 0, 0),
                distance: 11,
                matches: false,
            })
        );
    }

    #[test]
    fn a_rectangle_must_lie_on_the_picture() {
        let image = reds(&[&[200, 200]]);
        assert_eq!(
            find(&image, area(0, 0, 0, 1), ink()),
            Err(SpecError::EmptyRegion)
        );
        assert_eq!(
            find(&image, area(1, 0, 2, 1), ink()),
            Err(SpecError::Outside)
        );
        assert_eq!(
            find(&image, area(0, 1, 1, 1), ink()),
            Err(SpecError::Outside)
        );
    }

    #[test]
    fn a_layer_with_too_many_runs_keeps_only_its_count() {
        let mut layer = Layer::new();
        for run in 0..MAX_RUNS {
            layer.push(
                run.saturating_mul(2),
                run.saturating_mul(2).saturating_add(1),
            );
        }
        assert_eq!(layer.spans.as_ref().map(Vec::len), Some(MAX_RUNS));
        layer.push(usize::MAX.saturating_sub(3), usize::MAX);
        assert_eq!(layer.spans, None);
        assert_eq!(layer.count, MAX_RUNS.saturating_add(3));
        layer.push(0, 1);
        assert_eq!(layer.spans, None, "it stays without runs");
    }

    /// Whether `rule` takes the only pixel of `image`.
    fn takes(image: &Image, rule: MatchRule) -> bool {
        match find(image, whole(image), rule).expect("on the picture") {
            MatchView::Ink { ink: layer } | MatchView::Color { near: layer, .. } => {
                layer.count == 1
            },
            MatchView::BarFill { filled, empty } => filled.count == 1 || empty.count == 1,
        }
    }

    #[test]
    fn the_tint_and_the_number_reader_agree_at_the_tolerances_edge() {
        let rule = |tolerance| MatchRule::Ink {
            ink: color(200, 0, 0),
            tolerance,
        };
        let reader = |tolerance| {
            DigitReader::new(
                Rgb {
                    red: 200,
                    green: 0,
                    blue: 0,
                },
                tolerance,
            )
        };
        // Exactly at the tolerance on one channel is ink; one past is not.
        for (image, tolerance, ink) in [
            (reds(&[&[210]]), 10, true),
            (reds(&[&[211]]), 10, false),
            (reds(&[&[190]]), 10, true),
            (reds(&[&[189]]), 10, false),
        ] {
            assert_eq!(takes(&image, rule(tolerance)), ink);
            assert_eq!(
                reader(tolerance).shapes(&image).err(),
                (!ink).then_some(ReadError::NoInk),
                "the reader finds ink where the tint shows it"
            );
        }
    }

    #[test]
    fn the_tint_and_the_bar_reader_agree_at_half_the_colors_distance() {
        let rule = MatchRule::BarFill {
            filled: color(10, 0, 0),
            empty: color(0, 0, 0),
        };
        let bar = BarFill::new(
            Rgb {
                red: 10,
                green: 0,
                blue: 0,
            },
            Rgb {
                red: 0,
                green: 0,
                blue: 0,
            },
        )
        .expect("two colors");
        // 15 is half the colors' distance from the filled color, so it
        // counts; 16 is past it and counts as neither.
        let at = reds(&[&[15]]);
        assert_eq!(
            find(&at, whole(&at), rule),
            Ok(MatchView::BarFill {
                filled: layer(1, &[[0, 1]]),
                empty: layer(0, &[]),
            })
        );
        assert_eq!(bar.percent(&at).map(Number::raw), Ok(100.0));
        let past = reds(&[&[16]]);
        assert!(!takes(&past, rule));
        assert_eq!(bar.percent(&past), Err(CheckError::NoBar));
        // Midway between the colors is empty for both.
        let midway = reds(&[&[5]]);
        assert_eq!(
            find(&midway, whole(&midway), rule),
            Ok(MatchView::BarFill {
                filled: layer(0, &[]),
                empty: layer(1, &[[0, 1]]),
            })
        );
        assert_eq!(bar.percent(&midway).map(Number::raw), Ok(0.0));
    }

    #[test]
    fn the_tint_and_the_color_check_agree_at_the_tolerances_edge() {
        let rule = MatchRule::Color {
            color: color(200, 0, 0),
            tolerance: 10,
        };
        let check = ColorMatch {
            color: Rgb {
                red: 200,
                green: 0,
                blue: 0,
            },
            tolerance: 10,
        };
        // On a region of one pixel the average is the pixel, so the check
        // reads yes exactly where the tint shows.
        for (image, near) in [
            (reds(&[&[210]]), true),
            (reds(&[&[211]]), false),
            (reds(&[&[190]]), true),
            (reds(&[&[189]]), false),
        ] {
            assert_eq!(takes(&image, rule), near);
            assert_eq!(check.matches(&image), near);
            let MatchView::Color { matches, .. } =
                find(&image, whole(&image), rule).expect("on the picture")
            else {
                panic!("a color rule answers a color view");
            };
            assert_eq!(matches, near);
        }
    }

    #[test]
    fn rules_and_answers_cross_to_the_ui_tagged_by_kind() {
        let rule: MatchRule = serde_json::from_str(
            r#"{"kind":"barFill","filled":{"red":1,"green":2,"blue":3},"empty":{"red":0,"green":0,"blue":0}}"#,
        )
        .expect("the UI's spelling");
        assert_eq!(
            rule,
            MatchRule::BarFill {
                filled: color(1, 2, 3),
                empty: color(0, 0, 0)
            }
        );
        let image = reds(&[&[190, 211]]);
        let answer = find(
            &image,
            whole(&image),
            MatchRule::Color {
                color: color(200, 0, 0),
                tolerance: 10,
            },
        )
        .expect("on the picture");
        assert_eq!(
            serde_json::to_value(answer).expect("plain data"),
            serde_json::json!({
                "kind": "color",
                "near": {"count": 1, "spans": [[0, 1]]},
                "average": {"red": 200, "green": 0, "blue": 0},
                "distance": 0,
                "matches": true,
            })
        );
    }
}
