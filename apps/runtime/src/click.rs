//! Placing an automation's clicks on the screen.
//!
//! A click names the saved application it clicks in. The plan keeps what
//! placing it needs, [`ClickTargets`]: that application's name and window
//! match, and the search for each picture a click looks for. When a click
//! runs, [`place`] finds them and has `appytappy_sources::screen::place`
//! turn the click's spot, or where its picture is found in one frame of
//! its area, into a place on the screen in the application's window. A
//! click whose window is missing, minimized, or another size than its
//! picture was taken at, or whose picture is not there, is not placed, and
//! the log says why.

use std::collections::BTreeMap;

use appytappy_actions::PlacedClick;
use appytappy_config::app_file::AppFile;
use appytappy_config::click::{Click, ClickAt, ClickPicture};
use appytappy_config::window_match::WindowMatch;
use appytappy_sources::screen::Capture;
use appytappy_sources::screen::click::ClickSearch;
use appytappy_sources::screen::place as on_screen;
use appytappy_sources::system::System;

/// What placing an application's clicks needs.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ClickTargets {
    /// Each application clicked in, by ID.
    apps: BTreeMap<String, Target>,
    /// The search for each click picture, by the whole picture click, since
    /// two clicks may look for one picture with a different area, press, or
    /// tolerance.
    pictures: BTreeMap<ClickPicture, ClickSearch>,
}

/// An application a click clicks in.
#[derive(Debug, PartialEq)]
struct Target {
    /// Its name, for the log.
    name: String,
    /// How to find its window.
    window: WindowMatch,
}

impl ClickTargets {
    /// Adds `apps`, the applications clicks click in, by ID, and
    /// `pictures`, the search for each picture they look for, as
    /// `appytappy_sources::automation::Runnable` makes them.
    pub(crate) fn add<'apps>(
        &mut self,
        apps: impl IntoIterator<Item = &'apps (&'apps str, &'apps AppFile)>,
        pictures: impl IntoIterator<Item = (ClickPicture, ClickSearch)>,
    ) {
        for (id, file) in apps {
            self.apps.insert(
                (*id).to_owned(),
                Target {
                    name: file.name.clone(),
                    window: file.window.to_match(),
                },
            );
        }
        self.pictures.extend(pictures);
    }
}

/// Where `click` lands on the screen now, from `targets`, or why it does
/// not land anywhere.
///
/// # Errors
///
/// Returns why, as a sentence for the log, such as a target window that
/// is not open or a picture that is not found.
pub(crate) fn place<S: System + Capture>(
    screen: &S,
    click: &Click,
    targets: &ClickTargets,
) -> Result<PlacedClick, String> {
    let target = targets
        .apps
        .get(&click.app)
        .ok_or_else(|| format!("the application {} it clicks in was not loaded", click.app))?;
    let looked = match &click.at {
        ClickAt::Spot(spot) => on_screen::Target::Spot(*spot),
        ClickAt::Picture(wanted) => on_screen::Target::Picture(
            targets
                .pictures
                .get(wanted)
                .ok_or_else(|| format!("its picture {} was not loaded", wanted.image))?,
        ),
    };
    let placed = on_screen::place(screen, &target.name, &target.window, looked)?;
    Ok(PlacedClick {
        window: placed.window,
        process: placed.process,
        app: target.name.clone(),
        at: placed.at,
        button: click.button,
        delivery: click.delivery,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use appytappy_config::app_file::{
        AntiCheatDef, BuildDef, FORMAT, FallbackDef, MustMatchDef, SizeChangesDef, WindowDef,
    };
    use appytappy_config::click::{Area, ClickButton, ClickDelivery, Corner, Spot};
    use appytappy_config::process::ProcessId;
    use appytappy_sources::screen::{
        CaptureSession, Crop, Frame, FrameHandler, Image, Region, Size,
    };
    use appytappy_sources::system::{Bounds, ProcessEntry, WindowEntry, WindowPlace};
    use std::io;
    use std::time::{Duration, Instant};

    use super::*;

    fn app_file(name: &str) -> AppFile {
        AppFile {
            format: FORMAT,
            name: name.to_owned(),
            window: WindowDef {
                title: "*".to_owned(),
                class: "*".to_owned(),
                executable: "Target.exe".to_owned(),
                must_match: MustMatchDef {
                    title: false,
                    class: false,
                },
                fallback: FallbackDef::None,
            },
            anti_cheat: AntiCheatDef::None,
            overlay: None,
            checked: None,
            values: Vec::new(),
            references: Vec::new(),
            size_changes: SizeChangesDef::Often,
        }
    }

    /// A 200x100 window of gray 10 with a 4x2 patch of grays 50 to 120 at
    /// (150, 60).
    fn window_image() -> Image {
        let mut bgra = Vec::new();
        for y in 0..100_u32 {
            for x in 0..200_u32 {
                let gray = patch_gray(x, y).unwrap_or(10);
                bgra.extend([gray, gray, gray, u8::MAX]);
            }
        }
        Image::new(
            Size {
                width: 200,
                height: 100,
            },
            bgra,
        )
        .unwrap()
    }

    fn patch_gray(x: u32, y: u32) -> Option<u8> {
        let across = x.checked_sub(150).filter(|&across| across < 4)?;
        let down = y.checked_sub(60).filter(|&down| down < 2)?;
        let step = down.checked_mul(4)?.checked_add(across)?;
        u8::try_from(step.checked_mul(10)?.checked_add(50)?).ok()
    }

    fn patch_png() -> Vec<u8> {
        window_image()
            .crop(Region::new(150, 60, 4, 2).unwrap())
            .unwrap()
            .encode_png()
            .unwrap()
    }

    /// The target's window, handle 9 of process 8, at (1000, 500) on the
    /// screen.
    pub(crate) struct Screen {
        shown: bool,
        bounds: Option<Bounds>,
        frame: Image,
    }

    impl Screen {
        pub(crate) fn new() -> Self {
            Self {
                shown: true,
                bounds: Some(Bounds {
                    left: 1000,
                    top: 500,
                    right: 1200,
                    bottom: 600,
                }),
                frame: window_image(),
            }
        }
    }

    impl System for Screen {
        fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
            Ok(vec![ProcessEntry {
                id: ProcessId::new(8),
                parent: ProcessId::new(1),
                executable: "Target.exe".to_owned(),
                threads: 1,
            }])
        }

        fn windows(&self) -> io::Result<Vec<WindowEntry>> {
            Ok(vec![WindowEntry {
                handle: 9,
                process: ProcessId::new(8),
                title: "Target".to_owned(),
                class: "Window".to_owned(),
                visible: true,
                bounds: Bounds {
                    left: 993,
                    top: 500,
                    right: 1207,
                    bottom: 607,
                },
            }])
        }

        fn window_place(&self, _window: u64) -> io::Result<Option<WindowPlace>> {
            Ok(self.bounds.map(|bounds| WindowPlace {
                bounds,
                shown: self.shown,
                front: false,
            }))
        }

        fn executable_build(&self, _process: ProcessId) -> io::Result<BuildDef> {
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }
    }

    pub(crate) struct Closed;

    impl CaptureSession for Closed {
        fn close(self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Capture for Screen {
        type Session = Closed;

        fn start(
            &self,
            _window: u64,
            crop: Crop,
            _throttle: Duration,
            mut handler: FrameHandler,
        ) -> io::Result<Closed> {
            let Crop::Layout(layout) = crop else {
                return Err(io::Error::other("a grab crops its area"));
            };
            let region = layout.copies().first().map(|copy| copy.from);
            let image = region
                .and_then(|region| self.frame.crop(region))
                .ok_or_else(|| io::Error::other("outside the window"))?;
            handler(Ok(Frame {
                window_size: self.frame.size(),
                image: &image,
                received: Instant::now(),
                delay: Duration::ZERO,
            }));
            Ok(Closed)
        }
    }

    fn spot(corner: Corner, x: u32, y: u32) -> Click {
        Click::new(
            "target",
            ClickButton::Right,
            ClickDelivery::Cursor {
                return_focus: false,
            },
            ClickAt::Spot(Spot::new(corner, x, y).unwrap()),
        )
        .unwrap()
    }

    fn picture(window: (u32, u32), search: Area, press: (u32, u32)) -> Click {
        Click::new(
            "target",
            ClickButton::Double,
            ClickDelivery::Message,
            ClickAt::Picture(ClickPicture::new("patch-1.png", window, search, press, 0).unwrap()),
        )
        .unwrap()
    }

    fn area(x: u32, y: u32, width: u32, height: u32) -> Area {
        Area {
            x,
            y,
            width,
            height,
        }
    }

    /// What placing `clicks` in `Target App` needs, each picture being
    /// [`patch_png`].
    pub(crate) fn targets(clicks: &[Click]) -> ClickTargets {
        let file = app_file("Target App");
        let pictures = clicks.iter().filter_map(|click| match &click.at {
            ClickAt::Picture(wanted) => Some((
                wanted.clone(),
                ClickSearch::new(wanted, &patch_png()).unwrap(),
            )),
            ClickAt::Spot(_) => None,
        });
        let mut targets = ClickTargets::default();
        targets.add(&[("target", &file)], pictures);
        targets
    }

    #[test]
    fn a_click_is_placed_in_its_application_s_window_and_keeps_how_it_clicks() {
        let click = spot(Corner::BottomRight, 10, 5);
        let targets = targets(std::slice::from_ref(&click));
        let placed = place(&Screen::new(), &click, &targets).unwrap();
        assert_eq!(
            placed,
            PlacedClick {
                window: 9,
                process: ProcessId::new(8),
                app: "Target App".to_owned(),
                at: (1189, 594),
                button: ClickButton::Right,
                delivery: ClickDelivery::Cursor {
                    return_focus: false,
                },
            }
        );
    }

    #[test]
    fn a_picture_lands_where_it_is_found_in_its_area() {
        let click = picture((200, 100), area(100, 40, 100, 60), (3, 1));
        let targets = targets(std::slice::from_ref(&click));
        let placed = place(&Screen::new(), &click, &targets).unwrap();
        assert_eq!(placed.at, (1153, 561));
        assert_eq!(placed.button, ClickButton::Double);
        assert_eq!(placed.delivery, ClickDelivery::Message);
    }

    #[test]
    fn a_click_is_not_placed_without_what_the_plan_loaded_for_it() {
        let click = picture((200, 100), area(100, 40, 100, 60), (0, 0));
        let targets = targets(std::slice::from_ref(&click));
        let unloaded = picture((200, 100), area(0, 0, 100, 60), (0, 0));
        assert_eq!(
            place(&Screen::new(), &unloaded, &targets),
            Err("its picture patch-1.png was not loaded".to_owned())
        );
        let other = Click {
            app: "other".to_owned(),
            ..click
        };
        assert_eq!(
            place(&Screen::new(), &other, &targets),
            Err("the application other it clicks in was not loaded".to_owned())
        );
    }

    #[test]
    fn clicks_sharing_a_picture_each_look_in_their_own_area() {
        let near = picture((200, 100), area(100, 40, 100, 60), (3, 1));
        let far = picture((200, 100), area(0, 0, 100, 60), (3, 1));
        let targets = targets(&[near.clone(), far.clone()]);
        assert_eq!(
            place(&Screen::new(), &near, &targets).unwrap().at,
            (1153, 561)
        );
        assert_eq!(
            place(&Screen::new(), &far, &targets),
            Err("its picture was not found in Target App's window".to_owned())
        );
    }

    #[test]
    fn clicks_sharing_a_picture_each_keep_their_tolerance_and_press() {
        let strict = picture((200, 100), area(100, 40, 100, 60), (3, 1));
        let mut loose = strict.clone();
        if let ClickAt::Picture(wanted) = &mut loose.at {
            wanted.tolerance = 30;
        }
        let mut corner = strict.clone();
        if let ClickAt::Picture(wanted) = &mut corner.at {
            wanted.press = (0, 0);
        }
        let targets = targets(&[strict.clone(), loose.clone(), corner.clone()]);
        assert_eq!(targets.pictures.len(), 3);
        for (click, at) in [
            (&strict, (1153, 561)),
            (&loose, (1153, 561)),
            (&corner, (1150, 560)),
        ] {
            assert_eq!(place(&Screen::new(), click, &targets).unwrap().at, at);
        }
    }
}
