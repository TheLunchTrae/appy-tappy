//! Placing a click on the screen: finding its application's window now,
//! and turning its spot, or where its picture is found in one frame of its
//! area, into a place there.
//!
//! The runtime places each click with [`place`] just before it clicks, and
//! the setup app places the click it sends on request the same way, so a
//! click tested there lands where the runtime clicks.

use std::io;
use std::time::Duration;

use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::click::Spot;
use appytappy_config::window_match::WindowMatch;

use super::click::ClickSearch;
use super::{Capture, Size, grab};
use crate::system::{Bounds, Pick, SessionWindow, System, session_window, shows};

/// How long a picture click waits for the frame it looks in.
const GRAB_WAIT: Duration = Duration::from_secs(1);

/// What a click looks for in its window.
#[derive(Debug, Clone, Copy)]
pub enum Target<'search> {
    /// The same place every time, measured from a corner.
    Spot(Spot),
    /// A picture, wherever it is found in its area.
    Picture(&'search ClickSearch),
}

/// A click placed on the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    /// The window it clicks in.
    pub window: u64,
    /// That window's title, which tells an application's windows apart.
    pub title: String,
    /// The process that owns the window.
    pub process: ProcessId,
    /// Where it lands, in screen pixels.
    pub at: (i32, i32),
}

/// Where a click on `target` lands on the screen now, in the window of the
/// application called `name` that `window` matches.
///
/// It takes the window that [`session_window`] picks, of any of the
/// application's processes, but never one taken only as the fallback: that
/// may be another of the application's windows, where the spot or the
/// picture means something else. The place is measured from the window's
/// content on the screen, as a capture lends it. For a picture, the window
/// must have the size the picture was taken at; one frame of its area is
/// grabbed and searched, and a window that moved meanwhile is not clicked.
///
/// # Errors
///
/// Returns why the click lands nowhere, as a clause for a log or a message,
/// such as a window that is not open, is minimized or hidden, or has another
/// size than the picture was taken at, or a picture that is not found.
pub fn place<S: System + Capture>(
    screen: &S,
    name: &str,
    window: &WindowMatch,
    target: Target<'_>,
) -> Result<Placed, String> {
    let wanted = WindowMatch {
        fallback: false,
        ..window.clone()
    };
    let failed = |error: io::Error| format!("{name}'s window cannot be found: {error}");
    let SessionWindow {
        entry: window,
        shown,
    } = session_window(
        &wanted,
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
        .content_bounds(window.handle)
        .map_err(failed)?
        .ok_or_else(|| format!("{name}'s window closed"))?;
    let size = size(bounds).ok_or_else(|| format!("{name}'s window has no size"))?;
    let (x, y) = match target {
        Target::Spot(spot) => spot
            .place(size.width, size.height)
            .ok_or_else(|| format!("its spot lies outside {name}'s window"))?,
        Target::Picture(search) => {
            let taken = search.taken();
            let other_size = |size: Size| {
                format!(
                    "{name}'s window is {}x{}, but its picture was taken at {}x{}",
                    size.width, size.height, taken.width, taken.height
                )
            };
            if size != taken {
                return Err(other_size(size));
            }
            let (grabbed, image) = grab(screen, window.handle, search.area(), GRAB_WAIT)
                .map_err(|error| format!("{name}'s window cannot be captured: {error}"))?;
            if grabbed != taken {
                return Err(other_size(grabbed));
            }
            let landing = search
                .locate(&image)
                .ok_or_else(|| format!("its picture was not found in {name}'s window"))?;
            // The grab and the search take a moment, and a window that
            // moved meanwhile would put the click off by as much.
            if screen.content_bounds(window.handle).map_err(failed)? != Some(bounds) {
                return Err(format!(
                    "{name}'s window moved while its picture was looked for"
                ));
            }
            (landing.x, landing.y)
        },
    };
    let at = on_screen(bounds, x, y)
        .ok_or_else(|| format!("its place in {name}'s window lies off the screen"))?;
    Ok(Placed {
        window: window.handle,
        title: window.title,
        process: window.process,
        at,
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
    use std::sync::mpsc;
    use std::time::Instant;

    use appytappy_config::app_file::{BuildDef, FallbackDef, MustMatchDef, WindowDef};
    use appytappy_config::click::{Area, ClickPicture, Corner};

    use super::*;
    use crate::screen::{CaptureSession, Crop, Frame, FrameHandler, Image, Region};
    use crate::system::{ProcessEntry, WindowEntry, WindowPlace};

    /// The window rules of `Target.exe`, which take any of its windows.
    fn any_window() -> WindowDef {
        WindowDef {
            title: "*".to_owned(),
            class: "*".to_owned(),
            executable: "Target.exe".to_owned(),
            must_match: MustMatchDef {
                title: false,
                class: false,
            },
            fallback: FallbackDef::None,
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

        fn content_size(&self, _window: u64) -> io::Result<Option<Size>> {
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }

        fn content_bounds(&self, _window: u64) -> io::Result<Option<Bounds>> {
            let next = self.reads.as_ref().and_then(|reads| reads.try_recv().ok());
            Ok(next.or(self.bounds))
        }

        fn executable_build(&self, _process: ProcessId) -> io::Result<BuildDef> {
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

    fn spot(corner: Corner, x: u32, y: u32) -> Spot {
        Spot::new(corner, x, y).unwrap()
    }

    /// The search for [`patch_png`] in `search` of a window `window` in
    /// size, pressed at `press`.
    fn picture(window: (u32, u32), search: Area, press: (u32, u32)) -> ClickSearch {
        let wanted = ClickPicture::new("patch-1.png", window, search, press, 0).unwrap();
        ClickSearch::new(&wanted, &patch_png()).unwrap()
    }

    fn area(x: u32, y: u32, width: u32, height: u32) -> Area {
        Area {
            x,
            y,
            width,
            height,
        }
    }

    /// Places `target` in `Target App`'s window on `screen`.
    fn placed<S: System + Capture>(screen: &S, target: Target<'_>) -> Result<Placed, String> {
        place(screen, "Target App", &any_window().to_match(), target)
    }

    #[test]
    fn a_spot_lands_from_its_corner_of_the_window_s_frame() {
        assert_eq!(
            placed(
                &Screen::new(),
                Target::Spot(spot(Corner::BottomRight, 10, 5))
            ),
            Ok(Placed {
                window: 9,
                title: "Target".to_owned(),
                process: ProcessId::new(8),
                at: (1189, 594),
            })
        );
    }

    #[test]
    fn a_picture_lands_where_it_is_found_in_its_area() {
        let near = picture((200, 100), area(100, 40, 100, 60), (3, 1));
        assert_eq!(
            placed(&Screen::new(), Target::Picture(&near)).map(|placed| placed.at),
            Ok((1153, 561))
        );
        let far = picture((200, 100), area(0, 0, 100, 60), (3, 1));
        assert_eq!(
            placed(&Screen::new(), Target::Picture(&far)),
            Err("its picture was not found in Target App's window".to_owned())
        );
    }

    #[test]
    fn a_click_is_not_placed_without_its_window_at_its_picture_s_size() {
        let search = picture((200, 100), area(100, 40, 100, 60), (0, 0));
        let fits = picture((200, 100), area(100, 40, 100, 59), (0, 0));
        let mut hidden = Screen::new();
        hidden.shown = false;
        assert_eq!(
            placed(&hidden, Target::Picture(&search)),
            Err("Target App's window is minimized or hidden".to_owned())
        );
        let mut closed = Screen::new();
        closed.bounds = None;
        assert_eq!(
            placed(&closed, Target::Picture(&search)),
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
            placed(&resized, Target::Picture(&search)),
            Err("Target App's window is 300x100, but its picture was taken at 200x100".to_owned())
        );
        let mut shrunk = Screen::new();
        shrunk.frame = shrunk
            .frame
            .crop(Region::new(0, 0, 200, 99).unwrap())
            .unwrap();
        assert_eq!(
            placed(&shrunk, Target::Picture(&fits)),
            Err("Target App's window is 200x99, but its picture was taken at 200x100".to_owned())
        );
    }

    #[test]
    fn a_picture_is_not_clicked_in_a_window_that_moved_while_it_was_looked_for() {
        let search = picture((200, 100), area(100, 40, 100, 60), (3, 1));
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
            placed(&screen, Target::Picture(&search)),
            Err("Target App's window moved while its picture was looked for".to_owned())
        );
    }

    #[test]
    fn a_window_taken_only_as_the_fallback_is_not_clicked() {
        let mut rules = any_window();
        rules.title = "Main".to_owned();
        rules.must_match.title = true;
        rules.fallback = FallbackDef::Executable;
        let window = rules.to_match();
        assert!(window.fallback, "the rules ask for the fallback");
        assert_eq!(
            place(
                &Screen::new(),
                "Target App",
                &window,
                Target::Spot(spot(Corner::TopLeft, 0, 0))
            ),
            Err("Target App has no window open".to_owned())
        );
    }

    #[test]
    fn a_spot_outside_the_window_is_not_placed() {
        assert_eq!(
            placed(&Screen::new(), Target::Spot(spot(Corner::TopLeft, 200, 0))),
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
            fn content_size(&self, _window: u64) -> io::Result<Option<Size>> {
                Ok(None)
            }
            fn executable_build(&self, _process: ProcessId) -> io::Result<BuildDef> {
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
        let target = Target::Spot(spot(Corner::TopLeft, 0, 0));
        assert_eq!(
            placed(&Empty, target),
            Err("Target App has no window open".to_owned())
        );
        assert!(
            placed(&crate::system::Unsupported, target)
                .unwrap_err()
                .starts_with("Target App's window cannot be found: ")
        );
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
