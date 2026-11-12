//! What the value and automation editors show under a live reading while
//! debug details show: how long the check took, how old the frame was, and
//! the check's scores from [`Check::explain`].

use std::time::{Duration, Instant};

use appytappy_sources::screen::Image;
use appytappy_sources::screen::check::Check;
use appytappy_sources::screen::digits::{CharacterScore, GlyphScore};
use appytappy_sources::screen::workings::Workings;
use appytappy_sources::system::Rgb;
use serde::Serialize;

/// A live reading's debug details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadingDetails {
    /// How long the check took to read the region, in microseconds.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) took_us: u64,
    /// How long ago the frame it read arrived, in milliseconds.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) frame_age_ms: u64,
    /// The check's scores, or `None` when the region has none.
    pub(crate) workings: Option<WorkingsView>,
    /// Why the region has no scores, such as no ink in it.
    pub(crate) problem: Option<String>,
}

/// [`Workings`] as the page reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum WorkingsView {
    /// Each character's closest glyphs, left to right.
    Number {
        /// The characters.
        characters: Vec<CharacterView>,
    },
    /// The bar's pixels near either color, and those nearer the filled one.
    #[serde(rename_all = "camelCase")]
    BarFill {
        /// The pixels nearer the filled color.
        #[cfg_attr(test, ts(type = "number"))]
        filled: u64,
        /// The pixels near either color.
        #[cfg_attr(test, ts(type = "number"))]
        counted: u64,
    },
    /// The region's average color, the color looked for, and the tolerance.
    Color {
        /// The region's average color.
        average: ReadingColor,
        /// The color the check looks for.
        color: ReadingColor,
        /// How far each of red, green, and blue may be.
        tolerance: u8,
    },
    /// Each state's distance from the region.
    State {
        /// The states, in the check's order.
        states: Vec<StateView>,
    },
}

/// A character's closest glyph and the closest of another character.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct CharacterView {
    /// The closest glyph, if the reference has any.
    pub(crate) best: Option<GlyphView>,
    /// The closest glyph of another character, if the reference has one.
    pub(crate) runner_up: Option<GlyphView>,
}

/// How close a character is to one glyph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct GlyphView {
    /// The glyph's character.
    pub(crate) character: String,
    /// How many grid cells differ.
    pub(crate) differing: u32,
    /// The most that may differ for a match.
    pub(crate) allowed: u32,
}

/// A color's red, green, and blue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct ReadingColor {
    /// Red.
    pub(crate) red: u8,
    /// Green.
    pub(crate) green: u8,
    /// Blue.
    pub(crate) blue: u8,
}

/// How far the region is from one state's picture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct StateView {
    /// The state's name.
    pub(crate) name: String,
    /// The summed difference of the compared channels.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) total: u64,
    /// How many channels were compared.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) channels: u64,
    /// The largest average difference that shows the state.
    pub(crate) tolerance: u8,
}

/// The details of `check` reading `region`, a crop of a frame that arrived
/// `frame_age` ago. The read is timed on its own, so the time is what the
/// runtime's read of the region takes, without the scores.
pub(crate) fn details(check: &Check, region: &Image, frame_age: Duration) -> ReadingDetails {
    let started = Instant::now();
    #[expect(
        clippy::let_underscore_must_use,
        reason = "only the read's time counts here; the editor shows its value from its own read"
    )]
    let _ = check.read(region);
    let took = started.elapsed();
    let (workings, problem) = match check.explain(region) {
        Ok(workings) => (Some(workings.into()), None),
        Err(error) => (None, Some(error.to_string())),
    };
    ReadingDetails {
        took_us: u64::try_from(took.as_micros()).unwrap_or(u64::MAX),
        frame_age_ms: u64::try_from(frame_age.as_millis()).unwrap_or(u64::MAX),
        workings,
        problem,
    }
}

impl From<Workings> for WorkingsView {
    fn from(workings: Workings) -> Self {
        match workings {
            Workings::Number(characters) => Self::Number {
                characters: characters.into_iter().map(CharacterView::from).collect(),
            },
            Workings::BarFill { filled, counted } => Self::BarFill { filled, counted },
            Workings::Color {
                average,
                color,
                tolerance,
            } => Self::Color {
                average: average.into(),
                color: color.into(),
                tolerance,
            },
            Workings::State(states) => Self::State {
                states: states
                    .into_iter()
                    .map(|state| StateView {
                        name: state.name.as_str().to_owned(),
                        total: state.total,
                        channels: state.channels,
                        tolerance: state.tolerance,
                    })
                    .collect(),
            },
        }
    }
}

impl From<CharacterScore> for CharacterView {
    fn from(score: CharacterScore) -> Self {
        Self {
            best: score.best.map(GlyphView::from),
            runner_up: score.runner_up.map(GlyphView::from),
        }
    }
}

impl From<GlyphScore> for GlyphView {
    fn from(score: GlyphScore) -> Self {
        Self {
            character: score.character.to_string(),
            differing: score.differing,
            allowed: score.allowed,
        }
    }
}

impl From<Rgb> for ReadingColor {
    fn from(color: Rgb) -> Self {
        Self {
            red: color.red,
            green: color.green,
            blue: color.blue,
        }
    }
}
