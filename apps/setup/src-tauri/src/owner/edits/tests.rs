use std::collections::BTreeMap;
use std::path::PathBuf;

use appytappy_config::app_file::{
    AppFile, ColorDef, FixDef, PinDef, RegionDef, SizeChangesDef, StateDef, WindowDef,
};
use appytappy_config::automation_file::AutomationDef;
use appytappy_config::window_match::WindowFacts;
use appytappy_sources::screen::{FrameHandler, Size};

use crate::applications::AppSpec;
use crate::owner::messages::{FixSpec, Reading};
use crate::owner::test_support::{
    BAR, Harness, NO_STORE, add_example_app, frame_image, harness, harness_with, scratch_store,
    spec, with_saved_bar,
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

/// A frame of [`frame_image`]'s size, all gray.
fn gray_image() -> Image {
    Image::new(
        Size {
            width: 4,
            height: 2,
        },
        [40, 40, 40, 0].repeat(8),
    )
    .expect("matching length")
}

const LARGER: SizeDef = SizeDef {
    width: 8,
    height: 4,
};

/// The red square of screenshot 0 as `Shown` and the gray one of screenshot
/// 1 as `Hidden`.
const TWO_STATES: &str = r#"{"kind":"state","region":{"x":0,"y":0,"width":2,"height":2},
    "drawnOn":0,"states":[{"name":"Shown","screenshot":0},{"name":"Hidden","screenshot":1}],
    "tolerance":0}"#;

/// A harness with the state value `Icon visible` saved for `Example App`
/// with the states `Shown` and `Hidden`, drawn on the 4 by 2 screenshot 0,
/// then screenshot 1, all gray at that size, and screenshot 2 of the window
/// at 8 by 4, which is the newest frame.
fn with_saved_states(name: &str) -> (Harness, FrameHandler, PathBuf) {
    let (stores, root) = scratch_store(name);
    let mut harness = harness_with(false, Ok(stores));
    add_example_app(&mut harness);
    let mut handler = harness.pick();
    for (index, image) in [frame_image(), gray_image(), larger_image()]
        .iter()
        .enumerate()
    {
        harness.deliver(&mut handler, Ok(image));
        assert_eq!(harness.ask(Owner::screenshot), Ok(index));
    }
    harness
        .ask(|owner| owner.save(&spec("example", "Icon visible", TWO_STATES)))
        .expect("saves");
    (harness, handler, root)
}

/// An edit of the value `value` of `Example App` that names it `name` and
/// reads it as `drawing`, in the UI's JSON.
fn edit(value: &str, name: &str, drawing: &str) -> EditSpec {
    EditSpec {
        app: "example".to_owned(),
        value: value.to_owned(),
        value_name: name.to_owned(),
        pin: PinDef::TopLeft,
        drawing: serde_json::from_str(drawing).expect("valid JSON"),
    }
}

/// An edit's drawing that keeps the region, with the saved state pictures
/// under the names `names` and compared within `tolerance`.
fn kept_states(names: [&str; 2], tolerance: u8) -> String {
    format!(
        r#"{{"kind":"kept","check":{{"kind":"state","states":[
            {{"name":"{}","image":"icon_visible-1.png"}},
            {{"name":"{}","image":"icon_visible-2.png"}}],"tolerance":{tolerance}}}}}"#,
        names[0], names[1]
    )
}

/// An edit's drawing that takes both states again at `x`, under `names`.
fn drawn_states(x: u32, names: [&str; 2]) -> String {
    format!(
        r#"{{"kind":"drawn","screenshot":0,"check":{{"kind":"state",
            "region":{{"x":{x},"y":0,"width":2,"height":2}},"drawnOn":0,"states":[
            {{"name":"{}","screenshot":0}},{{"name":"{}","screenshot":1}}],"tolerance":0}}}}"#,
        names[0], names[1]
    )
}

/// An edit's drawing that keeps the region and looks for red in it.
const KEPT_COLOR: &str = r#"{"kind":"kept","check":{"kind":"color",
    "color":{"red":255,"green":0,"blue":0},"tolerance":10}}"#;

/// [`BAR`]'s check on the region as saved.
const KEPT_BAR: &str = r#"{"kind":"kept","check":{"kind":"barFill",
    "filled":{"red":255,"green":0,"blue":0},"empty":{"red":40,"green":40,"blue":40}}}"#;

/// A bar with another filled color on the region as saved.
const KEPT_GREEN_BAR: &str = r#"{"kind":"kept","check":{"kind":"barFill",
    "filled":{"red":0,"green":255,"blue":0},"empty":{"red":40,"green":40,"blue":40}}}"#;

fn saved_file(harness: &Harness) -> AppFile {
    harness
        .owner
        .stores()
        .expect("stores")
        .app("example")
        .expect("saved")
        .file
}

/// Every file in the application's folder, by name, with its bytes.
fn folder_files(harness: &Harness) -> BTreeMap<String, Vec<u8>> {
    let folder = harness
        .owner
        .stores()
        .expect("stores")
        .apps
        .folder("example");
    std::fs::read_dir(&folder)
        .expect("lists the folder")
        .map(|entry| {
            let entry = entry.expect("an entry");
            let name = entry.file_name().to_string_lossy().into_owned();
            (name, std::fs::read(entry.path()).expect("reads"))
        })
        .collect()
}

fn names(files: &BTreeMap<String, Vec<u8>>) -> Vec<&str> {
    files.keys().map(String::as_str).collect()
}

fn state(name: &str, image: &str) -> StateDef {
    StateDef {
        name: name.to_owned(),
        image: image.to_owned(),
    }
}

/// An automation named `name` that watches `value` of `Example App` for the
/// target `target`, in the UI's JSON.
fn automation(name: &str, value: &str, is: &str, target: &serde_json::Value) -> AutomationDef {
    serde_json::from_value(serde_json::json!({
        "id": "", "name": name, "enabled": true,
        "when": {"app": "example", "value": value, "is": is, "target": target},
        "actions": [{"kind": "media", "command": "pause"}], "undo": false
    }))
    .expect("the UI's spelling")
}

fn save_automation(harness: &mut Harness, automation: AutomationDef) {
    harness
        .ask(|owner| owner.save_automation(None, automation))
        .expect("saves");
}

/// Saves `Example App` again, with its window changing size as `changes`.
fn set_size_changes(harness: &mut Harness, changes: SizeChangesDef) {
    harness
        .ask(|owner| {
            owner.save_app(
                Some("example"),
                AppSpec {
                    name: "Example App".to_owned(),
                    window: WindowDef::exact(WindowFacts {
                        title: "Example App",
                        class: "ExampleWindow",
                        executable: "Example.exe",
                    }),
                    size_changes: changes,
                },
            )
        })
        .expect("saves");
}

/// Saves `spec`, with the user's word for losing fixes when `drop_fixes`.
fn save(harness: &mut Harness, spec: &EditSpec, drop_fixes: bool) -> Result<SavedValue, String> {
    harness.ask(|owner| owner.edit_value(spec, drop_fixes))
}

#[test]
fn a_rename_keeps_the_id_the_pictures_and_the_automation_that_watches_it() {
    let (mut harness, _handler, root) = with_saved_states("edit-rename");
    save_automation(
        &mut harness,
        automation("Pause music", "icon_visible", "equals", &"Shown".into()),
    );
    let before = saved_file(&harness);
    let pictures = folder_files(&harness);
    harness.forget_reloads();
    // The window is 8 by 4 now, and nothing here needs it at 4 by 2.
    let mut renamed = edit(
        "icon_visible",
        "  Icon ",
        &kept_states(["Shown", "Away"], 5),
    );
    renamed.pin = PinDef::BottomRight;
    assert_eq!(
        save(&mut harness, &renamed, false),
        Ok(SavedValue {
            app: "example".to_owned(),
            value: "icon_visible".to_owned(),
        })
    );
    assert_eq!(harness.reloads(), 1, "the runtime reads the value again");
    let after = saved_file(&harness);
    let value = &after.values[0];
    assert_eq!(
        (value.id.as_str(), value.name.as_str()),
        ("icon_visible", "Icon")
    );
    assert_eq!(
        value.screen,
        ScreenDef {
            check: CheckDef::State {
                states: vec![
                    state("Shown", "icon_visible-1.png"),
                    state("Away", "icon_visible-2.png")
                ],
                tolerance: 5,
            },
            pin: Some(PinDef::BottomRight),
            ..before.values[0].screen.clone()
        },
        "the region and the size it was drawn at stay"
    );
    assert_eq!(after.checked, before.checked, "the checked build stays");
    assert!(after.checked.is_some());
    let kept = folder_files(&harness);
    assert_eq!(
        names(&kept),
        ["app.json", "icon_visible-1.png", "icon_visible-2.png"]
    );
    assert_eq!(kept["icon_visible-1.png"], pictures["icon_visible-1.png"]);
    assert_eq!(kept["icon_visible-2.png"], pictures["icon_visible-2.png"]);
    let listed = harness
        .ask(|owner| owner.list_automations())
        .expect("lists");
    assert_eq!(listed[0].problem, None, "the automation still finds it");

    // Saved again as it is, nothing is written and no one is told.
    "Icon".clone_into(&mut renamed.value_name);
    assert!(save(&mut harness, &renamed, false).is_ok());
    assert_eq!(harness.reloads(), 0);

    // The name it has now is taken, and the old one is free again: a new
    // value named so takes the ID with a number.
    assert_eq!(
        harness.ask(|owner| owner.save(&spec("example", "icon", BAR))),
        Err("Example App already has a value named like \"icon\". Pick another name.".to_owned())
    );
    assert_eq!(
        harness.ask(|owner| owner.save(&spec("example", "Icon visible", BAR))),
        Ok(SavedValue {
            app: "example".to_owned(),
            value: "icon_visible_2".to_owned(),
        })
    );
    // A value keeps its own name, and cannot take another's.
    let files = folder_files(&harness);
    assert_eq!(
        save(
            &mut harness,
            &edit("icon_visible_2", "ICON", KEPT_BAR),
            false
        ),
        Err("Example App already has a value named like \"ICON\". Pick another name.".to_owned())
    );
    assert_eq!(
        folder_files(&harness),
        files,
        "a refused edit changes nothing"
    );
    let own = edit("icon_visible_2", "icon VISIBLE", KEPT_BAR);
    assert!(save(&mut harness, &own, false).is_ok());
    assert_eq!(saved_file(&harness).values[1].name, "icon VISIBLE");
    std::fs::remove_dir_all(root).expect("clean up");
}

#[test]
fn an_edit_that_would_break_an_automation_is_refused_and_changes_nothing() {
    let (mut harness, _handler, root) = with_saved_states("edit-automations");
    for (name, is, target) in [
        ("Pause", "equals", "Shown"),
        ("Mute", "notEquals", "Shown"),
        ("Stop", "equals", "Hidden"),
        ("Rest", "equals", "none"),
    ] {
        save_automation(
            &mut harness,
            automation(name, "icon_visible", is, &target.into()),
        );
    }
    // An automation that names no state of the value is broken already, so
    // no edit is what breaks it.
    harness
        .ask(|owner| {
            let stores = owner.stores()?;
            let mut file = stores
                .automations
                .load()
                .map_err(|error| error.to_string())?;
            let mut old = automation("Old", "icon_visible", "equals", &"Gone".into());
            "old".clone_into(&mut old.id);
            file.automations.push(old);
            stores
                .automations
                .save(&file)
                .map_err(|error| error.to_string())
        })
        .expect("writes");
    let files = folder_files(&harness);
    harness.forget_reloads();

    let renamed = edit(
        "icon_visible",
        "Icon visible",
        &kept_states(["Seen", "Away"], 0),
    );
    let states = "Pause and Mute use the state \"Shown\" of Icon visible, which this change \
                  renames or removes. Change or delete them first. Stop uses the state \
                  \"Hidden\" of Icon visible, which this change renames or removes. Change or \
                  delete it first."
        .to_owned();
    assert_eq!(save(&mut harness, &renamed, false), Err(states.clone()));
    assert_eq!(
        harness.ask(|owner| owner.try_edit(&renamed)),
        Ok(EditPreview {
            unread: None,
            dropped_fixes: Vec::new(),
            refused: Some(states),
        }),
        "the editor hears of it before the save"
    );
    let color = edit("icon_visible", "Icon visible", KEPT_COLOR);
    assert_eq!(
        save(&mut harness, &color, false),
        Err(
            "Pause, Mute, Stop and Rest use Icon visible as a state, and this change makes it \
             read yes or no. Change or delete them first."
                .to_owned()
        )
    );
    assert_eq!(folder_files(&harness), files, "nothing was written");
    assert_eq!(harness.reloads(), 0, "and no one was told");

    // An edit that keeps what they compare with is saved.
    let named = edit(
        "icon_visible",
        "Status",
        &kept_states(["Shown", "Hidden"], 3),
    );
    assert!(save(&mut harness, &named, false).is_ok());
    // One state renamed stops only the automations that name it.
    let hidden = edit("icon_visible", "Status", &kept_states(["Shown", "Away"], 3));
    assert_eq!(
        save(&mut harness, &hidden, false),
        Err(
            "Stop uses the state \"Hidden\" of Status, which this change renames or removes. \
             Change or delete it first."
                .to_owned()
        )
    );
    std::fs::remove_dir_all(root).expect("clean up");
}

#[test]
fn a_number_that_would_read_yes_or_no_names_the_automations_that_compare_it() {
    let (mut harness, root) = with_saved_bar("edit-kind");
    save_automation(
        &mut harness,
        automation("Low", "progress_bar", "below", &50.into()),
    );
    let color = edit("progress_bar", "Progress bar", KEPT_COLOR);
    assert_eq!(
        save(&mut harness, &color, false),
        Err(
            "Low uses Progress bar as a number, and this change makes it read yes or no. Change \
             or delete it first."
                .to_owned()
        )
    );
    // Another check that reads a number keeps it.
    let green = edit("progress_bar", "Progress bar", KEPT_GREEN_BAR);
    assert!(save(&mut harness, &green, false).is_ok());
    std::fs::remove_dir_all(root).expect("clean up");
}

/// A fix of `Icon visible` at 8 by 4, with both states taken on screenshot
/// 2.
fn larger_fix() -> FixSpec {
    FixSpec {
        app: "example".to_owned(),
        value: "icon_visible".to_owned(),
        check: serde_json::from_str(
            r#"{"kind":"state","region":{"x":0,"y":0,"width":4,"height":4},"drawnOn":2,
                "states":[{"name":"Shown","screenshot":2},{"name":"Hidden","screenshot":2}],
                "tolerance":0}"#,
        )
        .expect("valid JSON"),
        screenshot: 2,
    }
}

fn fixes_of(harness: &Harness) -> Vec<FixDef> {
    saved_file(harness).values.swap_remove(0).screen.fixes
}

#[test]
fn fixes_stay_while_they_suit_the_edit_and_go_only_once_the_user_agrees() {
    let (mut harness, _handler, root) = with_saved_states("edit-fixes");
    harness
        .ask(|owner| owner.save_fix(&larger_fix()))
        .expect("saves a fix");
    let fix = fixes_of(&harness);
    assert_eq!(
        fix[0].images,
        ["icon_visible-1-1.png", "icon_visible-1-2.png"]
    );
    let fix_pictures = folder_files(&harness);
    let dropped = |harness: &mut Harness, spec: &EditSpec| {
        harness
            .ask(|owner| owner.try_edit(spec))
            .map(|preview| preview.dropped_fixes)
    };

    // Renamed with their saved pictures, the states are the ones each fix
    // has a picture of.
    let renamed = edit(
        "icon_visible",
        "Icon visible",
        &kept_states(["Seen", "Away"], 0),
    );
    assert!(save(&mut harness, &renamed, false).is_ok());
    assert_eq!(fixes_of(&harness), fix);

    // Taken again under the names they have, they still are. The new
    // pictures take a number no saved picture has, and the old ones go.
    let moved = edit(
        "icon_visible",
        "Icon visible",
        &drawn_states(1, ["Seen", "Away"]),
    );
    assert_eq!(dropped(&mut harness, &moved), Ok(Vec::new()));
    assert!(save(&mut harness, &moved, false).is_ok());
    assert_eq!(fixes_of(&harness), fix);
    let value = saved_file(&harness).values.swap_remove(0);
    assert_eq!(
        value.screen.region,
        RegionDef {
            x: 1,
            y: 0,
            width: 2,
            height: 2,
        }
    );
    assert_eq!(
        value.screen.check.states(),
        [
            state("Seen", "icon_visible-2-1.png"),
            state("Away", "icon_visible-2-2.png")
        ]
    );
    let files = folder_files(&harness);
    assert_eq!(
        names(&files),
        [
            "app.json",
            "icon_visible-1-1.png",
            "icon_visible-1-2.png",
            "icon_visible-2-1.png",
            "icon_visible-2-2.png"
        ]
    );
    assert_eq!(
        files["icon_visible-1-1.png"], fix_pictures["icon_visible-1-1.png"],
        "the fix keeps its pictures"
    );

    // Taken again under another name, as another number of states, or read
    // with another kind of check, no fix's pictures say which state they
    // show.
    harness.forget_reloads();
    let other = edit(
        "icon_visible",
        "Icon visible",
        &drawn_states(1, ["Seen", "Gone"]),
    );
    assert_eq!(dropped(&mut harness, &other), Ok(vec![LARGER]));
    assert_eq!(
        save(&mut harness, &other, false),
        Err(
            "This change leaves the fix of Icon visible at 8 × 4 unusable, so saving removes \
             it. Agree to that in the editor to save."
                .to_owned()
        )
    );
    let one = edit(
        "icon_visible",
        "Icon visible",
        r#"{"kind":"drawn","screenshot":0,"check":{"kind":"state",
            "region":{"x":1,"y":0,"width":2,"height":2},"drawnOn":0,
            "states":[{"name":"Seen","screenshot":0}],"tolerance":0}}"#,
    );
    assert_eq!(dropped(&mut harness, &one), Ok(vec![LARGER]));
    let color = edit("icon_visible", "Icon visible", KEPT_COLOR);
    assert_eq!(dropped(&mut harness, &color), Ok(vec![LARGER]));
    assert!(save(&mut harness, &color, false).is_err());
    assert_eq!(folder_files(&harness), files, "nothing was written");
    assert_eq!(harness.reloads(), 0, "and no one was told");

    assert!(save(&mut harness, &other, true).is_ok());
    assert_eq!(fixes_of(&harness), []);
    assert_eq!(
        names(&folder_files(&harness)),
        ["app.json", "icon_visible-3-1.png", "icon_visible-3-2.png"],
        "the fix's pictures went with it"
    );
    std::fs::remove_dir_all(root).expect("clean up");
}

#[test]
fn agreeing_to_lose_fixes_removes_none_that_still_suits() {
    let (mut harness, root) = with_saved_bar("edit-bar-fix");
    let mut handler = harness.pick();
    harness.deliver(&mut handler, Ok(&larger_image()));
    assert_eq!(harness.ask(Owner::screenshot), Ok(0));
    let fix = FixSpec {
        app: "example".to_owned(),
        value: "progress_bar".to_owned(),
        check: serde_json::from_str(
            r#"{"kind":"barFill","region":{"x":1,"y":1,"width":6,"height":2},
                "filled":{"red":0,"green":255,"blue":0},"empty":{"red":0,"green":0,"blue":0}}"#,
        )
        .expect("valid JSON"),
        screenshot: 0,
    };
    harness.ask(|owner| owner.save_fix(&fix)).expect("saves");
    let green = edit("progress_bar", "Progress", KEPT_GREEN_BAR);
    assert!(save(&mut harness, &green, true).is_ok());
    assert_eq!(fixes_of(&harness).len(), 1, "a bar's fix suits a bar");
    let color = edit("progress_bar", "Progress", KEPT_COLOR);
    assert!(save(&mut harness, &color, true).is_ok());
    assert_eq!(fixes_of(&harness), []);
    std::fs::remove_dir_all(root).expect("clean up");
}

#[test]
fn the_region_moves_only_on_a_screenshot_of_the_size_the_value_was_drawn_at() {
    let (mut harness, _handler, root) = with_saved_states("edit-size");
    let drawn = |screenshot: usize| {
        edit(
            "icon_visible",
            "Icon visible",
            &format!(
                r#"{{"kind":"drawn","screenshot":{screenshot},"check":{{"kind":"color",
                    "region":{{"x":2,"y":0,"width":2,"height":2}},
                    "color":{{"red":255,"green":0,"blue":0}},"tolerance":10}}}}"#
            ),
        )
    };
    let files = folder_files(&harness);
    assert_eq!(
        save(&mut harness, &drawn(2), true),
        Err(
            "Icon visible was drawn at 4 × 2, and this screenshot is 8 × 4. Resize the window \
             to 4 × 2 to move its region, or fix it at this size instead."
                .to_owned()
        )
    );
    assert_eq!(
        save(&mut harness, &drawn(9), true),
        Err("Take a screenshot first.".to_owned())
    );
    assert_eq!(
        save(&mut harness, &edit("icon_visible", " ", KEPT_COLOR), true),
        Err("Name the value first.".to_owned())
    );
    assert_eq!(
        save(&mut harness, &edit("gone", "Gone", KEPT_COLOR), true),
        Err("Example App no longer has that value.".to_owned())
    );
    // An edit is read back as the runtime will read it, like a new value.
    let unread = edit(
        "icon_visible",
        "Icon visible",
        r#"{"kind":"kept","check":{"kind":"number","reference":"gone","whole":false}}"#,
    );
    assert_eq!(
        save(&mut harness, &unread, true),
        Err(
            "AppyTappy could not read this value as saved: its reference gone is not saved."
                .to_owned()
        )
    );
    let mut elsewhere = edit("icon_visible", "Icon visible", KEPT_COLOR);
    "gone".clone_into(&mut elsewhere.app);
    assert_eq!(
        save(&mut harness, &elsewhere, true),
        Err("That application is no longer saved.".to_owned())
    );
    assert_eq!(folder_files(&harness), files, "nothing was written");

    assert!(save(&mut harness, &drawn(0), true).is_ok());
    let screen = saved_file(&harness).values.swap_remove(0).screen;
    assert_eq!(
        (screen.window_size, screen.region),
        (
            SizeDef {
                width: 4,
                height: 2
            },
            RegionDef {
                x: 2,
                y: 0,
                width: 2,
                height: 2,
            }
        )
    );
    assert_eq!(
        screen.check,
        CheckDef::Color {
            color: ColorDef {
                red: 255,
                green: 0,
                blue: 0,
            },
            tolerance: 10,
        }
    );
    assert_eq!(
        names(&folder_files(&harness)),
        ["app.json"],
        "the pictures no check names went"
    );
    std::fs::remove_dir_all(root).expect("clean up");
}

#[test]
fn a_state_keeps_its_saved_picture_only_in_its_saved_place() {
    let (mut harness, _handler, root) = with_saved_states("edit-kept");
    let files = folder_files(&harness);
    let refused = "A state keeps its saved picture only while the value keeps the same states in \
                   the same order. Take each state again from a screenshot.";
    let swapped = edit(
        "icon_visible",
        "Icon visible",
        r#"{"kind":"kept","check":{"kind":"state","states":[
            {"name":"Hidden","image":"icon_visible-2.png"},
            {"name":"Shown","image":"icon_visible-1.png"}],"tolerance":0}}"#,
    );
    assert_eq!(save(&mut harness, &swapped, true), Err(refused.to_owned()));
    assert_eq!(folder_files(&harness), files);
    // Another kind of check on the same region needs no picture.
    let color = edit("icon_visible", "Icon visible", KEPT_COLOR);
    assert!(save(&mut harness, &color, true).is_ok());
    // It has no saved picture to keep any more.
    let back = edit(
        "icon_visible",
        "Icon visible",
        &kept_states(["Shown", "Hidden"], 0),
    );
    assert_eq!(save(&mut harness, &back, true), Err(refused.to_owned()));
    std::fs::remove_dir_all(root).expect("clean up");
}

#[test]
fn trying_an_edit_reads_the_live_frame_as_the_runtime_would_and_saves_nothing() {
    let (mut harness, mut handler, root) = with_saved_states("edit-try");
    let files = folder_files(&harness);
    harness.forget_reloads();
    let read = |harness: &mut Harness| harness.ask(|owner| owner.reading());
    let unchanged = EditPreview {
        unread: None,
        dropped_fixes: Vec::new(),
        refused: None,
    };
    // At the size it was drawn at, the region as edited is read.
    harness.deliver(&mut handler, Ok(&frame_image()));
    let moved = edit(
        "icon_visible",
        "Icon visible",
        r#"{"kind":"drawn","screenshot":0,"check":{"kind":"color",
            "region":{"x":2,"y":0,"width":2,"height":2},
            "color":{"red":255,"green":0,"blue":0},"tolerance":10}}"#,
    );
    assert_eq!(
        harness.ask(|owner| owner.try_edit(&moved)),
        Ok(unchanged.clone())
    );
    assert_eq!(
        read(&mut harness),
        Ok(Some(Reading::Flag(false))),
        "the right half of the frame is gray"
    );
    // At another size, it is read where the runtime reads it there: scaled
    // while the window changes size often.
    harness.deliver(&mut handler, Ok(&larger_image()));
    let kept = edit(
        "icon_visible",
        "Icon visible",
        &kept_states(["Seen", "Hidden"], 0),
    );
    assert_eq!(harness.ask(|owner| owner.try_edit(&kept)), Ok(unchanged));
    assert_eq!(
        read(&mut harness),
        Ok(Some(Reading::State("Seen".to_owned()))),
        "the whole larger frame is red"
    );
    set_size_changes(&mut harness, SizeChangesDef::Rarely);
    harness.forget_reloads();
    assert_eq!(
        harness.ask(|owner| owner.try_edit(&kept)),
        Ok(EditPreview {
            unread: Some(
                "Not read at 8 × 4, the window's size now: it is read only at the window sizes \
                 it was drawn at."
                    .to_owned()
            ),
            dropped_fixes: Vec::new(),
            refused: None,
        })
    );
    assert_eq!(read(&mut harness), Ok(None), "the check before it is gone");
    assert_eq!(
        harness.ask(|owner| owner.try_edit(&edit("icon_visible", "", KEPT_COLOR))),
        Err("Name the value first.".to_owned())
    );
    // Only the application's own save, above, changed its file.
    let pictures = |mut files: BTreeMap<String, Vec<u8>>| {
        files.remove("app.json");
        files
    };
    assert_eq!(
        pictures(folder_files(&harness)),
        pictures(files),
        "trying writes no picture"
    );
    assert_eq!(saved_file(&harness).values[0].name, "Icon visible");
    assert_eq!(harness.reloads(), 0, "and tells no one");
    std::fs::remove_dir_all(root).expect("clean up");
}

#[test]
fn before_a_frame_arrives_an_edit_is_tried_at_the_size_it_was_drawn_at() {
    let (mut harness, root) = with_saved_bar("edit-try-unseen");
    // Another capture, such as after Change window, has shown no frame yet.
    drop(harness.pick());
    set_size_changes(&mut harness, SizeChangesDef::Rarely);
    assert_eq!(
        harness
            .ask(|owner| owner.try_edit(&edit("progress_bar", "Progress bar", KEPT_BAR)))
            .map(|preview| preview.unread),
        Ok(None)
    );
    std::fs::remove_dir_all(root).expect("clean up");
}

#[test]
fn without_a_store_an_edit_says_why() {
    let mut harness = harness(false);
    let spec = edit("progress_bar", "Progress bar", KEPT_BAR);
    assert_eq!(
        harness.ask(|owner| owner.try_edit(&spec)),
        Err(NO_STORE.to_owned())
    );
    assert_eq!(save(&mut harness, &spec, false), Err(NO_STORE.to_owned()));
}
