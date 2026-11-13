//! Why a check the UI described cannot be built, in words the UI shows.

use std::error::Error;
use std::fmt;

use appytappy_config::app_file::{CheckDefError, MAX_STATES};
use appytappy_sources::screen::Size;
use appytappy_sources::screen::digits::LearnError;

use super::MAX_EXPECTED_CHARACTERS;

/// `text` as a sentence: capitalized, with a full stop.
pub(crate) fn sentence(text: &str) -> String {
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| {
        format!("{}{}.", first.to_uppercase(), characters.as_str())
    })
}

/// What a check cuts at its region besides the screenshot it was drawn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cut {
    /// A labeled screenshot, by index.
    Label(usize),
    /// The kept readings of the recording.
    Recording,
    /// The screenshot a state is taken from, by index.
    State(usize),
}

/// A check the UI described cannot be built.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SpecError {
    /// The region has no width or no height.
    EmptyRegion,
    /// A screenshot the check names does not exist in this session.
    NoScreenshot,
    /// The region reaches past the edge of the screenshot.
    Outside,
    /// The check's settings break a rule of the application's file.
    Check(CheckDefError),
    /// The selection of the state with this name leaves no pixel of its
    /// region to compare.
    NothingSelected(String),
    /// A state's name breaks the rules, and how, as a sentence.
    StateName(String),
    /// A reference has no saved sample, labeled screenshot, or kept reading
    /// to learn from.
    NoLabels,
    /// A recorded reading the check names is not in this session's
    /// recording.
    NoRecording,
    /// The region is not the one the session's recording recorded.
    RecordedElsewhere,
    /// A screenshot or the recording the check cuts at its region has
    /// another window size than the screenshot it was drawn on.
    OtherSize {
        /// What has the other size.
        cut: Cut,
        /// Its window size.
        size: Size,
        /// The screenshot the region was drawn on, by index.
        drawn_on: usize,
        /// That screenshot's window size.
        drawn: Size,
    },
    /// A picture could not be encoded as PNG.
    Picture,
    /// A saved reference the check names cannot be had, and why.
    Reference(String),
    /// A number reads only characters that its reference knows none of.
    NoCharacters {
        /// The reference's name.
        reference: String,
        /// The characters the number reads.
        characters: String,
    },
    /// A value's reference belongs to another application.
    OtherApplication,
    /// Characters still being taught were saved as a value, or something
    /// other than characters as a reference.
    NotAValue,
    /// A reference is to expect more than [`MAX_EXPECTED_CHARACTERS`]
    /// characters.
    TooManyExpected,
    /// A reference could not learn from a label.
    Learn {
        /// The text the user typed.
        text: String,
        /// Why the reference could not learn from it.
        source: LearnError,
    },
}

impl fmt::Display for SpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRegion => formatter.write_str("Draw a region on the screenshot first."),
            Self::NoScreenshot => {
                formatter.write_str("That screenshot is gone. Take a new one and try again.")
            },
            Self::Outside => formatter.write_str(
                "The region reaches past the edge of the window. Draw it inside the window.",
            ),
            Self::Check(CheckDefError::SameColors) => {
                formatter.write_str("Pick a filled color and an empty color that differ.")
            },
            Self::Check(CheckDefError::NoStates) => {
                formatter.write_str("Add a state from a screenshot.")
            },
            Self::Check(CheckDefError::TooManyStates) => write!(
                formatter,
                "A value can tell apart at most {MAX_STATES} states. Remove one."
            ),
            Self::Check(CheckDefError::SameStateName(name)) => write!(
                formatter,
                "Two states are named \"{name}\". Give each state its own name."
            ),
            Self::Check(CheckDefError::NoCharacters) => {
                formatter.write_str("Choose at least one character for the value to read.")
            },
            // A fix reads the characters its value chose, with a reference
            // of its own, so the wording holds where they cannot be chosen.
            Self::NoCharacters {
                reference,
                characters,
            } => write!(
                formatter,
                "{reference} knows none of the characters this value reads, {characters:?}. Teach \
                 it one of them, or change which characters the value reads."
            ),
            Self::NothingSelected(name) => write!(
                formatter,
                "Nothing in the region is selected for the state {name:?}. Select it again, clear its selection, or stop ignoring the background.",
            ),
            Self::StateName(problem) => formatter.write_str(problem),
            Self::NoLabels => formatter.write_str(
                "Type what at least one box shows, or keep a recorded reading.",
            ),
            Self::NoRecording => {
                formatter.write_str("That recording is gone. Record the region again.")
            },
            Self::RecordedElsewhere => formatter.write_str(
                "The reference reads another place than the recording recorded. Use its settings, or \
                 record again.",
            ),
            Self::OtherSize {
                cut,
                size,
                drawn_on,
                drawn,
            } => {
                // The UI numbers screenshots from 1.
                let drawn_on = drawn_on.saturating_add(1);
                let what = match cut {
                    Cut::Label(screenshot) => {
                        format!(
                            "Screenshot {}, which a box is on, is",
                            screenshot.saturating_add(1)
                        )
                    },
                    Cut::Recording => "The recording is of a window of".to_owned(),
                    Cut::State(screenshot) => format!(
                        "Screenshot {}, which a state is taken from, is",
                        screenshot.saturating_add(1)
                    ),
                };
                let fix = match cut {
                    Cut::Label(_) => "Delete its boxes, or draw them on a screenshot of that size.",
                    Cut::Recording => "Use its settings, or discard the recording.",
                    Cut::State(_) => "Take that state from a screenshot of that size.",
                };
                // A reference reads at a value's region or at one of its boxes.
                let place = match cut {
                    Cut::Label(_) | Cut::Recording => "the reference reads on",
                    Cut::State(_) => "the region was drawn on",
                };
                write!(
                    formatter,
                    "{what} {size}, but {place} screenshot {drawn_on}, which is {drawn}. {fix}"
                )
            },
            Self::Picture => formatter.write_str("A picture of the region could not be saved."),
            Self::Reference(why) => formatter.write_str(why),
            Self::OtherApplication => formatter.write_str(
                "That reference belongs to another application. Pick one of this application's.",
            ),
            Self::NotAValue => formatter.write_str("Pick a reference to read the number with."),
            Self::TooManyExpected => write!(
                formatter,
                "A reference needs at most {MAX_EXPECTED_CHARACTERS} characters. Remove some."
            ),
            Self::Learn { text, source } => {
                write!(formatter, "Could not learn from \"{text}\": {source}")
            },
        }
    }
}

impl Error for SpecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Learn { source, .. } => Some(source),
            _ => None,
        }
    }
}
