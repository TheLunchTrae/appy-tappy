//! The check the user set up in the UI, and how it becomes a screen check.
//!
//! The UI describes a check with plain data: the region, the check's kind,
//! and the colors and labels the user picked on screenshots. [`build`] turns
//! that into a [`Check`] from `appytappy-sources`, learning a number's
//! characters and cutting a reference picture from the screenshots it names.

use std::error::Error;
use std::fmt;

use appytappy_config::app_file::{CheckDef, ColorDef, RegionDef, SampleDef};
use appytappy_sources::screen::check::{BarFill, Check, ColorMatch, ReferenceMatch};
use appytappy_sources::screen::digits::{DigitReader, LearnError};
use appytappy_sources::screen::{Image, Region};
use appytappy_sources::system::Rgb;
use serde::Deserialize;

/// A rectangle of the window, in pixels from its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) struct RegionSpec {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
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

/// What one screenshot's region shows, typed by the user so the number check
/// learns its characters.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct Label {
    screenshot: usize,
    text: String,
}

/// A check as the UI describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum CheckSpec {
    /// Reads a number whose characters have the `ink` color.
    Number {
        region: RegionSpec,
        ink: ColorSpec,
        tolerance: u8,
        labels: Vec<Label>,
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
    Reference {
        region: RegionSpec,
        screenshot: usize,
        tolerance: u8,
    },
}

/// A check and the region it reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegionCheck {
    /// The region of the window's frame the check reads.
    pub(crate) region: Region,
    /// The check.
    pub(crate) check: Check,
}

/// A check ready to save: the region, the check as the file spells it, and
/// the pictures it names, each as a file name and PNG bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SavedCheck {
    /// The region the check reads.
    pub(crate) region: RegionDef,
    /// The check.
    pub(crate) check: CheckDef,
    /// The pictures `check` names.
    pub(crate) pictures: Vec<(String, Vec<u8>)>,
}

/// Turns `spec` into the saved form for the value `id`, cutting its pictures
/// from the session's `screenshots` at the region. Pictures are named after
/// the value: `id.png` for a reference, `id-1.png` onward for a number's
/// samples.
///
/// # Errors
///
/// Returns the [`SpecError`] that [`build`] would, so only a check that
/// works is saved, or [`SpecError::Picture`] when a picture cannot be
/// encoded.
pub(crate) fn to_saved(
    spec: &CheckSpec,
    screenshots: &[Image],
    id: &str,
) -> Result<SavedCheck, SpecError> {
    let built = build(spec, screenshots)?;
    let picture = |screenshot: usize| {
        screenshots
            .get(screenshot)
            .and_then(|image| image.crop(built.region))
            .ok_or(SpecError::NoScreenshot)?
            .encode_png()
            .map_err(|_encode| SpecError::Picture)
    };
    let mut pictures = Vec::new();
    let check = match spec {
        CheckSpec::Number {
            ink,
            tolerance,
            labels,
            ..
        } => {
            let mut samples = Vec::new();
            for (number, label) in (1_u32..).zip(labels) {
                let image = format!("{id}-{number}.png");
                pictures.push((image.clone(), picture(label.screenshot)?));
                samples.push(SampleDef {
                    image,
                    text: label.text.clone(),
                });
            }
            CheckDef::Number {
                ink: (*ink).into(),
                tolerance: *tolerance,
                samples,
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
        CheckSpec::Reference {
            screenshot,
            tolerance,
            ..
        } => {
            let image = format!("{id}.png");
            pictures.push((image.clone(), picture(*screenshot)?));
            CheckDef::Reference {
                image,
                tolerance: *tolerance,
            }
        },
    };
    let size = built.region.size();
    Ok(SavedCheck {
        region: RegionDef {
            x: built.region.x(),
            y: built.region.y(),
            width: size.width,
            height: size.height,
        },
        check,
        pictures,
    })
}

/// Builds the check `spec` describes from the session's `screenshots`.
///
/// # Errors
///
/// Returns a [`SpecError`] that tells the user what to fix: an empty region,
/// a region or screenshot that does not exist, two bar colors that are the
/// same, or a number label the check cannot learn from.
pub(crate) fn build(spec: &CheckSpec, screenshots: &[Image]) -> Result<RegionCheck, SpecError> {
    let region = match spec {
        CheckSpec::Number { region, .. }
        | CheckSpec::BarFill { region, .. }
        | CheckSpec::Color { region, .. }
        | CheckSpec::Reference { region, .. } => {
            Region::new(region.x, region.y, region.width, region.height)
                .ok_or(SpecError::EmptyRegion)?
        },
    };
    let crop = |screenshot: usize| {
        screenshots
            .get(screenshot)
            .ok_or(SpecError::NoScreenshot)?
            .crop(region)
            .ok_or(SpecError::Outside)
    };
    let check = match spec {
        CheckSpec::Number {
            ink,
            tolerance,
            labels,
            ..
        } => {
            if labels.is_empty() {
                return Err(SpecError::NoLabels);
            }
            let mut reader = DigitReader::new((*ink).into(), *tolerance);
            for label in labels {
                reader
                    .learn(&crop(label.screenshot)?, &label.text)
                    .map_err(|source| SpecError::Learn {
                        text: label.text.clone(),
                        source,
                    })?;
            }
            Check::Number(reader)
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
        CheckSpec::Reference {
            screenshot,
            tolerance,
            ..
        } => Check::Reference(ReferenceMatch {
            reference: crop(*screenshot)?,
            tolerance: *tolerance,
        }),
    };
    Ok(RegionCheck { region, check })
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
    /// A number check has no labeled screenshot to learn from.
    NoLabels,
    /// A picture could not be encoded as PNG.
    Picture,
    /// A number check could not learn from a label.
    Learn {
        /// The text the user typed.
        text: String,
        /// Why the check could not learn from it.
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
            Self::NoLabels => {
                formatter.write_str("Type what the region shows on at least one screenshot.")
            },
            Self::Picture => formatter.write_str("A picture of the region could not be saved."),
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
    use appytappy_config::signal::Value;
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
        let built = build(&spec, &[]).expect("valid bar");
        assert_eq!(built.region, Region::new(0, 0, 3, 2).expect("not empty"));
        let bar = screenshot().crop(built.region).expect("inside");
        let value = built.check.read(&bar).expect("readable");
        assert_eq!(
            value,
            Value::Number(appytappy_config::signal::Number::new(400.0 / 6.0).expect("finite"))
        );
    }

    #[test]
    fn a_reference_is_cut_from_the_named_screenshot_at_the_region() {
        let left = RegionSpec {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        };
        let spec = CheckSpec::Reference {
            region: left,
            screenshot: 0,
            tolerance: 0,
        };
        let built = build(&spec, &[screenshot()]).expect("valid reference");
        let red_square = screenshot().crop(built.region).expect("inside");
        let gray_square = screenshot()
            .crop(Region::new(2, 0, 2, 2).expect("not empty"))
            .expect("inside");
        assert_eq!(built.check.read(&red_square), Ok(Value::Flag(true)));
        assert_eq!(built.check.read(&gray_square), Ok(Value::Flag(false)));
    }

    #[test]
    fn a_color_check_matches_the_regions_average_color() {
        let spec = CheckSpec::Color {
            region: whole(),
            color: RED,
            tolerance: 10,
        };
        let built = build(&spec, &[]).expect("valid color");
        let red_square = screenshot()
            .crop(Region::new(0, 0, 2, 2).expect("not empty"))
            .expect("inside");
        assert_eq!(built.check.read(&red_square), Ok(Value::Flag(true)));
        assert_eq!(built.check.read(&screenshot()), Ok(Value::Flag(false)));
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
                CheckSpec::Reference {
                    region: whole(),
                    screenshot: 1,
                    tolerance: 0,
                },
                SpecError::NoScreenshot,
            ),
            (
                CheckSpec::Reference {
                    region: outside,
                    screenshot: 0,
                    tolerance: 0,
                },
                SpecError::Outside,
            ),
            (
                CheckSpec::Number {
                    region: whole(),
                    ink: RED,
                    tolerance: 0,
                    labels: Vec::new(),
                },
                SpecError::NoLabels,
            ),
        ];
        for (spec, expected) in cases {
            assert_eq!(build(&spec, &[screenshot()]), Err(expected), "{spec:?}");
        }
    }

    #[test]
    fn a_label_the_number_check_cannot_learn_names_the_text() {
        let spec = CheckSpec::Number {
            region: whole(),
            ink: RED,
            tolerance: 0,
            labels: vec![Label {
                screenshot: 0,
                text: "12".to_owned(),
            }],
        };
        let error = build(&spec, &[screenshot()]).expect_err("one blob, two characters");
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
        // owner.rs's requests parse the other kinds; this is the number.
        let json = r#"{"kind":"number","region":{"x":1,"y":2,"width":3,"height":4},
            "ink":{"red":1,"green":2,"blue":3},"tolerance":5,
            "labels":[{"screenshot":0,"text":"12"}]}"#;
        let spec: CheckSpec = serde_json::from_str(json).expect("valid JSON");
        assert_eq!(
            spec,
            CheckSpec::Number {
                region: RegionSpec {
                    x: 1,
                    y: 2,
                    width: 3,
                    height: 4
                },
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
            }
        );
    }

    #[test]
    fn a_saved_number_names_one_picture_per_label() {
        let spec = CheckSpec::Number {
            region: RegionSpec {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            ink: RED,
            tolerance: 0,
            labels: vec![Label {
                screenshot: 0,
                text: "1".to_owned(),
            }],
        };
        let saved = to_saved(&spec, &[screenshot()], "progress").expect("valid number");
        assert_eq!(
            saved.region,
            RegionDef {
                x: 0,
                y: 0,
                width: 2,
                height: 2
            }
        );
        let CheckDef::Number {
            ink,
            tolerance,
            samples,
        } = &saved.check
        else {
            panic!("a number: {:?}", saved.check);
        };
        assert_eq!((*ink, *tolerance), (ColorDef::from(RED), 0));
        assert_eq!(
            samples,
            &[SampleDef {
                image: "progress-1.png".to_owned(),
                text: "1".to_owned(),
            }]
        );
        assert_eq!(saved.pictures.len(), 1);
        assert_eq!(saved.pictures[0].0, "progress-1.png");
        // A PNG keeps the colors and drops the unused fourth byte.
        let picture = Image::decode_png(&saved.pictures[0].1).expect("a PNG");
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
    fn saved_checks_keep_their_settings() {
        let region = RegionSpec {
            x: 2,
            y: 0,
            width: 2,
            height: 2,
        };
        let reference = CheckSpec::Reference {
            region,
            screenshot: 0,
            tolerance: 7,
        };
        let saved = to_saved(&reference, &[screenshot()], "icon_visible").expect("valid");
        assert_eq!(
            saved.check,
            CheckDef::Reference {
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
        let saved = to_saved(&bar, &[], "bar").expect("valid");
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
            to_saved(&color, &[], "color").expect("valid").check,
            CheckDef::Color {
                color: GRAY.into(),
                tolerance: 4
            }
        );
        assert_eq!(
            to_saved(&bar, &[], "bar").map(|saved| saved.region),
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
    }
}
