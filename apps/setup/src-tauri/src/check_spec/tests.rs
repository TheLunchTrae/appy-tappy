//! Tests of building and saving the checks the UI describes.

use std::error::Error as _;

use appytappy_config::app_file::MAX_STATES;
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

fn shown(name: &str) -> Value {
    Value::State(StateName::new(name).expect("valid"))
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
        screenshot: 0,
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
    let built = build(&spec, session(&screenshots, None)).expect("valid states");
    let red_square = screenshot().crop(built.region).expect("inside");
    let gray_square = screenshot()
        .crop(Region::new(2, 0, 2, 2).expect("not empty"))
        .expect("inside");
    assert_eq!(built.check.read(&red_square), Ok(shown("Red")));
    assert_eq!(built.check.read(&gray_square), Ok(shown("Gray")));
    let white = Image::new(red_square.size(), [255; 16].to_vec()).expect("matching length");
    assert_eq!(
        built.check.read(&white),
        Ok(Value::State(StateName::none()))
    );

    let saved = to_saved(&spec, session(&screenshots, None), "app", "status").expect("valid");
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
    let gray = Image::decode_png(&saved.pictures[1].1).expect("a PNG");
    let gray = ImageMatch::new(gray, None, 0).expect("no mask");
    assert_eq!(gray.matches(&gray_square), Ok(true));
    assert_eq!(gray.matches(&red_square), Ok(false));
}

#[test]
fn state_names_must_be_valid_and_differ() {
    let screenshots = [screenshot()];
    let problem = |names: &[&str]| {
        let spec = states(left(), names.iter().map(|name| state(name, 0)).collect(), 0);
        build(&spec, session(&screenshots, None)).map(drop)
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
    assert_eq!(
        SpecError::Check(CheckDefError::SameColors).to_string(),
        "Pick a filled color and an empty color that differ."
    );
    assert_eq!(problem(&names[..MAX_STATES]), Ok(()));
}

#[test]
fn a_selected_image_compares_only_what_was_selected() {
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
        Ok(shown("Shown")),
        "a new background still matches"
    );
    assert_eq!(
        built.check.read(&repaint(0..2, blue)),
        Ok(Value::State(StateName::none()))
    );

    let saved = to_saved(
        &spec(whole()),
        session(&[screenshot()], None),
        "app",
        "icon",
    )
    .expect("valid");
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
        Some(SpecError::NothingSelected("Shown".to_owned())),
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
    let saved = to_saved(&spec(true), session(&[], None), "app", "progress")
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
    assert_eq!(
        to_saved(&spec(true), session(&[], None), "other", "progress"),
        Err(SpecError::OtherApplication)
    );
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
    let error = to_saved(&spec, session(&[screenshot()], None), "app", "progress")
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
            "",
        ),
        Err(SpecError::NotAValue)
    );
    // A saved reference teaches on its own, with no new label.
    let saved_only = characters(left(), Some(digits_ref()), Vec::new(), Vec::new());
    assert!(build(&saved_only, session(&[screenshot()], None)).is_ok());
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
            SpecError::Check(CheckDefError::SameColors),
        ),
        (
            states(whole(), vec![state("Shown", 1)], 0),
            SpecError::NoScreenshot,
        ),
        (
            states(outside, vec![state("Shown", 0)], 0),
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
fn what_a_check_cuts_at_its_region_has_the_size_it_was_drawn_at() {
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
    let mut recording = Recording::new(1, region, (RED.into(), 0), small.size(), now);
    recording.offer(&small, now).expect("a reading");
    let kept = vec![RecordedLabel {
        reading: 0,
        text: "1".to_owned(),
    }];
    let labeled_both = characters(left(), None, [one(0), one(1)].concat(), Vec::new());
    let error = build(&labeled_both, session(&screenshots, None)).expect_err("two sizes");
    assert_eq!(
        error.to_string(),
        "Screenshot 2 is 2x2, but the region was drawn on screenshot 1, which is 4x2. Clear \
         its label, or draw the region on it instead."
    );
    let recorded_elsewhere = characters(left(), None, one(0), kept.clone());
    assert_eq!(
        build(&recorded_elsewhere, session(&screenshots, Some(&recording)))
            .expect_err("recorded at another size")
            .to_string(),
        "The recording is of a window of 2x2, but the region was drawn on screenshot 1, \
         which is 4x2. Use its settings, or discard the recording."
    );
    let mut drawn_on_small = characters(left(), None, one(1), kept);
    if let CheckSpec::Characters { screenshot, .. } = &mut drawn_on_small {
        *screenshot = 1;
    }
    assert!(
        build(&drawn_on_small, session(&screenshots, Some(&recording))).is_ok(),
        "all of one size"
    );
    assert_eq!(
        recorded_problems(&labeled_both, session(&screenshots, None)),
        Err(error),
        "review checks the sizes too"
    );
    let image = states(left(), vec![state("Shown", 1)], 0);
    assert_eq!(
        drawn_size(&image, session(&screenshots, None), 0)
            .expect_err("compared at another size")
            .to_string(),
        "Screenshot 2, which a state is taken from, is 2x2, but the region was drawn on \
         screenshot 1, which is 4x2. Take that state from a screenshot of that size."
    );
    assert_eq!(
        drawn_size(&image, session(&screenshots, None), 1),
        Ok(small.size())
    );
    assert!(
        matches!(
            build(&image, session(&screenshots, None)),
            Err(SpecError::OtherSize { .. })
        ),
        "trying the check is refused too"
    );
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
    let error = build(&spec, session(&[screenshot()], None)).expect_err("one blob, two characters");
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
    let json = r#"{"kind":"characters","region":{"x":1,"y":2,"width":3,"height":4},"screenshot":0,
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
    let (reference, pictures) = to_saved_reference(
        &spec,
        session(&[screenshot()], None),
        "digits",
        "Digits",
        "0123456789%",
    )
    .expect("valid characters");
    assert_eq!(
        reference,
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
        screenshot: 0,
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
        "",
    )
    .expect("valid characters");
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
    let names: Vec<&str> = pictures.iter().map(|(name, _)| name.as_str()).collect();
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
        if let Some(sample) = saved.reference.samples.first_mut() {
            sample.image = sample_name("digits", 2);
        }
        Ok(saved)
    };
    let spec = CheckSpec::Characters {
        region: left(),
        screenshot: 0,
        reference: Some(digits_ref()),
        ink: RED,
        tolerance: 0,
        labels: one(0),
        recorded: Vec::new(),
    };
    let screenshots = [screenshot()];
    let session = Session {
        screenshots: &screenshots,
        recording: None,
        references: &gapped,
    };
    let (reference, pictures) =
        to_saved_reference(&spec, session, "digits", "Digits", "").expect("valid characters");
    let names: Vec<&str> = pictures.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["ref-digits-3.png"]);
    assert_eq!(reference.samples.len(), 2);
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
        "",
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
    assert!(build(&only_recorded, session(&[screenshot()], Some(&recording))).is_ok());
    assert_eq!(
        build(&only_recorded, session(&[screenshot()], None)),
        Err(SpecError::NoRecording)
    );
    assert_eq!(
        build(
            &characters(whole(), None, Vec::new(), recorded(1)),
            session(&[screenshot()], Some(&recording))
        ),
        Err(SpecError::NoRecording)
    );
    let moved = characters(left(), None, Vec::new(), recorded(0));
    let error =
        build(&moved, session(&[screenshot()], Some(&recording))).expect_err("another region");
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
        recorded_problems(&spec, session(&[screenshot()], Some(&recording))).expect("a valid spec");
    assert_eq!(problems.len(), 1);
    assert!(problems[0].is_some(), "{problems:?}");
    let spec = characters(left(), Some(digits_ref()), Vec::new(), as_text("1"));
    assert_eq!(
        recorded_problems(&spec, session(&[screenshot()], Some(&recording))),
        Ok(vec![None])
    );
    let color = CheckSpec::Color {
        region: left(),
        color: RED,
        tolerance: 0,
    };
    assert_eq!(
        recorded_problems(&color, session(&[screenshot()], Some(&recording))),
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
    let image = states(region, vec![state("Shown", 0)], 7);
    let saved = to_saved(
        &image,
        session(&[screenshot()], None),
        "app",
        "icon_visible",
    )
    .expect("valid");
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
    let saved = to_saved(&bar, session(&[], None), "app", "bar").expect("valid");
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
        to_saved(&color, session(&[], None), "app", "color")
            .expect("valid")
            .check,
        CheckDef::Color {
            color: GRAY.into(),
            tolerance: 4
        }
    );
    assert_eq!(
        to_saved(&bar, session(&[], None), "app", "bar").map(|saved| saved.region),
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
        SpecError::NothingSelected("Live".to_owned()).to_string(),
        "Nothing in the region is selected for the state \"Live\". Select it again, clear its selection, or stop ignoring the background."
    );
    assert_eq!(
        SpecError::OtherApplication.to_string(),
        "That reference belongs to another application. Pick one of this application's."
    );
}
