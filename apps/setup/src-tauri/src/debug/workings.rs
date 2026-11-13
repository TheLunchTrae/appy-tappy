//! What the value and automation editors show under a live reading while
//! debug details show: how long the check took, how old the frame was, and
//! the check's scores from [`Check::explain`].

use std::hint;
use std::time::{Duration, Instant};

use appytappy_config::signal::Value;
use appytappy_sources::screen::check::Check;
use appytappy_sources::screen::digits::{CharacterScore, GlyphScore};
use appytappy_sources::screen::workings::Workings;
use appytappy_sources::screen::{Image, Rgb};
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
    /// How many grid cells one of the two sets where the other sets
    /// neither that cell nor one beside it.
    pub(crate) apart: u32,
    /// How many grid cells only one of the two sets.
    pub(crate) differing: u32,
    /// How many grid cells either of the two sets.
    pub(crate) set: u32,
    /// Whether the character reads as this glyph: at most 1 in 32 of the
    /// set cells are apart.
    pub(crate) matches: bool,
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
    /// Whether the region is within the tolerance. The read is the closest
    /// state that is.
    pub(crate) within: bool,
    /// Whether the read took this state.
    pub(crate) read: bool,
}

/// The details of `check` reading `region`, a crop of a frame that arrived
/// `frame_age` ago. The read is timed on its own, so the time is what the
/// runtime's read of the region takes, without the scores.
pub(crate) fn details(check: &Check, region: &Image, frame_age: Duration) -> ReadingDetails {
    let started = Instant::now();
    // Kept from being optimized away, so the time is a whole read's.
    let read = hint::black_box(check.read(region));
    let took = started.elapsed();
    let (workings, problem) = match check.explain(region) {
        Ok(workings) => {
            let mut view = WorkingsView::from(workings);
            if let (WorkingsView::State { states }, Ok(Value::State(name))) = (&mut view, &read)
                && let Some(state) = states.iter_mut().find(|state| state.name == name.as_str())
            {
                state.read = true;
            }
            (Some(view), None)
        },
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
                        within: state.within(),
                        read: false,
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
            apart: score.apart,
            differing: score.differing,
            set: score.set,
            matches: score.matches(),
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

#[cfg(test)]
mod tests {
    use appytappy_config::signal::StateName;
    use appytappy_sources::screen::Size;
    use appytappy_sources::screen::check::{ImageMatch, StateMatch};

    use super::*;

    /// A row of pixels, white for `#` and black for `.`.
    fn row(cells: &str) -> Image {
        let bgra = cells
            .chars()
            .flat_map(|cell| {
                let value = if cell == '#' { 255 } else { 0 };
                [value, value, value, 255]
            })
            .collect();
        let size = Size {
            width: u32::try_from(cells.len()).expect("fits"),
            height: 1,
        };
        Image::new(size, bgra).expect("matching length")
    }

    /// A state check of `states`, each a name and its picture, all with
    /// `tolerance`.
    fn states(states: &[(&str, &str)], tolerance: u8) -> Check {
        Check::State(StateMatch::new(
            states
                .iter()
                .map(|(name, picture)| {
                    (
                        StateName::new(name).expect("a name"),
                        ImageMatch::new(row(picture), None, tolerance).expect("a picture"),
                    )
                })
                .collect(),
        ))
    }

    /// The names of the states `details` marks as read.
    fn read(details: &ReadingDetails) -> Vec<String> {
        let Some(WorkingsView::State { states }) = &details.workings else {
            panic!("state workings: {details:?}");
        };
        states
            .iter()
            .filter(|state| state.read)
            .map(|state| state.name.clone())
            .collect()
    }

    #[test]
    fn the_state_marked_read_is_the_one_the_read_takes() {
        let age = Duration::from_millis(12);
        let check = states(&[("On", "##"), ("Half", "#."), ("Off", "..")], 255);
        let details = details(&check, &row("##"), age);
        assert_eq!(read(&details), ["On"]);
        assert_eq!(details.frame_age_ms, 12);
        assert_eq!(details.problem, None);
        assert_eq!(read(&super::details(&check, &row("#."), age)), ["Half"]);
        // None within its tolerance: the read takes no state of the check.
        let strict = states(&[("On", "##"), ("Off", "..")], 0);
        assert_eq!(
            read(&super::details(&strict, &row("#."), age)),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_region_the_check_cannot_compare_says_why() {
        let check = states(&[("On", "##")], 255);
        let details = details(&check, &row("###"), Duration::ZERO);
        assert_eq!(details.workings, None);
        assert!(details.problem.is_some());
    }
}
