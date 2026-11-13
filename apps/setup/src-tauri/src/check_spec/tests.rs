//! Tests of turning the checks and characters the UI describes into their
//! saved form, and of the words for what cannot be saved or read.

use std::error::Error as _;
use std::io;

use crate::ticket::Ticket;

use appytappy_config::app_file::{
    CheckDefError, MAX_STATES, ReferenceDef, SampleDef, ScreenDef, SizeChangesDef, SizeDef,
};
use appytappy_config::names::sample_name;
use appytappy_config::signal::Number;
use appytappy_sources::screen::check::ImageMatch;
use appytappy_sources::screen::saved;

use super::characters::{RecordedLabel, Taught, expected_characters};
use super::*;
use crate::recording::Recording;
use crate::recording::follow::Follows;

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

fn state(name: &str, screenshot: usize) -> StateSpec {
    StateSpec {
        name: name.to_owned(),
        screenshot,
        selection: None,
    }
}

fn states(region: RegionSpec, states: Vec<StateSpec>, tolerance: u8) -> CheckSpec {
    CheckSpec::State {
        region,
        drawn_on: 0,
        states,
        tolerance,
    }
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
            expects: Some("123".to_owned()),
        },
        pictures: vec![(
            sample_name("digits", 1),
            screenshot()
                .crop(Region::new(0, 0, 2, 2).expect("not empty"))
                .expect("inside")
                .encode_png()
                .expect("encodes"),
        )],
    })
}

/// A test's screenshots are an array, each named by its place in it.
impl<const N: usize> Shots for [Image; N] {
    fn get(&self, index: usize) -> Option<&Image> {
        self.as_slice().get(index)
    }
}

fn digits_ref() -> ReferenceRef {
    ReferenceRef {
        app: "app".to_owned(),
        id: "digits".to_owned(),
    }
}

fn session<'a>(screenshots: &'a dyn Shots, recording: Option<&'a Recording>) -> Session<'a> {
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
) -> CharactersSpec {
    CharactersSpec {
        region,
        screenshot: 0,
        reference,
        ink: RED,
        tolerance: 0,
        labels,
        recorded,
    }
}

/// What `spec` teaches a reference saved as `digits`, expecting nothing.
fn teach(spec: &CharactersSpec, session: Session<'_>) -> Result<Taught, SpecError> {
    spec.teach(session, Some("digits"), "Digits", "")
}

/// What saving `spec` on screenshot 0 and reading it back gives: the value
/// `saved::build` makes of its saved form, or why there is none.
fn read_back(spec: &CheckSpec, screenshots: &dyn Shots) -> Result<saved::SavedValue, SpecError> {
    let size = drawn_size(spec, screenshots, 0)?;
    let saved = to_saved(spec, screenshots, "app", "tried")?;
    let screen = ScreenDef {
        window_size: SizeDef {
            width: size.width,
            height: size.height,
        },
        region: saved.region,
        check: saved.check,
        pin: None,
        reading_area: false,
        fixes: Vec::new(),
    };
    let built = saved::build(
        &screen,
        &mut saved::Readers::new(&[]),
        SizeChangesDef::Often,
        |picture| {
            saved
                .pictures
                .iter()
                .find(|(name, _)| name == picture)
                .map(|(_, png)| png.clone())
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        },
    )?;
    Ok(built)
}

fn one(screenshot: usize) -> Vec<Label> {
    vec![Label {
        screenshot,
        region: None,
        text: "1".to_owned(),
    }]
}

#[test]
fn each_state_is_cut_from_its_screenshot_at_the_region() {
    // Screenshot 2 is screenshot 1 with its halves swapped, so the left
    // region is red on one and gray on the other.
    let mut swapped = screenshot().pixels().to_vec();
    for row in swapped.chunks_mut(4) {
        row.rotate_left(2);
    }
    let swapped = Image::new(screenshot().size(), swapped.concat()).expect("matching length");
    let spec = states(left(), vec![state("Red", 0), state("Gray", 1)], 0);
    let screenshots = [screenshot(), swapped];
    let saved = to_saved(&spec, &screenshots, "app", "status").expect("valid");
    assert_eq!(
        saved.check,
        CheckDef::State {
            states: vec![
                StateDef {
                    name: "Red".to_owned(),
                    image: "status-1.png".to_owned(),
                },
                StateDef {
                    name: "Gray".to_owned(),
                    image: "status-2.png".to_owned(),
                },
            ],
            tolerance: 0,
        }
    );
    let names: Vec<&str> = saved
        .pictures
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(names, ["status-1.png", "status-2.png"]);
    let red_square = screenshot()
        .crop(Region::new(0, 0, 2, 2).expect("not empty"))
        .expect("inside");
    let gray_square = screenshot()
        .crop(Region::new(2, 0, 2, 2).expect("not empty"))
        .expect("inside");
    let gray = Image::decode_png(&saved.pictures[1].1).expect("a PNG");
    let gray = ImageMatch::new(gray, None, 0).expect("no mask");
    assert_eq!(gray.matches(&gray_square), Ok(true));
    assert_eq!(gray.matches(&red_square), Ok(false));
}

#[test]
fn what_the_saved_form_breaks_comes_back_in_the_pages_words() {
    let screenshots = [screenshot()];
    let problem = |names: &[&str]| {
        let spec = states(left(), names.iter().map(|name| state(name, 0)).collect(), 0);
        read_back(&spec, &screenshots).map(drop)
    };
    assert_eq!(problem(&["Paused", "Live"]), Ok(()));
    assert_eq!(problem(&[]), Err(SpecError::Check(CheckDefError::NoStates)));
    assert_eq!(
        problem(&["Paused", "paused"]),
        Err(SpecError::Check(CheckDefError::SameStateName(
            "paused".to_owned()
        )))
    );
    let none = problem(&["None"]).expect_err("reserved");
    assert_eq!(
        none.to_string(),
        "State name \"None\" is \"none\", which stands for no state."
    );
    let blank = problem(&[" "]).expect_err("blank");
    assert_eq!(blank.to_string(), "State name \" \" is empty.");
    let names: Vec<String> = (0..=MAX_STATES).map(|index| format!("S{index}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let too_many = problem(&names).expect_err("too many");
    assert_eq!(too_many, SpecError::Check(CheckDefError::TooManyStates));
    assert_eq!(
        too_many.to_string(),
        "A value can tell apart at most 16 states. Remove one."
    );
    assert_eq!(
        SpecError::Check(CheckDefError::NoStates).to_string(),
        "Add a state from a screenshot."
    );
    assert_eq!(
        SpecError::Check(CheckDefError::SameStateName("Live".to_owned())).to_string(),
        "Two states are named \"Live\". Give each state its own name."
    );
    assert_eq!(problem(&names[..MAX_STATES]), Ok(()));

    let same_colors = CheckSpec::BarFill {
        region: whole(),
        filled: RED,
        empty: RED,
    };
    let error = read_back(&same_colors, &screenshots).expect_err("one color");
    assert_eq!(error, SpecError::Check(CheckDefError::SameColors));
    assert_eq!(
        error.to_string(),
        "Pick a filled color and an empty color that differ."
    );
    let outside = CheckSpec::Color {
        region: RegionSpec {
            x: 3,
            y: 0,
            width: 2,
            height: 2,
        },
        color: RED,
        tolerance: 0,
    };
    assert_eq!(
        read_back(&outside, &screenshots).map(drop),
        Err(SpecError::Outside)
    );
    // The page has no words of its own for the rest.
    assert_eq!(
        SpecError::from(saved::SavedError::SameSize).to_string(),
        "AppyTappy could not read this value as saved: it is drawn at this size already."
    );
}

#[test]
fn a_selected_image_keeps_only_what_was_selected() {
    // The click selects the red half, so the gray half is background.
    let spec = |region| {
        states(
            region,
            vec![StateSpec {
                selection: Some(SelectionSpec {
                    tolerance: 10,
                    steps: vec![selection::SelectStep::Click {
                        x: 0,
                        y: 0,
                        combine: selection::Combine::Replace,
                    }],
                }),
                ..state("Shown", 0)
            }],
            0,
        )
    };
    let saved = to_saved(&spec(whole()), &[screenshot()], "app", "icon").expect("valid");
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
        to_saved(&spec(gray_half), &[screenshot()], "app", "icon").err(),
        Some(SpecError::NothingSelected("Shown".to_owned())),
        "a region that holds none of the selection has nothing to compare"
    );
}

#[test]
fn a_number_is_saved_with_its_applications_reference() {
    let spec = CheckSpec::Number {
        region: left(),
        reference: digits_ref(),
        whole: true,
        characters: Some("1%".to_owned()),
    };
    let saved =
        to_saved(&spec, &[], "app", "progress").expect("a value of the reference's application");
    assert_eq!(
        saved.check,
        CheckDef::Number {
            reference: "digits".to_owned(),
            whole: true,
            characters: Some("1%".to_owned()),
        }
    );
    assert!(
        saved.pictures.is_empty(),
        "the reference keeps the pictures"
    );
    assert_eq!(
        to_saved(&spec, &[], "other", "progress"),
        Err(SpecError::OtherApplication)
    );
}

#[test]
fn characters_being_taught_read_as_their_saved_reference_does() {
    let spec = characters(left(), None, one(0), Vec::new());
    let taught = teach(&spec, session(&[screenshot()], None)).expect("learns the 1");
    assert_eq!(taught.region, Region::new(0, 0, 2, 2).expect("not empty"));
    assert_eq!(taught.window_size, screenshot().size());
    let red_square = screenshot().crop(taught.region).expect("inside");
    assert_eq!(
        taught.reader.read(&red_square, false),
        Ok(Number::new(1.0).expect("finite"))
    );
    // The file and its pictures give the runtime the same reader.
    let from_file = saved::reader(&taught.reference, |picture| {
        taught
            .pictures
            .iter()
            .find(|(name, _)| name == picture)
            .map(|(_, png)| png.clone())
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    })
    .expect("learns from the file");
    assert_eq!(from_file, taught.reader);
    // A saved reference teaches on its own, with no new label.
    let saved_only = characters(left(), Some(digits_ref()), Vec::new(), Vec::new());
    assert!(teach(&saved_only, session(&[screenshot()], None)).is_ok());
}

#[test]
fn a_saved_reference_is_relearned_in_the_color_being_tried() {
    // The saved reference learned red characters. Tried in gray, its saved
    // sample is the gray half's shape, which the red square shows none of.
    let gray = CharactersSpec {
        ink: GRAY,
        ..characters(whole(), Some(digits_ref()), Vec::new(), Vec::new())
    };
    let error = teach(&gray, session(&[screenshot()], None)).expect_err("no gray in the sample");
    assert!(
        matches!(&error, SpecError::Learn { text, .. } if text == "1"),
        "{error:?}"
    );
}

#[test]
fn a_saved_sample_without_its_picture_says_how_to_recover() {
    let bare = |reference: &ReferenceRef| {
        let mut saved = digits(reference)?;
        saved.pictures.clear();
        Ok(saved)
    };
    let screenshots = [screenshot()];
    let session = Session {
        screenshots: &screenshots,
        recording: None,
        references: &bare,
    };
    let spec = characters(left(), Some(digits_ref()), Vec::new(), Vec::new());
    assert_eq!(
        teach(&spec, session)
            .expect_err("a sample is gone")
            .to_string(),
        "The reference Digits is missing its picture ref-digits-1.png. Remove the values that \
         read with it, then delete the reference and make it again."
    );
    let gone = characters(
        left(),
        Some(ReferenceRef {
            app: "app".to_owned(),
            id: "gone".to_owned(),
        }),
        Vec::new(),
        Vec::new(),
    );
    let error = teach(&gone, session).expect_err("not saved");
    assert_eq!(
        error,
        SpecError::Reference("That reference is not saved.".to_owned())
    );
    assert_eq!(error.to_string(), "That reference is not saved.");
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
    let screenshots = [screenshot()];
    let checks = [
        (
            CheckSpec::Color {
                region: empty,
                color: RED,
                tolerance: 0,
            },
            SpecError::EmptyRegion,
        ),
        (
            states(whole(), vec![state("Shown", 1)], 0),
            SpecError::NoScreenshot,
        ),
        (
            states(outside, vec![state("Shown", 0)], 0),
            SpecError::Outside,
        ),
    ];
    for (spec, expected) in checks {
        assert_eq!(
            to_saved(&spec, &screenshots, "app", "value"),
            Err(expected),
            "{spec:?}"
        );
    }
    let taught = [
        (
            characters(whole(), None, Vec::new(), Vec::new()),
            SpecError::NoLabels,
        ),
        (
            characters(empty, None, one(0), Vec::new()),
            SpecError::EmptyRegion,
        ),
        (
            characters(outside, None, one(0), Vec::new()),
            SpecError::Outside,
        ),
        (
            CharactersSpec {
                screenshot: 1,
                ..characters(whole(), None, one(0), Vec::new())
            },
            SpecError::NoScreenshot,
        ),
    ];
    for (spec, expected) in taught {
        assert_eq!(
            teach(&spec, session(&screenshots, None)).map(drop),
            Err(expected),
            "{spec:?}"
        );
    }
    assert_eq!(
        drawn(&screenshots, outside, 0),
        Err(SpecError::Outside),
        "a recording checks its region the same way"
    );
}

#[test]
fn what_is_cut_at_a_region_has_the_size_it_was_drawn_at() {
    let small = Image::new(
        Size {
            width: 2,
            height: 2,
        },
        [0, 0, 255, 0].repeat(4),
    )
    .expect("matching length");
    let screenshots = [screenshot(), small.clone()];
    let now = std::time::Instant::now();
    let region = Region::new(0, 0, 2, 2).expect("not empty");
    let mut recording = Recording::new(
        Ticket::numbered(1),
        region,
        (RED.into(), 0),
        (small.size(), Follows::Drawn),
        now,
    );
    recording.offer(&small, now).expect("a reading");
    let kept = vec![RecordedLabel {
        reading: 0,
        text: "1".to_owned(),
    }];
    let labeled_both = characters(left(), None, [one(0), one(1)].concat(), Vec::new());
    let error = teach(&labeled_both, session(&screenshots, None)).expect_err("two sizes");
    assert_eq!(
        error.to_string(),
        "Screenshot 2, which a box is on, is 2x2, but the reference reads on screenshot 1, \
         which is 4x2. Delete its boxes, or draw them on a screenshot of that size."
    );
    let recorded_elsewhere = characters(left(), None, one(0), kept.clone());
    assert_eq!(
        teach(&recorded_elsewhere, session(&screenshots, Some(&recording)))
            .expect_err("recorded at another size")
            .to_string(),
        "The recording is of a window of 2x2, but the reference reads on screenshot 1, \
         which is 4x2. Use its settings, or discard the recording."
    );
    let drawn_on_small = CharactersSpec {
        screenshot: 1,
        ..characters(left(), None, one(1), kept)
    };
    assert!(
        teach(&drawn_on_small, session(&screenshots, Some(&recording))).is_ok(),
        "all of one size"
    );
    assert_eq!(
        labeled_both.recorded_problems(session(&screenshots, None)),
        Err(error),
        "review checks the sizes too"
    );
    let image = states(left(), vec![state("Shown", 1)], 0);
    assert_eq!(
        drawn_size(&image, &screenshots, 0)
            .expect_err("compared at another size")
            .to_string(),
        "Screenshot 2, which a state is taken from, is 2x2, but the region was drawn on \
         screenshot 1, which is 4x2. Take that state from a screenshot of that size."
    );
    assert_eq!(drawn_size(&image, &screenshots, 1), Ok(small.size()));
    assert!(
        matches!(
            to_saved(&image, &screenshots, "app", "value"),
            Err(SpecError::OtherSize { .. })
        ),
        "the saved form is refused too"
    );
}

#[test]
fn a_label_the_reference_cannot_learn_names_the_text() {
    let spec = characters(
        whole(),
        None,
        vec![Label {
            screenshot: 0,
            region: None,
            text: "12".to_owned(),
        }],
        Vec::new(),
    );
    let error = teach(&spec, session(&[screenshot()], None)).expect_err("one blob, two characters");
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
    let json = r#"{"region":{"x":1,"y":2,"width":3,"height":4},"screenshot":0,
        "ink":{"red":1,"green":2,"blue":3},"tolerance":5,
        "labels":[{"screenshot":0,"text":"12"}]}"#;
    let spec: CharactersSpec = serde_json::from_str(json).expect("valid JSON");
    assert_eq!(
        spec,
        CharactersSpec {
            region: RegionSpec {
                x: 1,
                y: 2,
                width: 3,
                height: 4
            },
            screenshot: 0,
            reference: None,
            ink: ColorSpec {
                red: 1,
                green: 2,
                blue: 3
            },
            tolerance: 5,
            labels: vec![Label {
                screenshot: 0,
                region: None,
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
            characters: None,
        }
    );
}

#[test]
fn a_new_reference_names_one_picture_per_label() {
    let spec = characters(left(), None, one(0), Vec::new());
    let taught = spec
        .teach(
            session(&[screenshot()], None),
            Some("digits"),
            "Digits",
            "%9 876543210%",
        )
        .expect("valid characters");
    assert_eq!(
        taught.reference,
        ReferenceDef {
            id: "digits".to_owned(),
            name: "Digits".to_owned(),
            ink: RED.into(),
            tolerance: 0,
            // The one label teaches the one sample, whatever it expects.
            samples: vec![SampleDef {
                image: "ref-digits-1.png".to_owned(),
                text: "1".to_owned(),
            }],
            expects: Some("0123456789%".to_owned()),
        }
    );
    let pictures = taught.pictures;
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
fn expected_characters_are_saved_once_each_in_order_up_to_the_most() {
    assert_eq!(
        expected_characters(" b%1\u{7}a0 1\u{3000}%\u{85}"),
        Ok(Some("01%ab".to_owned())),
        "no whitespace, control character or repeat, and the digits first"
    );
    assert_eq!(
        expected_characters("\u{1d7d9}z\u{ff11}\u{feff}"),
        Ok(Some("z\u{feff}\u{ff11}\u{1d7d9}".to_owned())),
        "by code point, and only the digits 0 to 9 come first"
    );
    assert_eq!(expected_characters(""), Ok(None));
    assert_eq!(expected_characters(" \t\u{7}"), Ok(None));
    let most: String = ('\u{100}'..).take(MAX_EXPECTED_CHARACTERS).collect();
    assert_eq!(expected_characters(&most), Ok(Some(most.clone())));
    // A repeat or a space does not count toward the most.
    assert_eq!(
        expected_characters(&format!("{most} {most}")),
        Ok(Some(most.clone()))
    );
    assert_eq!(
        expected_characters(&format!("{most}%")),
        Err(SpecError::TooManyExpected)
    );
    assert_eq!(
        SpecError::TooManyExpected.to_string(),
        "A reference needs at most 128 characters. Remove some."
    );
    // The save refuses the list.
    let spec = characters(left(), None, one(0), Vec::new());
    assert_eq!(
        spec.teach(
            session(&[screenshot()], None),
            Some("digits"),
            "Digits",
            &format!("{most}%"),
        )
        .map(drop),
        Err(SpecError::TooManyExpected)
    );
}

#[test]
fn a_saved_reference_keeps_its_samples_and_numbers_new_ones_after_them() {
    let spec = CharactersSpec {
        tolerance: 3,
        ..characters(left(), Some(digits_ref()), one(0), Vec::new())
    };
    // Without an ID of its own, it is taught under the saved one's.
    let taught = spec
        .teach(session(&[screenshot()], None), None, "Red digits", "")
        .expect("valid characters");
    let reference = taught.reference;
    assert_eq!(
        reference.expects, None,
        "what it expected goes when the user chooses none"
    );
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
    let names: Vec<&str> = taught
        .pictures
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(
        names,
        ["ref-digits-2.png"],
        "only the new picture is written"
    );
}

#[test]
fn a_new_sample_skips_a_picture_name_already_taken() {
    // A file edited by hand left one sample, named as the second.
    let gapped = |reference: &ReferenceRef| {
        let mut saved = digits(reference)?;
        let second = sample_name("digits", 2);
        if let Some(sample) = saved.reference.samples.first_mut() {
            sample.image.clone_from(&second);
        }
        if let Some((name, _)) = saved.pictures.first_mut() {
            *name = second;
        }
        Ok(saved)
    };
    let spec = characters(left(), Some(digits_ref()), one(0), Vec::new());
    let screenshots = [screenshot()];
    let session = Session {
        screenshots: &screenshots,
        recording: None,
        references: &gapped,
    };
    let taught = teach(&spec, session).expect("valid characters");
    let names: Vec<&str> = taught
        .pictures
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(names, ["ref-digits-3.png"]);
    assert_eq!(taught.reference.samples.len(), 2);
}

#[test]
fn recorded_readings_are_learned_and_saved_after_the_labels() {
    let now = std::time::Instant::now();
    let region = Region::new(0, 0, 4, 2).expect("not empty");
    let mut recording = Recording::new(
        Ticket::numbered(1),
        region,
        (RED.into(), 0),
        (screenshot().size(), Follows::Drawn),
        now,
    );
    recording.offer(&screenshot(), now).expect("a reading");
    let recorded = |reading| {
        vec![RecordedLabel {
            reading,
            text: "1".to_owned(),
        }]
    };
    let screenshots = [screenshot()];
    let spec = characters(whole(), None, one(0), recorded(0));
    let taught = teach(&spec, session(&screenshots, Some(&recording))).expect("valid characters");
    let images: Vec<&str> = taught
        .reference
        .samples
        .iter()
        .map(|sample| sample.image.as_str())
        .collect();
    assert_eq!(images, ["ref-digits-1.png", "ref-digits-2.png"]);
    let picture = Image::decode_png(&taught.pictures[1].1).expect("a PNG");
    assert_eq!(picture.size(), recording.readings()[0].image.size());

    let problem = |spec: &CharactersSpec, recording| {
        teach(spec, session(&screenshots, recording))
            .map(drop)
            .err()
    };
    let only_recorded = characters(whole(), None, Vec::new(), recorded(0));
    assert_eq!(problem(&only_recorded, Some(&recording)), None);
    assert_eq!(problem(&only_recorded, None), Some(SpecError::NoRecording));
    assert_eq!(
        problem(
            &characters(whole(), None, Vec::new(), recorded(1)),
            Some(&recording)
        ),
        Some(SpecError::NoRecording)
    );
    let moved = characters(left(), None, Vec::new(), recorded(0));
    let error = problem(&moved, Some(&recording)).expect("another region");
    assert_eq!(error, SpecError::RecordedElsewhere);
    assert_eq!(
        error.to_string(),
        "The reference reads another place than the recording recorded. Use its settings, or record \
         again."
    );
}

#[test]
fn recorded_problems_judge_each_reading_after_the_saved_samples() {
    let now = std::time::Instant::now();
    let region = Region::new(0, 0, 2, 2).expect("not empty");
    let mut recording = Recording::new(
        Ticket::numbered(1),
        region,
        (RED.into(), 0),
        (screenshot().size(), Follows::Drawn),
        now,
    );
    recording.offer(&screenshot(), now).expect("a reading");
    let as_text = |text: &str| {
        vec![RecordedLabel {
            reading: 0,
            text: text.to_owned(),
        }]
    };
    // The saved reference took the red square for a 1.
    let spec = characters(left(), Some(digits_ref()), Vec::new(), as_text("7"));
    let problems = spec
        .recorded_problems(session(&[screenshot()], Some(&recording)))
        .expect("a valid spec");
    assert_eq!(problems.len(), 1);
    assert!(problems[0].is_some(), "{problems:?}");
    let spec = characters(left(), Some(digits_ref()), Vec::new(), as_text("1"));
    assert_eq!(
        spec.recorded_problems(session(&[screenshot()], Some(&recording))),
        Ok(vec![None])
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
    let image = states(region, vec![state("Shown", 0)], 7);
    let saved = to_saved(&image, &[screenshot()], "app", "icon_visible").expect("valid");
    assert_eq!(
        saved.check,
        CheckDef::State {
            states: vec![StateDef {
                name: "Shown".to_owned(),
                image: "icon_visible-1.png".to_owned(),
            }],
            tolerance: 7
        }
    );
    assert_eq!(saved.pictures[0].0, "icon_visible-1.png");
    assert_eq!((saved.region.x, saved.region.width), (2, 2));

    let bar = CheckSpec::BarFill {
        region,
        filled: RED,
        empty: GRAY,
    };
    let saved = to_saved(&bar, &[], "app", "bar").expect("valid");
    assert_eq!(
        saved.check,
        CheckDef::BarFill {
            filled: RED.into(),
            empty: GRAY.into()
        }
    );
    assert_eq!(saved.pictures, Vec::<(String, Vec<u8>)>::new());
    assert_eq!(
        saved.region,
        RegionDef {
            x: 2,
            y: 0,
            width: 2,
            height: 2
        }
    );

    let color = CheckSpec::Color {
        region,
        color: GRAY,
        tolerance: 4,
    };
    assert_eq!(
        to_saved(&color, &[], "app", "color").expect("valid").check,
        CheckDef::Color {
            color: GRAY.into(),
            tolerance: 4
        }
    );
    assert_eq!(
        SpecError::Picture.to_string(),
        "A picture of the region could not be saved."
    );
    assert_eq!(
        SpecError::NothingSelected("Live".to_owned()).to_string(),
        "Nothing in the region is selected for the state \"Live\". Select it again, clear its selection, or stop ignoring the background."
    );
    assert_eq!(
        SpecError::OtherApplication.to_string(),
        "That reference belongs to another application. Pick one of this application's."
    );
}
