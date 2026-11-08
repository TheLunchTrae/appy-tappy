//! The overlay's side of the owner thread: for each application whose
//! overlay is on, one capture of its window that reads its values.
//!
//! The capture reads the values on its own thread and sends only their
//! readings, so no frame is copied here. Like the runtime, it reads the
//! values drawn at the window's current size and copies only their regions,
//! packed by a [`Layout`]. Nothing is sent into the application:
//! the capture is the same one the automation editor uses, and the overlay
//! is a window of the setup app's own (see [`window`]).

pub(crate) mod window;

use std::collections::BTreeMap;
use std::io;
use std::sync::mpsc::Sender;
use std::time::Duration;

use appytappy_config::app_file::{AppFile, OverlayDef, OverlayModeDef, PanelDef};
use appytappy_sources::screen::check::Check;
use appytappy_sources::screen::saved::{self, SavedCheck};
use appytappy_sources::screen::{
    CaptureSession, Crop, Frame, FrameHandler, Layout, OutsideWindow, Region, Size,
};
use appytappy_sources::system::{WindowPlace, best_window};
use serde::Serialize;

use crate::owner::{Host, Message, Reading};

/// The shortest time between two frames an overlay reads: often enough for
/// readings it shows about four times a second.
pub(crate) const OVERLAY_INTERVAL: Duration = Duration::from_millis(250);

/// How many of the overlay's polls pass before it tries again after its
/// window or its file could not be read: about a second at the UI's pace.
pub(crate) const RETRY_POLLS: u32 = 5;

/// How many polls a moved panel's place waits before it is saved, so a drag
/// saves once it settles rather than at every step.
pub(crate) const SETTLE_POLLS: u32 = 3;

/// What one value read on a frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ValueRead {
    /// Its reading.
    Read(Reading),
    /// The window has this size, not the one the value was drawn at, so its
    /// region means nothing on it.
    Resized(Size),
    /// The check could not read it, and why.
    Unreadable(String),
}

/// What a capture read on one frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FrameRead {
    /// What each value read, in the order they were saved.
    Values(Vec<ValueRead>),
    /// The window now has this size, not the one the capture reads.
    Resized(Size),
}

/// An application's overlay, as its window shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OverlayView {
    /// The application's name.
    pub(crate) name: String,
    /// Where the readings show.
    pub(crate) mode: OverlayModeDef,
    /// Why there are no readings, such as that the application is not
    /// running, or `None` while there are.
    pub(crate) message: Option<String>,
    /// Each value's name and reading, in the order they were saved.
    pub(crate) values: Vec<ValueView>,
}

/// Where an overlay over its application's window goes, in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Place {
    /// The left edge of the application's window.
    pub(crate) x: i32,
    /// The top edge of the application's window.
    pub(crate) y: i32,
    /// The application window's width.
    pub(crate) width: u32,
    /// The application window's height.
    pub(crate) height: u32,
    /// The overlay card's size in logical pixels, as its page measured it.
    pub(crate) card: Card,
}

/// The size of an overlay's card in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Card {
    /// Its width.
    pub(crate) width: f64,
    /// Its height.
    pub(crate) height: f64,
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
    /// Where the value sits in the captured image.
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
    /// Each value's name, and its check or why it cannot be read, in the
    /// order they were saved.
    values: Vec<(String, Result<SavedCheck, String>)>,
    /// The window size whose values the capture reads, or `None` before a
    /// frame showed another.
    size: Option<Size>,
    /// How to find the window.
    window_match: appytappy_config::window_match::WindowMatch,
    /// The running capture.
    session: Option<S>,
    /// The captured window's handle.
    window: Option<u64>,
    /// What the newest frame read.
    latest: Option<Vec<ValueRead>>,
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
    /// Overlays that could not open, why, and the polls left before they
    /// try again.
    failed: BTreeMap<String, (String, u32)>,
    /// The size of each overlay's card, as its page measured it. It outlasts
    /// an overlay that closes to read its file again, since its window and
    /// card stay; the page tells it only when the card changes.
    cards: BTreeMap<String, Card>,
}

impl<S: CaptureSession> Overlays<S> {
    /// No overlays.
    pub(crate) const fn new() -> Self {
        Self {
            next: 0,
            open: BTreeMap::new(),
            failed: BTreeMap::new(),
            cards: BTreeMap::new(),
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
        messages: &Sender<Message<H>>,
        app: &str,
        file: impl FnOnce() -> Result<(AppFile, std::path::PathBuf), String>,
    ) -> Result<OverlayView, String> {
        if !self.open.contains_key(app) {
            if let Some((message, wait)) = self.failed.get_mut(app) {
                if *wait > 0 {
                    *wait = wait.saturating_sub(1);
                    return Err(message.clone());
                }
                self.failed.remove(app);
            }
            let opened = file()
                .and_then(|(file, folder)| open(&file, |name| std::fs::read(folder.join(name))));
            match opened {
                Ok(overlay) => {
                    self.open.insert(app.to_owned(), overlay);
                },
                Err(message) => {
                    self.failed
                        .insert(app.to_owned(), (message.clone(), RETRY_POLLS));
                    return Err(message);
                },
            }
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
        if let Some((_, polls)) = &mut overlay.placed {
            *polls = polls.saturating_add(1);
        }
        // Windows stops sending a minimized window's frames, so its last
        // readings would show as if they were new.
        let minimized = overlay
            .window
            .and_then(|window| host.window_place(window).ok().flatten())
            .is_some_and(|place| !place.shown);
        Ok(view(overlay, minimized))
    }

    /// Where `app`'s overlay goes over its window, or `None` while it
    /// should not show: the overlay is a panel or not open, its card is not
    /// measured, or its window is not the one in front.
    pub(crate) fn place<H: Host<Session = S>>(&self, host: &H, app: &str) -> Option<Place> {
        let overlay = self
            .open
            .get(app)
            .filter(|overlay| overlay.mode == OverlayModeDef::Overlay)?;
        let card = *self.cards.get(app)?;
        let window = host.window_place(overlay.window?).ok()??;
        place_of(window, card)
    }

    /// Keeps the size of `app`'s card, which its overlay window takes.
    pub(crate) fn card(&mut self, app: &str, card: Card) {
        self.cards.insert(app.to_owned(), card);
    }

    /// The mode of `app`'s open overlay, if it is open.
    pub(crate) fn mode(&self, app: &str) -> Option<OverlayModeDef> {
        self.open.get(app).map(|overlay| overlay.mode)
    }

    /// Keeps what `app`'s capture with `generation` read, or why it stopped.
    /// A stopped capture closes, and a poll about a second later looks for
    /// the window again. A window that changed to a size some values were
    /// drawn at gets a capture of those values on the next poll.
    pub(crate) fn read(
        &mut self,
        app: &str,
        generation: u64,
        read: Result<FrameRead, String>,
    ) -> io::Result<()> {
        let Some(overlay) = self
            .open
            .get_mut(app)
            .filter(|overlay| overlay.generation == generation && overlay.session.is_some())
        else {
            return Ok(());
        };
        match read {
            Ok(FrameRead::Values(values)) => {
                overlay.latest = Some(values);
                Ok(())
            },
            Ok(FrameRead::Resized(size)) => {
                let drawn = overlay
                    .values
                    .iter()
                    .any(|(_, check)| check.as_ref().is_ok_and(|check| check.window_size == size));
                if !drawn {
                    // Nothing was drawn at this size, so the capture goes on
                    // until the window is a size that was.
                    overlay.latest = Some(vec![ValueRead::Resized(size); overlay.values.len()]);
                    return Ok(());
                }
                overlay.size = Some(size);
                overlay.latest = None;
                overlay.window = None;
                overlay.wait = 0;
                overlay.session.take().map_or(Ok(()), CaptureSession::close)
            },
            Err(error) => {
                overlay.latest = None;
                overlay.window = None;
                overlay.message = Some(format!("{} cannot be read: {error}.", overlay.name));
                overlay.wait = RETRY_POLLS;
                overlay.session.take().map_or(Ok(()), CaptureSession::close)
            },
        }
    }

    /// Notes where the user put `app`'s panel, to save once it settles. A
    /// panel's overlay opens on its page's first poll, after the window is
    /// placed and shown, so the moves that open it are not kept.
    pub(crate) fn placed(&mut self, app: &str, panel: PanelDef) {
        if let Some(overlay) = self
            .open
            .get_mut(app)
            .filter(|overlay| overlay.mode == OverlayModeDef::PictureInPicture)
        {
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

    /// Closes `app`'s overlay, whose window closed, or every overlay with
    /// `None`, so the next poll opens it from its file again.
    ///
    /// # Errors
    ///
    /// Returns the first error a capture's thread ended with.
    pub(crate) fn close(&mut self, app: Option<&str>) -> io::Result<()> {
        let closing: Vec<String> = if let Some(app) = app {
            vec![app.to_owned()]
        } else {
            self.failed.clear();
            self.open.keys().cloned().collect()
        };
        closing
            .iter()
            .filter_map(|closed| {
                self.failed.remove(closed);
                if app.is_some() {
                    self.cards.remove(closed);
                }
                self.open.remove(closed)
            })
            .filter_map(|overlay| overlay.session)
            .map(CaptureSession::close)
            .fold(Ok(()), Result::and)
    }
}

/// The overlay `file` describes, without a capture yet. A value whose check
/// cannot be built says why in its line, and the others still read.
fn open<S>(
    file: &AppFile,
    picture: impl FnMut(&str) -> io::Result<Vec<u8>> + Clone,
) -> Result<Overlay<S>, String> {
    let overlay = file
        .overlay
        .filter(|overlay| overlay.enabled)
        .ok_or_else(|| format!("The overlay for {} is off.", file.name))?;
    let values = file
        .values
        .iter()
        .map(|value| {
            let check = saved::build(&value.screen, &file.references, picture.clone())
                .map_err(|error| format!("Cannot read: {error}."));
            (value.name.clone(), check)
        })
        .collect();
    Ok(Overlay {
        generation: 0,
        name: file.name.clone(),
        mode: overlay.mode,
        values,
        size: None,
        window_match: file.window.to_match(),
        session: None,
        window: None,
        latest: None,
        message: None,
        wait: 0,
        placed: None,
    })
}

/// What to capture for the values drawn at `size`, and where each value sits
/// in the captured image, or `None` for a value drawn at another size or that
/// cannot be read.
fn crop_for(
    values: &[(String, Result<SavedCheck, String>)],
    size: Size,
) -> Option<(Layout, Vec<Option<Readable>>)> {
    let drawn = || {
        values.iter().map(move |(_, check)| {
            check
                .as_ref()
                .ok()
                .filter(|check| check.window_size == size)
        })
    };
    let layout = Layout::new(drawn().map(|check| check.map(|check| check.region)))?;
    let readables = drawn()
        .enumerate()
        .map(|(position, check)| {
            Some(Readable {
                region: layout.packed(position)?,
                check: check?.check.clone(),
            })
        })
        .collect();
    Some((layout, readables))
}

/// Finds `overlay`'s window and starts reading it with `generation`.
fn start<H: Host>(
    host: &H,
    messages: &Sender<Message<H>>,
    app: &str,
    generation: u64,
    overlay: &mut Overlay<H::Session>,
) -> Result<(), String> {
    let no_values = || format!("{} has no values to show yet.", overlay.name);
    let size = overlay
        .size
        .or_else(|| {
            overlay
                .values
                .iter()
                .find_map(|(_, check)| check.as_ref().ok().map(|check| check.window_size))
        })
        .ok_or_else(no_values)?;
    let (crop, readables) = crop_for(&overlay.values, size).ok_or_else(no_values)?;
    let processes = host.processes().map_err(|error| error.to_string())?;
    let windows = host.windows().map_err(|error| error.to_string())?;
    let window = best_window(&overlay.window_match, processes, windows)
        .ok_or_else(|| format!("Open {} to see its values.", overlay.name))?;
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
            read: read_all(size, &readables, frame),
        });
        OVERLAY_INTERVAL
    });
    let session = host
        .capture(window, Crop::Layout(crop), OVERLAY_INTERVAL, handler)
        .map_err(|error| error.to_string())?;
    overlay.generation = generation;
    overlay.size = Some(size);
    overlay.session = Some(session);
    overlay.window = Some(window);
    overlay.latest = None;
    overlay.message = None;
    Ok(())
}

/// What each value drawn at `size` reads on `frame`. A frame of a window of
/// another size says so, and a value drawn at another size reads as that.
fn read_all(
    size: Size,
    readables: &[Option<Readable>],
    frame: io::Result<Frame<'_>>,
) -> Result<FrameRead, String> {
    let frame = match frame {
        Ok(frame) if frame.window_size != size => return Ok(FrameRead::Resized(frame.window_size)),
        Ok(frame) => frame,
        Err(error) => {
            return match OutsideWindow::in_error(&error) {
                Some(other) if other != size => Ok(FrameRead::Resized(other)),
                // The regions do not fit the size they were drawn at.
                Some(_) => Ok(FrameRead::Values(
                    readables
                        .iter()
                        .map(|_readable| ValueRead::Unreadable(error.to_string()))
                        .collect(),
                )),
                None => Err(error.to_string()),
            };
        },
    };
    Ok(FrameRead::Values(
        readables
            .iter()
            .map(|readable| {
                let Some(readable) = readable else {
                    return ValueRead::Resized(size);
                };
                frame.image.crop(readable.region).map_or_else(
                    || ValueRead::Resized(size),
                    |image| match readable.check.read(&image) {
                        Ok(value) => ValueRead::Read(value.into()),
                        Err(error) => ValueRead::Unreadable(error.to_string()),
                    },
                )
            })
            .collect(),
    ))
}

/// Where an overlay with `card` goes over a `window`, or `None` unless the
/// window shows and is in front.
fn place_of(window: WindowPlace, card: Card) -> Option<Place> {
    if !window.shown || !window.front {
        return None;
    }
    let bounds = window.bounds;
    Some(Place {
        x: bounds.left,
        y: bounds.top,
        width: u32::try_from(bounds.right.checked_sub(bounds.left)?).ok()?,
        height: u32::try_from(bounds.bottom.checked_sub(bounds.top)?).ok()?,
        card,
    })
}

/// What `overlay` shows; with `minimized`, only that its window is.
fn view<S>(overlay: &Overlay<S>, minimized: bool) -> OverlayView {
    let message = if minimized {
        Some(format!("Restore {} to see its values.", overlay.name))
    } else {
        overlay.message.clone()
    };
    let values = overlay
        .values
        .iter()
        .enumerate()
        .map(|(index, (name, check))| {
            let read = overlay.latest.as_ref().and_then(|reads| reads.get(index));
            let (reading, problem) = match (check, read) {
                (Err(problem), _) => (None, Some(problem.clone())),
                (Ok(_), None) => (None, None),
                (Ok(_), Some(ValueRead::Read(reading))) => (Some(reading.clone()), None),
                (Ok(check), Some(ValueRead::Resized(size))) => (
                    None,
                    Some(format!(
                        "Drawn on a {} window; it is {size} now.",
                        check.window_size
                    )),
                ),
                (Ok(_), Some(ValueRead::Unreadable(error))) => {
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
        message,
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
    use appytappy_sources::system::Bounds;

    use super::*;

    const CARD: Card = Card {
        width: 180.0,
        height: 64.0,
    };

    fn window(shown: bool, front: bool) -> WindowPlace {
        WindowPlace {
            bounds: Bounds {
                left: -8,
                top: -8,
                right: 1280,
                bottom: 712,
            },
            shown,
            front,
        }
    }

    #[test]
    fn regions_outside_a_window_of_their_own_size_are_unreadable_and_another_size_is_a_resize() {
        let size = Size {
            width: 4,
            height: 2,
        };
        let outside = |window_size| Err(OutsideWindow { window_size }.into());
        let readables = [None];
        assert_eq!(
            read_all(size, &readables, outside(size)),
            Ok(FrameRead::Values(vec![ValueRead::Unreadable(
                "the region lies outside the 4x2 window".to_owned()
            )]))
        );
        let wider = Size {
            width: 6,
            height: 2,
        };
        assert_eq!(
            read_all(size, &readables, outside(wider)),
            Ok(FrameRead::Resized(wider))
        );
    }

    #[test]
    fn an_overlay_goes_on_its_window_only_while_it_shows_in_front() {
        assert_eq!(
            place_of(window(true, true), CARD),
            Some(Place {
                x: -8,
                y: -8,
                width: 1288,
                height: 720,
                card: CARD,
            })
        );
        assert_eq!(place_of(window(false, true), CARD), None);
        assert_eq!(place_of(window(true, false), CARD), None);
    }
}
