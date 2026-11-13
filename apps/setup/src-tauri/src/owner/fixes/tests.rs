use std::path::{Path, PathBuf};

use appytappy_config::app_file::{AppFile, RegionDef};
use appytappy_sources::screen::{FrameHandler, Image};

use crate::owner::ReferenceSaveSpec;
use crate::owner::test_support::{
    FakeHost, Harness, STATE, add_example_app, frame_image, harness_with, scratch_store, spec,
    with_saved_bar,
};

use super::*;

/// An 8 by 4 frame, twice the size of [`frame_image`], all red.
fn larger_image() -> Image {
    Image::new(
        Size {
            width: 8,
            height: 4,
        },
        [0, 0, 255, 0].repeat(32),
    )
    .expect("matching length")
}

const LARGER: SizeDef = SizeDef {
    width: 8,
    height: 4,
};

/// A harness with the state value `Icon visible` saved for `Example App`
/// on a 4 by 2 screenshot, a second screenshot of the window at 8 by 4,
/// and the stores' folder.
fn with_saved_state(name: &str) -> (Harness, FrameHandler, PathBuf) {
    let (stores, root) = scratch_store(name);
    let mut harness = harness_with(false, Ok(stores));
    add_example_app(&mut harness);
    let mut handler = harness.pick();
    harness.deliver(&mut handler, Ok(&frame_image()));
    assert_eq!(harness.ask(Owner::screenshot), Ok(0));
    harness
        .ask(|owner| owner.save(&spec("example", "Icon visible", STATE)))
        .expect("saves");
    harness.deliver(&mut handler, Ok(&larger_image()));
    assert_eq!(harness.ask(Owner::screenshot), Ok(1));
    (harness, handler, root)
}

/// The fix of `value` drawn on screenshot 1 with `check`.
fn fix(value: &str, check: &str) -> FixSpec {
    FixSpec {
        app: "example".to_owned(),
        value: value.to_owned(),
        check: serde_json::from_str(check).expect("valid JSON"),
        screenshot: 1,
    }
}

/// `Icon visible`'s one state taken on screenshot 1 at `region`, under the
/// state's name `name`.
fn state_fix(name: &str) -> FixSpec {
    fix(
        "icon_visible",
        &format!(
            r#"{{"kind":"state","region":{{"x":2,"y":0,"width":4,"height":4}},"drawnOn":1,
                "states":[{{"name":"{name}","screenshot":1}}],"tolerance":0}}"#
        ),
    )
}

fn saved_file(harness: &Harness) -> AppFile {
    harness
        .owner
        .stores()
        .expect("stores")
        .app("example")
        .expect("saved")
        .file
}

fn folder(harness: &Harness) -> PathBuf {
    harness
        .owner
        .stores()
        .expect("stores")
        .apps
        .folder("example")
}

fn fixes_of(harness: &Harness) -> Vec<FixDef> {
    saved_file(harness)
        .values
        .into_iter()
        .next()
        .expect("a value")
        .screen
        .fixes
}

fn exists(folder: &Path, picture: &str) -> bool {
    folder.join(picture).exists()
}

#[test]
fn a_fix_saves_its_own_pictures_replaces_the_one_at_its_size_and_can_be_removed() {
    let (mut harness, _handler, root) = with_saved_state("fix-state");
    let saved = harness.ask(|owner| owner.save_fix(&state_fix("Shown")));
    assert_eq!(
        saved,
        Ok(SavedValue {
            app: "example".to_owned(),
            value: "icon_visible".to_owned(),
        })
    );
    let first = FixDef {
        window_size: LARGER,
        region: RegionDef {
            x: 2,
            y: 0,
            width: 4,
            height: 4,
        },
        images: vec!["icon_visible-1-1.png".to_owned()],
        reference: None,
    };
    assert_eq!(fixes_of(&harness), std::slice::from_ref(&first));
    let folder = folder(&harness);
    assert!(exists(&folder, "icon_visible-1.png"), "the first drawing's");
    assert!(exists(&folder, "icon_visible-1-1.png"));
    // Fixed again at the same size, it takes the next number, so the
    // pictures it replaces are never overwritten, and they go.
    harness
        .ask(|owner| owner.save_fix(&state_fix("Shown")))
        .expect("saves");
    let again = FixDef {
        images: vec!["icon_visible-2-1.png".to_owned()],
        ..first
    };
    assert_eq!(fixes_of(&harness), [again]);
    assert!(!exists(&folder, "icon_visible-1-1.png"));
    assert!(exists(&folder, "icon_visible-2-1.png"));
    assert_eq!(
        harness.ask(|owner| owner.remove_fix("example", "icon_visible", LARGER)),
        Ok(())
    );
    assert_eq!(fixes_of(&harness), []);
    assert!(!exists(&folder, "icon_visible-2-1.png"));
    assert!(exists(&folder, "icon_visible-1.png"));
    assert_eq!(
        harness.ask(|owner| owner.remove_fix("example", "icon_visible", LARGER)),
        Err("Icon visible has no fix at 8 × 4 any more.".to_owned())
    );
    assert_eq!(
        harness.ask(|owner| owner.remove_fix("example", "gone", LARGER)),
        Err("Example App no longer has that value.".to_owned())
    );
    std::fs::remove_dir_all(root).expect("clean up");
}

#[test]
fn a_fix_that_does_not_suit_its_value_says_what_to_change() {
    let (mut harness, _handler, root) = with_saved_state("fix-wrong");
    assert_eq!(
        harness.ask(|owner| owner.save_fix(&state_fix("Hidden"))),
        Err("Take a picture of each of the value's states, in order: Shown.".to_owned())
    );
    let bar = fix(
        "icon_visible",
        r#"{"kind":"barFill","region":{"x":0,"y":0,"width":8,"height":4},
            "filled":{"red":255,"green":0,"blue":0},"empty":{"red":40,"green":40,"blue":40}}"#,
    );
    assert_eq!(
        harness.ask(|owner| owner.save_fix(&bar)),
        Err("A fix reads the value the way it was first drawn. Pick that kind.".to_owned())
    );
    let mut drawn_size = state_fix("Shown");
    drawn_size.check = serde_json::from_str(STATE).expect("valid JSON");
    drawn_size.screenshot = 0;
    assert_eq!(
        harness.ask(|owner| owner.save_fix(&drawn_size)),
        Err(
            "Icon visible was drawn at this size, so it needs no fix here. Resize the window \
             first."
                .to_owned()
        )
    );
    let mut gone = state_fix("Shown");
    "gone".clone_into(&mut gone.value);
    assert_eq!(
        harness.ask(|owner| owner.save_fix(&gone)),
        Err("Example App no longer has that value.".to_owned())
    );
    let mut unshot = state_fix("Shown");
    unshot.screenshot = 5;
    assert_eq!(
        harness.ask(|owner| owner.save_fix(&unshot)),
        Err("Take a screenshot first.".to_owned())
    );
    assert_eq!(fixes_of(&harness), [], "nothing was saved");
    std::fs::remove_dir_all(root).expect("clean up");
}

#[test]
fn a_bar_fix_keeps_only_its_region_and_the_flow_starts_from_it() {
    let (mut harness, root) = with_saved_bar("fix-bar");
    let place = |owner: &mut Owner<FakeHost>| owner.place_value("example", "progress_bar", LARGER);
    // Twice the height, the bar drawn on the whole window doubles.
    assert_eq!(
        harness.ask(place),
        Ok(Some(RegionSpec {
            x: 0,
            y: 0,
            width: 8,
            height: 4,
        }))
    );
    let mut handler = harness.pick();
    harness.deliver(&mut handler, Ok(&larger_image()));
    assert_eq!(harness.ask(Owner::screenshot), Ok(0));
    assert_eq!(harness.ask(Owner::screenshot), Ok(1));
    let bar = fix(
        "progress_bar",
        r#"{"kind":"barFill","region":{"x":1,"y":1,"width":6,"height":2},
            "filled":{"red":0,"green":255,"blue":0},"empty":{"red":0,"green":0,"blue":0}}"#,
    );
    harness.ask(|owner| owner.save_fix(&bar)).expect("saves");
    let region = RegionDef {
        x: 1,
        y: 1,
        width: 6,
        height: 2,
    };
    assert_eq!(
        fixes_of(&harness),
        [FixDef {
            window_size: LARGER,
            region,
            images: Vec::new(),
            reference: None,
        }],
        "its colors are the first drawing's"
    );
    assert_eq!(
        harness.ask(place),
        Ok(Some(RegionSpec {
            x: 1,
            y: 1,
            width: 6,
            height: 2,
        })),
        "the fix at this size"
    );
    // Half as wide, a bar pinned left that fills the window passes its
    // right edge.
    let narrow = SizeDef {
        width: 2,
        height: 2,
    };
    assert_eq!(
        harness.ask(|owner| owner.place_value("example", "progress_bar", narrow)),
        Ok(None)
    );
    assert_eq!(
        harness.ask(|owner| owner.place_value("example", "gone", narrow)),
        Err("Example App no longer has that value.".to_owned())
    );
    std::fs::remove_dir_all(root).expect("clean up");
}

/// A number check of `region`, as JSON, that reads with the reference
/// `reference` of `Example App`.
fn number(region: &str, reference: &str) -> String {
    format!(
        r#"{{"kind":"number","region":{region},
            "reference":{{"app":"example","id":"{reference}"}},"whole":true}}"#
    )
}

#[test]
fn a_number_fix_names_its_reference_only_when_it_is_not_the_first_drawing_s() {
    let (stores, root) = scratch_store("fix-number");
    let mut harness = harness_with(false, Ok(stores));
    add_example_app(&mut harness);
    let mut handler = harness.pick();
    harness.deliver(&mut handler, Ok(&frame_image()));
    assert_eq!(harness.ask(Owner::screenshot), Ok(0));
    for name in ["Digits", "Small"] {
        // The red square on screenshot 0 taught as a 1.
        let reference = ReferenceSaveSpec {
            app: "example".to_owned(),
            reference_name: name.to_owned(),
            expects: String::new(),
            characters: serde_json::from_str(
                r#"{"region":{"x":0,"y":0,"width":2,"height":2},
                    "screenshot":0,"reference":null,"ink":{"red":255,"green":0,"blue":0},
                    "tolerance":0,"labels":[{"screenshot":0,"text":"1"}]}"#,
            )
            .expect("valid JSON"),
        };
        harness
            .ask(|owner| owner.save_reference(&reference))
            .expect("saves");
    }
    let drawn = number(r#"{"x":0,"y":0,"width":2,"height":2}"#, "digits");
    harness
        .ask(|owner| owner.save(&spec("example", "Progress", &drawn)))
        .expect("saves");
    harness.deliver(&mut handler, Ok(&larger_image()));
    assert_eq!(harness.ask(Owner::screenshot), Ok(1));
    let reading_with = |reference: &str| {
        fix(
            "progress",
            &number(r#"{"x":0,"y":0,"width":4,"height":4}"#, reference),
        )
    };
    let references = |harness: &Harness| -> Vec<Option<String>> {
        fixes_of(harness)
            .into_iter()
            .map(|fix| fix.reference)
            .collect()
    };
    harness
        .ask(|owner| owner.save_fix(&reading_with("digits")))
        .expect("saves");
    assert_eq!(
        references(&harness),
        [None],
        "the first drawing's reference"
    );
    harness
        .ask(|owner| owner.save_fix(&reading_with("small")))
        .expect("saves");
    assert_eq!(
        references(&harness),
        [Some("small".to_owned())],
        "a reference of its own"
    );
    std::fs::remove_dir_all(root).expect("clean up");
}

#[test]
fn new_pictures_take_the_smallest_number_no_saved_picture_uses() {
    let free = |images: &[&str]| free_picture_number("icon", images.iter().copied());
    assert_eq!(free(&[]), 1);
    assert_eq!(free(&["icon-1-1.png", "icon-1-2.png"]), 2);
    assert_eq!(free(&["icon-2-1.png", "icon-1-1.png"]), 3);
    assert_eq!(free(&["icon-2-1.png"]), 1);
    // A new value's own pictures, and pictures named otherwise, take none.
    assert_eq!(free(&["other-1-1.png", "icon-1.png"]), 1);
}
