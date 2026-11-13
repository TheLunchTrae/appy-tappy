use std::collections::BTreeMap;
use std::path::PathBuf;

use appytappy_config::app_file::{
    AppFile, ColorDef, FixDef, OriginDef, PinDef, RegionDef, SizeChangesDef, StateDef, WindowDef,
};
use appytappy_config::automation_file::AutomationDef;
use appytappy_config::window_match::WindowFacts;
use appytappy_sources::screen::check::Check;
use appytappy_sources::screen::saved::Readers;
use appytappy_sources::screen::{FrameHandler, Image, Size};

use crate::applications::AppSpec;
use crate::owner::test_support::{
    BAR, Harness, NO_STORE, add_example_app, frame_image, harness, harness_with, scratch_store,
    spec, with_saved_bar,
};
use crate::owner::{FixSpec, ReferenceSaveSpec};
use crate::reading::Reading;

use super::*;

/// A frame of `size`, all red.
fn red_image(size: SizeDef) -> Image {
    let (width, height) = (size.width, size.height);
    let pixels = usize::try_from(width.checked_mul(height).expect("small")).expect("fits");
    Image::new(Size { width, height }, [0, 0, 255, 0].repeat(pixels)).expect("matching length")
}

/// An 8 by 4 frame, twice the size of [`frame_image`], all red.
fn larger_image() -> Image {
    red_image(LARGER)
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

const WIDER: SizeDef = SizeDef {
    width: 12,
    height: 6,
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
        reading_area: false,
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

/// Saves `spec`, with the user's word for losing the fixes at the window
/// sizes `drop_fixes`.
fn save(
    harness: &mut Harness,
    spec: &EditSpec,
    drop_fixes: &[SizeDef],
) -> Result<SavedValue, String> {
    harness.ask(|owner| owner.edit_value(spec, drop_fixes))
}

#[test]
fn a_value_says_whether_a_recording_of_looks_kept_a_screenshot_it_is_set_up_on() {
    let (mut harness, mut handler, root) = with_saved_states("edit-origin");
    let origin = |harness: &Harness, id: &str| {
        saved_file(harness)
            .values
            .into_iter()
            .find(|value| value.id == id)
            .expect("the value")
            .origin
    };
    assert_eq!(origin(&harness, "icon_visible"), OriginDef::ByHand);
    // A recording of the whole frame's looks keeps a red frame as
    // screenshot 3.
    let region = serde_json::from_str(r#"{"x":0,"y":0,"width":4,"height":2}"#).expect("JSON");
    harness
        .ask(|owner| owner.start_look_recording(region, 10, 0))
        .expect("records");
    harness.deliver(
        &mut handler,
        Ok(&red_image(SizeDef {
            width: 4,
            height: 2,
        })),
    );
    let recorded_state = r#"{"kind":"state","region":{"x":0,"y":0,"width":2,"height":2},
        "drawnOn":0,"states":[{"name":"Shown","screenshot":0},{"name":"Hidden","screenshot":3}],
        "tolerance":0}"#;
    harness
        .ask(|owner| owner.save(&spec("example", "Icon again", recorded_state)))
        .expect("saves");
    assert_eq!(
        origin(&harness, "icon_again"),
        OriginDef::Recording,
        "one of its states is taken from a recorded screenshot"
    );
    let mut on_recorded = spec("example", "Progress bar", BAR);
    on_recorded.screenshot = 3;
    harness
        .ask(|owner| owner.save(&on_recorded))
        .expect("saves");
    assert_eq!(
        origin(&harness, "progress_bar"),
        OriginDef::Recording,
        "its region is drawn on a recorded screenshot"
    );
    // An edit that keeps the region keeps how the value was made.
    save(
        &mut harness,
        &edit("progress_bar", "Progress", KEPT_GREEN_BAR),
        &[],
    )
    .expect("saves");
    assert_eq!(origin(&harness, "progress_bar"), OriginDef::Recording);
    // One that draws it again says how it is made now.
    let drawn_bar = r#"{"kind":"drawn","screenshot":0,"check":{"kind":"barFill",
        "region":{"x":0,"y":0,"width":4,"height":2},
        "filled":{"red":255,"green":0,"blue":0},"empty":{"red":40,"green":40,"blue":40}}}"#;
    save(
        &mut harness,
        &edit("progress_bar", "Progress", drawn_bar),
        &[],
    )
    .expect("saves");
    assert_eq!(origin(&harness, "progress_bar"), OriginDef::ByHand);
    let drawn_recorded = format!(r#"{{"kind":"drawn","screenshot":0,"check":{recorded_state}}}"#);
    save(
        &mut harness,
        &edit("icon_visible", "Icon visible", &drawn_recorded),
        &[],
    )
    .expect("saves");
    assert_eq!(origin(&harness, "icon_visible"), OriginDef::Recording);
    std::fs::remove_dir_all(root).expect("clean up");
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
        save(&mut harness, &renamed, &[]),
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
    assert!(save(&mut harness, &renamed, &[]).is_ok());
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
        save(&mut harness, &edit("icon_visible_2", "ICON", KEPT_BAR), &[]),
        Err("Example App already has a value named like \"ICON\". Pick another name.".to_owned())
    );
    assert_eq!(
        folder_files(&harness),
        files,
        "a refused edit changes nothing"
    );
    let own = edit("icon_visible_2", "icon VISIBLE", KEPT_BAR);
    assert!(save(&mut harness, &own, &[]).is_ok());
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
    assert_eq!(save(&mut harness, &renamed, &[]), Err(states.clone()));
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
        save(&mut harness, &color, &[]),
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
    assert!(save(&mut harness, &named, &[]).is_ok());
    // One state renamed stops only the automations that name it.
    let hidden = edit("icon_visible", "Status", &kept_states(["Shown", "Away"], 3));
    assert_eq!(
        save(&mut harness, &hidden, &[]),
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
        save(&mut harness, &color, &[]),
        Err(
            "Low uses Progress bar as a number, and this change makes it read yes or no. Change \
             or delete it first."
                .to_owned()
        )
    );
    // Another check that reads a number keeps it.
    let green = edit("progress_bar", "Progress bar", KEPT_GREEN_BAR);
    assert!(save(&mut harness, &green, &[]).is_ok());
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
    assert!(save(&mut harness, &renamed, &[]).is_ok());
    assert_eq!(fixes_of(&harness), fix);

    // Taken again under the names they have, they still are. The new
    // pictures take a number no saved picture has, and the old ones go.
    let moved = edit(
        "icon_visible",
        "Icon visible",
        &drawn_states(1, ["Seen", "Away"]),
    );
    assert_eq!(dropped(&mut harness, &moved), Ok(Vec::new()));
    assert!(save(&mut harness, &moved, &[]).is_ok());
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
        save(&mut harness, &other, &[]),
        Err(
            "This change leaves the fix of Icon visible at 8 × 4 unusable, so saving removes \
             it. Agree to exactly that in the editor to save."
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
    assert!(save(&mut harness, &color, &[]).is_err());
    assert_eq!(folder_files(&harness), files, "nothing was written");
    assert_eq!(harness.reloads(), 0, "and no one was told");

    assert!(save(&mut harness, &other, &[LARGER]).is_ok());
    assert_eq!(fixes_of(&harness), []);
    assert_eq!(
        names(&folder_files(&harness)),
        ["app.json", "icon_visible-3-1.png", "icon_visible-3-2.png"],
        "the fix's pictures went with it"
    );
    std::fs::remove_dir_all(root).expect("clean up");
}

#[test]
fn the_user_s_word_names_exactly_the_fixes_the_edit_removes() {
    let (mut harness, root) = with_saved_bar("edit-bar-fix");
    let mut handler = harness.pick();
    for (screenshot, size) in [LARGER, WIDER].into_iter().enumerate() {
        harness.deliver(&mut handler, Ok(&red_image(size)));
        assert_eq!(harness.ask(Owner::screenshot), Ok(screenshot));
        let fix = FixSpec {
            app: "example".to_owned(),
            value: "progress_bar".to_owned(),
            check: serde_json::from_str(
                r#"{"kind":"barFill","region":{"x":1,"y":1,"width":6,"height":2},
                "filled":{"red":0,"green":255,"blue":0},"empty":{"red":0,"green":0,"blue":0}}"#,
            )
            .expect("valid JSON"),
            screenshot,
        };
        harness.ask(|owner| owner.save_fix(&fix)).expect("saves");
    }
    let files = folder_files(&harness);
    // A bar's fixes suit a bar, so agreeing to lose one is agreeing to more
    // than the edit removes.
    let green = edit("progress_bar", "Progress", KEPT_GREEN_BAR);
    assert_eq!(
        save(&mut harness, &green, &[LARGER]),
        Err(
            "This change removes no fix of Progress, so there is none to agree to lose. Check \
             the change and save again."
                .to_owned()
        )
    );
    assert_eq!(folder_files(&harness), files, "nothing was written");
    assert!(save(&mut harness, &green, &[]).is_ok());
    assert_eq!(fixes_of(&harness).len(), 2);

    // Read as yes or no, it loses both, once the user agreed to exactly
    // those two.
    let files = folder_files(&harness);
    let color = edit("progress_bar", "Progress", KEPT_COLOR);
    let both = "This change leaves the fixes of Progress at 8 × 4 and 12 × 6 unusable, so saving \
                removes them. Agree to exactly that in the editor to save.";
    let other = SizeDef {
        width: 1,
        height: 1,
    };
    for agreed in [
        Vec::new(),
        vec![LARGER],
        vec![LARGER, LARGER],
        vec![LARGER, other],
        vec![LARGER, WIDER, other],
    ] {
        assert_eq!(
            save(&mut harness, &color, &agreed),
            Err(both.to_owned()),
            "{agreed:?}"
        );
    }
    assert_eq!(folder_files(&harness), files, "nothing was written");
    assert!(save(&mut harness, &color, &[WIDER, LARGER]).is_ok());
    assert_eq!(fixes_of(&harness), []);
    std::fs::remove_dir_all(root).expect("clean up");
}

/// A number check of `region`, as JSON, read with the reference `reference`
/// of `Example App`.
fn number(region: &str, reference: &str) -> String {
    format!(
        r#"{{"kind":"number","region":{region},
            "reference":{{"app":"example","id":"{reference}"}},"whole":true}}"#
    )
}

/// An edit's drawing that keeps the region and reads a number with the
/// reference `reference`.
fn kept_number(reference: &str, whole: bool) -> String {
    format!(
        r#"{{"kind":"kept","check":{{"kind":"number","reference":"{reference}","whole":{whole}}}}}"#
    )
}

/// The check the saved value reads with on a window of each of `sizes`, as
/// the runtime builds it.
fn checks_at(harness: &Harness, sizes: &[SizeDef]) -> Vec<Check> {
    let app = harness
        .owner
        .stores()
        .expect("stores")
        .app("example")
        .expect("saved");
    let value = read_back(
        &app.file.values[0].screen,
        &app,
        Readers::new(&app.file.references),
        &Vec::new(),
    )
    .expect("reads back");
    sizes
        .iter()
        .map(|size| {
            let at = value
                .at(Size {
                    width: size.width,
                    height: size.height,
                })
                .expect("reads there");
            value.check(&at).clone()
        })
        .collect()
}

#[test]
fn a_number_s_fixes_keep_the_reference_they_read_with_when_the_value_s_changes() {
    let (stores, root) = scratch_store("edit-number-fixes");
    let mut harness = harness_with(false, Ok(stores));
    add_example_app(&mut harness);
    let mut handler = harness.pick();
    let widest = SizeDef {
        width: 16,
        height: 8,
    };
    for (index, image) in [
        frame_image(),
        red_image(LARGER),
        red_image(WIDER),
        red_image(widest),
    ]
    .iter()
    .enumerate()
    {
        harness.deliver(&mut handler, Ok(image));
        assert_eq!(harness.ask(Owner::screenshot), Ok(index));
    }
    // Three references that read the red square of screenshot 0
    // differently.
    for (name, text) in [("Digits", "1"), ("Small", "7"), ("Tiny", "4")] {
        let reference = ReferenceSaveSpec {
            app: "example".to_owned(),
            reference_name: name.to_owned(),
            expects: String::new(),
            characters: serde_json::from_str(&format!(
                r#"{{"region":{{"x":0,"y":0,"width":2,"height":2}},
                    "screenshot":0,"reference":null,"ink":{{"red":255,"green":0,"blue":0}},
                    "tolerance":0,"labels":[{{"screenshot":0,"text":"{text}"}}]}}"#
            ))
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
    // One fix reads with the value's reference, and two with their own.
    for (screenshot, reference) in [(1, "digits"), (2, "small"), (3, "tiny")] {
        let fix = FixSpec {
            app: "example".to_owned(),
            value: "progress".to_owned(),
            check: serde_json::from_str(&number(
                r#"{"x":0,"y":0,"width":4,"height":4}"#,
                reference,
            ))
            .expect("valid JSON"),
            screenshot,
        };
        harness.ask(|owner| owner.save_fix(&fix)).expect("saves");
    }
    let references = |harness: &Harness| -> Vec<Option<String>> {
        fixes_of(harness)
            .into_iter()
            .map(|fix| fix.reference)
            .collect()
    };
    let own = [None, Some("small".to_owned()), Some("tiny".to_owned())];
    assert_eq!(references(&harness), own);
    let sizes = [LARGER, WIDER, widest];
    let before = checks_at(&harness, &sizes);
    assert_ne!(before[0], before[1], "the references read differently");
    assert_ne!(before[1], before[2], "the references read differently");

    // The value reads with the other reference, and each fix with the one
    // it read with: the fix that names a third reference is left alone.
    let small = edit("progress", "Progress", &kept_number("small", true));
    assert_eq!(
        harness.ask(|owner| owner.try_edit(&small)),
        Ok(EditPreview {
            unread: None,
            dropped_fixes: Vec::new(),
            refused: None,
        })
    );
    assert!(save(&mut harness, &small, &[]).is_ok());
    assert_eq!(
        references(&harness),
        [Some("digits".to_owned()), None, Some("tiny".to_owned())],
        "each fix names a reference only when it is not the value's"
    );
    assert_eq!(checks_at(&harness, &sizes), before);
    assert_eq!(
        harness.ask(|owner| owner.delete_reference("example", "digits")),
        Err("Progress reads with Digits. Change or remove it first.".to_owned()),
        "a fix still reads with it"
    );
    let digits = edit("progress", "Progress", &kept_number("digits", true));
    assert!(save(&mut harness, &digits, &[]).is_ok());
    assert_eq!(references(&harness), own);
    assert_eq!(checks_at(&harness, &sizes), before);

    // A fix has no setting of its own for the whole part, so it follows the
    // value's.
    let fraction = edit("progress", "Progress", &kept_number("digits", false));
    assert!(save(&mut harness, &fraction, &[]).is_ok());
    assert_eq!(references(&harness), own, "the same reference pins none");
    for check in checks_at(&harness, &sizes) {
        assert!(
            matches!(check, Check::Number { whole: false, .. }),
            "{check:?}"
        );
    }
    std::fs::remove_dir_all(root).expect("clean up");
}

#[test]
fn an_edit_sets_whether_the_region_is_a_reading_area_only_for_a_check_that_reads_one() {
    let (mut harness, root) = with_saved_bar("edit-reading-area");
    let area = |harness: &Harness| saved_file(harness).values[0].screen.reading_area;
    assert!(!area(&harness));
    let mut bar = edit("progress_bar", "Progress bar", KEPT_BAR);
    bar.reading_area = true;
    assert!(save(&mut harness, &bar, &[]).is_ok());
    assert!(area(&harness));
    // Saved again as it is, nothing is written.
    harness.forget_reloads();
    assert!(save(&mut harness, &bar, &[]).is_ok());
    assert_eq!(harness.reloads(), 0);
    // A color averages its whole region, so the edit that makes it one
    // cannot keep the reading area, tried or saved.
    let refused = "Only a number or a bar fill check reads a reading area. Turn off This content \
                   may move.";
    let mut color = edit("progress_bar", "Progress bar", KEPT_COLOR);
    color.reading_area = true;
    let files = folder_files(&harness);
    assert_eq!(
        harness.ask(|owner| owner.try_edit(&color)),
        Err(refused.to_owned())
    );
    assert_eq!(save(&mut harness, &color, &[]), Err(refused.to_owned()));
    assert_eq!(folder_files(&harness), files, "nothing was written");
    color.reading_area = false;
    assert!(save(&mut harness, &color, &[]).is_ok());
    assert!(!area(&harness));
    std::fs::remove_dir_all(root).expect("clean up");
}

#[test]
fn a_value_saved_without_a_pin_keeps_none_while_the_edit_names_the_guess() {
    let (mut harness, _handler, root) = with_saved_states("edit-pin");
    harness
        .ask(|owner| {
            let stores = owner.stores()?;
            let mut file = stores.app("example")?.file;
            file.values[0].screen.pin = None;
            stores
                .apps
                .save("example", &file, &[])
                .map_err(|error| error.to_string())
        })
        .expect("a value saved with no pin");
    let files = folder_files(&harness);
    harness.forget_reloads();
    // The region is in the left third and the middle third down.
    let mut same = edit(
        "icon_visible",
        "Icon visible",
        &kept_states(["Shown", "Hidden"], 0),
    );
    same.pin = PinDef::MiddleLeft;
    assert!(save(&mut harness, &same, &[]).is_ok());
    assert_eq!(folder_files(&harness), files, "nothing was written");
    assert_eq!(harness.reloads(), 0, "and no one was told");
    let pin = |harness: &Harness| saved_file(harness).values[0].screen.pin;
    // A side picked by hand is saved, and stays once it is the guess again.
    let mut picked = same.clone();
    picked.pin = PinDef::BottomRight;
    assert!(save(&mut harness, &picked, &[]).is_ok());
    assert_eq!(pin(&harness), Some(PinDef::BottomRight));
    assert!(save(&mut harness, &same, &[]).is_ok());
    assert_eq!(pin(&harness), Some(PinDef::MiddleLeft));
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
        save(&mut harness, &drawn(2), &[]),
        Err(
            "Icon visible was drawn at 4 × 2, and this screenshot is 8 × 4. Resize the window \
             to 4 × 2 to move its region, or fix it at this size instead."
                .to_owned()
        )
    );
    assert_eq!(
        save(&mut harness, &drawn(9), &[]),
        Err("Take a screenshot first.".to_owned())
    );
    assert_eq!(
        save(&mut harness, &edit("icon_visible", " ", KEPT_COLOR), &[]),
        Err("Name the value first.".to_owned())
    );
    assert_eq!(
        save(&mut harness, &edit("gone", "Gone", KEPT_COLOR), &[]),
        Err("Example App no longer has that value.".to_owned())
    );
    // An edit is read back as the runtime will read it, like a new value.
    let unread = edit(
        "icon_visible",
        "Icon visible",
        r#"{"kind":"kept","check":{"kind":"number","reference":"gone","whole":false}}"#,
    );
    assert_eq!(
        save(&mut harness, &unread, &[]),
        Err("Example App no longer has that reference.".to_owned())
    );
    let mut elsewhere = edit("icon_visible", "Icon visible", KEPT_COLOR);
    "gone".clone_into(&mut elsewhere.app);
    assert_eq!(
        save(&mut harness, &elsewhere, &[]),
        Err("That application is no longer saved.".to_owned())
    );
    assert_eq!(folder_files(&harness), files, "nothing was written");

    assert!(save(&mut harness, &drawn(0), &[]).is_ok());
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
    assert_eq!(save(&mut harness, &swapped, &[]), Err(refused.to_owned()));
    assert_eq!(folder_files(&harness), files);
    // Another kind of check on the same region needs no picture.
    let color = edit("icon_visible", "Icon visible", KEPT_COLOR);
    assert!(save(&mut harness, &color, &[]).is_ok());
    // It has no saved picture to keep any more.
    let back = edit(
        "icon_visible",
        "Icon visible",
        &kept_states(["Shown", "Hidden"], 0),
    );
    assert_eq!(save(&mut harness, &back, &[]), Err(refused.to_owned()));
    std::fs::remove_dir_all(root).expect("clean up");
}

#[test]
fn trying_an_edit_reads_the_live_frame_as_the_runtime_would_and_saves_nothing() {
    let (mut harness, mut handler, root) = with_saved_states("edit-try");
    let files = folder_files(&harness);
    harness.forget_reloads();
    let read = |harness: &mut Harness| harness.ask(Owner::reading);
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
    assert_eq!(
        harness.ask(|owner| owner.window_size()),
        Some(SizeDef {
            width: 4,
            height: 2
        })
    );
    // An edit that cannot be tried leaves no check from the one before it.
    assert_eq!(
        harness.ask(|owner| owner.try_edit(&edit("icon_visible", "", KEPT_COLOR))),
        Err("Name the value first.".to_owned())
    );
    assert_eq!(read(&mut harness), Ok(None));
    // At another size, it is read where the runtime reads it there: scaled
    // while the window changes size often.
    harness.deliver(&mut handler, Ok(&larger_image()));
    assert_eq!(harness.ask(|owner| owner.window_size()), Some(LARGER));
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
    assert_eq!(
        read(&mut harness),
        Err(
            "Not read at 8 × 4, the window's size now: it is read only at the window sizes it \
             was drawn at."
                .to_owned()
        ),
        "each read says why"
    );
    // The edit stays the live check, so a frame of a size it is read at
    // reads with no new try, once that size settled.
    harness.deliver(&mut handler, Ok(&frame_image()));
    assert_eq!(read(&mut harness), Ok(None), "not settled yet");
    harness.wait(appytappy_sources::screen::settle::SETTLE);
    assert!(
        matches!(read(&mut harness), Ok(Some(Reading::State(_)))),
        "read at the size it was drawn at"
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
    assert_eq!(harness.ask(|owner| owner.window_size()), None);
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
    assert_eq!(save(&mut harness, &spec, &[]), Err(NO_STORE.to_owned()));
}
