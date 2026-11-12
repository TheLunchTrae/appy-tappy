//! What the value editor's preview outlines while debug details show: the
//! rectangles the runtime copies out of each frame of a window of one size,
//! and where each saved value reads in that frame.

use std::collections::BTreeSet;
use std::io;

use appytappy_config::app_file::{AppFile, RegionDef};
use appytappy_config::automation_file::AutomationsFile;

use appytappy_sources::screen::saved;
use appytappy_sources::screen::{Layout, Region, Size};
use serde::Serialize;

/// The capture of one application at one window size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaptureOutline {
    /// The rectangles the runtime copies out of each frame, in the frame's
    /// pixels. Empty when no enabled automation watches a value that reads
    /// at this size.
    pub(crate) copies: Vec<RegionDef>,
    /// Each saved value, in the application's order.
    pub(crate) values: Vec<OutlinedValue>,
}

/// One saved value in a [`CaptureOutline`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct OutlinedValue {
    /// The value's ID.
    pub(crate) id: String,
    /// The value's name.
    pub(crate) name: String,
    /// Whether an enabled automation watches it, so the runtime copies it.
    pub(crate) watched: bool,
    /// Where it reads in the frame, or `None` where it does not read.
    pub(crate) region: Option<RegionDef>,
    /// Why it does not read at this size, if it does not.
    pub(crate) problem: Option<String>,
}

/// The outline of `file`, saved as the application `app`, on a window of
/// `size`, with `automations` saying which values the runtime watches.
/// `picture` reads a picture in the application's folder, as the runtime
/// reads it to build a value's check.
pub(crate) fn outline(
    app: &str,
    file: &AppFile,
    automations: &AutomationsFile,
    size: Size,
    picture: impl FnMut(&str) -> io::Result<Vec<u8>> + Clone,
) -> CaptureOutline {
    let watched: BTreeSet<&str> = automations
        .automations
        .iter()
        .filter(|automation| automation.enabled && automation.when.app == app)
        .map(|automation| automation.when.value.as_str())
        .collect();
    let placed: Vec<(bool, Result<Region, String>)> = file
        .values
        .iter()
        .map(|value| {
            let region = saved::build(
                &value.screen,
                &file.references,
                file.size_changes,
                picture.clone(),
            )
            .map_err(|error| format!("It cannot be read: {error}."))
            .and_then(|check| {
                check
                    .at(size)
                    .map(|at| at.region)
                    .map_err(|unread| format!("It does not read here: {unread}."))
            });
            (watched.contains(value.id.as_str()), region)
        })
        .collect();
    let copies = Layout::new(
        placed
            .iter()
            .map(|(watched, region)| region.as_ref().ok().filter(|_| *watched).copied()),
    )
    .map_or_else(Vec::new, |layout| {
        layout
            .copies()
            .iter()
            .map(|copy| region_def(copy.from))
            .collect()
    });
    let values = file
        .values
        .iter()
        .zip(placed)
        .map(|(value, (watched, region))| OutlinedValue {
            id: value.id.clone(),
            name: value.name.clone(),
            watched,
            region: region.as_ref().ok().copied().map(region_def),
            problem: region.err(),
        })
        .collect();
    CaptureOutline { copies, values }
}

/// `region` as the page reads it.
const fn region_def(region: Region) -> RegionDef {
    let size = region.size();
    RegionDef {
        x: region.x(),
        y: region.y(),
        width: size.width,
        height: size.height,
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::app_file::{
        AntiCheatDef, CheckDef, ColorDef, FORMAT, ScreenDef, SizeChangesDef, SizeDef, ValueDef,
        WindowDef,
    };
    use appytappy_config::automation_file::{
        ActionDef, AutomationDef, ComparisonDef, MediaCommandDef, TargetDef, WhenDef,
    };
    use appytappy_config::window_match::WindowFacts;

    use super::*;

    const WINDOW: Size = Size {
        width: 100,
        height: 50,
    };

    fn region(x: u32, y: u32, width: u32, height: u32) -> RegionDef {
        RegionDef {
            x,
            y,
            width,
            height,
        }
    }

    fn value(id: &str, at: RegionDef) -> ValueDef {
        ValueDef {
            id: id.to_owned(),
            name: id.to_uppercase(),
            screen: ScreenDef {
                window_size: SizeDef {
                    width: WINDOW.width,
                    height: WINDOW.height,
                },
                region: at,
                check: CheckDef::Color {
                    color: ColorDef {
                        red: 255,
                        green: 255,
                        blue: 255,
                    },
                    tolerance: 10,
                },
                pin: None,
                fixes: Vec::new(),
            },
        }
    }

    fn app(size_changes: SizeChangesDef, values: Vec<ValueDef>) -> AppFile {
        AppFile {
            format: FORMAT,
            name: "Example App".to_owned(),
            window: WindowDef::exact(WindowFacts {
                title: "Example App",
                class: "ExampleWindow",
                executable: "Example.exe",
            }),
            size_changes,
            anti_cheat: AntiCheatDef::Unknown,
            overlay: None,
            checked: None,
            references: Vec::new(),
            values,
        }
    }

    fn watching(app: &str, value: &str, enabled: bool) -> AutomationDef {
        AutomationDef {
            id: format!("{app}-{value}"),
            name: format!("Watch {value}"),
            enabled,
            when: WhenDef {
                app: app.to_owned(),
                value: value.to_owned(),
                is: ComparisonDef::Equals,
                target: TargetDef::Flag(true),
            },
            actions: vec![ActionDef::Media {
                command: MediaCommandDef::Pause,
                app: None,
            }],
            undo: false,
            cooldown_ms: 0,
        }
    }

    fn automations(watching: Vec<AutomationDef>) -> AutomationsFile {
        AutomationsFile {
            automations: watching,
            ..AutomationsFile::default()
        }
    }

    fn no_pictures(_name: &str) -> io::Result<Vec<u8>> {
        Err(io::Error::from(io::ErrorKind::NotFound))
    }

    /// Each value's region and whether it is watched.
    fn regions(outline: &CaptureOutline) -> Vec<(bool, Option<RegionDef>)> {
        outline
            .values
            .iter()
            .map(|value| (value.watched, value.region))
            .collect()
    }

    #[test]
    fn watched_values_far_apart_are_copied_one_by_one() {
        let file = app(
            SizeChangesDef::Rarely,
            vec![
                value("left", region(0, 0, 4, 4)),
                value("right", region(90, 40, 4, 4)),
                value("middle", region(40, 20, 4, 4)),
            ],
        );
        let outline = outline(
            "example",
            &file,
            &automations(vec![
                watching("example", "left", true),
                watching("example", "right", true),
            ]),
            WINDOW,
            no_pictures,
        );
        assert_eq!(outline.copies, [region(0, 0, 4, 4), region(90, 40, 4, 4)]);
        assert_eq!(
            regions(&outline),
            [
                (true, Some(region(0, 0, 4, 4))),
                (true, Some(region(90, 40, 4, 4))),
                (false, Some(region(40, 20, 4, 4))),
            ]
        );
        assert_eq!(outline.values[1].id, "right");
        assert_eq!(outline.values[1].name, "RIGHT");
        assert!(outline.values.iter().all(|value| value.problem.is_none()));
    }

    #[test]
    fn overlapping_watched_values_are_copied_as_one_rectangle() {
        let file = app(
            SizeChangesDef::Rarely,
            vec![
                value("a", region(10, 10, 20, 20)),
                value("b", region(12, 12, 18, 18)),
            ],
        );
        let outline = outline(
            "example",
            &file,
            &automations(vec![
                watching("example", "a", true),
                watching("example", "b", true),
            ]),
            WINDOW,
            no_pictures,
        );
        assert_eq!(outline.copies, [region(10, 10, 20, 20)]);
    }

    #[test]
    fn only_enabled_automations_of_this_application_count() {
        let file = app(SizeChangesDef::Rarely, vec![value("a", region(0, 0, 4, 4))]);
        let outline = outline(
            "example",
            &file,
            &automations(vec![
                watching("example", "a", false),
                watching("other", "a", true),
            ]),
            WINDOW,
            no_pictures,
        );
        assert_eq!(outline.copies, []);
        assert_eq!(regions(&outline), [(false, Some(region(0, 0, 4, 4)))]);
    }

    #[test]
    fn a_value_reads_where_its_window_size_puts_it() {
        let values = vec![value("a", region(10, 10, 4, 4))];
        let watched = automations(vec![watching("example", "a", true)]);
        let doubled = Size {
            width: 200,
            height: 100,
        };
        let scaled = outline(
            "example",
            &app(SizeChangesDef::Often, values.clone()),
            &watched,
            doubled,
            no_pictures,
        );
        assert_eq!(scaled.copies, [region(20, 20, 8, 8)]);
        assert_eq!(regions(&scaled), [(true, Some(region(20, 20, 8, 8)))]);
        let unread = outline(
            "example",
            &app(SizeChangesDef::Rarely, values),
            &watched,
            doubled,
            no_pictures,
        );
        assert_eq!(unread.copies, []);
        assert_eq!(regions(&unread), [(true, None)]);
        assert_eq!(
            unread.values[0].problem.as_deref(),
            Some("It does not read here: it is read only at the window sizes it was drawn at.")
        );
    }

    #[test]
    fn a_value_that_cannot_be_built_says_why() {
        let file = app(
            SizeChangesDef::Rarely,
            vec![value("a", region(98, 0, 4, 4))],
        );
        let outline = outline(
            "example",
            &file,
            &automations(vec![watching("example", "a", true)]),
            WINDOW,
            no_pictures,
        );
        assert_eq!(outline.copies, []);
        assert_eq!(regions(&outline), [(true, None)]);
        assert!(
            outline.values[0]
                .problem
                .as_deref()
                .is_some_and(|problem| problem.starts_with("It cannot be read: ")),
            "{:?}",
            outline.values[0].problem
        );
    }
}
