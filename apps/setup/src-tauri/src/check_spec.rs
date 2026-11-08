//! The check the user set up in the UI, and how it becomes a screen check.
//!
//! The UI describes a check with plain data: the region, the check's kind,
//! and the colors and labels the user picked on screenshots. [`build`] turns
//! that into a [`Check`] from `appytappy-sources`: a number reads with a
//! saved reference's characters, and an image check cuts its picture from
//! the screenshot it names. A reference being taught is described the same
//! way, as [`CheckSpec::Characters`], so the UI can see what it reads before
//! it is saved.

use std::error::Error;
use std::fmt;

use appytappy_config::app_file::{
    CheckDef, ColorDef, ReferenceDef, RegionDef, SampleDef, sample_name,
};
use appytappy_sources::screen::check::{BarFill, Check, ColorMatch, ImageMatch};
use appytappy_sources::screen::digits::{DigitReader, LearnError};
use appytappy_sources::screen::mask::Mask;
use appytappy_sources::screen::{Image, Region};
use appytappy_sources::system::Rgb;
use serde::{Deserialize, Serialize};

use crate::recording::Recording;
use crate::selection::{self, SelectionSpec};

/// A rectangle of the window, in pixels from its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct RegionSpec {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// A color the user picked on a screenshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) struct ColorSpec {
    red: u8,
    green: u8,
    blue: u8,
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
pub(crate) struct Label {
    pub(crate) screenshot: usize,
    pub(crate) text: String,
}

/// What one recorded reading shows, as the user confirmed it in review, so
/// a reference learns its characters like a labeled screenshot's.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct RecordedLabel {
    /// The reading's index in the recording.
    pub(crate) reading: usize,
    /// What it shows.
    pub(crate) text: String,
}

/// A saved reference of a saved application.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct ReferenceRef {
    /// The application's ID.
    pub(crate) app: String,
    /// The reference's ID in the application's file.
    pub(crate) id: String,
}

/// A check as the UI describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum CheckSpec {
    /// Reads a number with the characters of a saved reference.
    Number {
        region: RegionSpec,
        reference: ReferenceRef,
        /// Whether digits after the decimal point are ignored.
        whole: bool,
    },
    /// Reads a number with the characters a reference is being taught: the
    /// saved samples of `reference`, when it is saved, then `labels`, then
    /// `recorded`, all with `ink` and `tolerance`. Only the reference
    /// editor builds it; a value names a saved reference instead.
    Characters {
        region: RegionSpec,
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
    /// Whether the region looks like it does on one screenshot.
    Image {
        region: RegionSpec,
        screenshot: usize,
        tolerance: u8,
        /// The thing to compare, selected on the screenshot; without one the
        /// whole region is compared.
        #[serde(default)]
        selection: Option<SelectionSpec>,
    },
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
    /// The region of the window's frame the check reads.
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
    pub(crate) screenshots: &'a [Image],
    pub(crate) recording: Option<&'a Recording>,
    pub(crate) references: References<'a>,
}

/// Turns `spec` into the saved form for the value `id` of the application
/// `app`, cutting an image check's picture, `id.png`, from its screenshot.
///
/// # Errors
///
/// Returns the [`SpecError`] that [`build`] would, so only a check that
/// works is saved; [`SpecError::OtherApplication`] for a reference of
/// another application, or none yet; [`SpecError::NotAValue`] for
/// characters still being taught; or [`SpecError::Picture`] when a picture
/// cannot be encoded.
pub(crate) fn to_saved(
    spec: &CheckSpec,
    session: Session<'_>,
    app: Option<&str>,
    id: &str,
) -> Result<SavedCheck, SpecError> {
    let built = build(spec, session)?;
    let mut pictures = Vec::new();
    let check = match spec {
        CheckSpec::Number {
            reference, whole, ..
        } => {
            if app != Some(reference.app.as_str()) {
                return Err(SpecError::OtherApplication);
            }
            CheckDef::Number {
                reference: reference.id.clone(),
                whole: *whole,
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
        CheckSpec::Image {
            screenshot,
            tolerance,
            selection,
            ..
        } => {
            let image = format!("{id}.png");
            let (picture, mask) = picture_of(
                session.screenshots,
                built.region,
                *screenshot,
                selection.as_ref(),
            )?;
            let png = match &mask {
                Some(mask) => picture.encode_png_masked(mask),
                None => picture.encode_png(),
            };
            pictures.push((image.clone(), png.map_err(|_encode| SpecError::Picture)?));
            CheckDef::Image {
                image,
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
/// # Errors
///
/// Returns the [`SpecError`] that [`build`] would, so only a reference that
/// learns every sample is saved; [`SpecError::NotAValue`] for any other
/// check; or [`SpecError::Picture`] when a picture cannot be encoded.
pub(crate) fn to_saved_reference(
    spec: &CheckSpec,
    session: Session<'_>,
    id: &str,
    name: &str,
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
                cut(session.screenshots, built.region, label.screenshot)?,
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
        // A saved reference's next number is free: its pictures are named
        // `1` onward, one per sample.
        number = number.saturating_add(1);
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
        },
        pictures,
    ))
}

/// Builds the check `spec` describes from the `session`.
///
/// # Errors
///
/// Returns a [`SpecError`] that tells the user what to fix: an empty region,
/// a region, screenshot, recorded reading, or saved reference that does not
/// exist, two bar colors that are the same, or a label the reference cannot
/// learn from.
pub(crate) fn build(spec: &CheckSpec, session: Session<'_>) -> Result<RegionCheck, SpecError> {
    let region = match spec {
        CheckSpec::Number { region, .. }
        | CheckSpec::Characters { region, .. }
        | CheckSpec::BarFill { region, .. }
        | CheckSpec::Color { region, .. }
        | CheckSpec::Image { region, .. } => to_region(*region)?,
    };
    let check = match spec {
        CheckSpec::Number {
            reference, whole, ..
        } => {
            let saved = (session.references)(reference).map_err(SpecError::Reference)?;
            Check::Number {
                reader: saved_reader(&saved.reference, &saved.pictures, None)?,
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
        CheckSpec::BarFill { filled, empty, .. } => Check::BarFill(
            BarFill::new((*filled).into(), (*empty).into())
                .map_err(|_same| SpecError::SameColors)?,
        ),
        CheckSpec::Color {
            color, tolerance, ..
        } => Check::Color(ColorMatch {
            color: (*color).into(),
            tolerance: *tolerance,
        }),
        CheckSpec::Image {
            screenshot,
            tolerance,
            selection,
            ..
        } => {
            let (picture, mask) =
                picture_of(session.screenshots, region, *screenshot, selection.as_ref())?;
            // The mask is the region's, so its size is always the picture's.
            Check::Image(
                ImageMatch::new(picture, mask.as_ref(), *tolerance)
                    .map_err(|_size| SpecError::NothingSelected)?,
            )
        },
    };
    Ok(RegionCheck { region, check })
}

/// The picture an image check compares against: `region` of the screenshot
/// at `screenshot`, and with a `selection` the mask of the pixels of
/// `region` it selects on that screenshot.
fn picture_of(
    screenshots: &[Image],
    region: Region,
    screenshot: usize,
    selection: Option<&SelectionSpec>,
) -> Result<(Image, Option<Mask>), SpecError> {
    let image = screenshots.get(screenshot).ok_or(SpecError::NoScreenshot)?;
    let picture = image.crop(region).ok_or(SpecError::Outside)?;
    let mask = selection
        .map(|selection| {
            selection::select(image, selection)
                .and_then(|selected| selected.mask_in(region))
                .ok_or(SpecError::NothingSelected)
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

/// The reader of a [`CheckSpec::Characters`] before its recorded readings:
/// the saved reference's samples relearned with the spec's own color and
/// tolerance, then its labeled screenshots, cut at `region`.
fn taught_reader(
    spec: &CheckSpec,
    session: Session<'_>,
    region: Region,
) -> Result<DigitReader, SpecError> {
    let CheckSpec::Characters {
        reference,
        ink,
        tolerance,
        labels,
        ..
    } = spec
    else {
        return Err(SpecError::NotAValue);
    };
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
            &cut(session.screenshots, region, label.screenshot)?,
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
fn cut(screenshots: &[Image], region: Region, screenshot: usize) -> Result<Image, SpecError> {
    screenshots
        .get(screenshot)
        .ok_or(SpecError::NoScreenshot)?
        .crop(region)
        .ok_or(SpecError::Outside)
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

/// `text` as a sentence: capitalized, with a full stop.
pub(crate) fn sentence(text: &str) -> String {
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| {
        format!("{}{}.", first.to_uppercase(), characters.as_str())
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

/// A check the UI described cannot be built.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SpecError {
    /// The region has no width or no height.
    EmptyRegion,
    /// A screenshot the check names does not exist in this session.
    NoScreenshot,
    /// The region reaches past the edge of the screenshot.
    Outside,
    /// The bar's filled and empty colors are the same.
    SameColors,
    /// An image check's selection leaves no pixel of its region to compare.
    NothingSelected,
    /// A reference has no saved sample, labeled screenshot, or kept reading
    /// to learn from.
    NoLabels,
    /// A recorded reading the check names is not in this session's
    /// recording.
    NoRecording,
    /// The region is not the one the session's recording recorded.
    RecordedElsewhere,
    /// A picture could not be encoded as PNG.
    Picture,
    /// A saved reference the check names cannot be had, and why.
    Reference(String),
    /// A value's reference belongs to another application, or the value's
    /// application is not saved yet.
    OtherApplication,
    /// Characters still being taught were saved as a value, or something
    /// other than characters as a reference.
    NotAValue,
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
            Self::SameColors => {
                formatter.write_str("Pick a filled color and an empty color that differ.")
            },
            Self::NothingSelected => formatter.write_str(
                "Nothing in the region is selected. Select the thing again, or stop ignoring the background.",
            ),
            Self::NoLabels => formatter.write_str(
                "Type what the region shows on at least one screenshot, or keep a recorded reading.",
            ),
            Self::NoRecording => {
                formatter.write_str("That recording is gone. Record the region again.")
            },
            Self::RecordedElsewhere => formatter.write_str(
                "The region changed since it was recorded. Put it back, or record it again.",
            ),
            Self::Picture => formatter.write_str("A picture of the region could not be saved."),
            Self::Reference(why) => formatter.write_str(why),
            Self::OtherApplication => formatter.write_str(
                "That reference belongs to another application. Pick one of this application's.",
            ),
            Self::NotAValue => formatter.write_str("Pick a reference to read the number with."),
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

#[cfg(test)]
mod tests {
    use appytappy_config::signal::{Number, Value};
    use appytappy_sources::screen::Size;

    use super::*;

    const RED: ColorSpec = ColorSpec {
        red: 255,
        green: 0,
        blue: 0,
    };
    const GRAY: ColorSpec = ColorSpec {
        red: 40,
        green: 40,
        blue: 40,
    };

    /// A 4 by 2 image: its left half red and its right half gray.
    fn screenshot() -> Image {
        let red = [0, 0, 255, 0];
        let gray = [40, 40, 40, 0];
        let row = [red, red, gray, gray].concat();
        Image::new(
            Size {
                width: 4,
                height: 2,
            },
            [row.clone(), row].concat(),
        )
        .expect("matching length")
    }

    fn whole() -> RegionSpec {
        RegionSpec {
            x: 0,
            y: 0,
            width: 4,
            height: 2,
        }
    }

    fn left() -> RegionSpec {
        RegionSpec {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        }
    }

    fn no_references(_reference: &ReferenceRef) -> Result<SavedReference, String> {
        Err("That reference is not saved.".to_owned())
    }

    /// The saved reference `digits` of the application `app`: red
    /// characters, the red square of [`screenshot`] being a `1`.
    fn digits(reference: &ReferenceRef) -> Result<SavedReference, String> {
        if (reference.app.as_str(), reference.id.as_str()) != ("app", "digits") {
            return no_references(reference);
        }
        Ok(SavedReference {
            reference: ReferenceDef {
                id: "digits".to_owned(),
                name: "Digits".to_owned(),
                ink: RED.into(),
                tolerance: 0,
                samples: vec![SampleDef {
                    image: sample_name("digits", 1),
                    text: "1".to_owned(),
                }],
            },
            pictures: vec![
                screenshot()
                    .crop(Region::new(0, 0, 2, 2).expect("not empty"))
                    .expect("inside"),
            ],
        })
    }

    fn digits_ref() -> ReferenceRef {
        ReferenceRef {
            app: "app".to_owned(),
            id: "digits".to_owned(),
        }
    }

    fn session<'a>(screenshots: &'a [Image], recording: Option<&'a Recording>) -> Session<'a> {
        Session {
            screenshots,
            recording,
            references: &digits,
        }
    }

    fn characters(
        region: RegionSpec,
        reference: Option<ReferenceRef>,
        labels: Vec<Label>,
        recorded: Vec<RecordedLabel>,
    ) -> CheckSpec {
        CheckSpec::Characters {
            region,
            reference,
            ink: RED,
            tolerance: 0,
            labels,
            recorded,
        }
    }

    fn one(screenshot: usize) -> Vec<Label> {
        vec![Label {
            screenshot,
            text: "1".to_owned(),
        }]
    }

    #[test]
    fn a_problem_reads_as_a_sentence_of_its_own() {
        assert_eq!(sentence("a character was given"), "A character was given.");
        assert_eq!(sentence(""), "");
    }

    #[test]
    fn a_bar_fill_reads_the_share_of_filled_pixels() {
        // Two red columns and one gray column, so a swap of the filled and
        // empty colors reads a third instead of two thirds.
        let spec = CheckSpec::BarFill {
            region: RegionSpec {
                x: 0,
                y: 0,
                width: 3,
                height: 2,
            },
            filled: RED,
            empty: GRAY,
        };
        let built = build(&spec, session(&[], None)).expect("valid bar");
        assert_eq!(built.region, Region::new(0, 0, 3, 2).expect("not empty"));
        let bar = screenshot().crop(built.region).expect("inside");
        let value = built.check.read(&bar).expect("readable");
        assert_eq!(
            value,
            Value::Number(Number::new(400.0 / 6.0).expect("finite"))
        );
    }

    #[test]
    fn an_image_is_cut_from_the_named_screenshot_at_the_region() {
        let spec = CheckSpec::Image {
            region: left(),
            screenshot: 0,
            tolerance: 0,
            selection: None,
        };
        let built = build(&spec, session(&[screenshot()], None)).expect("valid image");
        let red_square = screenshot().crop(built.region).expect("inside");
        let gray_square = screenshot()
            .crop(Region::new(2, 0, 2, 2).expect("not empty"))
            .expect("inside");
        assert_eq!(built.check.read(&red_square), Ok(Value::Flag(true)));
        assert_eq!(built.check.read(&gray_square), Ok(Value::Flag(false)));
    }

    #[test]
    fn a_selected_image_compares_only_what_was_selected() {
        // The click selects the red half, so the gray half is background.
        let spec = |region| CheckSpec::Image {
            region,
            screenshot: 0,
            tolerance: 0,
            selection: Some(SelectionSpec {
                tolerance: 10,
                steps: vec![selection::SelectStep::Click {
                    x: 0,
                    y: 0,
                    combine: selection::Combine::Replace,
                }],
            }),
        };
        let built = build(&spec(whole()), session(&[screenshot()], None)).expect("valid image");
        let repaint = |columns: std::ops::Range<usize>, color: [u8; 4]| {
            let mut pixels = screenshot().pixels().to_vec();
            for (index, pixel) in pixels.iter_mut().enumerate() {
                if columns.contains(&(index % 4)) {
                    *pixel = color;
                }
            }
            Image::new(screenshot().size(), pixels.concat()).expect("matching length")
        };
        let blue = [255, 0, 0, 0];
        assert_eq!(
            built.check.read(&repaint(2..4, blue)),
            Ok(Value::Flag(true)),
            "a new background still matches"
        );
        assert_eq!(
            built.check.read(&repaint(0..2, blue)),
            Ok(Value::Flag(false))
        );

        let saved =
            to_saved(&spec(whole()), session(&[screenshot()], None), None, "icon").expect("valid");
        let (picture, mask) =
            Image::decode_png_masked(&saved.pictures[0].1).expect("a PNG with a mask");
        assert_eq!(picture.size(), screenshot().size());
        assert_eq!(
            mask.expect("the background is left out").kept(),
            [true, true, false, false, true, true, false, false]
        );

        let gray_half = RegionSpec {
            x: 2,
            y: 0,
            width: 2,
            height: 2,
        };
        assert_eq!(
            build(&spec(gray_half), session(&[screenshot()], None)).err(),
            Some(SpecError::NothingSelected),
            "a region that holds none of the selection has nothing to compare"
        );
    }

    #[test]
    fn a_color_check_matches_the_regions_average_color() {
        let spec = CheckSpec::Color {
            region: whole(),
            color: RED,
            tolerance: 10,
        };
        let built = build(&spec, session(&[], None)).expect("valid color");
        let red_square = screenshot()
            .crop(Region::new(0, 0, 2, 2).expect("not empty"))
            .expect("inside");
        assert_eq!(built.check.read(&red_square), Ok(Value::Flag(true)));
        assert_eq!(built.check.read(&screenshot()), Ok(Value::Flag(false)));
    }

    #[test]
    fn a_number_reads_with_its_saved_reference() {
        let spec = |whole| CheckSpec::Number {
            region: left(),
            reference: digits_ref(),
            whole,
        };
        let built = build(&spec(true), session(&[], None)).expect("a saved reference");
        let red_square = screenshot().crop(built.region).expect("inside");
        assert_eq!(
            built.check.read(&red_square),
            Ok(Value::Number(Number::new(1.0).expect("finite")))
        );
        assert!(matches!(built.check, Check::Number { whole: true, .. }));
        assert!(matches!(
            build(&spec(false), session(&[], None)).map(|built| built.check),
            Ok(Check::Number { whole: false, .. })
        ));
        let saved = to_saved(&spec(true), session(&[], None), Some("app"), "progress")
            .expect("a value of the reference's application");
        assert_eq!(
            saved.check,
            CheckDef::Number {
                reference: "digits".to_owned(),
                whole: true,
            }
        );
        assert!(
            saved.pictures.is_empty(),
            "the reference keeps the pictures"
        );
        for app in [None, Some("other")] {
            assert_eq!(
                to_saved(&spec(true), session(&[], None), app, "progress"),
                Err(SpecError::OtherApplication)
            );
        }
        let gone = CheckSpec::Number {
            region: left(),
            reference: ReferenceRef {
                app: "app".to_owned(),
                id: "gone".to_owned(),
            },
            whole: false,
        };
        let error = build(&gone, session(&[], None)).expect_err("not saved");
        assert_eq!(
            error,
            SpecError::Reference("That reference is not saved.".to_owned())
        );
        assert_eq!(error.to_string(), "That reference is not saved.");
    }

    #[test]
    fn characters_being_taught_read_but_are_not_a_value() {
        let spec = characters(left(), None, one(0), Vec::new());
        let built = build(&spec, session(&[screenshot()], None)).expect("learns the 1");
        let red_square = screenshot().crop(built.region).expect("inside");
        assert_eq!(
            built.check.read(&red_square),
            Ok(Value::Number(Number::new(1.0).expect("finite")))
        );
        let error = to_saved(
            &spec,
            session(&[screenshot()], None),
            Some("app"),
            "progress",
        )
        .expect_err("a reference, not a value");
        assert_eq!(error, SpecError::NotAValue);
        assert_eq!(
            error.to_string(),
            "Pick a reference to read the number with."
        );
        assert_eq!(
            to_saved_reference(
                &CheckSpec::Color {
                    region: left(),
                    color: RED,
                    tolerance: 0,
                },
                session(&[], None),
                "digits",
                "Digits",
            ),
            Err(SpecError::NotAValue)
        );
        // A saved reference teaches on its own, with no new label.
        let saved_only = characters(left(), Some(digits_ref()), Vec::new(), Vec::new());
        assert!(build(&saved_only, session(&[], None)).is_ok());
    }

    #[test]
    fn mistakes_come_back_as_errors_the_user_can_fix() {
        let empty = RegionSpec {
            x: 0,
            y: 0,
            width: 0,
            height: 2,
        };
        let outside = RegionSpec {
            x: 3,
            y: 0,
            width: 2,
            height: 2,
        };
        let cases = [
            (
                CheckSpec::Color {
                    region: empty,
                    color: RED,
                    tolerance: 0,
                },
                SpecError::EmptyRegion,
            ),
            (
                CheckSpec::BarFill {
                    region: whole(),
                    filled: RED,
                    empty: RED,
                },
                SpecError::SameColors,
            ),
            (
                CheckSpec::Image {
                    region: whole(),
                    screenshot: 1,
                    tolerance: 0,
                    selection: None,
                },
                SpecError::NoScreenshot,
            ),
            (
                CheckSpec::Image {
                    region: outside,
                    screenshot: 0,
                    tolerance: 0,
                    selection: None,
                },
                SpecError::Outside,
            ),
            (
                characters(whole(), None, Vec::new(), Vec::new()),
                SpecError::NoLabels,
            ),
        ];
        for (spec, expected) in cases {
            assert_eq!(
                build(&spec, session(&[screenshot()], None)),
                Err(expected),
                "{spec:?}"
            );
        }
    }

    #[test]
    fn a_label_the_reference_cannot_learn_names_the_text() {
        let spec = characters(
            whole(),
            None,
            vec![Label {
                screenshot: 0,
                text: "12".to_owned(),
            }],
            Vec::new(),
        );
        let error =
            build(&spec, session(&[screenshot()], None)).expect_err("one blob, two characters");
        assert!(matches!(&error, SpecError::Learn { text, .. } if text == "12"));
        assert!(
            error
                .to_string()
                .starts_with("Could not learn from \"12\": ")
        );
        assert!(error.source().is_some());
    }

    #[test]
    fn the_ui_describes_checks_in_camel_case_json() {
        // The owner's requests parse the other kinds; these are the numbers.
        let json = r#"{"kind":"characters","region":{"x":1,"y":2,"width":3,"height":4},
            "ink":{"red":1,"green":2,"blue":3},"tolerance":5,
            "labels":[{"screenshot":0,"text":"12"}]}"#;
        let spec: CheckSpec = serde_json::from_str(json).expect("valid JSON");
        assert_eq!(
            spec,
            CheckSpec::Characters {
                region: RegionSpec {
                    x: 1,
                    y: 2,
                    width: 3,
                    height: 4
                },
                reference: None,
                ink: ColorSpec {
                    red: 1,
                    green: 2,
                    blue: 3
                },
                tolerance: 5,
                labels: vec![Label {
                    screenshot: 0,
                    text: "12".to_owned(),
                }],
                recorded: Vec::new(),
            }
        );
        let json = r#"{"kind":"number","region":{"x":0,"y":0,"width":2,"height":2},
            "reference":{"app":"app","id":"digits"},"whole":true}"#;
        assert_eq!(
            serde_json::from_str::<CheckSpec>(json).expect("valid JSON"),
            CheckSpec::Number {
                region: left(),
                reference: digits_ref(),
                whole: true,
            }
        );
    }

    #[test]
    fn a_new_reference_names_one_picture_per_label() {
        let spec = characters(left(), None, one(0), Vec::new());
        let (reference, pictures) =
            to_saved_reference(&spec, session(&[screenshot()], None), "digits", "Digits")
                .expect("valid characters");
        assert_eq!(
            reference,
            ReferenceDef {
                id: "digits".to_owned(),
                name: "Digits".to_owned(),
                ink: RED.into(),
                tolerance: 0,
                samples: vec![SampleDef {
                    image: "ref-digits-1.png".to_owned(),
                    text: "1".to_owned(),
                }],
            }
        );
        assert_eq!(pictures.len(), 1);
        assert_eq!(pictures[0].0, "ref-digits-1.png");
        // A PNG keeps the colors and drops the unused fourth byte.
        let picture = Image::decode_png(&pictures[0].1).expect("a PNG");
        let cropped = screenshot()
            .crop(Region::new(0, 0, 2, 2).expect("not empty"))
            .expect("inside the screenshot");
        assert_eq!(picture.size(), cropped.size());
        let colors = |image: &Image| -> Vec<[u8; 3]> {
            image
                .pixels()
                .iter()
                .map(|&[blue, green, red, _]| [blue, green, red])
                .collect()
        };
        assert_eq!(colors(&picture), colors(&cropped));
    }

    #[test]
    fn a_saved_reference_keeps_its_samples_and_numbers_new_ones_after_them() {
        let spec = CheckSpec::Characters {
            region: left(),
            reference: Some(digits_ref()),
            ink: RED,
            tolerance: 3,
            labels: one(0),
            recorded: Vec::new(),
        };
        let (reference, pictures) = to_saved_reference(
            &spec,
            session(&[screenshot()], None),
            "digits",
            "Red digits",
        )
        .expect("valid characters");
        let images: Vec<&str> = reference
            .samples
            .iter()
            .map(|sample| sample.image.as_str())
            .collect();
        assert_eq!(images, ["ref-digits-1.png", "ref-digits-2.png"]);
        assert_eq!(
            (reference.name.as_str(), reference.tolerance),
            ("Red digits", 3)
        );
        let names: Vec<&str> = pictures.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            ["ref-digits-2.png"],
            "only the new picture is written"
        );
    }

    #[test]
    fn recorded_readings_are_learned_and_saved_after_the_labels() {
        let now = std::time::Instant::now();
        let region = Region::new(0, 0, 4, 2).expect("not empty");
        let mut recording = Recording::new(1, region, (RED.into(), 0), screenshot().size(), now);
        recording.offer(&screenshot(), now).expect("a reading");
        let recorded = |reading| {
            vec![RecordedLabel {
                reading,
                text: "1".to_owned(),
            }]
        };
        let spec = characters(whole(), None, one(0), recorded(0));
        let (reference, pictures) = to_saved_reference(
            &spec,
            session(&[screenshot()], Some(&recording)),
            "digits",
            "Digits",
        )
        .expect("valid characters");
        let images: Vec<&str> = reference
            .samples
            .iter()
            .map(|sample| sample.image.as_str())
            .collect();
        assert_eq!(images, ["ref-digits-1.png", "ref-digits-2.png"]);
        let picture = Image::decode_png(&pictures[1].1).expect("a PNG");
        assert_eq!(picture.size(), recording.readings()[0].image.size());

        let only_recorded = characters(whole(), None, Vec::new(), recorded(0));
        assert!(build(&only_recorded, session(&[], Some(&recording))).is_ok());
        assert_eq!(
            build(&only_recorded, session(&[], None)),
            Err(SpecError::NoRecording)
        );
        assert_eq!(
            build(
                &characters(whole(), None, Vec::new(), recorded(1)),
                session(&[], Some(&recording))
            ),
            Err(SpecError::NoRecording)
        );
        let moved = characters(left(), None, Vec::new(), recorded(0));
        let error = build(&moved, session(&[], Some(&recording))).expect_err("another region");
        assert_eq!(error, SpecError::RecordedElsewhere);
        assert_eq!(
            error.to_string(),
            "The region changed since it was recorded. Put it back, or record it again."
        );
    }

    #[test]
    fn recorded_problems_judge_each_reading_after_the_saved_samples() {
        let now = std::time::Instant::now();
        let region = Region::new(0, 0, 2, 2).expect("not empty");
        let mut recording = Recording::new(1, region, (RED.into(), 0), screenshot().size(), now);
        recording.offer(&screenshot(), now).expect("a reading");
        let as_text = |text: &str| {
            vec![RecordedLabel {
                reading: 0,
                text: text.to_owned(),
            }]
        };
        // The saved reference took the red square for a 1.
        let spec = characters(left(), Some(digits_ref()), Vec::new(), as_text("7"));
        let problems =
            recorded_problems(&spec, session(&[], Some(&recording))).expect("a valid spec");
        assert_eq!(problems.len(), 1);
        assert!(problems[0].is_some(), "{problems:?}");
        let spec = characters(left(), Some(digits_ref()), Vec::new(), as_text("1"));
        assert_eq!(
            recorded_problems(&spec, session(&[], Some(&recording))),
            Ok(vec![None])
        );
        let color = CheckSpec::Color {
            region: left(),
            color: RED,
            tolerance: 0,
        };
        assert_eq!(
            recorded_problems(&color, session(&[], Some(&recording))),
            Ok(Vec::new())
        );
    }

    #[test]
    fn saved_checks_keep_their_settings() {
        let region = RegionSpec {
            x: 2,
            y: 0,
            width: 2,
            height: 2,
        };
        let image = CheckSpec::Image {
            region,
            screenshot: 0,
            tolerance: 7,
            selection: None,
        };
        let saved =
            to_saved(&image, session(&[screenshot()], None), None, "icon_visible").expect("valid");
        assert_eq!(
            saved.check,
            CheckDef::Image {
                image: "icon_visible.png".to_owned(),
                tolerance: 7
            }
        );
        assert_eq!(saved.pictures[0].0, "icon_visible.png");
        assert_eq!((saved.region.x, saved.region.width), (2, 2));

        let bar = CheckSpec::BarFill {
            region,
            filled: RED,
            empty: GRAY,
        };
        let saved = to_saved(&bar, session(&[], None), None, "bar").expect("valid");
        assert_eq!(
            saved.check,
            CheckDef::BarFill {
                filled: RED.into(),
                empty: GRAY.into()
            }
        );
        assert_eq!(saved.pictures, Vec::<(String, Vec<u8>)>::new());

        let color = CheckSpec::Color {
            region,
            color: GRAY,
            tolerance: 4,
        };
        assert_eq!(
            to_saved(&color, session(&[], None), None, "color")
                .expect("valid")
                .check,
            CheckDef::Color {
                color: GRAY.into(),
                tolerance: 4
            }
        );
        assert_eq!(
            to_saved(&bar, session(&[], None), None, "bar").map(|saved| saved.region),
            Ok(RegionDef {
                x: 2,
                y: 0,
                width: 2,
                height: 2
            })
        );
        assert_eq!(
            SpecError::Picture.to_string(),
            "A picture of the region could not be saved."
        );
        assert_eq!(
            SpecError::NothingSelected.to_string(),
            "Nothing in the region is selected. Select the thing again, or stop ignoring the background."
        );
        assert_eq!(
            SpecError::OtherApplication.to_string(),
            "That reference belongs to another application. Pick one of this application's."
        );
    }
}
