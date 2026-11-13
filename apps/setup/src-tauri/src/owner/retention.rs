//! Tests of what the setup app saves of a window's pictures: only the
//! regions a reference, a state check, a fix, or a click needs, which read
//! back as the runtime reads them.

use std::fs;
use std::path::{Path, PathBuf};

use appytappy_config::app_file::SizeDef;
use appytappy_config::automation_file::{ClickAtDef, PointDef};
use appytappy_config::signal::{Number, StateName, Value};
use appytappy_sources::screen::saved::{self, SavedValue};
use appytappy_sources::screen::{FrameHandler, Image, Size};

use crate::check_spec::{ColorSpec, RegionSpec};
use crate::stores::Stores;

use super::messages::{FixSpec, ReferenceSaveSpec};
use super::test_support::{
    Harness, STATE, WINDOW, add_example_app, characters_in, frame_image, harness_with,
    scratch_store, spec,
};
use super::{ClickPictureView, FoundClick, Owner};

/// The window at twice the size of [`WINDOW`].
const LARGER: Size = Size {
    width: 8,
    height: 4,
};

/// An 8 by 4 frame, all red.
fn larger_image() -> Image {
    Image::new(LARGER, [0, 0, 255, 0].repeat(32)).expect("matching length")
}

/// A 4 by 2 frame with only its left column red and the rest gray, so the
/// left 2 by 2 region shows a narrower character than [`frame_image`]'s.
fn narrow_image() -> Image {
    let row = [[0, 0, 255, 0], [40; 4], [40; 4], [40; 4]].concat();
    Image::new(WINDOW, [row.clone(), row].concat()).expect("matching length")
}

/// The left 2 by 2 pixels of the window, where the number and the state
/// show.
fn left() -> RegionSpec {
    RegionSpec {
        x: 0,
        y: 0,
        width: 2,
        height: 2,
    }
}

fn red() -> ColorSpec {
    serde_json::from_str(r#"{"red":255,"green":0,"blue":0}"#).expect("valid JSON")
}

/// Every file under `folder`, as its path from there with `/` between the
/// parts and its size when it is a picture, sorted.
fn files_under(folder: &Path) -> Vec<(String, Option<Size>)> {
    fn walk(folder: &Path, root: &Path, found: &mut Vec<(String, Option<Size>)>) {
        for entry in fs::read_dir(folder).expect("lists") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                walk(&path, root, found);
                continue;
            }
            let parts: Vec<&str> = path
                .strip_prefix(root)
                .expect("under the root")
                .iter()
                .map(|part| part.to_str().expect("utf-8"))
                .collect();
            let size = path.extension().is_some_and(|kind| kind == "png").then(|| {
                Image::decode_png(&fs::read(&path).expect("reads"))
                    .expect("a PNG")
                    .size()
            });
            found.push((parts.join("/"), size));
        }
    }
    let mut found = Vec::new();
    walk(folder, folder, &mut found);
    found.sort_by(|left, right| left.0.cmp(&right.0));
    found
}

/// A harness that saved, for `Example App`: the reference `Digits`,
/// learned from a labeled screenshot and a recorded reading; the number
/// `Count`, which reads with it; the state `Icon visible`, with a fix at
/// [`LARGER`]; and a click picture. Each is drawn on a region smaller than
/// its screenshot.
fn with_everything_saved(name: &str) -> (Harness, FrameHandler, Stores, PathBuf, ClickPictureView) {
    let (stores, root) = scratch_store(name);
    let mut harness = harness_with(false, Ok(stores.clone()));
    add_example_app(&mut harness);
    let mut handler = harness.pick();
    harness.deliver(&mut handler, Ok(&frame_image()));
    assert_eq!(harness.ask(Owner::screenshot), Ok(0));

    harness
        .ask(|owner| owner.start_recording(characters_in(left(), red()), false))
        .expect("records");
    harness.deliver(&mut handler, Ok(&narrow_image()));
    harness.ask(Owner::stop_recording);
    let digits = ReferenceSaveSpec {
        app: "example".to_owned(),
        reference_name: "Digits".to_owned(),
        expects: String::new(),
        characters: serde_json::from_str(
            r#"{"region":{"x":0,"y":0,"width":2,"height":2},"screenshot":0,
                "reference":null,"ink":{"red":255,"green":0,"blue":0},"tolerance":0,
                "labels":[{"screenshot":0,"text":"1"}],"recorded":[{"reading":0,"text":"7"}]}"#,
        )
        .expect("valid JSON"),
    };
    harness
        .ask(|owner| owner.save_reference(&digits))
        .expect("saves the reference");
    let count = r#"{"kind":"number","region":{"x":0,"y":0,"width":2,"height":2},
        "reference":{"app":"example","id":"digits"},"whole":true}"#;
    harness
        .ask(|owner| owner.save(&spec("example", "Count", count)))
        .expect("saves the number");
    harness
        .ask(|owner| owner.save(&spec("example", "Icon visible", STATE)))
        .expect("saves the state");
    let click = harness
        .ask(|owner| {
            owner.add_click_picture(
                0,
                RegionSpec {
                    x: 2,
                    y: 0,
                    width: 2,
                    height: 2,
                },
                None,
            )
        })
        .expect("keeps the click picture");

    harness.deliver(&mut handler, Ok(&larger_image()));
    assert_eq!(harness.ask(Owner::screenshot), Ok(1));
    let fix = FixSpec {
        app: "example".to_owned(),
        value: "icon_visible".to_owned(),
        check: serde_json::from_str(
            r#"{"kind":"state","region":{"x":2,"y":0,"width":4,"height":4},"drawnOn":1,
                "states":[{"name":"Shown","screenshot":1}],"tolerance":0}"#,
        )
        .expect("valid JSON"),
        screenshot: 1,
    };
    harness
        .ask(|owner| owner.save_fix(&fix))
        .expect("saves the fix");
    (harness, handler, stores, root, click)
}

/// The saved value `id` of `Example App`, built from its saved file and
/// pictures as the runtime builds it.
fn read_back(stores: &Stores, id: &str) -> SavedValue {
    let app = stores.app("example").expect("saved");
    let value = app
        .file
        .values
        .iter()
        .find(|value| value.id == id)
        .expect("a saved value");
    saved::build(
        &value.screen,
        &mut saved::Readers::new(&app.file.references),
        app.file.size_changes,
        |name| app.picture(name),
    )
    .expect("the runtime can read it")
}

/// What `value` reads on `frame`, a whole frame of its window.
fn read_on(value: &SavedValue, frame: &Image) -> Value {
    let at = value.at(frame.size()).expect("reads at this size");
    let region = frame.crop(at.region).expect("inside the frame");
    value.check(&at).read(&region).expect("reads")
}

#[test]
fn each_saved_picture_holds_only_its_region() {
    let (_harness, _handler, _stores, root, click) = with_everything_saved("retention-regions");
    let small = Some(Size {
        width: 2,
        height: 2,
    });
    let fixed = Some(Size {
        width: 4,
        height: 4,
    });
    assert_eq!(click.image, "click-1.png");
    // Nothing else is written, and each picture has its region's size, not
    // its screenshot's 4 by 2 or 8 by 4.
    assert_eq!(
        files_under(&root),
        [
            ("AppyTappy/apps/example/app.json".to_owned(), None),
            (
                "AppyTappy/apps/example/icon_visible-1-1.png".to_owned(),
                fixed
            ),
            (
                "AppyTappy/apps/example/icon_visible-1.png".to_owned(),
                small
            ),
            ("AppyTappy/apps/example/ref-digits-1.png".to_owned(), small),
            ("AppyTappy/apps/example/ref-digits-2.png".to_owned(), small),
            ("AppyTappy/clicks/click-1.png".to_owned(), small),
        ]
    );
    fs::remove_dir_all(root).expect("clean up");
}

#[test]
fn what_is_saved_reads_as_the_runtime_reads_it() {
    let (mut harness, mut handler, stores, root, click) = with_everything_saved("retention-reads");
    let number = |raw: f64| Value::Number(Number::new(raw).expect("finite"));
    let state = |name: &str| Value::State(StateName::new(name).expect("valid"));

    // The reference's two samples, one from a labeled screenshot and one
    // from a recorded reading, each teach their character.
    let count = read_back(&stores, "count");
    assert_eq!(read_on(&count, &frame_image()), number(1.0));
    assert_eq!(read_on(&count, &narrow_image()), number(7.0));

    let icon = read_back(&stores, "icon_visible");
    assert_eq!(read_on(&icon, &frame_image()), state("Shown"));
    assert_eq!(
        read_on(&icon, &narrow_image()),
        Value::State(StateName::none())
    );
    // At the fix's size it reads the fix's own region with its own picture.
    let at = icon.at(LARGER).expect("fixed at this size");
    assert!(at.is_drawn());
    assert_eq!(
        (at.region.x(), at.region.size()),
        (
            2,
            Size {
                width: 4,
                height: 4
            }
        )
    );
    assert_eq!(read_on(&icon, &larger_image()), state("Shown"));

    // The click picture is found where it was taken, on the gray half.
    harness.deliver(&mut handler, Ok(&frame_image()));
    let wanted = ClickAtDef::Picture {
        image: click.image,
        window_size: SizeDef {
            width: click.window_width,
            height: click.window_height,
        },
        search: appytappy_config::app_file::RegionDef {
            x: 1,
            y: 0,
            width: 3,
            height: 2,
        },
        press: PointDef { x: 0, y: 1 },
        tolerance: 0,
    };
    assert_eq!(
        harness.ask(|owner| owner.find_click(&wanted)),
        Ok(FoundClick {
            x: 2,
            y: 1,
            likeness: Some(100),
        })
    );
    fs::remove_dir_all(root).expect("clean up");
}
