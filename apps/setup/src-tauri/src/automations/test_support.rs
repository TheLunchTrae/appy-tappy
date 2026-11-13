//! Saved stores and automations the automations' tests share.

use std::path::{Path, PathBuf};

use appytappy_config::app_file::{
    AppFile, CheckDef, ColorDef, FORMAT, OriginDef, RegionDef, ScreenDef, SizeChangesDef, SizeDef,
    ValueDef, WindowDef,
};

use appytappy_config::action::MediaCommand;
use appytappy_config::anti_cheat::AntiCheatClass;
use appytappy_config::automation_file::{
    ActionDef, AutomationDef, ClickAtDef, ClickButtonDef, ClickDeliveryDef, PointDef, TargetDef,
    WhenDef,
};
use appytappy_config::rule::Comparison;
use appytappy_config::window_match::WindowFacts;
use appytappy_sources::screen::{Image, Size};

use crate::stores::{self, Stores};

/// Fresh stores for one test, holding the application `example` with a
/// yes-or-no value `icon_visible` and a number value `health`.
pub(super) fn stores(name: &str) -> (Stores, PathBuf) {
    let (stores, folder) = stores::scratch(&format!("automations-{name}"));
    let white = ColorDef {
        red: 255,
        green: 255,
        blue: 255,
    };
    let value = |id: &str, name: &str, check| ValueDef {
        id: id.to_owned(),
        name: name.to_owned(),
        origin: OriginDef::ByHand,
        screen: ScreenDef {
            window_size: SizeDef {
                width: 100,
                height: 50,
            },
            region: RegionDef {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
            check,
            pin: None,
            reading_area: false,
            fixes: Vec::new(),
        },
    };
    let app = AppFile {
        format: FORMAT,
        name: "Example App".to_owned(),
        window: WindowDef::exact(WindowFacts {
            title: "Example App",
            class: "ExampleWindow",
            executable: "Example.exe",
        }),
        size_changes: SizeChangesDef::Often,
        anti_cheat: AntiCheatClass::Unknown,
        overlay: None,
        checked: None,
        references: Vec::new(),
        values: vec![
            value(
                "icon_visible",
                "Icon visible",
                CheckDef::Color {
                    color: white,
                    tolerance: 10,
                },
            ),
            value(
                "health",
                "Health",
                CheckDef::BarFill {
                    filled: white,
                    empty: ColorDef {
                        red: 0,
                        green: 0,
                        blue: 0,
                    },
                },
            ),
        ],
    };
    stores.apps.save("example", &app, &[]).expect("saves");
    (stores, folder)
}

pub(super) fn draft(name: &str) -> AutomationDef {
    AutomationDef {
        id: String::new(),
        name: name.to_owned(),
        enabled: true,
        when: WhenDef {
            app: "example".to_owned(),
            value: "icon_visible".to_owned(),
            is: Comparison::Equals,
            target: TargetDef::Flag(true),
        },
        actions: vec![ActionDef::Media {
            command: MediaCommand::Pause,
            app: Some("Player.exe".to_owned()),
        }],
        undo: true,
        cooldown_ms: 0,
    }
}

/// The automations' file of the stores in `folder`.
pub(super) fn automations_file(folder: &Path) -> PathBuf {
    folder.join("AppyTappy").join("automations.json")
}

/// A draft that clicks on the click picture `image` in `example`, whose
/// anti-cheat is set to none so it can click.
pub(super) fn clicking(stores: &Stores, name: &str, image: &str) -> AutomationDef {
    let mut app = stores.app("example").expect("saved").file;
    app.anti_cheat = AntiCheatClass::None;
    stores.apps.save("example", &app, &[]).expect("saves");
    let mut clicking = draft(name);
    clicking.undo = false;
    clicking.actions = vec![ActionDef::Click {
        app: "example".to_owned(),
        button: ClickButtonDef::Left,
        delivery: ClickDeliveryDef::Cursor,
        return_focus: true,
        at: ClickAtDef::Picture {
            image: image.to_owned(),
            window_size: SizeDef {
                width: 100,
                height: 50,
            },
            search: RegionDef {
                x: 0,
                y: 0,
                width: 20,
                height: 20,
            },
            press: PointDef { x: 1, y: 1 },
            tolerance: 20,
        },
    }];
    clicking
}

/// A 4 by 4 picture's PNG bytes, every pixel `shade`.
pub(super) fn png(shade: u8) -> Vec<u8> {
    let size = Size {
        width: 4,
        height: 4,
    };
    Image::new(size, [shade, shade, shade, 255].repeat(16))
        .expect("fits")
        .encode_png()
        .expect("encodes")
}
