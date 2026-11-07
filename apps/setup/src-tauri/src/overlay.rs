//! The overlay's side of the owner thread: for each application whose
//! overlay is on, one capture of its window that reads all of its values.
//!
//! The capture reads the values on its own thread and sends only their
//! readings, so no frame is copied here. It captures the smallest part of
//! the window that holds every value's region. Nothing is sent into the
//! application: the capture is the same one the automation editor uses, and
//! the overlay is a window of the setup app's own (see `overlay_window`).

use std::collections::BTreeMap;
use std::io;
use std::sync::mpsc::Sender;
use std::time::Duration;

use appytappy_config::app_file::{AppFile, OverlayDef, OverlayModeDef, PanelDef};
use appytappy_sources::screen::check::Check;
use appytappy_sources::screen::saved;
use appytappy_sources::screen::{
    CaptureSession, Crop, Frame, FrameHandler, OutsideWindow, Region, Size,
};
use appytappy_sources::system::{WindowEntry, best_window};
use serde::Serialize;

use crate::owner::{Host, Message, Reading};

/// The shortest time between two frames an overlay reads: often enough for
/// readings it shows about four times a second.
pub(crate) const OVERLAY_INTERVAL: Duration = Duration::from_millis(250);

/// How many of the overlay's polls pass before it looks for its window
/// again after not finding it: about a second at the UI's pace.
pub(crate) const RETRY_POLLS: u32 = 5;

/// How many polls a moved panel's place waits before it is saved, so a drag
/// saves once it settles rather than at every step.
pub(crate) const SETTLE_POLLS: u32 = 3;

/// The left edge Windows gives a minimized window.
const MINIMIZED_LEFT: i32 = -32_000;

/// What one value read on a frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ValueRead {
    /// Its reading.
    Read(Reading),
    /// The window is another size than when the value was drawn, so its
    /// region means nothing on it.
    Resized(Size),
    /// The check could not read it, and why.
    Unreadable(String),
}

/// An application's overlay, as its window shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OverlayView {
    /// The application's name.
    pub(crate) name: String,
    /// Where the readings show.
    pub(crate) mode: OverlayModeDef,
    /// Where the application's window is and whether it is in front, or
    /// `None` while it is not running or is minimized.
    pub(crate) place: Option<Place>,
    /// Why there are no readings, such as that the application is not
    /// running, or `None` while there are.
    pub(crate) message: Option<String>,
    /// Each value's name and reading, in the order they were saved.
    pub(crate) values: Vec<ValueView>,
}

/// Where an application's window is on the screen, in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Place {
    /// The left edge.
    pub(crate) x: i32,
    /// The top edge.
    pub(crate) y: i32,
    /// The width.
    pub(crate) width: u32,
    /// The height.
    pub(crate) height: u32,
    /// Whether the window is the one in front.
    pub(crate) front: bool,
}

/// One value's line in the overlay.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ValueView {
    /// The value's name.
    pub(crate) name: String,
    /// Its reading, or `None` when there is none.
    pub(crate) reading: Option<Reading>,
    /// Why it has no reading, or `None` when it has one or no frame has
    /// arrived yet.
    pub(crate) problem: Option<String>,
}

/// An overlay the setup app shows, for its windows to open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OverlayWindow {
    /// The application's ID.
    pub(crate) app: String,
    /// Its name.
    pub(crate) name: String,
    /// Where the readings show.
    pub(crate) mode: OverlayModeDef,
    /// Where the user last put the panel, if they moved it.
    pub(crate) panel: Option<PanelDef>,
}

/// A value the overlay reads.
#[derive(Debug, Clone)]
struct Readable {
    /// The window's size when the value's region was drawn.
    window_size: Size,
    /// The region, relative to the part of the window the capture takes.
    region: Region,
    /// The check that reads it.
    check: Check,
}

/// One application's overlay.
#[derive(Debug)]
struct Overlay<S> {
    /// Counts this overlay's captures, so a frame from an earlier one is
    /// dropped.
    generation: u64,
    /// The application's name.
    name: String,
    /// Where the readings show.
    mode: OverlayModeDef,
    /// Each value's name, in the order they were saved.
    names: Vec<String>,
    /// The part of the window that holds every value's region, or `None`
    /// when the application has no values.
    crop: Option<Region>,
    /// What the capture reads, in the order of `names`.
    readables: Vec<Readable>,
    /// How to find the window.
    window_match: appytappy_config::window_match::WindowMatch,
    /// The running capture.
    session: Option<S>,
    /// The captured window's handle.
    window: Option<u64>,
    /// What the newest frame read, or why the capture stopped.
    latest: Option<Result<Vec<ValueRead>, String>>,
    /// Why the overlay has no capture, while it has none.
    message: Option<String>,
    /// Polls left before it looks for the window again.
    wait: u32,
    /// A moved panel's place not yet saved, and the polls since it moved.
    placed: Option<(PanelDef, u32)>,
}

/// Every open overlay, by application ID.
#[derive(Debug)]
pub(crate) struct Overlays<S> {
    /// The next capture's generation.
    next: u64,
    open: BTreeMap<String, Overlay<S>>,
}

impl<S: CaptureSession> Overlays<S> {
    /// No overlays.
    pub(crate) const fn new() -> Self {
        Self {
            next: 0,
            open: BTreeMap::new(),
        }
    }

    /// What `app`'s overlay shows now. It opens the overlay from `file` the
    /// first time, and looks for the window while it has no capture.
    ///
    /// # Errors
    ///
    /// Returns why the overlay cannot show, such as that it is off.
    pub(crate) fn view<H: Host<Session = S>>(
        &mut self,
        host: &H,
        messages: &Sender<Message>,
        app: &str,
        file: impl FnOnce() -> Result<(AppFile, std::path::PathBuf), String>,
    ) -> Result<OverlayView, String> {
        if !self.open.contains_key(app) {
            let (file, folder) = file()?;
            let overlay = open(&file, |name| std::fs::read(folder.join(name)))?;
            self.open.insert(app.to_owned(), overlay);
        }
        let generation = self.next;
        let overlay = self
            .open
            .get_mut(app)
            .ok_or_else(|| "The overlay closed.".to_owned())?;
        if overlay.session.is_none() {
            if overlay.wait == 0 {
                match start(host, messages, app, generation, overlay) {
                    Ok(()) => self.next = self.next.wrapping_add(1),
                    Err(message) => {
                        overlay.message = Some(message);
                        overlay.wait = RETRY_POLLS;
                    },
                }
            } else {
                overlay.wait = overlay.wait.saturating_sub(1);
            }
        }
        // Only an overlay over the window needs to know where it is.
        let place = match overlay.window {
            Some(window) if overlay.mode == OverlayModeDef::Overlay => place(host, window)?,
            _ => None,
        };
        if let Some((_, polls)) = &mut overlay.placed {
            *polls = polls.saturating_add(1);
        }
        Ok(view(overlay, place))
    }

    /// Keeps what `app`'s capture with `generation` read, or why it stopped.
    /// A stopped capture closes, and the next poll looks for the window
    /// again.
    pub(crate) fn read(
        &mut self,
        app: &str,
        generation: u64,
        read: Result<Vec<ValueRead>, String>,
    ) -> io::Result<()> {
        let Some(overlay) = self
            .open
            .get_mut(app)
            .filter(|overlay| overlay.generation == generation && overlay.session.is_some())
        else {
            return Ok(());
        };
        match read {
            Ok(values) => {
                overlay.latest = Some(Ok(values));
                Ok(())
            },
            Err(error) => {
                overlay.latest = None;
                overlay.window = None;
                overlay.message = Some(format!("{} cannot be read: {error}.", overlay.name));
                overlay.session.take().map_or(Ok(()), CaptureSession::close)
            },
        }
    }

    /// Notes where the user put `app`'s panel, to save once it settles.
    pub(crate) fn placed(&mut self, app: &str, panel: PanelDef) {
        if let Some(overlay) = self.open.get_mut(app) {
            overlay.placed = Some((panel, 0));
        }
    }

    /// The panel places that settled, by application ID, which the caller
    /// saves. With `all`, every place not yet saved, such as on closing.
    pub(crate) fn settled(&mut self, all: bool) -> Vec<(String, PanelDef)> {
        self.open
            .iter_mut()
            .filter_map(|(app, overlay)| {
                let (panel, polls) = overlay.placed?;
                (all || polls >= SETTLE_POLLS).then(|| {
                    overlay.placed = None;
                    (app.clone(), panel)
                })
            })
            .collect()
    }

    /// Closes `app`'s overlay, or every overlay with `None`, so the next
    /// poll opens it from its file again.
    ///
    /// # Errors
    ///
    /// Returns the first error a capture's thread ended with.
    pub(crate) fn close(&mut self, app: Option<&str>) -> io::Result<()> {
        let closing: Vec<String> = match app {
            Some(app) => vec![app.to_owned()],
            None => self.open.keys().cloned().collect(),
        };
        closing
            .iter()
            .filter_map(|app| self.open.remove(app))
            .filter_map(|overlay| overlay.session)
            .map(CaptureSession::close)
            .fold(Ok(()), Result::and)
    }
}

/// The overlay `file` describes, without a capture yet.
fn open<S>(
    file: &AppFile,
    picture: impl FnMut(&str) -> io::Result<Vec<u8>> + Clone,
) -> Result<Overlay<S>, String> {
    let overlay = file
        .overlay
        .filter(|overlay| overlay.enabled)
        .ok_or_else(|| format!("The overlay for {} is off.", file.name))?;
    let built: Vec<saved::SavedCheck> = file
        .values
        .iter()
        .map(|value| {
            saved::build(&value.screen, picture.clone())
                .map_err(|error| format!("{} cannot be read: {error}.", value.name))
        })
        .collect::<Result<_, _>>()?;
    let crop = built.iter().map(|check| check.region).reduce(union);
    let readables = built
        .into_iter()
        .filter_map(|check| {
            let origin = crop?;
            Some(Readable {
                window_size: check.window_size,
                region: Region::new(
                    check.region.x().saturating_sub(origin.x()),
                    check.region.y().saturating_sub(origin.y()),
                    check.region.size().width,
                    check.region.size().height,
                )?,
                check: check.check,
            })
        })
        .collect();
    Ok(Overlay {
        generation: 0,
        name: file.name.clone(),
        mode: overlay.mode,
        names: file.values.iter().map(|value| value.name.clone()).collect(),
        crop,
        readables,
        window_match: file.window.to_match(),
        session: None,
        window: None,
        latest: None,
        message: None,
        wait: 0,
        placed: None,
    })
}

/// The smallest region that holds both `a` and `b`.
fn union(a: Region, b: Region) -> Region {
    let x = a.x().min(b.x());
    let y = a.y().min(b.y());
    let right = a.right().max(b.right());
    let bottom = a.bottom().max(b.bottom());
    // Both regions are inside the `u32` range, so their union is too, and
    // it is no smaller than either.
    Region::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y)).unwrap_or(a)
}

/// Finds `overlay`'s window and starts reading it with `generation`.
fn start<H: Host>(
    host: &H,
    messages: &Sender<Message>,
    app: &str,
    generation: u64,
    overlay: &mut Overlay<H::Session>,
) -> Result<(), String> {
    let crop = overlay
        .crop
        .ok_or_else(|| format!("{} has no values to show yet.", overlay.name))?;
    let processes = host.processes().map_err(|error| error.to_string())?;
    let windows = host.windows().map_err(|error| error.to_string())?;
    let window = best_window(&overlay.window_match, processes, windows)
        .ok_or_else(|| format!("Open {} to see its values.", overlay.name))?;
    let readables = overlay.readables.clone();
    let messages = messages.clone();
    let app = app.to_owned();
    let handler: FrameHandler = Box::new(move |frame| {
        // The owner stopped when the send fails, and closes this capture.
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a failed send means the owner stopped and will close this capture"
        )]
        let _ = messages.send(Message::Overlay {
            app: app.clone(),
            generation,
            read: read_all(&readables, frame),
        });
        OVERLAY_INTERVAL
    });
    let session = host
        .capture(window, Crop::Region(crop), OVERLAY_INTERVAL, handler)
        .map_err(|error| error.to_string())?;
    overlay.generation = generation;
    overlay.session = Some(session);
    overlay.window = Some(window);
    overlay.latest = None;
    overlay.message = None;
    Ok(())
}

/// What each value reads on `frame`. A window too small for the capture's
/// region is another size, not a stopped capture.
fn read_all(
    readables: &[Readable],
    frame: io::Result<Frame<'_>>,
) -> Result<Vec<ValueRead>, String> {
    let frame = match frame {
        Ok(frame) => frame,
        Err(error) => {
            let size = OutsideWindow::in_error(&error).ok_or_else(|| error.to_string())?;
            return Ok(readables
                .iter()
                .map(|_readable| ValueRead::Resized(size))
                .collect());
        },
    };
    Ok(readables
        .iter()
        .map(|readable| {
            if frame.window_size != readable.window_size {
                return ValueRead::Resized(frame.window_size);
            }
            frame.image.crop(readable.region).map_or_else(
                || ValueRead::Resized(frame.window_size),
                |image| match readable.check.read(&image) {
                    Ok(value) => ValueRead::Read(value.into()),
                    Err(error) => ValueRead::Unreadable(error.to_string()),
                },
            )
        })
        .collect())
}

/// Where `window` is, or `None` while it is minimized or gone.
fn place<H: Host>(host: &H, window: u64) -> Result<Option<Place>, String> {
    let windows = host.windows().map_err(|error| error.to_string())?;
    let front = host
        .foreground_window()
        .map_err(|error| error.to_string())?
        == Some(window);
    Ok(windows
        .into_iter()
        .find(|entry| entry.handle == window)
        .and_then(|entry| place_of(&entry, front)))
}

/// Where `entry` is, or `None` while it is hidden or minimized.
fn place_of(entry: &WindowEntry, front: bool) -> Option<Place> {
    let bounds = entry.bounds;
    if !entry.visible || bounds.left <= MINIMIZED_LEFT {
        return None;
    }
    Some(Place {
        x: bounds.left,
        y: bounds.top,
        width: u32::try_from(bounds.right.checked_sub(bounds.left)?).ok()?,
        height: u32::try_from(bounds.bottom.checked_sub(bounds.top)?).ok()?,
        front,
    })
}

/// What `overlay` shows, with its window at `place`.
fn view<S>(overlay: &Overlay<S>, place: Option<Place>) -> OverlayView {
    let reads = match &overlay.latest {
        Some(Ok(reads)) => Some(reads.as_slice()),
        _ => None,
    };
    let values = overlay
        .names
        .iter()
        .zip(&overlay.readables)
        .enumerate()
        .map(|(index, (name, readable))| {
            let (reading, problem) = match reads.and_then(|reads| reads.get(index)) {
                None => (None, None),
                Some(ValueRead::Read(reading)) => (Some(*reading), None),
                Some(ValueRead::Resized(size)) => (
                    None,
                    Some(format!(
                        "Drawn on a {} window; it is {size} now.",
                        readable.window_size
                    )),
                ),
                Some(ValueRead::Unreadable(error)) => {
                    (None, Some(format!("Cannot read: {error}.")))
                },
            };
            ValueView {
                name: name.clone(),
                reading,
                problem,
            }
        })
        .collect();
    OverlayView {
        name: overlay.name.clone(),
        mode: overlay.mode,
        place,
        message: overlay.message.clone(),
        values,
    }
}

/// The overlays `files` turn on, by application ID.
pub(crate) fn windows<'file>(
    files: impl IntoIterator<Item = (&'file str, &'file AppFile)>,
) -> Vec<OverlayWindow> {
    files
        .into_iter()
        .filter_map(|(app, file)| {
            let overlay: OverlayDef = file.overlay.filter(|overlay| overlay.enabled)?;
            Some(OverlayWindow {
                app: app.to_owned(),
                name: file.name.clone(),
                mode: overlay.mode,
                panel: overlay.panel,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use appytappy_config::anti_cheat::ProcessId;
    use appytappy_sources::system::Bounds;

    use super::*;

    fn entry(visible: bool, left: i32) -> WindowEntry {
        WindowEntry {
            handle: 7,
            process: ProcessId::new(3),
            title: "Example App".to_owned(),
            class: "ExampleWindow".to_owned(),
            visible,
            bounds: Bounds {
                left,
                top: -8,
                right: left.saturating_add(1288),
                bottom: 712,
            },
        }
    }

    #[test]
    fn a_window_has_a_place_only_while_it_shows_and_is_not_minimized() {
        assert_eq!(
            place_of(&entry(true, -8), false),
            Some(Place {
                x: -8,
                y: -8,
                width: 1288,
                height: 720,
                front: false,
            })
        );
        assert_eq!(
            place_of(&entry(true, 0), true).map(|place| place.front),
            Some(true)
        );
        // Windows moves a minimized window to -32000, and a hidden one
        // keeps its last place.
        assert_eq!(place_of(&entry(true, MINIMIZED_LEFT), true), None);
        assert!(place_of(&entry(true, MINIMIZED_LEFT + 1), true).is_some());
        assert_eq!(place_of(&entry(false, 0), true), None);
    }
}
