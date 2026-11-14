//! Whether an automation can run, and what running it needs.
//!
//! [`Runnable::new`] is the one answer for the runtime and the setup app. It
//! checks the automation against the file's rules with
//! [`AutomationDef::compile`], makes the search for each picture its clicks
//! look for with [`ClickSearch::new`], and builds the check of the value it
//! watches with [`saved::build`]. The runtime runs what it makes, and the
//! setup app's save and list ask it too, so a check added here holds in both.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::io;

use appytappy_config::action::Action;
use appytappy_config::app_file::AppFile;
use appytappy_config::automation_file::{AutomationDef, Compiled};
use appytappy_config::click::{Click, ClickAt, ClickPicture, InvalidClick};

use crate::screen::click::{ClickPictureError, ClickSearch};
use crate::screen::saved::{self, Readers, SavedError, SavedValue};

/// Each application's [`Readers`], by the application's ID.
///
/// One load of the saved files passes the same ones to every
/// [`Runnable::new`], so the automations that watch numbers of one
/// reference build its reader once.
pub type AppReaders<'apps> = BTreeMap<&'apps str, Readers<'apps>>;

/// An automation that can run, with everything running it needs.
#[derive(Debug)]
pub struct Runnable<'apps> {
    /// Its rule, the value it watches, and the applications its clicks
    /// click in.
    pub compiled: Compiled<'apps>,
    /// The check of the value it watches.
    pub check: SavedValue,
    /// The search for each picture its clicks look for, by the picture
    /// click, in the order of its clicks.
    pub pictures: Vec<(ClickPicture, ClickSearch)>,
}

impl<'apps> Runnable<'apps> {
    /// Checks `automation` against `apps`, the saved applications whose
    /// files read, by ID, and makes what running it needs. `readers` keeps
    /// each reference's reader from one automation to the next. `picture`
    /// returns the bytes of a picture in an application's folder, given the
    /// application's ID and the picture's file name, and `click_picture` the
    /// bytes of a click picture, given its file name.
    ///
    /// # Errors
    ///
    /// Returns the first [`Problem`]: a rule of the file it breaks, then a
    /// click picture that cannot be read or used, then a value check that
    /// cannot be built.
    pub fn new(
        automation: &AutomationDef,
        apps: &[(&'apps str, &'apps AppFile)],
        readers: &mut AppReaders<'apps>,
        mut picture: impl FnMut(&str, &str) -> io::Result<Vec<u8>>,
        mut click_picture: impl FnMut(&str) -> io::Result<Vec<u8>>,
    ) -> Result<Self, Problem> {
        let compiled = automation.compile(apps).map_err(Problem::Rule)?;
        let mut pictures = Vec::new();
        for (action, number) in compiled.rule.actions().iter().zip(1_usize..) {
            let Action::Click(Click {
                at: ClickAt::Picture(wanted),
                ..
            }) = action
            else {
                continue;
            };
            let image = || wanted.image.clone();
            let bytes = click_picture(&wanted.image).map_err(|error| Problem::UnreadPicture {
                action: number,
                image: image(),
                error,
            })?;
            let search = ClickSearch::new(wanted, &bytes).map_err(|error| Problem::Picture {
                action: number,
                image: image(),
                error,
            })?;
            pictures.push((wanted.clone(), search));
        }
        let &Compiled {
            app_id, app, value, ..
        } = &compiled;
        let readers = readers
            .entry(app_id)
            .or_insert_with(|| Readers::new(&app.references));
        let check = saved::build(&value.screen, readers, app.size_changes, |name| {
            picture(app_id, name)
        })
        .map_err(|error| Problem::Value {
            value: value.name.clone(),
            app: app.name.clone(),
            error,
        })?;
        Ok(Self {
            compiled,
            check,
            pictures,
        })
    }
}

/// Why an automation cannot run. Its [`fmt::Display`] is a clause for the
/// runtime's log, after "cannot run: ".
#[derive(Debug)]
pub enum Problem {
    /// It breaks a rule of the automations file, as
    /// [`AutomationDef::compile`] says in this clause.
    Rule(String),
    /// The picture that its click action `action`, counted from 1, looks
    /// for cannot be read from its file.
    UnreadPicture {
        /// The action's place among its actions, counted from 1.
        action: usize,
        /// The picture's file name.
        image: String,
        /// Why.
        error: io::Error,
    },
    /// The picture that its click action `action`, counted from 1, looks
    /// for is not a PNG file it can read or does not suit the click.
    Picture {
        /// The action's place among its actions, counted from 1.
        action: usize,
        /// The picture's file name.
        image: String,
        /// Why.
        error: ClickPictureError,
    },
    /// The check of the value it watches cannot be built.
    Value {
        /// The value's name.
        value: String,
        /// Its application's name.
        app: String,
        /// Why.
        error: SavedError,
    },
}

impl fmt::Display for Problem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rule(reason) => formatter.write_str(reason),
            Self::UnreadPicture { image, error, .. } => {
                write!(formatter, "its picture {image} cannot be read: {error}")
            },
            Self::Picture { image, error, .. } => match error {
                ClickPictureError::Read(error) => {
                    write!(formatter, "its picture {image} cannot be read: {error}")
                },
                ClickPictureError::Misfit(InvalidClick::PressOutside) => write!(
                    formatter,
                    "the place it presses lies outside its picture {image}"
                ),
                ClickPictureError::Misfit(InvalidClick::LargePicture) => write!(
                    formatter,
                    "its picture {image} is larger than the area it is looked for in"
                ),
                ClickPictureError::Misfit(misfit) => {
                    write!(formatter, "its picture {image} cannot be used: {misfit}")
                },
            },
            Self::Value { value, app, error } => write!(formatter, "{value} of {app}: {error}"),
        }
    }
}

impl Error for Problem {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Rule(_) => None,
            Self::UnreadPicture { error, .. } => Some(error),
            Self::Picture { error, .. } => Some(error),
            Self::Value { error, .. } => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::action::MediaCommand;
    use appytappy_config::anti_cheat::AntiCheatClass;
    use appytappy_config::app_file::{
        CheckDef, ColorDef, FORMAT as APP_FORMAT, FallbackDef, OriginDef, ReferenceDef, RegionDef,
        SampleDef, ScreenDef, SizeChangesDef, SizeDef, StateDef, ValueDef, WindowDef,
    };
    use appytappy_config::automation_file::{
        ActionDef, ClickAtDef, ClickButtonDef, ClickDeliveryDef, CornerDef, PointDef, TargetDef,
        WhenDef,
    };
    use appytappy_config::rule::Comparison;
    use appytappy_config::window_match::MustMatch;

    use super::*;
    use crate::screen::{Image, Size};

    fn app(name: &str, check: CheckDef) -> AppFile {
        AppFile {
            format: APP_FORMAT,
            name: name.to_owned(),
            window: WindowDef {
                title: "*".to_owned(),
                class: "*".to_owned(),
                executable: "Example.exe".to_owned(),
                must_match: MustMatch {
                    title: false,
                    class: false,
                },
                fallback: FallbackDef::None,
            },
            size_changes: SizeChangesDef::Often,
            anti_cheat: AntiCheatClass::None,
            overlay: None,
            checked: None,
            values: vec![ValueDef {
                id: "progress".to_owned(),
                name: "The progress".to_owned(),
                origin: OriginDef::ByHand,
                screen: ScreenDef {
                    window_size: SizeDef {
                        width: 800,
                        height: 600,
                    },
                    region: RegionDef {
                        x: 0,
                        y: 0,
                        width: 4,
                        height: 2,
                    },
                    check,
                    pin: None,
                    reading_area: false,
                    fixes: Vec::new(),
                },
            }],
            references: Vec::new(),
        }
    }

    fn bar() -> CheckDef {
        CheckDef::BarFill {
            filled: ColorDef {
                red: 255,
                green: 0,
                blue: 0,
            },
            empty: ColorDef {
                red: 0,
                green: 0,
                blue: 0,
            },
        }
    }

    /// A 4 by 2 picture of one gray.
    fn png() -> Vec<u8> {
        Image::new(
            Size {
                width: 4,
                height: 2,
            },
            vec![9; 32],
        )
        .expect("matching length")
        .encode_png()
        .expect("encodes")
    }

    fn click(at: ClickAtDef) -> ActionDef {
        ActionDef::Click {
            app: "target".to_owned(),
            button: ClickButtonDef::Left,
            delivery: ClickDeliveryDef::Cursor,
            return_focus: true,
            at,
        }
    }

    fn spot() -> ActionDef {
        click(ClickAtDef::Spot {
            corner: CornerDef::TopLeft,
            x: 1,
            y: 1,
        })
    }

    /// A click on the picture `image`, looked for in a `width` by 10 area
    /// and pressed `press_x` across.
    fn on_picture(image: &str, width: u32, press_x: u32) -> ActionDef {
        click(ClickAtDef::Picture {
            image: image.to_owned(),
            window_size: SizeDef {
                width: 800,
                height: 600,
            },
            search: RegionDef {
                x: 0,
                y: 0,
                width,
                height: 10,
            },
            press: PointDef { x: press_x, y: 0 },
            tolerance: 10,
        })
    }

    fn automation(actions: Vec<ActionDef>) -> AutomationDef {
        AutomationDef {
            id: "on".to_owned(),
            name: "On".to_owned(),
            enabled: true,
            when: WhenDef {
                app: "example".to_owned(),
                value: "progress".to_owned(),
                is: Comparison::AtLeast,
                target: TargetDef::Number(90.0),
            },
            actions,
            undo: false,
            cooldown_ms: 0,
        }
    }

    fn click_pictures(name: &str) -> io::Result<Vec<u8>> {
        match name {
            "button-1.png" => Ok(png()),
            "broken-1.png" => Ok(b"not a png".to_vec()),
            _ => Err(io::Error::from(io::ErrorKind::NotFound)),
        }
    }

    fn no_pictures(_app: &str, _name: &str) -> io::Result<Vec<u8>> {
        Err(io::Error::from(io::ErrorKind::NotFound))
    }

    #[test]
    fn a_runnable_automation_has_its_value_s_check_and_its_click_pictures() {
        let example = app("Example App", bar());
        let target = app("Target App", bar());
        let apps = [("example", &example), ("target", &target)];
        let picture = on_picture("button-1.png", 10, 0);
        let runnable = Runnable::new(
            &automation(vec![spot(), picture.clone()]),
            &apps,
            &mut AppReaders::new(),
            no_pictures,
            click_pictures,
        )
        .expect("runs");
        assert_eq!(runnable.compiled.app_id, "example");
        assert_eq!(
            runnable
                .compiled
                .targets
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            ["target", "target"]
        );
        let ActionDef::Click { at, .. } = &picture else {
            panic!("a click");
        };
        let Ok(ClickAt::Picture(wanted)) = at.to_at() else {
            panic!("a picture click");
        };
        let search = ClickSearch::new(&wanted, &png()).expect("fits");
        assert_eq!(runnable.pictures, [(wanted, search)]);
        let built = saved::build(
            &example.values[0].screen,
            &mut saved::Readers::new(&[]),
            SizeChangesDef::Often,
            |_| Err(io::Error::from(io::ErrorKind::NotFound)),
        )
        .expect("builds");
        assert_eq!(runnable.check, built);
    }

    #[test]
    fn the_value_s_pictures_come_from_its_application() {
        let state = CheckDef::State {
            states: vec![StateDef {
                name: "Shown".to_owned(),
                image: "icon.png".to_owned(),
            }],
            tolerance: 0,
        };
        let example = app("Example App", state);
        let mut watching = automation(vec![ActionDef::Media {
            command: MediaCommand::Pause,
            app: None,
        }]);
        watching.when.is = Comparison::Equals;
        watching.when.target = TargetDef::State("Shown".to_owned());
        let mut asked = Vec::new();
        let runnable = Runnable::new(
            &watching,
            &[("example", &example)],
            &mut AppReaders::new(),
            |app, name| {
                asked.push(format!("{app}/{name}"));
                Ok(png())
            },
            click_pictures,
        );
        assert!(runnable.is_ok(), "{runnable:?}");
        assert_eq!(asked, ["example/icon.png"]);
    }

    #[test]
    fn automations_of_one_load_build_a_reference_s_reader_once() {
        // One white column on black: the character `1`.
        let sample = Image::new(
            Size {
                width: 3,
                height: 4,
            },
            [[0; 4], [255; 4], [0; 4]].concat().repeat(4),
        )
        .expect("matching length")
        .encode_png()
        .expect("encodes");
        let mut example = app(
            "Example App",
            CheckDef::Number {
                reference: "counter".to_owned(),
                whole: true,
                characters: None,
            },
        );
        example.references = vec![ReferenceDef {
            id: "counter".to_owned(),
            name: "Counter".to_owned(),
            origin: OriginDef::ByHand,
            ink: ColorDef {
                red: 255,
                green: 255,
                blue: 255,
            },
            tolerance: 0,
            samples: vec![SampleDef {
                image: "ref-counter-1.png".to_owned(),
                text: "1".to_owned(),
            }],
            expects: None,
        }];
        let watching = automation(vec![ActionDef::Media {
            command: MediaCommand::Pause,
            app: None,
        }]);
        let mut asked = Vec::new();
        let mut readers = AppReaders::new();
        let checks = [(); 2].map(|()| {
            Runnable::new(
                &watching,
                &[("example", &example)],
                &mut readers,
                |app, name| {
                    asked.push(format!("{app}/{name}"));
                    Ok(sample.clone())
                },
                click_pictures,
            )
            .expect("runs")
            .check
        });
        assert_eq!(asked, ["example/ref-counter-1.png"]);
        assert_eq!(checks[0], checks[1]);
    }

    #[test]
    fn what_stops_an_automation_is_named_in_order() {
        let example = app("Example App", bar());
        let target = app("Target App", bar());
        let mut empty = example.clone();
        empty.values[0].screen.region.width = 0;
        let apps = [("example", &example), ("target", &target)];
        let empty_apps = [("example", &empty), ("target", &target)];
        let problem = |actions: Vec<ActionDef>, apps: &[(&str, &AppFile)]| {
            Runnable::new(
                &automation(actions),
                apps,
                &mut AppReaders::new(),
                no_pictures,
                click_pictures,
            )
            .map(drop)
            .map_err(|problem| problem.to_string())
        };
        for (actions, apps, expected) in [
            (Vec::new(), &apps[..], "add an action"),
            (
                vec![spot(), on_picture("gone-1.png", 10, 0)],
                &apps[..],
                "its picture gone-1.png cannot be read: ",
            ),
            (
                vec![on_picture("broken-1.png", 10, 0)],
                &apps[..],
                "its picture broken-1.png cannot be read: ",
            ),
            (
                vec![on_picture("button-1.png", 10, 4)],
                &apps[..],
                "the place it presses lies outside its picture button-1.png",
            ),
            (
                vec![on_picture("button-1.png", 3, 0)],
                &apps[..],
                "its picture button-1.png is larger than the area it is looked for in",
            ),
            (
                vec![on_picture("button-1.png", 10, 0)],
                &empty_apps[..],
                "The progress of Example App: ",
            ),
            (
                vec![on_picture("gone-1.png", 10, 0)],
                &empty_apps[..],
                "its picture gone-1.png cannot be read: ",
            ),
        ] {
            let found = problem(actions, apps).expect_err(expected);
            assert!(found.starts_with(expected), "{found:?} for {expected:?}");
        }
        let numbered = Runnable::new(
            &automation(vec![spot(), on_picture("broken-1.png", 10, 4)]),
            &apps,
            &mut AppReaders::new(),
            no_pictures,
            click_pictures,
        );
        let Err(Problem::Picture { action, image, .. }) = numbered else {
            panic!("an unusable picture: {numbered:?}");
        };
        assert_eq!((action, image.as_str()), (2, "broken-1.png"));
        let unread = Runnable::new(
            &automation(vec![spot(), spot(), on_picture("gone-1.png", 10, 0)]),
            &apps,
            &mut AppReaders::new(),
            no_pictures,
            click_pictures,
        );
        let Err(Problem::UnreadPicture { action, .. }) = unread else {
            panic!("a missing picture: {unread:?}");
        };
        assert_eq!(action, 3);
    }

    #[test]
    fn a_problem_keeps_its_cause() {
        let misfit = Problem::Picture {
            action: 1,
            image: "button-1.png".to_owned(),
            error: ClickPictureError::Misfit(InvalidClick::Area),
        };
        assert_eq!(
            misfit.to_string(),
            format!(
                "its picture button-1.png cannot be used: {}",
                InvalidClick::Area
            )
        );
        assert!(misfit.source().is_some());
        let unread = Problem::UnreadPicture {
            action: 1,
            image: "gone-1.png".to_owned(),
            error: io::Error::other("gone"),
        };
        assert_eq!(
            unread.source().map(ToString::to_string),
            Some("gone".to_owned())
        );
        assert!(Problem::Rule("add an action".to_owned()).source().is_none());
        let example = app("Example App", bar());
        let mut empty = example.clone();
        empty.values[0].screen.region.width = 0;
        let value = Runnable::new(
            &automation(vec![spot()]),
            &[("example", &empty), ("target", &example)],
            &mut AppReaders::new(),
            no_pictures,
            click_pictures,
        )
        .expect_err("an empty region");
        assert!(matches!(value, Problem::Value { .. }));
        assert!(value.source().is_some());
    }
}
