//! The check the user set up in the UI, and how it becomes the saved form.
//!
//! The UI describes a value's check with plain data, a [`CheckSpec`]: the
//! region, the check's kind, and the colors and screenshots the user picked.
//! [`to_saved`] turns that into what the application's file spells, cutting
//! a state check's pictures from the screenshots it names. Nothing here
//! judges whether the check works: `saved::build` in `appytappy-sources`
//! does, for the check the user tries on the live window and for the one
//! the runtime reads, so the two cannot differ. [`SpecError`] says what
//! only the session knows, such as a screenshot that is gone, and puts
//! what `saved::build` refuses in the UI's words.
//!
//! A reference being taught is a [`CharactersSpec`], which `characters.rs`
//! turns into a saved reference the same way.

use appytappy_config::app_file::{CheckDef, ColorDef, RegionDef, StateDef, state_image_name};
use appytappy_sources::screen::{EmptyRegion, Image, Region, Rgb, Size};
use serde::{Deserialize, Serialize};

use crate::screenshots::Shots;
use crate::selection::{self, SelectionSpec};

pub(crate) mod bar;
pub(crate) mod characters;
mod error;

pub(crate) use characters::{
    CharactersSpec, Label, MAX_EXPECTED_CHARACTERS, SavedReference, Session, sample_problem_of,
};
pub(crate) use error::{Cut, SpecError, sentence};

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
        RegionDef::from(region).into()
    }
}

impl From<RegionDef> for RegionSpec {
    fn from(region: RegionDef) -> Self {
        let RegionDef {
            x,
            y,
            width,
            height,
        } = region;
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

impl TryFrom<RegionSpec> for Region {
    type Error = EmptyRegion;

    fn try_from(region: RegionSpec) -> Result<Self, EmptyRegion> {
        let RegionSpec {
            x,
            y,
            width,
            height,
        } = region;
        Self::try_from(RegionDef {
            x,
            y,
            width,
            height,
        })
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
    fn from(Rgb { red, green, blue }: Rgb) -> Self {
        Self { red, green, blue }
    }
}

impl From<ColorSpec> for Rgb {
    fn from(ColorSpec { red, green, blue }: ColorSpec) -> Self {
        Self { red, green, blue }
    }
}

impl From<ColorSpec> for ColorDef {
    fn from(color: ColorSpec) -> Self {
        Rgb::from(color).into()
    }
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

/// A value's check as the UI describes it.
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

/// Pictures to save beside an application's file, each as a file name and
/// PNG bytes.
pub(crate) type Pictures = Vec<(String, Vec<u8>)>;

/// A check in its saved form: the region, the check as the file spells it,
/// and the pictures it names, each as a file name and PNG bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SavedCheck {
    /// The region the check reads.
    pub(crate) region: RegionDef,
    /// The check.
    pub(crate) check: CheckDef,
    /// The pictures `check` names.
    pub(crate) pictures: Pictures,
}

impl CheckSpec {
    /// The region the check reads, as the UI drew it.
    const fn region(&self) -> RegionSpec {
        match self {
            Self::Number { region, .. }
            | Self::BarFill { region, .. }
            | Self::Color { region, .. }
            | Self::State { region, .. } => *region,
        }
    }
}

/// Turns `spec` into the saved form for the value `id` of the application
/// `app`, cutting a state check's pictures, `{id}-1.png` onward in the
/// order of its states, from their `screenshots`.
///
/// The saved form is not known to work: `saved::build` says whether it
/// does.
///
/// # Errors
///
/// Returns the [`SpecError`] for what the saved form cannot be made from:
/// an empty region, a state's screenshot that is gone, has another window
/// size than the one the region was drawn on, or does not hold the region,
/// a selection that selects nothing, a reference of another application,
/// or a picture that cannot be encoded.
pub(crate) fn to_saved(
    spec: &CheckSpec,
    screenshots: &dyn Shots,
    app: &str,
    id: &str,
) -> Result<SavedCheck, SpecError> {
    to_saved_fix(spec, screenshots, app, id, None)
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
    screenshots: &dyn Shots,
    app: &str,
    id: &str,
    fix: Option<usize>,
) -> Result<SavedCheck, SpecError> {
    let region = to_region(spec.region())?;
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
            drawn_on,
            states,
            tolerance,
            ..
        } => {
            drawn_size(spec, screenshots, *drawn_on)?;
            let mut saved = Vec::with_capacity(states.len());
            for (state, number) in states.iter().zip(1_usize..) {
                let image = state_image_name(id, fix, number);
                pictures.push((image.clone(), state_picture(screenshots, region, state)?));
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
        region: region.into(),
        check,
        pictures,
    })
}

/// The window size of screenshot `drawn_on`, the one the region of `spec`
/// was drawn on, once every screenshot that a state of `spec` is taken from
/// has that size too.
///
/// # Errors
///
/// Returns [`SpecError::NoScreenshot`] for a screenshot that does not exist,
/// or [`SpecError::OtherSize`] naming the first screenshot of another size.
pub(crate) fn drawn_size(
    spec: &CheckSpec,
    screenshots: &dyn Shots,
    drawn_on: usize,
) -> Result<Size, SpecError> {
    let drawn = size_of(screenshots, drawn_on)?;
    if let CheckSpec::State { states, .. } = spec {
        for state in states {
            same_size(
                Cut::State(state.screenshot),
                size_of(screenshots, state.screenshot)?,
                drawn_on,
                drawn,
            )?;
        }
    }
    Ok(drawn)
}

/// The window size of screenshot `screenshot`.
fn size_of(screenshots: &dyn Shots, screenshot: usize) -> Result<Size, SpecError> {
    screenshots
        .get(screenshot)
        .map(Image::size)
        .ok_or(SpecError::NoScreenshot)
}

/// Fails when `cut`, of the window size `size`, is cut at a region drawn
/// on screenshot `drawn_on`, of the window size `drawn`, and the two
/// differ: the region means another spot on a window of another size.
fn same_size(cut: Cut, size: Size, drawn_on: usize, drawn: Size) -> Result<(), SpecError> {
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
}

/// The picture a state is compared against, as PNG bytes: `region` of its
/// screenshot, and with a selection only the pixels of `region` it selects
/// on that screenshot, the rest transparent.
fn state_picture(
    screenshots: &dyn Shots,
    region: Region,
    state: &StateSpec,
) -> Result<Vec<u8>, SpecError> {
    let image = screenshots
        .get(state.screenshot)
        .ok_or(SpecError::NoScreenshot)?;
    let picture = image.crop(region).ok_or(SpecError::Outside)?;
    let png = match &state.selection {
        Some(selection) => {
            let mask = selection::select(image, selection)
                .and_then(|selected| selected.mask_in(region))
                .ok_or_else(|| SpecError::NothingSelected(state.name.clone()))?;
            picture.encode_png_masked(&mask)
        },
        None => picture.encode_png(),
    };
    png.map_err(|_encode| SpecError::Picture)
}

/// `region` as a [`Region`].
fn to_region(region: RegionSpec) -> Result<Region, SpecError> {
    Region::try_from(region).map_err(|EmptyRegion| SpecError::EmptyRegion)
}

/// The region `region` describes and the window size of the screenshot it
/// was drawn on, `screenshot`, which must hold it.
///
/// # Errors
///
/// Returns [`SpecError::EmptyRegion`], [`SpecError::NoScreenshot`], or
/// [`SpecError::Outside`].
pub(crate) fn drawn(
    screenshots: &dyn Shots,
    region: RegionSpec,
    screenshot: usize,
) -> Result<(Region, Size), SpecError> {
    let region = to_region(region)?;
    let size = size_of(screenshots, screenshot)?;
    if region.fits(size) {
        Ok((region, size))
    } else {
        Err(SpecError::Outside)
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

#[cfg(test)]
mod tests;
