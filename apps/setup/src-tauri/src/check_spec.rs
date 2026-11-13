//! The check the user set up in the UI, and how it becomes a screen check.
//!
//! The UI describes a check with plain data: the region, the check's kind,
//! and the colors and labels the user picked on screenshots. [`build`] turns
//! that into a [`Check`] from `appytappy-sources`: a number reads with a
//! saved reference's characters, and a state check cuts each state's
//! picture from the screenshot it names. A reference being taught is described the same
//! way, as [`CheckSpec::Characters`], so the UI can see what it reads before
//! it is saved.

use std::collections::BTreeSet;

use appytappy_config::app_file::{
    CheckDef, CheckDefError, ColorDef, ReferenceDef, RegionDef, SampleDef, StateDef, check_states,
    is_label_character, sample_name, state_image_name,
};
use appytappy_config::signal::StateName;
use appytappy_sources::screen::check::{BarFill, Check, ColorMatch, ImageMatch, StateMatch};
use appytappy_sources::screen::digits::DigitReader;
use appytappy_sources::screen::mask::Mask;
use appytappy_sources::screen::{Image, Region, Size};
use appytappy_sources::system::Rgb;
use serde::{Deserialize, Serialize};

use crate::recording::Recording;
use crate::screenshots::Shots;
use crate::selection::{self, SelectionSpec};

pub(crate) mod bar;
mod error;

pub(crate) use error::{Cut, SpecError, sentence};

/// The most characters a reference is saved as expecting: room for the
/// digits, both cases of the English letters, and every other printable
/// ASCII character, with some to spare.
pub(crate) const MAX_EXPECTED_CHARACTERS: usize = 128;

/// A rectangle of the window, in pixels from its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct RegionSpec {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl From<Region> for RegionSpec {
    fn from(region: Region) -> Self {
        let size = region.size();
        Self {
            x: region.x(),
            y: region.y(),
            width: size.width,
            height: size.height,
        }
    }
}

/// A color of a screenshot: one the user picked on it, one proposed to them,
/// or one the setup app read from it for the UI to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct ColorSpec {
    red: u8,
    green: u8,
    blue: u8,
}

impl From<Rgb> for ColorSpec {
    fn from(color: Rgb) -> Self {
        Self {
            red: color.red,
            green: color.green,
            blue: color.blue,
        }
    }
}

impl From<ColorSpec> for ColorDef {
    fn from(color: ColorSpec) -> Self {
        Self {
            red: color.red,
            green: color.green,
            blue: color.blue,
        }
    }
}

impl From<ColorSpec> for Rgb {
    fn from(color: ColorSpec) -> Self {
        Self {
            red: color.red,
            green: color.green,
            blue: color.blue,
        }
    }
}

/// What one screenshot's region shows, typed by the user so a reference
/// learns its characters.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct Label {
    pub(crate) screenshot: usize,
    /// Where on the screenshot the text shows, when it is not the spec's
    /// region, as for text found like a sample elsewhere on it.
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) region: Option<RegionSpec>,
    pub(crate) text: String,
}

/// What one recorded reading shows, as the user confirmed it in review, so
/// a reference learns its characters like a labeled screenshot's.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct RecordedLabel {
    /// The reading's index in the recording.
    pub(crate) reading: usize,
    /// What it shows.
    pub(crate) text: String,
}

/// A saved reference of a saved application.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct ReferenceRef {
    /// The application's ID.
    pub(crate) app: String,
    /// The reference's ID in the application's file.
    pub(crate) id: String,
}

/// A check as the UI describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum CheckSpec {
    /// Reads a number with the characters of a saved reference.
    Number {
        region: RegionSpec,
        reference: ReferenceRef,
        /// Whether digits after the decimal point are ignored.
        whole: bool,
        /// The characters the value reads, or `None` for every character
        /// the reference knows, as [`CheckDef::Number`] saves them.
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        characters: Option<String>,
    },
    /// Reads a number with the characters a reference is being taught: the
    /// saved samples of `reference`, when it is saved, then `labels`, then
    /// `recorded`, all with `ink` and `tolerance`. Only the reference
    /// editor builds it; a value names a saved reference instead.
    Characters {
        region: RegionSpec,
        /// The screenshot the region was drawn on. Every labeled screenshot
        /// and the recording must have its window size, since the region
        /// means another spot on a window of another size.
        screenshot: usize,
        #[serde(default)]
        reference: Option<ReferenceRef>,
        ink: ColorSpec,
        tolerance: u8,
        labels: Vec<Label>,
        /// Readings of the session's recording, which must be of `region`.
        #[serde(default)]
        recorded: Vec<RecordedLabel>,
    },
    /// Measures how full a bar is from a filled and an empty color.
    #[serde(rename_all = "camelCase")]
    BarFill {
        region: RegionSpec,
        filled: ColorSpec,
        empty: ColorSpec,
    },
    /// Whether the region's average color is near `color`.
    Color {
        region: RegionSpec,
        color: ColorSpec,
        tolerance: u8,
    },
    /// Which of a few named looks the region shows, each taken from a
    /// screenshot.
    State {
        region: RegionSpec,
        /// The screenshot the region was drawn on; every state's screenshot
        /// must have its window size.
        #[serde(rename = "drawnOn")]
        drawn_on: usize,
        states: Vec<StateSpec>,
        tolerance: u8,
    },
}

/// One state of a state check, as the user set it up.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct StateSpec {
    /// The name the value reads when the region looks like this.
    pub(crate) name: String,
    /// The screenshot whose region shows this state.
    pub(crate) screenshot: usize,
    /// The thing to compare, selected on the screenshot; without one the
    /// whole region is compared.
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) selection: Option<SelectionSpec>,
}

/// A saved reference and its samples' pictures, in the order of its
/// samples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SavedReference {
    /// The reference as its application's file has it.
    pub(crate) reference: ReferenceDef,
    /// Each sample's picture.
    pub(crate) pictures: Vec<Image>,
}

/// Where a check finds the saved references it names: the saved reference,
/// or why it cannot be had.
pub(crate) type References<'a> = &'a dyn Fn(&ReferenceRef) -> Result<SavedReference, String>;

/// A check and the region it reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegionCheck {
    /// The region of the window's content the check reads.
    pub(crate) region: Region,
    /// The check.
    pub(crate) check: Check,
}

/// Pictures to save beside an application's file, each as a file name and
/// PNG bytes.
pub(crate) type Pictures = Vec<(String, Vec<u8>)>;

/// A check ready to save: the region, the check as the file spells it, and
/// the pictures it names, each as a file name and PNG bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SavedCheck {
    /// The region the check reads.
    pub(crate) region: RegionDef,
    /// The check.
    pub(crate) check: CheckDef,
    /// The pictures `check` names.
    pub(crate) pictures: Pictures,
}

/// The session's screenshots, recording, and saved references, which the
/// checks the UI describes name.
#[derive(Clone, Copy)]
pub(crate) struct Session<'a> {
    pub(crate) screenshots: &'a dyn Shots,
    pub(crate) recording: Option<&'a Recording>,
    pub(crate) references: References<'a>,
}

/// Turns `spec` into the saved form for the value `id` of the application
/// `app`, cutting a state check's pictures, `{id}-1.png` onward in the
/// order of its states, from their screenshots.
///
/// # Errors
///
/// Returns the [`SpecError`] that [`build`] would, so only a check that
/// works is saved; [`SpecError::OtherApplication`] for a reference of
/// another application; [`SpecError::NotAValue`] for
/// characters still being taught; or [`SpecError::Picture`] when a picture
/// cannot be encoded.
pub(crate) fn to_saved(
    spec: &CheckSpec,
    session: Session<'_>,
    app: &str,
    id: &str,
) -> Result<SavedCheck, SpecError> {
    to_saved_fix(spec, session, app, id, None)
}

/// Turns `spec` into the saved form of the value `id` drawn again at
/// another size, as the fix numbered `fix` names its pictures:
/// `{id}-{fix}-1.png` onward. With `None` it is the value's first drawing,
/// as [`to_saved`] saves it.
///
/// # Errors
///
/// Returns what [`to_saved`] does.
pub(crate) fn to_saved_fix(
    spec: &CheckSpec,
    session: Session<'_>,
    app: &str,
    id: &str,
    fix: Option<usize>,
) -> Result<SavedCheck, SpecError> {
    let built = build(spec, session)?;
    let mut pictures = Vec::new();
    let check = match spec {
        CheckSpec::Number {
            reference,
            whole,
            characters,
            ..
        } => {
            if app != reference.app {
                return Err(SpecError::OtherApplication);
            }
            CheckDef::Number {
                reference: reference.id.clone(),
                whole: *whole,
                characters: characters.clone(),
            }
        },
        CheckSpec::Characters { .. } => return Err(SpecError::NotAValue),
        CheckSpec::BarFill { filled, empty, .. } => CheckDef::BarFill {
            filled: (*filled).into(),
            empty: (*empty).into(),
        },
        CheckSpec::Color {
            color, tolerance, ..
        } => CheckDef::Color {
            color: (*color).into(),
            tolerance: *tolerance,
        },
        CheckSpec::State {
            states, tolerance, ..
        } => {
            let mut saved = Vec::with_capacity(states.len());
            for (state, number) in states.iter().zip(1_usize..) {
                let image = state_image_name(id, fix, number);
                let (picture, mask) = picture_of(session.screenshots, built.region, state)?;
                let png = match &mask {
                    Some(mask) => picture.encode_png_masked(mask),
                    None => picture.encode_png(),
                };
                pictures.push((image.clone(), png.map_err(|_encode| SpecError::Picture)?));
                saved.push(StateDef {
                    name: state.name.clone(),
                    image,
                });
            }
            CheckDef::State {
                states: saved,
                tolerance: *tolerance,
            }
        },
    };
    Ok(SavedCheck {
        region: region_def(built.region),
        check,
        pictures,
    })
}

/// The reference that `spec`, a [`CheckSpec::Characters`], teaches, saved
/// with the ID `id` and the name `name`, and the pictures of its new
/// samples, each as a file name and PNG bytes. A saved reference keeps its
/// samples and their pictures, and the new ones are numbered after them:
/// `ref-{id}-1.png` onward, the labeled screenshots first and then the
/// recorded readings.
///
/// `expected` is the characters the user says it needs, saved as
/// [`expected_characters`] lists them. It is guidance for the setup app: no
/// sample is learned or left out because of it.
///
/// # Errors
///
/// Returns the [`SpecError`] that [`build`] would, so only a reference that
/// learns every sample is saved; [`SpecError::NotAValue`] for any other
/// check; [`SpecError::TooManyExpected`] for more expected characters than
/// a reference saves; or [`SpecError::Picture`] when a picture cannot be
/// encoded.
pub(crate) fn to_saved_reference(
    spec: &CheckSpec,
    session: Session<'_>,
    id: &str,
    name: &str,
    expected: &str,
) -> Result<(ReferenceDef, Pictures), SpecError> {
    let built = build(spec, session)?;
    let CheckSpec::Characters {
        reference,
        ink,
        tolerance,
        labels,
        recorded,
        ..
    } = spec
    else {
        return Err(SpecError::NotAValue);
    };
    let mut samples = match reference {
        Some(saved) => {
            (session.references)(saved)
                .map_err(SpecError::Reference)?
                .reference
                .samples
        },
        None => Vec::new(),
    };
    let new = labels
        .iter()
        .map(|label| {
            Ok((
                label_image(session.screenshots, built.region, label)?,
                &label.text,
            ))
        })
        .chain(recorded.iter().map(|label| {
            Ok((
                recorded_image(session.recording, built.region, label.reading)?.clone(),
                &label.text,
            ))
        }));
    let mut pictures = Vec::new();
    let mut number = samples.len();
    for shown in new {
        let (image, text) = shown?;
        // The next number no sample's picture has, even when a file edited
        // by hand left a gap. Of the next `samples.len() + 1` numbers at
        // least one is free; past them all is free too.
        number = (1..=samples.len().saturating_add(1))
            .map(|step| number.saturating_add(step))
            .find(|&next| {
                let name = sample_name(id, next);
                samples.iter().all(|sample| sample.image != name)
            })
            .unwrap_or_else(|| number.saturating_add(samples.len()).saturating_add(1));
        let name = sample_name(id, number);
        pictures.push((
            name.clone(),
            image.encode_png().map_err(|_encode| SpecError::Picture)?,
        ));
        samples.push(SampleDef {
            image: name,
            text: text.clone(),
        });
    }
    Ok((
        ReferenceDef {
            id: id.to_owned(),
            name: name.to_owned(),
            ink: (*ink).into(),
            tolerance: *tolerance,
            samples,
            expects: expected_characters(expected)?,
        },
        pictures,
    ))
}

/// The characters of `expected` as a reference saves them, or `None` when
/// it has none: each character a label teaches, as [`is_label_character`]
/// says, that is no control character, once, the digits `0` to `9` first
/// and the rest by code point.
///
/// # Errors
///
/// Returns [`SpecError::TooManyExpected`] for more than
/// [`MAX_EXPECTED_CHARACTERS`] of them.
pub(crate) fn expected_characters(expected: &str) -> Result<Option<String>, SpecError> {
    // A digit sorts before every other character, and each group by code
    // point.
    let characters: BTreeSet<(bool, char)> = expected
        .chars()
        .filter(|&character| is_label_character(character) && !character.is_control())
        .map(|character| (!character.is_ascii_digit(), character))
        .collect();
    if characters.len() > MAX_EXPECTED_CHARACTERS {
        return Err(SpecError::TooManyExpected);
    }
    let list: String = characters
        .into_iter()
        .map(|(_, character)| character)
        .collect();
    Ok((!list.is_empty()).then_some(list))
}

/// The window size of screenshot `drawn_on`, the one the region of `spec`
/// was drawn on, once every other screenshot and the recording that `spec`
/// cuts at the region have that size too.
///
/// # Errors
///
/// Returns [`SpecError::NoScreenshot`] for a screenshot that does not exist,
/// or [`SpecError::OtherSize`] naming the first screenshot or the recording
/// of another size.
pub(crate) fn drawn_size(
    spec: &CheckSpec,
    session: Session<'_>,
    drawn_on: usize,
) -> Result<Size, SpecError> {
    let size_of = |screenshot: usize| {
        session
            .screenshots
            .get(screenshot)
            .map(Image::size)
            .ok_or(SpecError::NoScreenshot)
    };
    let drawn = size_of(drawn_on)?;
    let same = |cut: Cut, size: Size| {
        if size == drawn {
            Ok(())
        } else {
            Err(SpecError::OtherSize {
                cut,
                size,
                drawn_on,
                drawn,
            })
        }
    };
    match spec {
        CheckSpec::State { states, .. } => {
            for state in states {
                same(Cut::State(state.screenshot), size_of(state.screenshot)?)?;
            }
        },
        CheckSpec::Characters {
            labels, recorded, ..
        } => {
            for label in labels {
                same(Cut::Label(label.screenshot), size_of(label.screenshot)?)?;
            }
            if let Some(recording) = session.recording.filter(|_| !recorded.is_empty()) {
                same(Cut::Recording, recording.window_size())?;
            }
        },
        CheckSpec::Number { .. } | CheckSpec::BarFill { .. } | CheckSpec::Color { .. } => {},
    }
    Ok(drawn)
}

/// Builds the check `spec` describes from the `session`.
///
/// # Errors
///
/// Returns a [`SpecError`] that tells the user what to fix: an empty region,
/// a region, screenshot, recorded reading, or saved reference that does not
/// exist, a rule of the application's file that the check's settings break,
/// such as two bar colors that are the same, a number that reads no
/// character its reference knows, or a label the reference cannot learn
/// from.
pub(crate) fn build(spec: &CheckSpec, session: Session<'_>) -> Result<RegionCheck, SpecError> {
    let region = match spec {
        CheckSpec::Number { region, .. }
        | CheckSpec::Characters { region, .. }
        | CheckSpec::BarFill { region, .. }
        | CheckSpec::Color { region, .. }
        | CheckSpec::State { region, .. } => to_region(*region)?,
    };
    let check = match spec {
        CheckSpec::Number {
            reference,
            whole,
            characters,
            ..
        } => {
            // The file refuses a check that names characters and reads none.
            if characters
                .as_deref()
                .is_some_and(|characters| !characters.chars().any(is_label_character))
            {
                return Err(SpecError::Check(CheckDefError::NoCharacters));
            }
            let saved = (session.references)(reference).map_err(SpecError::Reference)?;
            // The glyphs the runtime reads the saved value with, as
            // `saved::build` chooses them.
            let reader = saved_reader(&saved.reference, &saved.pictures, None)?
                .reading(characters.as_deref())
                .ok_or_else(|| SpecError::NoCharacters {
                    reference: saved.reference.name.clone(),
                    characters: characters.clone().unwrap_or_default(),
                })?;
            Check::Number {
                reader,
                whole: *whole,
            }
        },
        CheckSpec::Characters {
            reference,
            labels,
            recorded,
            ..
        } => {
            if reference.is_none() && labels.is_empty() && recorded.is_empty() {
                return Err(SpecError::NoLabels);
            }
            let mut reader = taught_reader(spec, session, region)?;
            for label in recorded {
                learn(
                    &mut reader,
                    recorded_image(session.recording, region, label.reading)?,
                    &label.text,
                )?;
            }
            Check::Number {
                reader,
                whole: false,
            }
        },
        // `BarFill::new` refuses the same colors, as `CheckDef::check` does.
        CheckSpec::BarFill { filled, empty, .. } => Check::BarFill(
            BarFill::new((*filled).into(), (*empty).into())
                .map_err(|_same| SpecError::Check(CheckDefError::SameColors))?,
        ),
        CheckSpec::Color {
            color, tolerance, ..
        } => Check::Color(ColorMatch {
            color: (*color).into(),
            tolerance: *tolerance,
        }),
        CheckSpec::State {
            drawn_on,
            states,
            tolerance,
            ..
        } => {
            drawn_size(spec, session, *drawn_on)?;
            let names: Vec<&str> = states.iter().map(|state| state.name.as_str()).collect();
            check_states(&names).map_err(SpecError::Check)?;
            Check::State(StateMatch::new(state_pictures(
                session.screenshots,
                region,
                states,
                *tolerance,
            )?))
        },
    };
    Ok(RegionCheck { region, check })
}

/// Each of `states` as a name and the picture it is compared with, within
/// `tolerance`.
///
/// # Errors
///
/// Returns a [`SpecError`] when a name breaks the rules or a picture cannot
/// be cut.
fn state_pictures(
    screenshots: &dyn Shots,
    region: Region,
    states: &[StateSpec],
    tolerance: u8,
) -> Result<Vec<(StateName, ImageMatch)>, SpecError> {
    states
        .iter()
        .map(|state| {
            let name = StateName::new(&state.name)
                .map_err(|error| SpecError::StateName(sentence(&error.to_string())))?;
            let (picture, mask) = picture_of(screenshots, region, state)?;
            // The mask is the region's, so its size is always the picture's.
            let picture = ImageMatch::new(picture, mask.as_ref(), tolerance)
                .map_err(|_size| SpecError::NothingSelected(state.name.clone()))?;
            Ok((name, picture))
        })
        .collect()
}

/// The picture a state is compared against: `region` of its screenshot,
/// and with a selection the mask of the pixels of `region` it selects on
/// that screenshot.
fn picture_of(
    screenshots: &dyn Shots,
    region: Region,
    state: &StateSpec,
) -> Result<(Image, Option<Mask>), SpecError> {
    let image = screenshots
        .get(state.screenshot)
        .ok_or(SpecError::NoScreenshot)?;
    let picture = image.crop(region).ok_or(SpecError::Outside)?;
    let mask = state
        .selection
        .as_ref()
        .map(|selection| {
            selection::select(image, selection)
                .and_then(|selected| selected.mask_in(region))
                .ok_or_else(|| SpecError::NothingSelected(state.name.clone()))
        })
        .transpose()?;
    Ok((picture, mask))
}

/// Why the reference `spec` teaches could not learn each of its recorded
/// readings: `None` for one it learns. It learns the saved samples and the
/// labeled screenshots first and then the readings in order, as [`build`]
/// does, but leaves out each reading it cannot learn, so the ones after it
/// are judged on the ones that worked. Review flags the readings this
/// finds, so the reference saved from the kept ones learns them all. Any
/// other check has no recorded readings.
pub(crate) fn recorded_problems(
    spec: &CheckSpec,
    session: Session<'_>,
) -> Result<Vec<Option<String>>, SpecError> {
    let CheckSpec::Characters {
        region, recorded, ..
    } = spec
    else {
        return Ok(Vec::new());
    };
    let region = to_region(*region)?;
    let mut reader = taught_reader(spec, session, region)?;
    Ok(recorded
        .iter()
        .map(|label| {
            let image = match recorded_image(session.recording, region, label.reading) {
                Ok(image) => image,
                Err(error) => return Some(error.to_string()),
            };
            let mut trial = reader.clone();
            match trial.learn(image, &label.text) {
                Ok(()) => {
                    reader = trial;
                    None
                },
                Err(error) => Some(sentence(&error.to_string())),
            }
        })
        .collect())
}

/// For each of `found`, labels the user has not added to `spec` yet, why
/// the reference `spec` teaches cannot learn it: `None` for one it learns.
/// Like [`recorded_problems`], it learns the reference first and then each
/// label in order, leaving out the ones it cannot learn. Any other check
/// learns no labels.
pub(crate) fn label_problems(
    spec: &CheckSpec,
    session: Session<'_>,
    found: &[Label],
) -> Result<Vec<Option<String>>, SpecError> {
    let CheckSpec::Characters { region, .. } = spec else {
        return Ok(Vec::new());
    };
    let region = to_region(*region)?;
    let mut reader = taught_reader(spec, session, region)?;
    Ok(found
        .iter()
        .map(|label| {
            let image = match label_image(session.screenshots, region, label) {
                Ok(image) => image,
                Err(error) => return Some(error.to_string()),
            };
            let mut trial = reader.clone();
            match trial.learn(&image, &label.text) {
                Ok(()) => {
                    reader = trial;
                    None
                },
                Err(error) => Some(sentence(&error.to_string())),
            }
        })
        .collect())
}

/// The reader of a [`CheckSpec::Characters`] before its recorded readings:
/// the saved reference's samples relearned with the spec's own color and
/// tolerance, then its labeled screenshots, cut at `region`.
fn taught_reader(
    spec: &CheckSpec,
    session: Session<'_>,
    region: Region,
) -> Result<DigitReader, SpecError> {
    let CheckSpec::Characters {
        screenshot,
        reference,
        ink,
        tolerance,
        labels,
        ..
    } = spec
    else {
        return Err(SpecError::NotAValue);
    };
    drawn_size(spec, session, *screenshot)?;
    let mut reader = match reference {
        Some(saved) => {
            let saved = (session.references)(saved).map_err(SpecError::Reference)?;
            saved_reader(&saved.reference, &saved.pictures, Some((*ink, *tolerance)))?
        },
        None => DigitReader::new((*ink).into(), *tolerance),
    };
    for label in labels {
        learn(
            &mut reader,
            &label_image(session.screenshots, region, label)?,
            &label.text,
        )?;
    }
    Ok(reader)
}

/// A reader of `reference`'s characters, learned from `pictures`, its
/// samples' pictures in order. `settings` replaces its color and tolerance,
/// as a reference being edited does.
fn saved_reader(
    reference: &ReferenceDef,
    pictures: &[Image],
    settings: Option<(ColorSpec, u8)>,
) -> Result<DigitReader, SpecError> {
    let mut reader = match settings {
        Some((ink, tolerance)) => DigitReader::new(ink.into(), tolerance),
        None => DigitReader::new(reference.ink.into(), reference.tolerance),
    };
    for (sample, picture) in reference.samples.iter().zip(pictures) {
        learn(&mut reader, picture, &sample.text)?;
    }
    Ok(reader)
}

/// `region` as a [`Region`].
fn to_region(region: RegionSpec) -> Result<Region, SpecError> {
    Region::new(region.x, region.y, region.width, region.height).ok_or(SpecError::EmptyRegion)
}

/// `region` as the file spells it.
fn region_def(region: Region) -> RegionDef {
    let size = region.size();
    RegionDef {
        x: region.x(),
        y: region.y(),
        width: size.width,
        height: size.height,
    }
}

/// The `region` of screenshot `screenshot`.
fn cut(screenshots: &dyn Shots, region: Region, screenshot: usize) -> Result<Image, SpecError> {
    screenshots
        .get(screenshot)
        .ok_or(SpecError::NoScreenshot)?
        .crop(region)
        .ok_or(SpecError::Outside)
}

/// What `label` labels: its own region of its screenshot, or `region`.
fn label_image(screenshots: &dyn Shots, region: Region, label: &Label) -> Result<Image, SpecError> {
    let region = match label.region {
        Some(own) => to_region(own)?,
        None => region,
    };
    cut(screenshots, region, label.screenshot)
}

/// Teaches `reader` that `image` shows `text`.
fn learn(reader: &mut DigitReader, image: &Image, text: &str) -> Result<(), SpecError> {
    reader
        .learn(image, text)
        .map_err(|source| SpecError::Learn {
            text: text.to_owned(),
            source,
        })
}

/// The pixels of reading `reading` of `recording`, which must be of
/// `region`. The reference relearns them with its own characters' color and
/// tolerance, like the labeled screenshots, so they need not match the
/// recording's.
fn recorded_image(
    recording: Option<&Recording>,
    region: Region,
    reading: usize,
) -> Result<&Image, SpecError> {
    let recording = recording.ok_or(SpecError::NoRecording)?;
    if recording.region != region {
        return Err(SpecError::RecordedElsewhere);
    }
    recording
        .readings()
        .get(reading)
        .map(|kept| &kept.image)
        .ok_or(SpecError::NoRecording)
}

#[cfg(test)]
mod round_trip;
#[cfg(test)]
mod tests;
