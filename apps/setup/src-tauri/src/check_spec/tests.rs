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
use appytappy_sources::screen::digits::{InkRule, LearnError};
use appytappy_sources::screen::saved::{self, SavedError};

use super::characters::{RecordedLabel, Taught, UnlearnedSample};
use super::error::cant_learn;
use super::*;
use crate::recording::Recording;
use crate::recording::follow::Follows;

const RED: ColorDef = ColorDef {
    red: 255,
    green: 0,
    blue: 0,
};
const GRAY: ColorDef = ColorDef {
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

fn states(region: RegionDef, states: Vec<StateSpec>, tolerance: u8) -> CheckSpec {
    CheckSpec::State {
        region,
        drawn_on: 0,
        states,
        tolerance,
    }
}

fn whole() -> RegionDef {
    RegionDef {
        x: 0,
        y: 0,
        width: 4,
        height: 2,
    }
}

fn left() -> RegionDef {
    RegionDef {
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
            origin: OriginDef::ByHand,
            ink: Some(RED),
            tolerance: 0,
            samples: vec![SampleDef {
                image: sample_name("digits", 1),
                text: "1".to_owned(),
            }],
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
    fn is_recorded(&self, _index: usize) -> bool {
        false
    }

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
    region: RegionDef,
    reference: Option<ReferenceRef>,
    labels: Vec<Label>,
    recorded: Vec<RecordedLabel>,
) -> CharactersSpec {
    CharactersSpec {
        region,
        screenshot: 0,
        reference,
        ink: Some(RED),
        tolerance: 0,
        labels,
        recorded,
        dropped: Vec::new(),
    }
}

/// What `spec` teaches a reference saved as `digits`, expecting nothing.
fn teach(spec: &CharactersSpec, session: Session<'_>) -> Result<Taught, SpecError> {
    spec.teach(session, Some("digits"), "Digits", &[])
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
        "State name \"None\" is \"none\", which stands for no state (state 1)."
    );
    let blank = problem(&[" "]).expect_err("blank");
    assert_eq!(blank.to_string(), "State name \" \" is empty (state 1).");
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
        region: RegionDef {
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
        "AppyTappy can't read this value as saved: it is drawn at this size already."
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

    let gray_half = RegionDef {
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
        ink: Some(GRAY),
        ..characters(whole(), Some(digits_ref()), Vec::new(), Vec::new())
    };
    let error = teach(&gray, session(&[screenshot()], None)).expect_err("no gray in the sample");
    assert!(
        matches!(&error, SpecError::SavedLearn { text, .. } if text == "1"),
        "{error:?}"
    );
    // The reference is taught in its own editor, which lists the picture
    // no box shows, so the words point at that list and name no reference
    // to open.
    assert_eq!(
        error.to_string(),
        "Can't learn from its saved picture of \"1\": nothing in it stands out from its \
         background, or has the reference's text color. Remove it under Saved pictures it \
         can't learn, or change the text color."
    );
    assert!(error.source().is_some());
}

/// The gray half of [`screenshot`], labeled `text`.
fn gray_half(text: &str) -> Vec<Label> {
    vec![Label {
        screenshot: 0,
        region: Some(RegionDef {
            x: 2,
            y: 0,
            width: 2,
            height: 2,
        }),
        text: text.to_owned(),
    }]
}

#[test]
fn a_saved_sample_the_reference_leaves_out_is_not_learned_or_saved() {
    let shots = [screenshot()];
    let picture = sample_name("digits", 1);
    // In gray the saved red sample teaches nothing, and a new box does not
    // help: the save is refused over a picture no box shows.
    let gray = CharactersSpec {
        ink: Some(GRAY),
        ..characters(whole(), Some(digits_ref()), gray_half("2"), Vec::new())
    };
    assert!(matches!(
        teach(&gray, session(&shots, None)),
        Err(SpecError::SavedLearn { .. })
    ));
    let dropped = CharactersSpec {
        dropped: vec![picture.clone()],
        ..gray.clone()
    };
    let taught = teach(&dropped, session(&shots, None)).expect("learns the box alone");
    assert_eq!(
        taught.reference.samples,
        [SampleDef {
            image: sample_name("digits", 2),
            text: "2".to_owned(),
        }],
        "the old picture stays readable until the edited reference is saved"
    );
    let [(name, png)] = taught.pictures.as_slice() else {
        panic!("one new picture, not {:?}", taught.pictures.len());
    };
    assert_eq!(name, &taught.reference.samples[0].image);
    let gray_square = screenshot()
        .crop(Region::new(2, 0, 2, 2).expect("not empty"))
        .expect("inside");
    assert_eq!(
        Some(png),
        gray_square.encode_png().ok().as_ref(),
        "the new picture is the box, not the sample it replaces"
    );
    // The fit and the judging of more labels leave it out too.
    assert_eq!(
        dropped.label_problems(session(&shots, None), &gray_half("2")),
        Ok(vec![None])
    );
    assert!(
        gray.label_problems(session(&shots, None), &gray_half("2"))
            .is_err()
    );
    // A name the reference has no sample of leaves nothing out.
    let other = CharactersSpec {
        dropped: vec![sample_name("digits", 7)],
        ..gray
    };
    assert!(teach(&other, session(&shots, None)).is_err());
    // With every sample left out and nothing new, there is nothing to
    // learn from.
    let nothing = CharactersSpec {
        dropped: vec![picture],
        ..characters(whole(), Some(digits_ref()), Vec::new(), Vec::new())
    };
    assert_eq!(
        teach(&nothing, session(&shots, None)).err(),
        Some(SpecError::NoLabels)
    );
}

#[test]
fn the_saved_samples_a_reference_cannot_learn_are_named_with_why() {
    let saved = digits(&digits_ref()).expect("saved");
    assert_eq!(saved.unlearned(Some(RED), 0), []);
    let unlearned = |problem: &str| UnlearnedSample {
        image: sample_name("digits", 1),
        text: "1".to_owned(),
        problem: problem.to_owned(),
        missing: false,
    };
    assert_eq!(
        saved.unlearned(Some(GRAY), 0),
        [unlearned(
            "Nothing in it stands out from its background, or has the reference's text color."
        )]
    );
    // By contrast the plain red square shows no character either, and the
    // settings saved for it fit no tolerance that helps.
    assert_eq!(saved.unlearned(None, 40).len(), 1);
    assert_eq!(saved.unlearned_with_settings(None, 40).len(), 1);
    assert_eq!(saved.unlearned_with_settings(Some(RED), 0), []);

    // A sample after one that cannot be learned is judged without it: the
    // second red square is a 1 like the first would be, were it learned
    // as a 7.
    let second = sample_name("digits", 2);
    let mut two = saved.clone();
    two.reference.samples[0].text = "77".to_owned();
    two.reference.samples.push(SampleDef {
        image: second.clone(),
        text: "1".to_owned(),
    });
    two.pictures
        .push((second.clone(), saved.pictures[0].1.clone()));
    let named: Vec<String> = two
        .unlearned(Some(RED), 0)
        .into_iter()
        .map(|sample| sample.image)
        .collect();
    assert_eq!(named, [sample_name("digits", 1)]);
    // Without the samples it names, the reference learns every sample.
    assert!(
        two.clone()
            .without(&named)
            .with_settings("Digits", Some(RED), 0)
            .is_ok()
    );
    assert!(two.clone().with_settings("Digits", Some(RED), 0).is_err());

    // A picture that is gone cannot be learned, and can be left out.
    two.pictures.remove(1);
    assert_eq!(
        two.unlearned(Some(RED), 0)
            .into_iter()
            .map(|sample| (sample.image, sample.problem, sample.missing))
            .collect::<Vec<_>>(),
        [
            (
                sample_name("digits", 1),
                "It shows 1 character but the text has 2. Characters that touch count as one."
                    .to_owned(),
                false
            ),
            (
                second,
                "Its picture is missing or can't be opened.".to_owned(),
                true
            ),
        ]
    );
    // A value that reads with the reference names it and says to edit it,
    // since the picture shows only in the reference's editor.
    let from_value = |error: &SavedError| {
        sample_problem_of(error, "Digits", &two.reference.samples)
            .map(|problem| problem.to_string())
    };
    assert_eq!(
        from_value(&SavedError::Learn {
            name: sample_name("digits", 1),
            source: LearnError::NoInk,
        }),
        Some(
            "Digits can't learn from its saved picture of \"77\": nothing in it stands out from \
             its background, or has the reference's text color. Edit Digits and remove that \
             picture under Saved pictures it can't learn, or change its text color."
                .to_owned()
        )
    );
    assert_eq!(
        from_value(&SavedError::Learn {
            name: sample_name("other", 1),
            source: LearnError::NoInk,
        }),
        None,
        "another reference's picture"
    );
    // A reference left with no sample has nothing to learn from.
    assert_eq!(
        saved
            .without(&[sample_name("digits", 1)])
            .with_settings("Digits", Some(RED), 0),
        Err(SpecError::NoLabels)
    );
}

/// A 5 by 5 screenshot, black but for two strokes of 200 with a shadow of
/// 60 between them: two characters by contrast at the tolerances 60 to
/// 199.
fn shadowed() -> Image {
    let blank = [0_u8; 5];
    let strokes = [0, 200, 60, 200, 0];
    let bgra = [blank, strokes, strokes, strokes, blank]
        .concat()
        .into_iter()
        .flat_map(|gray| [gray, gray, gray, 0])
        .collect();
    Image::new(
        Size {
            width: 5,
            height: 5,
        },
        bgra,
    )
    .expect("matching length")
}

#[test]
fn a_reference_by_contrast_takes_a_tolerance_its_samples_read_with() {
    let shots = [shadowed()];
    let all = RegionDef {
        x: 0,
        y: 0,
        width: 5,
        height: 5,
    };
    let spec = |reference, ink, tolerance| CharactersSpec {
        ink,
        tolerance,
        ..characters(
            all,
            reference,
            vec![Label {
                screenshot: 0,
                region: None,
                text: "11".to_owned(),
            }],
            Vec::new(),
        )
    };
    let fitted = |spec: CharactersSpec| spec.contrast_tolerance(session(&shots, None));
    assert_eq!(fitted(spec(None, None, 40)), Ok(128));
    assert_eq!(fitted(spec(None, None, 100)), Ok(100), "it reads as it is");
    assert_eq!(
        fitted(spec(None, Some(RED), 40)),
        Ok(40),
        "a text color's tolerance is the user's to set"
    );
    // The saved reference's sample is one plain color, which shows no
    // character by contrast, whatever the tolerance, so the box alone
    // chooses it.
    assert_eq!(fitted(spec(Some(digits_ref()), None, 40)), Ok(128));
    assert_eq!(
        fitted(CharactersSpec {
            screenshot: 3,
            ..spec(None, None, 40)
        }),
        Err(SpecError::NoScreenshot)
    );
    // The reference is saved without a text color, and reads by contrast.
    let taught = teach(&spec(None, None, 128), session(&shots, None)).expect("learns the 1");
    assert_eq!(taught.reference.ink, None);
    assert_eq!(taught.reference.tolerance, 128);
    assert_eq!(
        taught.reader.read(&shadowed(), false),
        Ok(Number::new(11.0).expect("finite"))
    );
    assert!(teach(&spec(None, None, 40), session(&shots, None)).is_err());
    // A box as tight as the strokes learns them too: the sample is cut
    // with the pixels around the box, which are its background.
    let tight = CharactersSpec {
        labels: vec![Label {
            screenshot: 0,
            region: Some(RegionDef {
                x: 1,
                y: 1,
                width: 3,
                height: 3,
            }),
            text: "11".to_owned(),
        }],
        ..spec(None, None, 128)
    };
    let taught = teach(&tight, session(&shots, None)).expect("learns the tight box");
    let [(_, picture)] = taught.pictures.as_slice() else {
        panic!("one sample, not {:?}", taught.pictures.len());
    };
    assert_eq!(
        Image::decode_png(picture)
            .ok()
            .map(|picture| picture.size()),
        Some(shadowed().size())
    );
}

#[test]
fn a_saved_reference_that_loses_its_text_color_takes_a_tolerance_its_samples_read_with() {
    let white = ColorDef {
        red: 200,
        green: 200,
        blue: 200,
    };
    let saved = || SavedReference {
        reference: ReferenceDef {
            ink: Some(white),
            tolerance: 10,
            samples: vec![SampleDef {
                image: sample_name("digits", 1),
                text: "11".to_owned(),
            }],
            ..digits(&digits_ref()).expect("saved").reference
        },
        pictures: vec![(
            sample_name("digits", 1),
            shadowed().encode_png().expect("encodes"),
        )],
    };
    let settings = |ink, tolerance| saved().with_settings("Digits", ink, tolerance);
    // The shadow between the strokes is ink at 40, so they read as one
    // character there.
    let by_contrast = settings(None, 40).expect("learns the strokes");
    assert_eq!((by_contrast.ink, by_contrast.tolerance), (None, 128));
    assert_eq!(settings(None, 100).map(|saved| saved.tolerance), Ok(100));
    // A text color's tolerance is the user's to set.
    assert_eq!(
        settings(Some(white), 10).map(|saved| saved.tolerance),
        Ok(10)
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
        "Its saved picture ref-digits-1.png is missing. Remove it under Saved pictures it can't \
         learn."
    );
    // Left out, it stops nothing.
    let without = CharactersSpec {
        dropped: vec![sample_name("digits", 1)],
        ..characters(left(), Some(digits_ref()), one(0), Vec::new())
    };
    assert!(teach(&without, session).is_ok());
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
    let empty = RegionDef {
        x: 0,
        y: 0,
        width: 0,
        height: 2,
    };
    let outside = RegionDef {
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
        InkRule::Color {
            color: RED.into(),
            tolerance: 0,
        },
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
    assert_eq!(
        error.to_string(),
        "Can't learn from the box with the text \"12\": it shows 1 character but the text has \
         2. Characters that touch count as one."
    );
    assert!(error.source().is_some());
    // A kept reading comes after the boxes, and the words say which it is.
    let now = std::time::Instant::now();
    let mut recording = Recording::new(
        Ticket::numbered(1),
        Region::new(0, 0, 4, 2).expect("not empty"),
        InkRule::Color {
            color: RED.into(),
            tolerance: 0,
        },
        (screenshot().size(), Follows::Drawn),
        now,
    );
    recording.offer(&screenshot(), now).expect("a reading");
    let kept = vec![RecordedLabel {
        reading: 0,
        text: "12".to_owned(),
    }];
    let spec = characters(whole(), None, one(0), kept);
    let error = teach(&spec, session(&[screenshot()], Some(&recording)))
        .expect_err("one blob, two characters");
    assert_eq!(
        error.to_string(),
        "Can't learn from the kept reading \"12\": it shows 1 character but the text has 2. \
         Characters that touch count as one."
    );
}

#[test]
fn a_failed_lesson_says_why_in_the_setup_apps_words() {
    for (error, says) in [
        (
            LearnError::MoreCharacters {
                found: 3,
                expected: 2,
            },
            "it shows 3 characters but the text has 2. A character drawn as separate marks side \
             by side, as some percent signs are, counts once for each mark, and so does \
             anything else that stands out as the characters do.",
        ),
        (
            LearnError::Conflict {
                learned: '"',
                new: '\'',
            },
            "a character given as \"'\" has the same shape as one learned as \"\"\". Change the \
             text of one of them, or leave one out.",
        ),
        (
            LearnError::Full,
            "the reference holds 256 character shapes already, the most one can hold. Remove \
             boxes, readings, or saved pictures it doesn't need.",
        ),
        (
            LearnError::TooManyCharacters,
            "it shows more than 24 characters.",
        ),
    ] {
        assert_eq!(cant_learn(&error), says);
    }
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
            region: RegionDef {
                x: 1,
                y: 2,
                width: 3,
                height: 4
            },
            screenshot: 0,
            reference: None,
            ink: Some(ColorDef {
                red: 1,
                green: 2,
                blue: 3
            }),
            tolerance: 5,
            labels: vec![Label {
                screenshot: 0,
                region: None,
                text: "12".to_owned(),
            }],
            recorded: Vec::new(),
            dropped: Vec::new(),
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
fn reference_pictures_leave_every_saved_picture_untouched() {
    let spec = characters(left(), None, [one(0), one(1)].concat(), Vec::new());
    let taught = spec
        .teach(
            session(&[screenshot(), screenshot()], None),
            Some("digits"),
            "Digits",
            &["ref-digits-1.png", "ref-digits-2.png", "ref-digits-3.png"],
        )
        .expect("learns without overwriting another saved item's picture");
    assert_eq!(
        taught
            .pictures
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["ref-digits-4.png", "ref-digits-5.png"]
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
            &[],
        )
        .expect("valid characters");
    assert_eq!(
        taught.reference,
        ReferenceDef {
            id: "digits".to_owned(),
            name: "Digits".to_owned(),
            origin: OriginDef::ByHand,
            ink: Some(RED),
            tolerance: 0,
            samples: vec![SampleDef {
                image: "ref-digits-1.png".to_owned(),
                text: "1".to_owned(),
            }],
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
fn expected_characters_are_listed_once_each_in_order() {
    assert_eq!(
        expected_characters(" b%1\u{7}a0 1\u{3000}%\u{85}"),
        Some("01%ab".to_owned()),
        "no whitespace, control character or repeat, and the digits first"
    );
    assert_eq!(
        expected_characters("\u{1d7d9}z\u{ff11}\u{feff}"),
        Some("z\u{feff}\u{ff11}\u{1d7d9}".to_owned()),
        "by code point, and only the digits 0 to 9 come first"
    );
    assert_eq!(expected_characters(""), None);
    assert_eq!(expected_characters(" \t\u{7}"), None);
}

#[test]
fn a_list_of_expected_characters_is_held_to_the_most_where_it_is_set_or_changed() {
    let needing = |characters: &str| CheckDef::Number {
        reference: "digits".to_owned(),
        whole: false,
        characters: Some(characters.to_owned()),
    };
    let most: String = ('\u{100}'..).take(MAX_EXPECTED_CHARACTERS).collect();
    let over = format!("{most}%");
    assert_eq!(expected_within_limit(&needing(&most), None), Ok(()));
    // A repeat or a space does not count toward the most.
    assert_eq!(
        expected_within_limit(&needing(&format!("{most} {most}")), None),
        Ok(())
    );
    assert_eq!(
        expected_within_limit(&needing(&over), None),
        Err(SpecError::TooManyExpected)
    );
    assert_eq!(
        expected_within_limit(&needing(&over), Some(&needing(&most))),
        Err(SpecError::TooManyExpected),
        "a list that grew past the most"
    );
    // A value saved with a longer list, such as one written by hand, keeps
    // it through a save that leaves it as it is, in whatever order.
    assert_eq!(
        expected_within_limit(&needing(&over), Some(&needing(&format!("% {most}")))),
        Ok(())
    );
    let bar = CheckDef::BarFill {
        filled: ColorDef {
            red: 1,
            green: 2,
            blue: 3,
        },
        empty: ColorDef {
            red: 4,
            green: 5,
            blue: 6,
        },
    };
    assert_eq!(expected_within_limit(&bar, Some(&needing(&over))), Ok(()));
    assert_eq!(
        SpecError::TooManyExpected.to_string(),
        "A value needs at most 128 characters. Remove some."
    );
}

#[test]
fn a_number_is_saved_with_the_characters_it_needs_listed_once_in_order() {
    let needing = |characters: &str| {
        let spec = CheckSpec::Number {
            region: left(),
            reference: digits_ref(),
            whole: false,
            characters: Some(characters.to_owned()),
        };
        match to_saved(&spec, &[], "app", "progress")
            .expect("a number")
            .check
        {
            CheckDef::Number { characters, .. } => characters,
            other => panic!("a number check, not {other:?}"),
        }
    };
    assert_eq!(needing("% 1 0 1"), Some("01%".to_owned()));
    // A value that names none reads every character its reference knows.
    assert_eq!(needing(" "), None);
}

#[test]
fn a_saved_reference_keeps_its_samples_and_numbers_new_ones_after_them() {
    let spec = CharactersSpec {
        tolerance: 3,
        ..characters(left(), Some(digits_ref()), one(0), Vec::new())
    };
    // Without an ID of its own, it is taught under the saved one's.
    let taught = spec
        .teach(session(&[screenshot()], None), None, "Red digits", &[])
        .expect("valid characters");
    let reference = taught.reference;
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
    let taught = spec
        .teach(session, Some("digits"), "Digits", &["ref-digits-3.png"])
        .expect("valid characters");
    let names: Vec<&str> = taught
        .pictures
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(
        names,
        ["ref-digits-4.png"],
        "another saved item owns the third name"
    );
    assert_eq!(taught.reference.samples.len(), 2);
}

#[test]
fn recorded_readings_are_learned_and_saved_after_the_labels() {
    let now = std::time::Instant::now();
    let region = Region::new(0, 0, 4, 2).expect("not empty");
    let mut recording = Recording::new(
        Ticket::numbered(1),
        region,
        InkRule::Color {
            color: RED.into(),
            tolerance: 0,
        },
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
        InkRule::Color {
            color: RED.into(),
            tolerance: 0,
        },
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
    let region = RegionDef {
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
            filled: RED,
            empty: GRAY
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
            color: GRAY,
            tolerance: 4
        }
    );
    assert_eq!(
        SpecError::Picture.to_string(),
        "AppyTappy can't save a picture of the region. Try again."
    );
    // Names and characters show as typed, in plain quotes.
    assert_eq!(
        SpecError::NothingSelected("C:\\Live".to_owned()).to_string(),
        "Nothing in the region is selected for the state \"C:\\Live\". Select it again, clear its selection, or stop ignoring the background."
    );
    assert_eq!(
        SpecError::NoCharacters {
            reference: "Digits".to_owned(),
            characters: "\\'".to_owned(),
        }
        .to_string(),
        "Digits knows none of the characters this value needs, \"\\'\". Teach it one of them, \
         or change the characters the value needs."
    );
    assert_eq!(
        SpecError::OtherApplication.to_string(),
        "That reference belongs to another application. Pick one of this application's."
    );
}
