//! The overlay's side of the owner thread: for each application whose
//! overlay is on, one capture of its window that reads its values.
//!
//! The capture reads the values on its own thread and sends only their
//! readings, so no frame is copied here. Like the runtime, it places each
//! value at the window's size once the size settles, as its saved check
//! resolves that size, and copies only the regions of the values that read
//! there, as a [`Placed`] packs them, and each frame means to it what it
//! means to the runtime ([`FrameMeaning`]). While a new size settles, as when the
//! user drags the window's corner, it reads nothing, so its capture starts
//! again once rather than for each size the window passes. Nothing is sent
//! into the application: the capture is the same one the automation editor
//! uses, and the overlay is a window of the setup app's own (see
//! [`window`]).
//!
//! The Values page reads an application's values the same way, with a
//! second set of these captures that opens whether the overlay is on or
//! not (see [`Purpose`]).

pub(crate) mod window;

use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use appytappy_config::app_file::{AppFile, OverlayDef, OverlayModeDef, PanelDef, SizeDef};
use appytappy_sources::screen::saved::{self, SavedValue, Unread};
use appytappy_sources::screen::settle::Settle;
use appytappy_sources::screen::{
    CaptureSession, Crop, Frame, FrameHandler, FrameMeaning, Placed, Size,
};
use appytappy_sources::system::WindowPlace;
use serde::Serialize;

use crate::owner::{Host, Message, Reading, at_size};
use crate::stores::SavedApp;

/// The shortest time between two frames an overlay reads: often enough for
/// readings it shows about four times a second.
pub(crate) const OVERLAY_INTERVAL: Duration = Duration::from_millis(250);

/// How many of the overlay's polls pass before it tries again after its
/// window or its file could not be read: about a second at the UI's pace.
pub(crate) const RETRY_POLLS: u32 = 5;

/// How many polls a moved panel's place waits before it is saved, so a drag
/// saves once it settles rather than at every step.
pub(crate) const SETTLE_POLLS: u32 = 3;

/// Who a set of captures reads for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Purpose {
    /// The overlays, which open only for an application whose overlay is
    /// on.
    Overlay,
    /// The Values page, which reads the values of the application it shows
    /// whether its overlay is on or not.
    ValuesPage,
}

/// What one value read on a frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ValueRead {
    /// Its reading.
    Read(Reading),
    /// It is not read on a window of this size, and why.
    NotRead(Size, Unread),
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct OverlayView {
    /// The application's name.
    pub(crate) name: String,
    /// Where the readings show.
    pub(crate) mode: OverlayModeDef,
    /// Why there are no readings, such as that the application is not
    /// running, or `None` while there are.
    pub(crate) message: Option<String>,
    /// The size of the window the readings are from, or `None` before a
    /// frame arrived.
    pub(crate) window_size: Option<SizeDef>,
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ValueView {
    /// The value's ID.
    pub(crate) id: String,
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
    /// order they were saved. A capture's handler reads with them on its
    /// own thread, so they are shared and never change.
    values: Values,
    /// Each value's ID, in the same order.
    ids: Vec<String>,
    /// The window size whose values the capture reads, or `None` before a
    /// frame showed another.
    size: Option<Size>,
    /// The size of the window the newest frame showed.
    now: Option<Size>,
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
    /// The window size frames show, settled, while a capture runs.
    settle: Option<Settle>,
    /// What the next capture copies at a settled size where values read,
    /// resolved when the size settled. That size is the overlay's
    /// [`Overlay::size`] from then on, so the capture is for it.
    prepared: Option<Placed>,
}

/// An overlay's values: each one's name, and its check or why it cannot be
/// read.
type Values = Arc<[(String, Result<SavedValue, String>)]>;

/// Every open overlay, by application ID.
#[derive(Debug)]
pub(crate) struct Overlays<S> {
    /// Who its captures read for.
    purpose: Purpose,
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
    /// No overlays, which read for `purpose`.
    pub(crate) const fn new(purpose: Purpose) -> Self {
        Self {
            purpose,
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
        file: impl FnOnce() -> Result<SavedApp, String>,
    ) -> Result<OverlayView, String> {
        if !self.open.contains_key(app) {
            if let Some((message, wait)) = self.failed.get_mut(app) {
                if *wait > 0 {
                    *wait = wait.saturating_sub(1);
                    return Err(message.clone());
                }
                self.failed.remove(app);
            }
            let purpose = self.purpose;
            let opened =
                file().and_then(|saved| open(&saved.file, purpose, |name| saved.picture(name)));
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
        settled(overlay, host.now());
        if overlay.session.is_none() {
            if overlay.wait == 0 {
                match start(host, messages, self.purpose, app, generation, overlay) {
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

    /// Keeps what `app`'s capture with `generation` read at `time`, or why
    /// it stopped. A stopped capture closes, and a poll about a second later
    /// looks for the window again. A window of another size shows no
    /// readings until its size settles; a poll then places the values there.
    pub(crate) fn read(
        &mut self,
        app: &str,
        generation: u64,
        read: Result<FrameRead, String>,
        time: Instant,
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
                overlay.now = overlay.size;
                overlay.latest = Some(values);
                if let (Some(settle), Some(size)) = (&mut overlay.settle, overlay.size) {
                    settle.saw(size, time);
                }
                Ok(())
            },
            Ok(FrameRead::Resized(size)) => {
                overlay.now = Some(size);
                if let Some(settle) = &mut overlay.settle {
                    settle.saw(size, time);
                    // At a settled size where nothing reads, the readings
                    // say why each value does not.
                    if settle.at() != size {
                        overlay.latest = None;
                    }
                }
                Ok(())
            },
            Err(error) => {
                overlay.latest = None;
                overlay.now = None;
                overlay.settle = None;
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

/// The overlay `file` describes, without a capture yet, for `purpose`. A
/// value whose check cannot be built says why in its line, and the others
/// still read.
fn open<S>(
    file: &AppFile,
    purpose: Purpose,
    picture: impl FnMut(&str) -> io::Result<Vec<u8>> + Clone,
) -> Result<Overlay<S>, String> {
    let mode = match purpose {
        Purpose::Overlay => {
            file.overlay
                .filter(|overlay| overlay.enabled)
                .ok_or_else(|| format!("The overlay for {} is off.", file.name))?
                .mode
        },
        Purpose::ValuesPage => file
            .overlay
            .map_or(OverlayModeDef::Overlay, |overlay| overlay.mode),
    };
    let values = file
        .values
        .iter()
        .map(|value| {
            let check = saved::build(
                &value.screen,
                &file.references,
                file.size_changes,
                picture.clone(),
            )
            .map_err(|error| format!("Cannot read: {error}."));
            (value.name.clone(), check)
        })
        .collect();
    Ok(Overlay {
        generation: 0,
        name: file.name.clone(),
        mode,
        values,
        ids: file.values.iter().map(|value| value.id.clone()).collect(),
        size: None,
        now: None,
        window_match: file.window.to_match(),
        session: None,
        window: None,
        latest: None,
        message: None,
        wait: 0,
        placed: None,
        settle: None,
        prepared: None,
    })
}

/// Acts on the size `overlay`'s frames settled at by `now`, once they
/// have. Where some values read, it closes the capture so the poll starts
/// one for them, copying what was resolved here; where none does, the
/// capture goes on and each value says why it does not read there.
fn settled<S: CaptureSession>(overlay: &mut Overlay<S>, now: Instant) {
    let Some(size) = overlay
        .settle
        .as_mut()
        .and_then(|settle| settle.settled(now))
    else {
        return;
    };
    if overlay.size == Some(size) {
        return;
    }
    let placed = place(&overlay.values, size);
    if placed.layout().is_none() {
        overlay.latest = Some(
            placed
                .values()
                .iter()
                .filter_map(|at| at.as_ref().err())
                .map(|unread| ValueRead::NotRead(size, *unread))
                .collect(),
        );
        return;
    }
    overlay.prepared = Some(placed);
    overlay.size = Some(size);
    overlay.latest = None;
    overlay.window = None;
    overlay.wait = 0;
    overlay.settle = None;
    if let Some(session) = overlay.session.take() {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a capture that ends badly is replaced at once, which shows any problem it \
                      had"
        )]
        let _ = session.close();
    }
}

/// Where the values are placed at `size`. It resolves each value with
/// [`SavedValue::at`], as the runtime does, so a state check scaled to too
/// few pixels does not read. A value whose check could not be built is not
/// placed; its line says why.
fn place(values: &[(String, Result<SavedValue, String>)], size: Size) -> Placed {
    Placed::new(
        size,
        values.iter().map(|(_, value)| match value {
            Ok(value) => value.at(size),
            Err(_broken) => Err(Unread::NotDrawn),
        }),
    )
}

/// Finds `overlay`'s window and starts reading it with `generation`.
fn start<H: Host>(
    host: &H,
    messages: &Sender<Message<H>>,
    purpose: Purpose,
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
                .find_map(|(_, check)| check.as_ref().ok().map(|check| check.drawn.window_size))
        })
        .ok_or_else(no_values)?;
    let placed = overlay
        .prepared
        .take()
        .unwrap_or_else(|| place(&overlay.values, size));
    let values = Arc::clone(&overlay.values);
    let layout = placed.layout().cloned().ok_or_else(no_values)?;
    let processes = host.processes().map_err(|error| error.to_string())?;
    let windows = host.windows().map_err(|error| error.to_string())?;
    let window = host
        .read_window(&overlay.window_match, processes, windows)
        .map(|read| read.entry.handle)
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
            purpose,
            app: app.clone(),
            generation,
            read: read_all(&placed, &values, frame),
        });
        OVERLAY_INTERVAL
    });
    let session = host
        .capture(window, Crop::Layout(layout), OVERLAY_INTERVAL, handler)
        .map_err(|error| error.to_string())?;
    overlay.generation = generation;
    overlay.size = Some(size);
    overlay.settle = Some(Settle::new(size));
    overlay.session = Some(session);
    overlay.window = Some(window);
    overlay.latest = None;
    overlay.now = None;
    overlay.message = None;
    Ok(())
}

/// What each of `values` reads on `frame` where `placed` places it. A
/// frame of a window of another size says so, and a value that does not
/// read at the size says why.
fn read_all(
    placed: &Placed,
    values: &[(String, Result<SavedValue, String>)],
    frame: io::Result<Frame<'_>>,
) -> Result<FrameRead, String> {
    let size = placed.size();
    let frame = match placed.meaning(frame).map_err(|error| error.to_string())? {
        FrameMeaning::Resized(other) => return Ok(FrameRead::Resized(other)),
        FrameMeaning::Readable(frame) => Ok(frame),
        FrameMeaning::Unreadable(outside) => Err(outside),
    };
    Ok(FrameRead::Values(
        values
            .iter()
            .zip(placed.values())
            .enumerate()
            .map(|(position, ((_, value), at))| {
                let (value, at) = match (value, at) {
                    (Ok(value), Ok(at)) => (value, at),
                    (Ok(_), Err(unread)) => return ValueRead::NotRead(size, *unread),
                    (Err(_broken), _) => return ValueRead::NotRead(size, Unread::NotDrawn),
                };
                let frame = match &frame {
                    Ok(frame) => frame,
                    Err(outside) => return ValueRead::Unreadable(outside.to_string()),
                };
                placed.cropped(position, frame.image).map_or(
                    ValueRead::NotRead(size, Unread::Outside),
                    |image| match value.check(at).read(&image) {
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
                (Ok(_), Some(ValueRead::NotRead(size, unread))) => {
                    (None, Some(format!("Not read {}.", at_size(*size, *unread))))
                },
                (Ok(_), Some(ValueRead::Unreadable(error))) => {
                    (None, Some(format!("Cannot read: {error}.")))
                },
            };
            ValueView {
                id: overlay.ids.get(index).cloned().unwrap_or_default(),
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
        window_size: overlay.now.map(|size| SizeDef {
            width: size.width,
            height: size.height,
        }),
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
    use appytappy_config::app_file::{ScreenDef, SizeChangesDef, WindowDef};
    use appytappy_sources::screen::mask::Mask;
    use appytappy_sources::screen::settle::SETTLE;
    use appytappy_sources::screen::{Image, NoSession, OutsideWindow, Region};
    use appytappy_sources::system::Bounds;

    use super::*;

    const DRAWN: Size = Size {
        width: 4,
        height: 2,
    };

    /// A state value drawn on the whole of a 4 by 2 window whose one
    /// picture keeps one pixel of eight, so at half the size it keeps none.
    fn sparse() -> SavedValue {
        let icon = Image::new(DRAWN, [0, 0, 255, 255].repeat(8)).expect("matching length");
        let mut kept = vec![false; 8];
        kept[0] = true;
        let mask = Mask::new(DRAWN, kept).expect("keeps one");
        let png = icon.encode_png_masked(&mask).expect("encodes");
        let screen: ScreenDef = serde_json::from_value(serde_json::json!({
            "windowSize": { "width": 4, "height": 2 },
            "region": { "x": 0, "y": 0, "width": 4, "height": 2 },
            "check": {
                "kind": "state",
                "states": [{ "name": "Dot", "image": "dot.png" }],
                "tolerance": 0
            },
            "fixes": []
        }))
        .expect("the file's spelling");
        saved::build(&screen, &[], SizeChangesDef::Often, |_| Ok(png.clone())).expect("builds")
    }

    /// An overlay of `values` whose capture reads at `size`.
    fn reading_at(
        values: Vec<(String, Result<SavedValue, String>)>,
        size: Size,
    ) -> Overlay<NoSession> {
        let window: WindowDef = serde_json::from_value(serde_json::json!({
            "title": "Example App",
            "class": "ExampleWindow",
            "executable": "Example.exe",
            "mustMatch": { "title": false, "class": false },
            "fallback": "none"
        }))
        .expect("the file's spelling");
        Overlay {
            generation: 0,
            name: "Example App".to_owned(),
            mode: OverlayModeDef::Overlay,
            ids: values.iter().map(|_| "dot".to_owned()).collect(),
            values: values.into(),
            size: Some(size),
            now: Some(size),
            window_match: window.to_match(),
            session: None,
            window: Some(7),
            latest: None,
            message: None,
            wait: 0,
            placed: None,
            settle: Some(Settle::new(size)),
            prepared: None,
        }
    }

    #[test]
    fn a_settled_size_where_every_value_is_too_small_says_so_and_keeps_the_capture() {
        let values = vec![("Dot".to_owned(), Ok(sparse()))];
        let half = Size {
            width: 2,
            height: 1,
        };
        let start = Instant::now();
        let later = start.checked_add(SETTLE).expect("in range");
        let mut overlay = reading_at(values, DRAWN);
        overlay.settle.as_mut().expect("settling").saw(half, start);
        settled(&mut overlay, start);
        assert_eq!(overlay.latest, None, "not settled yet");
        settled(&mut overlay, later);
        assert_eq!(
            overlay.latest,
            Some(vec![ValueRead::NotRead(half, Unread::TooSmall)])
        );
        assert_eq!(overlay.size, Some(DRAWN), "the capture goes on");
        assert_eq!(overlay.window, Some(7));
        assert!(overlay.prepared.is_none());
        assert_eq!(
            view(&overlay, false).values[0].problem.as_deref(),
            Some(
                "Not read at 2 × 1, the window's size now: scaled to this window size, its states leave no pixel to \
                 compare."
            )
        );
        // Twice the size, it reads, so the next poll starts a capture with
        // what was resolved here.
        let large = Size {
            width: 8,
            height: 4,
        };
        overlay.settle.as_mut().expect("settling").saw(large, later);
        settled(&mut overlay, later.checked_add(SETTLE).expect("in range"));
        assert_eq!(overlay.size, Some(large));
        assert_eq!((overlay.window, overlay.wait), (None, 0));
        assert!(overlay.settle.is_none());
        let placed = overlay.prepared.as_ref().expect("prepared");
        assert_eq!(
            placed
                .layout()
                .expect("the value reads")
                .copies()
                .iter()
                .map(|copy| copy.from)
                .collect::<Vec<_>>(),
            [Region::new(0, 0, 8, 4).expect("not empty")]
        );
    }

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
        let outside = |window_size| Err(OutsideWindow { window_size }.into());
        // The second value's check could not be built, so it is not placed.
        let values = vec![
            ("Dot".to_owned(), Ok(sparse())),
            ("Broken".to_owned(), Err("no picture".to_owned())),
        ];
        let placed = place(&values, DRAWN);
        assert_eq!(
            read_all(&placed, &values, outside(DRAWN)),
            Ok(FrameRead::Values(vec![
                ValueRead::Unreadable("the region lies outside the 4x2 window".to_owned()),
                ValueRead::NotRead(DRAWN, Unread::NotDrawn),
            ]))
        );
        let wider = Size {
            width: 6,
            height: 2,
        };
        assert_eq!(
            read_all(&placed, &values, outside(wider)),
            Ok(FrameRead::Resized(wider))
        );
        assert_eq!(
            read_all(&placed, &values, Err(io::Error::other("the window closed"))),
            Err("the window closed".to_owned())
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
