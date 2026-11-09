//! Placing an automation's clicks on the screen.
//!
//! A click names the saved application it clicks in. The plan keeps what
//! placing it needs, [`ClickTargets`]: that application's window match and
//! the search for each picture a click looks for. When a click runs,
//! [`place`] takes the application's window that [`session_window`] picks,
//! of any of its processes, and turns the click's spot, or where its picture is found in one frame of
//! its area, into a place on the screen. A click whose window is missing,
//! minimized, or another size than its picture was taken at, or whose
//! picture is not there, is not placed, and the log says why.

use std::collections::BTreeMap;
use std::io;
use std::time::Duration;

use appytappy_actions::PlacedClick;
use appytappy_config::app_file::AppFile;
use appytappy_config::click::{Click, ClickAt, ClickPicture, InvalidClick};
use appytappy_config::window_match::WindowMatch;
use appytappy_sources::screen::find::PictureSearch;
use appytappy_sources::screen::{Capture, Image, Region, Size, grab};
use appytappy_sources::system::{Bounds, Pick, SessionWindow, System, session_window, shows};

/// How long a picture click waits for the frame it looks in.
const GRAB_WAIT: Duration = Duration::from_secs(1);

/// What placing an application's clicks needs.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ClickTargets {
    /// Each application clicked in, by ID.
    apps: BTreeMap<String, Target>,
    /// The search for each click picture, by its file name and tolerance,
    /// since two clicks may look for one picture with different ones.
    pictures: BTreeMap<(String, u8), PictureSearch>,
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
    /// Adds what the clicks among `actions` need: each target in `apps`,
    /// the saved applications by ID, and each picture, whose bytes
    /// `picture` reads by file name. Nothing is added when one cannot be.
    ///
    /// # Errors
    ///
    /// Returns why, as a sentence for the log, when a target is not among
    /// `apps` or a picture cannot be read, cannot be looked for in its
    /// area, or has its press outside it.
    pub(crate) fn add<'action>(
        &mut self,
        clicks: impl IntoIterator<Item = &'action Click>,
        apps: &[(&str, &AppFile)],
        mut picture: impl FnMut(&str) -> io::Result<Vec<u8>>,
    ) -> Result<(), String> {
        let mut targets = Vec::new();
        let mut pictures = Vec::new();
        for click in clicks {
            let Some((_, file)) = apps.iter().find(|(id, _)| *id == click.app) else {
                return Err("the application it clicks in is no longer saved".to_owned());
            };
            let mut window = file.window.to_match();
            // A window taken only because none qualified may be another of
            // the application's windows, where the spot or the picture
            // means something else.
            window.fallback = false;
            targets.push((
                click.app.clone(),
                Target {
                    name: file.name.clone(),
                    window,
                },
            ));
            // Each click's picture is checked against its own area and
            // press, even when another click looks for the same picture.
            if let ClickAt::Picture(wanted) = &click.at {
                let bytes = picture(&wanted.image).map_err(|error| {
                    format!("its picture {} cannot be read: {error}", wanted.image)
                })?;
                pictures.push((
                    (wanted.image.clone(), wanted.tolerance),
                    search(wanted, &bytes)?,
                ));
            }
        }
        self.apps.extend(targets);
        self.pictures.extend(pictures);
        Ok(())
    }

    /// Adds everything `other` holds.
    pub(crate) fn merge(&mut self, other: Self) {
        self.apps.extend(other.apps);
        self.pictures.extend(other.pictures);
    }
}

/// The search for `wanted`'s picture, decoded from `bytes`.
fn search(wanted: &ClickPicture, bytes: &[u8]) -> Result<PictureSearch, String> {
    let name = &wanted.image;
    let (image, mask) = Image::decode_png_masked(bytes)
        .map_err(|error| format!("its picture {name} cannot be read: {error}"))?;
    let size = image.size();
    wanted
        .fits_picture(size.width, size.height)
        .map_err(|misfit| {
            if misfit == InvalidClick::PressOutside {
                format!("the place it presses lies outside its picture {name}")
            } else {
                format!("its picture {name} is larger than the area it is looked for in")
            }
        })?;
    PictureSearch::new(image, mask.as_ref(), wanted.tolerance)
        .map_err(|error| format!("its picture {name} cannot be used: {error}"))
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
        .ok_or_else(|| "the application it clicks in is no longer saved".to_owned())?;
    let name = &target.name;
    let failed = |error: io::Error| format!("{name}'s window cannot be found: {error}");
    let SessionWindow {
        entry: window,
        shown,
    } = session_window(
        &target.window,
        Pick::Any,
        screen.processes().map_err(failed)?,
        screen.windows().map_err(failed)?,
        |handle| shows(screen.window_place(handle)),
    )
    .ok_or_else(|| format!("{name} has no window open"))?;
    if !shown {
        return Err(format!("{name}'s window is minimized or hidden"));
    }
    let bounds = screen
        .frame_bounds(window.handle)
        .map_err(failed)?
        .ok_or_else(|| format!("{name}'s window closed"))?;
    let size = size(bounds).ok_or_else(|| format!("{name}'s window has no size"))?;
    let (x, y) = match &click.at {
        ClickAt::Spot(spot) => spot
            .place(size.width, size.height)
            .ok_or_else(|| format!("its spot lies outside {name}'s window"))?,
        ClickAt::Picture(wanted) => {
            let search = targets
                .pictures
                .get(&(wanted.image.clone(), wanted.tolerance))
                .ok_or_else(|| format!("its picture {} was not loaded", wanted.image))?;
            let taken = Size {
                width: wanted.window_width,
                height: wanted.window_height,
            };
            let other_size = |size: Size| {
                format!(
                    "{name}'s window is {}x{}, but its picture was taken at {}x{}",
                    size.width, size.height, taken.width, taken.height
                )
            };
            if size != taken {
                return Err(other_size(size));
            }
            let area = wanted.search;
            let region = Region::new(area.x, area.y, area.width, area.height)
                .ok_or_else(|| "its area is empty".to_owned())?;
            let (frame_size, image) = grab(screen, window.handle, region, GRAB_WAIT)
                .map_err(|error| format!("{name}'s window cannot be captured: {error}"))?;
            if frame_size != taken {
                return Err(other_size(frame_size));
            }
            let found = search
                .find(&image)
                .ok_or_else(|| format!("its picture was not found in {name}'s window"))?;
            // The grab and the search take a moment, and a window that
            // moved meanwhile would put the click off by as much.
            if screen.frame_bounds(window.handle).map_err(failed)? != Some(bounds) {
                return Err(format!(
                    "{name}'s window moved while its picture was looked for"
                ));
            }
            let (press_x, press_y) = wanted.press;
            area.x
                .checked_add(found.x)
                .and_then(|x| x.checked_add(press_x))
                .zip(
                    area.y
                        .checked_add(found.y)
                        .and_then(|y| y.checked_add(press_y)),
                )
                .ok_or_else(|| format!("its picture's place lies outside {name}'s window"))?
        },
    };
    let at = on_screen(bounds, x, y)
        .ok_or_else(|| format!("its place in {name}'s window lies off the screen"))?;
    Ok(PlacedClick {
        window: window.handle,
        process: window.process,
        app: name.clone(),
        at,
        button: click.button,
        return_focus: click.return_focus,
    })
}

/// The size of `bounds`, or `None` when it is empty.
fn size(bounds: Bounds) -> Option<Size> {
    let width = u32::try_from(bounds.right.checked_sub(bounds.left)?).ok()?;
    let height = u32::try_from(bounds.bottom.checked_sub(bounds.top)?).ok()?;
    (width > 0 && height > 0).then_some(Size { width, height })
}

/// The screen place of (`x`, `y`) in a window at `bounds`.
fn on_screen(bounds: Bounds, x: u32, y: u32) -> Option<(i32, i32)> {
    Some((
        bounds.left.checked_add(i32::try_from(x).ok()?)?,
        bounds.top.checked_add(i32::try_from(y).ok()?)?,
    ))
}

#[cfg(test)]
mod tests {
    use appytappy_config::anti_cheat::ProcessId;
    use appytappy_config::app_file::{
        AntiCheatDef, BuildDef, FORMAT, FallbackDef, MustMatchDef, SizeChangesDef, WindowDef,
    };
    use appytappy_config::click::{Area, ClickButton, Corner, Spot};
    use appytappy_sources::screen::{CaptureSession, Crop, Frame, FrameHandler};
    use appytappy_sources::system::{ProcessEntry, Rgb, WindowEntry, WindowPlace};
    use std::sync::mpsc;
    use std::time::Instant;

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
    struct Screen {
        shown: bool,
        bounds: Option<Bounds>,
        frame: Image,
        /// The frame bounds each next read answers, first first; once it
        /// runs out, a read answers `bounds`.
        reads: Option<mpsc::Receiver<Bounds>>,
    }

    impl Screen {
        fn new() -> Self {
            Self {
                shown: true,
                bounds: Some(Bounds {
                    left: 1000,
                    top: 500,
                    right: 1200,
                    bottom: 600,
                }),
                frame: window_image(),
                reads: None,
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

        fn frame_size(&self, _window: u64) -> io::Result<Option<Size>> {
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }

        fn frame_bounds(&self, _window: u64) -> io::Result<Option<Bounds>> {
            let next = self.reads.as_ref().and_then(|reads| reads.try_recv().ok());
            Ok(next.or(self.bounds))
        }

        fn executable_build(&self, _process: ProcessId) -> io::Result<BuildDef> {
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }

        fn screen_pixel(&self, _x: i32, _y: i32) -> io::Result<Rgb> {
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }
    }

    struct Closed;

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
            false,
            ClickAt::Spot(Spot::new(corner, x, y).unwrap()),
        )
        .unwrap()
    }

    fn picture(window: (u32, u32), search: Area, press: (u32, u32)) -> Click {
        Click::new(
            "target",
            ClickButton::Double,
            true,
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

    fn targets(clicks: &[Click]) -> Result<ClickTargets, String> {
        let file = app_file("Target App");
        let mut targets = ClickTargets::default();
        targets.add(clicks, &[("target", &file)], |name| {
            if name == "patch-1.png" {
                Ok(patch_png())
            } else {
                Err(io::Error::from(io::ErrorKind::NotFound))
            }
        })?;
        Ok(targets)
    }

    #[test]
    fn a_spot_lands_from_its_corner_of_the_window_s_frame() {
        let click = spot(Corner::BottomRight, 10, 5);
        let targets = targets(std::slice::from_ref(&click)).unwrap();
        let placed = place(&Screen::new(), &click, &targets).unwrap();
        assert_eq!(
            placed,
            PlacedClick {
                window: 9,
                process: ProcessId::new(8),
                app: "Target App".to_owned(),
                at: (1189, 594),
                button: ClickButton::Right,
                return_focus: false,
            }
        );
    }

    #[test]
    fn a_picture_lands_where_it_is_found_in_its_area() {
        let click = picture((200, 100), area(100, 40, 100, 60), (3, 1));
        let targets = targets(std::slice::from_ref(&click)).unwrap();
        let placed = place(&Screen::new(), &click, &targets).unwrap();
        assert_eq!(placed.at, (1153, 561));
        assert_eq!(placed.button, ClickButton::Double);
        assert!(placed.return_focus);
    }

    #[test]
    fn a_click_is_not_placed_without_its_window_or_its_picture() {
        let click = picture((200, 100), area(100, 40, 100, 60), (0, 0));
        let targets = targets(std::slice::from_ref(&click)).unwrap();
        let mut hidden = Screen::new();
        hidden.shown = false;
        assert_eq!(
            place(&hidden, &click, &targets),
            Err("Target App's window is minimized or hidden".to_owned())
        );
        let mut closed = Screen::new();
        closed.bounds = None;
        assert_eq!(
            place(&closed, &click, &targets),
            Err("Target App's window is minimized or hidden".to_owned()),
            "a window with no place shows as gone"
        );
        let mut resized = Screen::new();
        resized.bounds = Some(Bounds {
            left: 0,
            top: 0,
            right: 300,
            bottom: 100,
        });
        assert_eq!(
            place(&resized, &click, &targets),
            Err("Target App's window is 300x100, but its picture was taken at 200x100".to_owned())
        );
        let mut shrunk = Screen::new();
        shrunk.frame = shrunk
            .frame
            .crop(Region::new(0, 0, 200, 99).unwrap())
            .unwrap();
        let fits = picture((200, 100), area(100, 40, 100, 59), (0, 0));
        assert_eq!(
            place(&shrunk, &fits, &targets),
            Err("Target App's window is 200x99, but its picture was taken at 200x100".to_owned())
        );
        let elsewhere = picture((200, 100), area(0, 0, 100, 60), (0, 0));
        assert_eq!(
            place(&Screen::new(), &elsewhere, &targets),
            Err("its picture was not found in Target App's window".to_owned())
        );
        let other = Click {
            app: "other".to_owned(),
            ..click
        };
        assert_eq!(
            place(&Screen::new(), &other, &targets),
            Err("the application it clicks in is no longer saved".to_owned())
        );
    }

    #[test]
    fn a_picture_is_not_clicked_in_a_window_that_moved_while_it_was_looked_for() {
        let click = picture((200, 100), area(100, 40, 100, 60), (3, 1));
        let targets = targets(std::slice::from_ref(&click)).unwrap();
        let (sender, reads) = mpsc::channel();
        let screen = Screen {
            reads: Some(reads),
            ..Screen::new()
        };
        let bounds = screen.bounds.unwrap();
        sender.send(bounds).unwrap();
        sender
            .send(Bounds {
                left: 1001,
                right: 1201,
                ..bounds
            })
            .unwrap();
        assert_eq!(
            place(&screen, &click, &targets),
            Err("Target App's window moved while its picture was looked for".to_owned())
        );
    }

    #[test]
    fn a_window_taken_only_as_the_fallback_is_not_clicked() {
        let click = spot(Corner::TopLeft, 0, 0);
        let mut file = app_file("Target App");
        file.window.title = "Main".to_owned();
        file.window.must_match.title = true;
        file.window.fallback = FallbackDef::Executable;
        let mut targets = ClickTargets::default();
        targets
            .add([&click], &[("target", &file)], |_| Ok(Vec::new()))
            .unwrap();
        assert_eq!(
            place(&Screen::new(), &click, &targets),
            Err("Target App has no window open".to_owned())
        );
    }

    #[test]
    fn clicks_sharing_a_picture_each_keep_their_tolerance_and_press() {
        let strict = picture((200, 100), area(100, 40, 100, 60), (3, 1));
        let mut loose = strict.clone();
        if let ClickAt::Picture(wanted) = &mut loose.at {
            wanted.tolerance = 30;
        }
        assert_eq!(targets(&[strict.clone(), loose]).unwrap().pictures.len(), 2);
        let mut outside = strict.clone();
        if let ClickAt::Picture(wanted) = &mut outside.at {
            wanted.press = (9, 0);
        }
        assert_eq!(
            targets(&[strict, outside]),
            Err("the place it presses lies outside its picture patch-1.png".to_owned())
        );
    }

    #[test]
    fn a_spot_outside_the_window_is_not_placed() {
        let click = spot(Corner::TopLeft, 200, 0);
        let targets = targets(std::slice::from_ref(&click)).unwrap();
        assert_eq!(
            place(&Screen::new(), &click, &targets),
            Err("its spot lies outside Target App's window".to_owned())
        );
    }

    #[test]
    fn a_window_with_no_window_open_is_not_clicked() {
        struct Empty;
        impl System for Empty {
            fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
                Ok(Vec::new())
            }
            fn windows(&self) -> io::Result<Vec<WindowEntry>> {
                Ok(Vec::new())
            }
            fn window_place(&self, _window: u64) -> io::Result<Option<WindowPlace>> {
                Ok(None)
            }
            fn frame_size(&self, _window: u64) -> io::Result<Option<Size>> {
                Ok(None)
            }
            fn executable_build(&self, _process: ProcessId) -> io::Result<BuildDef> {
                Err(io::Error::from(io::ErrorKind::Unsupported))
            }
            fn screen_pixel(&self, _x: i32, _y: i32) -> io::Result<Rgb> {
                Err(io::Error::from(io::ErrorKind::Unsupported))
            }
        }
        impl Capture for Empty {
            type Session = Closed;
            fn start(
                &self,
                _window: u64,
                _crop: Crop,
                _throttle: Duration,
                _handler: FrameHandler,
            ) -> io::Result<Closed> {
                Err(io::Error::from(io::ErrorKind::Unsupported))
            }
        }
        let click = spot(Corner::TopLeft, 0, 0);
        let targets = targets(std::slice::from_ref(&click)).unwrap();
        assert_eq!(
            place(&Empty, &click, &targets),
            Err("Target App has no window open".to_owned())
        );
        let failing = appytappy_sources::system::Unsupported;
        assert!(
            place(&failing, &click, &targets)
                .unwrap_err()
                .starts_with("Target App's window cannot be found: ")
        );
    }

    #[test]
    fn targets_need_their_application_and_a_usable_picture() {
        let good = picture((200, 100), area(100, 40, 100, 60), (3, 1));
        assert_eq!(
            targets(&[spot(Corner::TopLeft, 0, 0), good.clone()])
                .unwrap()
                .pictures
                .len(),
            1
        );
        let missing = Click {
            app: "other".to_owned(),
            ..good.clone()
        };
        assert_eq!(
            targets(&[missing]),
            Err("the application it clicks in is no longer saved".to_owned())
        );
        let mut unread = picture((200, 100), area(100, 40, 100, 60), (0, 0));
        if let ClickAt::Picture(wanted) = &mut unread.at {
            wanted.image = "gone-1.png".to_owned();
        }
        assert!(
            targets(&[unread])
                .unwrap_err()
                .starts_with("its picture gone-1.png cannot be read: ")
        );
        assert_eq!(
            targets(&[picture((200, 100), area(0, 0, 3, 2), (0, 0))]),
            Err("its picture patch-1.png is larger than the area it is looked for in".to_owned())
        );
        assert_eq!(
            targets(&[picture((200, 100), area(0, 0, 4, 1), (0, 0))]),
            Err("its picture patch-1.png is larger than the area it is looked for in".to_owned())
        );
        assert!(
            targets(&[picture((200, 100), area(0, 0, 4, 2), (3, 1))]).is_ok(),
            "a picture as large as its area, pressed at its last pixel, is looked for"
        );
        for press in [(4, 0), (0, 2)] {
            assert_eq!(
                targets(&[picture((200, 100), area(0, 0, 10, 10), press)]),
                Err("the place it presses lies outside its picture patch-1.png".to_owned()),
                "{press:?}"
            );
        }
        let file = app_file("Target App");
        let mut failed = ClickTargets::default();
        let spot = spot(Corner::TopLeft, 0, 0);
        assert!(
            failed
                .add([&spot, &missing_picture()], &[("target", &file)], |_| Err(
                    io::Error::from(io::ErrorKind::NotFound)
                ))
                .is_err()
        );
        assert_eq!(failed, ClickTargets::default(), "nothing is added");
        let broken = |_: &str| Ok(b"not a png".to_vec());
        assert!(
            ClickTargets::default()
                .add([&good], &[("target", &file)], broken)
                .unwrap_err()
                .starts_with("its picture patch-1.png cannot be read: ")
        );
    }

    fn missing_picture() -> Click {
        picture((200, 100), area(100, 40, 100, 60), (0, 0))
    }

    #[test]
    fn a_window_s_size_and_screen_place_come_from_its_bounds() {
        let bounds = |left, top, right, bottom| Bounds {
            left,
            top,
            right,
            bottom,
        };
        assert_eq!(
            size(bounds(10, 20, 30, 60)),
            Some(Size {
                width: 20,
                height: 40
            })
        );
        assert_eq!(size(bounds(10, 20, 10, 60)), None);
        assert_eq!(size(bounds(10, 20, 30, 20)), None);
        assert_eq!(size(bounds(30, 20, 10, 60)), None);
        assert_eq!(on_screen(bounds(-100, 50, 0, 0), 30, 4), Some((-70, 54)));
        assert_eq!(on_screen(bounds(i32::MAX, 0, 0, 0), 1, 0), None);
        assert_eq!(on_screen(bounds(0, i32::MAX, 0, 0), 0, 1), None);
        assert_eq!(on_screen(bounds(0, 0, 0, 0), u32::MAX, 0), None);
    }
}
