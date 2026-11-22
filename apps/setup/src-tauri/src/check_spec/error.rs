//! Why a check the UI described cannot be saved or read, in words the UI
//! shows.

use std::error::Error;
use std::fmt;

use appytappy_config::app_file::{CheckDefError, MAX_STATES};
use appytappy_sources::screen::Size;
use appytappy_sources::screen::digits::{LearnError, MAX_CHARACTERS, MAX_GLYPHS};
use appytappy_sources::screen::saved::SavedError;

use crate::reading::cant_build;
use crate::words::sentence;

use super::MAX_EXPECTED_CHARACTERS;

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

/// What a reference being taught adds to its saved samples, and so what a
/// lesson that fails names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lesson {
    /// A box on a screenshot, with the text typed for it.
    Box,
    /// A kept reading of the recording.
    Reading,
}

/// A check the UI described cannot be saved, or cannot be read as saved.
///
/// The session's own problems are its own variants: a screenshot, a
/// recording, or a selection that the saved form cannot be made from. What
/// `saved::build` refuses arrives [from a `SavedError`](From), which keeps
/// the UI's words for the problems the user can fix on the page.
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
    /// A saved reference being taught cannot be had, and why.
    Reference(String),
    /// The saved reference a value reads with lacks a sample's picture, or
    /// cannot decode it.
    MissingSample {
        /// The reference's name.
        reference: String,
        /// The picture's file name.
        picture: String,
    },
    /// A number reads only characters that its reference does not read.
    NotRead {
        /// The reference's name.
        reference: String,
        /// The characters the number reads.
        characters: String,
    },
    /// A number reads only characters that its reference knows none of.
    NoCharacters {
        /// The reference's name.
        reference: String,
        /// The characters the number reads.
        characters: String,
    },
    /// A value's reference belongs to another application.
    OtherApplication,
    /// The saved form cannot be read for a reason the page has no words of
    /// its own for, in `saved::build`'s.
    Unreadable(String),
    /// A number value is to need more than [`MAX_EXPECTED_CHARACTERS`]
    /// characters.
    TooManyExpected,
    /// The saved reference a value reads with cannot learn from one of its
    /// saved samples.
    ReferenceLearn {
        /// The reference's name.
        reference: String,
        /// The text the sample shows.
        text: String,
        /// Why the reference cannot learn from it.
        source: LearnError,
    },
    /// The saved reference being edited lacks a sample's picture, or
    /// cannot decode it.
    MissingPicture {
        /// The picture's file name.
        picture: String,
    },
    /// The saved reference being edited cannot learn from one of its saved
    /// samples.
    SavedLearn {
        /// The text the sample shows.
        text: String,
        /// Why the reference cannot learn from it.
        source: LearnError,
    },
    /// A reference could not learn from a box or a kept reading.
    Learn {
        /// Which of the two.
        lesson: Lesson,
        /// The text the user typed, or kept.
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
            Self::Check(CheckDefError::StateName { position, error }) => {
                formatter.write_str(&sentence(&format!("{error} (state {position})")))
            },
            Self::Check(CheckDefError::SameStateName(name)) => write!(
                formatter,
                "Two states are named \"{name}\". Give each state its own name."
            ),
            Self::Check(CheckDefError::NoCharacters) => {
                formatter.write_str("Choose at least one character for the value to read.")
            },
            // A fix needs the characters its value chose, with a reference
            // of its own, so the wording holds where they cannot be chosen.
            Self::NotRead {
                reference,
                characters,
            } => write!(
                formatter,
                "{reference} reads none of the characters this value needs, \"{characters}\". \
                 Edit {reference} to read them, or change the characters the value needs."
            ),
            Self::NoCharacters {
                reference,
                characters,
            } => write!(
                formatter,
                "{reference} knows none of the characters this value needs, \"{characters}\". \
                 Teach it one of them, or change the characters the value needs."
            ),
            Self::NothingSelected(name) => write!(
                formatter,
                "Nothing in the region is selected for the state \"{name}\". Select it again, clear its selection, or stop ignoring the background.",
            ),
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
            Self::Picture => {
                formatter.write_str("AppyTappy can't save a picture of the region. Try again.")
            },
            Self::Reference(why) => formatter.write_str(why),
            Self::OtherApplication => formatter.write_str(
                "That reference belongs to another application. Pick one of this application's.",
            ),
            Self::MissingSample { reference, picture } => write!(
                formatter,
                "{reference} is missing its saved picture {picture}. Edit {reference} and remove \
                 that picture under Saved pictures it can't learn."
            ),
            Self::ReferenceLearn {
                reference,
                text,
                source,
            } => write!(
                formatter,
                "{reference} can't learn from its saved picture of \"{text}\": {} Edit \
                 {reference} and remove that picture under Saved pictures it can't learn.",
                cant_learn(source)
            ),
            // The reference's own editor says these, where the picture
            // shows in the list they name.
            Self::MissingPicture { picture } => write!(
                formatter,
                "Its saved picture {picture} is missing. Remove it under Saved pictures it can't \
                 learn."
            ),
            Self::SavedLearn { text, source } => write!(
                formatter,
                "Can't learn from its saved picture of \"{text}\": {} Remove it under Saved \
                 pictures it can't learn.",
                cant_learn(source)
            ),
            Self::Unreadable(why) => {
                write!(formatter, "AppyTappy can't read this value as saved: {why}.")
            },
            Self::TooManyExpected => write!(
                formatter,
                "A value needs at most {MAX_EXPECTED_CHARACTERS} characters. Remove some."
            ),
            Self::Learn {
                lesson,
                text,
                source,
            } => {
                let from = match lesson {
                    Lesson::Box => "the box with the text",
                    Lesson::Reading => "the kept reading",
                };
                write!(
                    formatter,
                    "Can't learn from {from} \"{text}\": {}",
                    cant_learn(source)
                )
            },
        }
    }
}

/// Why a reference can't learn from a picture, in the setup app's words:
/// the end of a sentence that says it can't, with its full stop. The
/// runtime's log says the same in the lesson's own words.
pub(crate) fn cant_learn(error: &LearnError) -> String {
    let shown = |found: usize| {
        if found == 1 {
            "1 character".to_owned()
        } else {
            format!("{found} characters")
        }
    };
    match error {
        LearnError::NoInk => "nothing in it stands out from its background.".to_owned(),
        LearnError::TooManyCharacters => {
            format!("it shows more than {MAX_CHARACTERS} characters.")
        },
        LearnError::FewerCharacters { found, expected } => format!(
            "it shows {} but the text has {expected}. Characters that touch count as one.",
            shown(*found)
        ),
        LearnError::MoreCharacters { found, expected } => format!(
            "it shows {} but the text has {expected}. A character drawn as separate marks side \
             by side, as some percent signs are, counts once for each mark, and so does \
             anything else that stands out as the characters do.",
            shown(*found)
        ),
        LearnError::Conflict { learned, new } => format!(
            "a character given as \"{new}\" has the same shape as one learned as \
             \"{learned}\". Change the text of one of them, or leave one out."
        ),
        LearnError::Full => format!(
            "the reference holds {MAX_GLYPHS} character shapes already, the most one can hold. \
             Remove boxes, readings, or saved pictures it doesn't need."
        ),
    }
}

impl From<SavedError> for SpecError {
    fn from(error: SavedError) -> Self {
        match error {
            SavedError::EmptyRegion => Self::EmptyRegion,
            SavedError::OutsideWindow { .. } => Self::Outside,
            SavedError::Check(broken) => Self::Check(broken),
            other => Self::Unreadable(cant_build(&other)),
        }
    }
}

impl Error for SpecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Learn { source, .. }
            | Self::SavedLearn { source, .. }
            | Self::ReferenceLearn { source, .. } => Some(source),
            _ => None,
        }
    }
}
