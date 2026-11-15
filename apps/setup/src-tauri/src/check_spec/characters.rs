//! A reference being taught: the characters the user labeled on
//! screenshots and kept from a recording, and how they become a saved
//! reference.
//!
//! [`CharactersSpec::teach`] writes the reference as its application's file
//! spells it, with the pictures of its new samples, and learns it back with
//! `saved::reader`, as the runtime does. So the reader the editor tries on
//! the live window is the one the saved file gives.

use std::io;

use appytappy_config::app_file::{ColorDef, OriginDef, ReferenceDef, RegionDef, SampleDef};
use appytappy_config::names::sample_name;
use appytappy_sources::screen::digits::{DigitReader, InkRule};
use appytappy_sources::screen::saved::{self, SavedError};
use appytappy_sources::screen::{Image, Region, Size};
use serde::Deserialize;

use crate::recording::Recording;
use crate::screenshots::Shots;
use crate::words::sentence;

use super::{
    Cut, Pictures, ReferenceRef, SpecError, contrast, cut, drawn, ink_rule, same_size, size_of,
    to_region,
};

/// The ID a reference is taught under before it is saved under its own.
const UNSAVED: &str = "unsaved";

/// What one screenshot's region shows, typed by the user so a reference
/// learns its characters.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct Label {
    pub(crate) screenshot: usize,
    /// Where on the screenshot the text shows, when it is not the spec's
    /// region: the box the user drew around it, or kept from found text.
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) region: Option<RegionDef>,
    pub(crate) text: String,
}

/// What one recorded reading shows, as the user confirmed it in review, so
/// a reference learns its characters like a box's.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct RecordedLabel {
    /// The reading's index in the recording.
    pub(crate) reading: usize,
    /// What it shows.
    pub(crate) text: String,
}

/// The characters a reference is being taught, as the UI describes them:
/// the saved samples of `reference`, when it is saved, then `labels`, then
/// `recorded`, their characters told from the background by `ink` and
/// `tolerance`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct CharactersSpec {
    /// Where the characters show, which a recording records and the live
    /// window is read at.
    pub(crate) region: RegionDef,
    /// The screenshot the region was drawn on. Every screenshot with a
    /// box and the recording must have its window size, since the region
    /// means another spot on a window of another size.
    pub(crate) screenshot: usize,
    /// The saved reference these characters add to, or `None` for a new
    /// one.
    #[serde(default)]
    pub(crate) reference: Option<ReferenceRef>,
    /// The reference's text color, or `None` when it finds its characters
    /// by contrast.
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) ink: Option<ColorDef>,
    /// The reference's tolerance, as `ReferenceDef` has it.
    pub(crate) tolerance: u8,
    /// The labeled screenshots.
    pub(crate) labels: Vec<Label>,
    /// Readings of the session's recording, which must be of `region`.
    #[serde(default)]
    pub(crate) recorded: Vec<RecordedLabel>,
}

/// A saved reference and those of its samples' pictures that can be read,
/// each as a file name and PNG bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SavedReference {
    /// The reference as its application's file has it.
    pub(crate) reference: ReferenceDef,
    /// Its samples' pictures.
    pub(crate) pictures: Pictures,
}

impl SavedReference {
    /// The reference as it is saved under the name `name`, with the text
    /// color `ink`, or none to find its characters by contrast, and the
    /// tolerance `tolerance`. Its samples stay as they are saved, so no
    /// screenshot is needed, and it learns them back with the new text
    /// color and tolerance. Without a text color, the tolerance is the one
    /// [`contrast::fitted`] chooses from those samples: `tolerance` while
    /// every sample reads with it.
    ///
    /// # Errors
    ///
    /// Returns why a sample cannot be learned.
    pub(crate) fn with_settings(
        self,
        name: &str,
        ink: Option<ColorDef>,
        tolerance: u8,
    ) -> Result<ReferenceDef, SpecError> {
        let saved_name = self.reference.name.clone();
        let tolerance = match ink {
            Some(_) => tolerance,
            None => contrast::fitted(&self.samples(), tolerance),
        };
        let reference = ReferenceDef {
            name: name.to_owned(),
            ink,
            tolerance,
            ..self.reference
        };
        learned(&reference, self.pictures.iter(), Some(saved_name)).map(|_reader| reference)
    }
}

impl SavedReference {
    /// Each sample's picture and the text it shows, in order. A sample
    /// whose picture cannot be read is left out.
    fn samples(&self) -> Vec<(Image, &str)> {
        self.reference
            .samples
            .iter()
            .filter_map(|sample| {
                let (_, png) = self
                    .pictures
                    .iter()
                    .find(|(name, _)| *name == sample.image)?;
                Some((Image::decode_png(png).ok()?, sample.text.as_str()))
            })
            .collect()
    }
}

/// The reader `reference` learns from its samples' `pictures`, as
/// `saved::reader` learns it. Only a saved sample can be missing, so that
/// problem names the saved reference, `saved_name`, whatever the one
/// learned here is called.
fn learned<'a>(
    reference: &ReferenceDef,
    pictures: impl Iterator<Item = &'a (String, Vec<u8>)> + Clone,
    saved_name: Option<String>,
) -> Result<DigitReader, SpecError> {
    saved::reader(reference, |picture| {
        pictures
            .clone()
            .find(|(name, _)| name == picture)
            .map(|(_, png)| png.clone())
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    })
    .map_err(|error| match sample_problem(error, reference) {
        SpecError::MissingSample { picture, .. } => SpecError::MissingSample {
            reference: saved_name.unwrap_or_else(|| reference.name.clone()),
            picture,
        },
        problem => problem,
    })
}

/// The session's screenshots, recording, and saved references, which the
/// characters the UI describes name.
#[derive(Clone, Copy)]
pub(crate) struct Session<'a> {
    pub(crate) screenshots: &'a dyn Shots,
    pub(crate) recording: Option<&'a Recording>,
    /// The saved reference a [`ReferenceRef`] names, or why it cannot be
    /// had.
    pub(crate) references: &'a dyn Fn(&ReferenceRef) -> Result<SavedReference, String>,
}

/// A reference as taught, and what it learned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Taught {
    /// The region the characters were drawn in.
    pub(crate) region: Region,
    /// The window size of the screenshot the region was drawn on.
    pub(crate) window_size: Size,
    /// The reference as its application's file spells it: a saved
    /// reference's samples, then the new ones.
    pub(crate) reference: ReferenceDef,
    /// The pictures of its new samples.
    pub(crate) pictures: Pictures,
    /// The reader `saved::reader` learns from those pictures and the saved
    /// reference's, as the session had them.
    pub(crate) reader: DigitReader,
}

impl CharactersSpec {
    /// How the reference tells ink from background.
    fn ink_rule(&self) -> InkRule {
        ink_rule(self.ink, self.tolerance)
    }

    /// The region and the window size of the screenshot it was drawn on,
    /// which must hold it.
    ///
    /// # Errors
    ///
    /// Returns [`SpecError::EmptyRegion`], [`SpecError::NoScreenshot`], or
    /// [`SpecError::Outside`].
    pub(crate) fn drawn(&self, screenshots: &dyn Shots) -> Result<(Region, Size), SpecError> {
        drawn(screenshots, self.region, self.screenshot)
    }

    /// The reference these characters teach, saved with the ID `id` and the
    /// name `name`, and what it learned. A saved reference keeps its samples
    /// and their pictures, and the new ones are numbered after them:
    /// `ref-{id}-1.png` onward, the labeled screenshots first and then the
    /// recorded readings. `None` for `id` teaches a saved reference under
    /// its own ID, or a new one under a stand-in.
    ///
    /// # Errors
    ///
    /// Returns [`SpecError::NoLabels`] when there is nothing to learn from,
    /// or what [`Self::learn`] does.
    pub(crate) fn teach(
        &self,
        session: Session<'_>,
        id: Option<&str>,
        name: &str,
    ) -> Result<Taught, SpecError> {
        if self.reference.is_none() && self.labels.is_empty() && self.recorded.is_empty() {
            return Err(SpecError::NoLabels);
        }
        self.learn(session, id, name, &self.recorded)
    }

    /// The reference the saved samples, the labeled screenshots and, of the
    /// recorded readings, `recorded` teach, as [`Self::teach`] saves it.
    ///
    /// # Errors
    ///
    /// Returns a [`SpecError`] that tells the user what to fix: an empty
    /// region, a region, screenshot, recorded reading, or saved reference
    /// that does not exist, a labeled screenshot or a recording of another
    /// window size, a picture that cannot be encoded, or a label the
    /// reference cannot learn from.
    fn learn(
        &self,
        session: Session<'_>,
        id: Option<&str>,
        name: &str,
        recorded: &[RecordedLabel],
    ) -> Result<Taught, SpecError> {
        let (region, window_size) = self.drawn(session.screenshots)?;
        self.same_sizes(session, window_size)?;
        let saved = self
            .reference
            .as_ref()
            .map(|saved| (session.references)(saved).map_err(SpecError::Reference))
            .transpose()?;
        let id = id
            .or_else(|| self.reference.as_ref().map(|saved| saved.id.as_str()))
            .unwrap_or(UNSAVED);
        let (mut samples, kept, made, saved_name): (Vec<SampleDef>, Pictures, OriginDef, _) = saved
            .map_or_else(Default::default, |saved| {
                (
                    saved.reference.samples,
                    saved.pictures,
                    saved.reference.origin,
                    Some(saved.reference.name),
                )
            });
        // A recording that teaches a reference made by hand makes it one a
        // recording made. One that says where it was imported from goes on
        // saying so.
        let by_recording = !recorded.is_empty()
            || self
                .labels
                .iter()
                .any(|label| session.screenshots.is_recorded(label.screenshot));
        let origin = if made == OriginDef::ByHand && by_recording {
            OriginDef::Recording
        } else {
            made
        };
        let new = self.new_samples(session, region, recorded)?;
        let mut pictures = Vec::new();
        let mut number = samples.len();
        for (image, text) in new {
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
                text: text.to_owned(),
            });
        }
        let reference = ReferenceDef {
            id: id.to_owned(),
            name: name.to_owned(),
            origin,
            ink: self.ink,
            tolerance: self.tolerance,
            samples,
        };
        let reader = learned(&reference, pictures.iter().chain(&kept), saved_name)?;
        Ok(Taught {
            region,
            window_size,
            reference,
            pictures,
            reader,
        })
    }

    /// The pictures and texts of the labeled screenshots and then, of the
    /// recorded readings, `recorded`: the samples these characters add to
    /// the saved ones, in the order the reference learns them.
    fn new_samples<'a>(
        &'a self,
        session: Session<'_>,
        region: Region,
        recorded: &'a [RecordedLabel],
    ) -> Result<Vec<(Image, &'a str)>, SpecError> {
        let labeled = self.labels.iter().map(|label| {
            let image = label_image(session.screenshots, region, label, self.ink_rule())?;
            Ok((image, label.text.as_str()))
        });
        let kept_readings = recorded.iter().map(|label| {
            let image = recorded_image(session.recording, region, label.reading)?;
            Ok((image.clone(), label.text.as_str()))
        });
        labeled.chain(kept_readings).collect()
    }

    /// The tolerance the reference these characters teach finds its
    /// characters by contrast with, chosen from its samples as
    /// [`contrast::fitted`] describes: the spec's own while every sample
    /// reads with it. A reference with a text color keeps its tolerance,
    /// which the user sets.
    ///
    /// # Errors
    ///
    /// Returns what [`Self::of_samples`] does.
    pub(crate) fn contrast_tolerance(&self, session: Session<'_>) -> Result<u8, SpecError> {
        if self.ink.is_some() {
            return Ok(self.tolerance);
        }
        self.of_samples(session, |samples| contrast::fitted(samples, self.tolerance))
    }

    /// Whether the reference these characters teach finds its characters
    /// by contrast with a tolerance that leaves an outline or a shadow of
    /// one of its samples out, as [`contrast::hides_a_tone`] describes, so
    /// that it reads only over backgrounds that hide it too. A reference
    /// with a text color counts only that color, whatever is behind it.
    ///
    /// # Errors
    ///
    /// Returns what [`Self::of_samples`] does.
    pub(crate) fn hides_a_tone(&self, session: Session<'_>) -> Result<bool, SpecError> {
        if self.ink.is_some() {
            return Ok(false);
        }
        self.of_samples(session, |samples| {
            contrast::hides_a_tone(samples, self.tolerance)
        })
    }

    /// What `of` makes of the samples the reference these characters teach
    /// learns from, each a picture and the text it shows: the saved ones,
    /// then the labeled screenshots, then the recorded readings. A saved
    /// sample whose picture cannot be read is left out; teaching the
    /// reference reports it.
    ///
    /// # Errors
    ///
    /// Returns a [`SpecError`] for an empty region, or a region,
    /// screenshot, recorded reading, or saved reference that does not
    /// exist, and for a labeled screenshot or a recording of another
    /// window size.
    fn of_samples<T>(
        &self,
        session: Session<'_>,
        of: impl FnOnce(&[(Image, &str)]) -> T,
    ) -> Result<T, SpecError> {
        let (region, window_size) = self.drawn(session.screenshots)?;
        self.same_sizes(session, window_size)?;
        let saved = self
            .reference
            .as_ref()
            .map(|saved| (session.references)(saved).map_err(SpecError::Reference))
            .transpose()?;
        let mut samples: Vec<(Image, &str)> =
            saved.iter().flat_map(SavedReference::samples).collect();
        samples.extend(self.new_samples(session, region, &self.recorded)?);
        Ok(of(&samples))
    }

    /// Fails unless every labeled screenshot, and the recording when a
    /// reading of it is kept, has the window size `drawn` of the screenshot
    /// the region was drawn on.
    fn same_sizes(&self, session: Session<'_>, drawn: Size) -> Result<(), SpecError> {
        for label in &self.labels {
            same_size(
                Cut::Label(label.screenshot),
                size_of(session.screenshots, label.screenshot)?,
                self.screenshot,
                drawn,
            )?;
        }
        if let Some(recording) = session.recording.filter(|_| !self.recorded.is_empty()) {
            same_size(
                Cut::Recording,
                recording.window_size(),
                self.screenshot,
                drawn,
            )?;
        }
        Ok(())
    }

    /// Why the reference these characters teach could not learn each of its
    /// recorded readings: `None` for one it learns. It learns the saved
    /// samples and the labeled screenshots first and then the readings in
    /// order, as [`Self::teach`] does, but leaves out each reading it
    /// cannot learn, so the ones after it are judged on the ones that
    /// worked. Review flags the readings this finds, so the reference saved
    /// from the kept ones learns them all.
    ///
    /// # Errors
    ///
    /// Returns what [`Self::learn`] does for the saved samples and the
    /// labeled screenshots.
    pub(crate) fn recorded_problems(
        &self,
        session: Session<'_>,
    ) -> Result<Vec<Option<String>>, SpecError> {
        self.problems(session, &self.recorded, |region, label| {
            recorded_image(session.recording, region, label.reading).cloned()
        })
    }

    /// For each of `found`, labels the user has not added yet, why the
    /// reference these characters teach cannot learn it: `None` for one it
    /// learns. Like [`Self::recorded_problems`], it learns the reference
    /// first and then each label in order, leaving out the ones it cannot
    /// learn.
    ///
    /// # Errors
    ///
    /// Returns what [`Self::recorded_problems`] does.
    pub(crate) fn label_problems(
        &self,
        session: Session<'_>,
        found: &[Label],
    ) -> Result<Vec<Option<String>>, SpecError> {
        self.problems(session, found, |region, label| {
            label_image(session.screenshots, region, label, self.ink_rule())
        })
    }

    /// Why the reference, taught without its recorded readings, cannot
    /// learn each of `more` in order, whose text [`AsRef`] gives and whose
    /// pixels `image` cuts at the region.
    fn problems<L: AsRef<str>>(
        &self,
        session: Session<'_>,
        more: &[L],
        image: impl Fn(Region, &L) -> Result<Image, SpecError>,
    ) -> Result<Vec<Option<String>>, SpecError> {
        let taught = self.learn(session, None, "", &[])?;
        let mut reader = taught.reader;
        Ok(more
            .iter()
            .map(|label| {
                let image = match image(taught.region, label) {
                    Ok(image) => image,
                    Err(error) => return Some(error.to_string()),
                };
                let mut trial = reader.clone();
                match trial.learn(&image, label.as_ref()) {
                    Ok(()) => {
                        reader = trial;
                        None
                    },
                    Err(error) => Some(sentence(&error.to_string())),
                }
            })
            .collect())
    }
}

impl AsRef<str> for Label {
    fn as_ref(&self) -> &str {
        &self.text
    }
}

impl AsRef<str> for RecordedLabel {
    fn as_ref(&self) -> &str {
        &self.text
    }
}

/// Why `reference` cannot be learned, from what `saved::reader` or
/// `saved::build` said of one of its samples' pictures: in the UI's words
/// for a picture that is missing or teaches nothing, and `None` for an
/// error that names no sample of it.
pub(crate) fn sample_problem_of(error: &SavedError, reference: &ReferenceDef) -> Option<SpecError> {
    let (SavedError::Missing { name, .. }
    | SavedError::Picture { name, .. }
    | SavedError::Learn { name, .. }) = error
    else {
        return None;
    };
    let sample = reference
        .samples
        .iter()
        .find(|sample| sample.image == *name)?;
    Some(match error {
        SavedError::Learn { source, .. } => SpecError::Learn {
            text: sample.text.clone(),
            source: source.clone(),
        },
        _ => SpecError::MissingSample {
            reference: reference.name.clone(),
            picture: name.clone(),
        },
    })
}

/// [`sample_problem_of`], or the error in `saved::reader`'s own words.
fn sample_problem(error: SavedError, reference: &ReferenceDef) -> SpecError {
    sample_problem_of(&error, reference).unwrap_or_else(|| error.into())
}

/// What `label` labels, as a reference that tells ink from background by
/// `rule` is given it: its own region of its screenshot, or `region`.
fn label_image(
    screenshots: &dyn Shots,
    region: Region,
    label: &Label,
    rule: InkRule,
) -> Result<Image, SpecError> {
    let region = match label.region {
        Some(own) => to_region(own)?,
        None => region,
    };
    cut(screenshots, region, label.screenshot, rule)
}

/// The pixels of reading `reading` of `recording`, which must be of
/// `region`. The reference relearns them with its own text color and
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
