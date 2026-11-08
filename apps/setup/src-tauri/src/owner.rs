//! The thread that owns the setup app's state.
//!
//! Tauri runs commands on its own threads, and the capture hands frames over
//! on another. None of them touches the state: each sends a [`Message`] here,
//! and a request carries a oneshot channel the owner answers on. The state is
//! the picked window's capture, its newest frame, the session's screenshots,
//! and the check the user is setting up.
//!
//! This file keeps the state, the thread's loop and the request dispatch. The
//! host, the messages and the work behind each kind of request live in modules
//! below it, and `test_support` holds the stand-ins the tests share.

mod capture;
mod host;
mod messages;
mod overlays;
mod recording;
mod references;
#[cfg(test)]
mod test_support;
mod values;
mod watch;

use std::io;
use std::ops::ControlFlow;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use appytappy_sources::screen::{CaptureSession, Image};

use crate::applications;
use crate::automations::{self, Stores};
use crate::check_spec::{self, RegionCheck};
use crate::ocr;
use crate::overlay::Overlays;
use crate::pixels;
use crate::recording::Recording;
use crate::selection::{self, SelectionView};
use crate::window_list;

pub(crate) use self::host::Host;
use self::messages::CaptureFailure;
pub(crate) use self::messages::{
    AppSummary, Live, Message, Reading, ReferenceSaveSpec, Reply, Request, SaveSpec,
    SavedReferenceId, SavedValue,
};
use self::watch::Watched;

/// The shortest time between two frames the preview takes: about 30 a
/// second, smooth to the eye at a fraction of an application's frame rate.
const PREVIEW_INTERVAL: Duration = Duration::from_millis(33);

/// The shortest time between two frames the automation editor's live reading
/// takes: often enough for a reading it shows about twice a second.
const WATCH_INTERVAL: Duration = Duration::from_millis(250);

/// The setup app's state.
#[derive(Debug)]
pub(crate) struct Owner<H: Host> {
    host: H,
    /// Where the applications' and automations' files live, or why that is
    /// unknown.
    stores: Result<Stores, String>,
    /// The setup app's own process, left out of the window list.
    own_process: u32,
    /// A sender for this thread's messages, handed to each capture.
    messages: Sender<Message>,
    /// Counts captures, so a frame from a window the user left is dropped.
    generation: u64,
    session: Option<H::Session>,
    latest: Option<Image>,
    /// Why the picked window's capture failed, until a good frame arrives.
    failure: Option<CaptureFailure>,
    screenshots: Vec<Image>,
    check: Option<RegionCheck>,
    /// The region the user records, with the readings it showed.
    recording: Option<Recording>,
    /// Counts recordings, so text read for one the user discarded or
    /// replaced is dropped.
    recording_id: u64,
    /// Sends new readings to the text reader's thread.
    ocr: Sender<ocr::Job>,
    /// The saved value the automation editor watches.
    watched: Option<Watched>,
    /// The overlays that show, each with a capture of its own.
    overlays: Overlays<H::Session>,
}

impl<H: Host> Owner<H> {
    /// An owner with no window picked.
    pub(crate) const fn new(
        host: H,
        stores: Result<Stores, String>,
        own_process: u32,
        messages: Sender<Message>,
        ocr: Sender<ocr::Job>,
    ) -> Self {
        Self {
            host,
            stores,
            own_process,
            messages,
            generation: 0,
            session: None,
            latest: None,
            failure: None,
            screenshots: Vec::new(),
            check: None,
            recording: None,
            recording_id: 0,
            ocr,
            watched: None,
            overlays: Overlays::new(),
        }
    }

    /// Applies one message; breaks on [`Message::Shutdown`].
    pub(crate) fn handle(&mut self, message: Message) -> ControlFlow<()> {
        match message {
            Message::Request(request) => self.answer(request),
            Message::Frame {
                generation,
                image,
                received,
            } => {
                if generation == self.generation {
                    self.frame(image, received);
                }
            },
            Message::Recognized {
                recording,
                index,
                text,
            } => {
                if let Some(current) = self
                    .recording
                    .as_mut()
                    .filter(|current| current.id == recording)
                {
                    current.read(index, text);
                }
            },
            Message::CaptureFailed {
                generation,
                failure,
            } => {
                if generation == self.generation {
                    self.capture_failed(failure);
                }
            },
            Message::Watch { generation, seen } => {
                if let Some(watched) = self
                    .watched
                    .as_mut()
                    .filter(|_| generation == self.generation)
                {
                    match seen {
                        Ok(seen) => watched.seen = Some(seen),
                        Err(error) => {
                            watched.failure = Some(error);
                            // The editor watches again, so nothing needs this
                            // capture, even if it would go on.
                            #[expect(
                                clippy::let_underscore_must_use,
                                reason = "the capture already failed, and the editor shows why"
                            )]
                            let _ = self.close();
                        },
                    }
                }
            },
            Message::Overlay {
                app,
                generation,
                read,
            } => {
                // The overlay shows why its capture stopped, and looks for
                // the window again.
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the capture already failed, and the overlay shows why"
                )]
                let _ = self.overlays.read(&app, generation, read);
            },
            Message::PanelPlaced { app, panel } => self.overlays.placed(&app, panel),
            Message::Shutdown => return ControlFlow::Break(()),
        }
        ControlFlow::Continue(())
    }

    /// Stops the capture, if one runs.
    ///
    /// # Errors
    ///
    /// Returns the error the capture's thread ended with.
    pub(crate) fn close(&mut self) -> io::Result<()> {
        self.session.take().map_or(Ok(()), CaptureSession::close)
    }

    /// Saves where each panel is, closes the overlays, and stops the
    /// capture, as the app ends.
    ///
    /// # Errors
    ///
    /// Returns the error the capture's thread ended with. An overlay's
    /// capture that ended with an error already showed it.
    pub(crate) fn shut_down(&mut self) -> io::Result<()> {
        self.save_panels(true);
        #[expect(
            clippy::let_underscore_must_use,
            reason = "an overlay's capture error already showed in its window, which is closing"
        )]
        let _ = self.overlays.close(None);
        self.close()
    }

    fn answer(&mut self, request: Request) {
        let saves = request.saves();
        if request.changes_values() {
            // Each overlay reads its values again, from their saved files,
            // after keeping where the user just put its panel.
            self.save_panels(true);
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a capture that ended with an error stopped already, and the next poll starts another"
            )]
            let _ = self.overlays.close(None);
        }
        match request {
            Request::ListWindows(reply) => send(reply, self.window_list()),
            Request::Pick { window, reply } => send(reply, self.pick(window)),
            Request::Preview { max_width, reply } => send(
                reply,
                match (&self.failure, &self.latest) {
                    (Some(failure), _) => Err(failure.error.clone()),
                    (None, latest) => {
                        Ok(latest.as_ref().map(|image| pixels::rgba(image, max_width)))
                    },
                },
            ),
            Request::Screenshot(reply) => send(reply, self.screenshot()),
            Request::ScreenshotPixels {
                index,
                max_width,
                reply,
            } => send(
                reply,
                self.screenshots
                    .get(index)
                    .map(|image| pixels::rgba(image, max_width)),
            ),
            Request::Select {
                screenshot,
                selection,
                reply,
            } => send(
                reply,
                self.screenshots
                    .get(screenshot)
                    .map(|image| {
                        selection::select(image, &selection)
                            .as_ref()
                            .map(SelectionView::from)
                    })
                    .ok_or_else(|| check_spec::SpecError::NoScreenshot.to_string()),
            ),
            Request::SetCheck { spec, reply } => send(
                reply,
                self.with_session(|session| check_spec::build(&spec, session))
                    .map(|check| self.check = Some(check))
                    .map_err(|error| error.to_string()),
            ),
            Request::Stop(reply) => {
                self.stop();
                send(reply, ());
            },
            Request::Reading(reply) => send(reply, self.reading()),
            Request::StartRecording {
                region,
                ink,
                tolerance,
                screenshot,
                more,
                reply,
            } => send(
                reply,
                self.start_recording(region, ink, tolerance, screenshot, more),
            ),
            Request::StopRecording(reply) => {
                if let Some(recording) = &mut self.recording {
                    recording.stop(Instant::now());
                }
                send(reply, ());
            },
            Request::DiscardRecording(reply) => {
                self.recording = None;
                send(reply, ());
            },
            Request::RecordingState(reply) => send(
                reply,
                self.recording
                    .as_ref()
                    .map(|recording| recording.view(Instant::now())),
            ),
            Request::RecordedPixels {
                index,
                max_width,
                reply,
            } => send(
                reply,
                self.recording
                    .as_ref()
                    .and_then(|recording| recording.readings().get(index))
                    .map(|kept| pixels::rgba(&kept.image, max_width)),
            ),
            Request::ReviewProblems { spec, reply } => send(
                reply,
                self.with_session(|session| check_spec::recorded_problems(&spec, session))
                    .map_err(|error| error.to_string()),
            ),
            Request::WatchValue { app, value, reply } => send(reply, self.watch(&app, &value)),
            Request::WatchedReading { watch, when, reply } => {
                send(reply, self.watched_reading(watch, when.as_deref()));
            },
            Request::StopWatching { watch, reply } => {
                if self.watched.is_some() && watch == self.generation {
                    self.stop();
                }
                send(reply, ());
            },
            Request::MatchWindows { window, reply } => send(
                reply,
                self.window_list()
                    .map(|list| window_list::matching(&list.windows, &window)),
            ),
            Request::ListApps(reply) => send(reply, self.apps()),
            Request::SaveApp {
                existing,
                spec,
                reply,
            } => send(
                reply,
                self.stores()
                    .and_then(|stores| applications::save(stores, existing.as_deref(), spec)),
            ),
            Request::RemoveApp { app, reply } => send(
                reply,
                self.stores()
                    .and_then(|stores| applications::remove(stores, &app)),
            ),
            Request::SaveValue { spec, reply } => send(reply, self.save(&spec)),
            Request::SaveReference { spec, reply } => send(reply, self.save_reference(&spec)),
            Request::DeleteReference {
                app,
                reference,
                reply,
            } => send(reply, self.delete_reference(&app, &reference)),
            Request::DeleteValue { app, value, reply } => {
                send(reply, self.delete(&app, &value));
            },
            Request::SetAntiCheat { app, class, reply } => {
                send(reply, self.set_anti_cheat(&app, class));
            },
            Request::ListAutomations(reply) => {
                send(reply, self.stores().and_then(automations::list));
            },
            Request::SaveAutomation {
                existing,
                automation,
                reply,
            } => send(
                reply,
                self.stores()
                    .and_then(|stores| automations::save(stores, existing.as_deref(), *automation)),
            ),
            Request::SetAutomationEnabled { id, enabled, reply } => send(
                reply,
                self.stores()
                    .and_then(|stores| automations::set_enabled(stores, &id, enabled)),
            ),
            Request::DeleteAutomation { id, reply } => send(
                reply,
                self.stores()
                    .and_then(|stores| automations::delete(stores, &id)),
            ),
            Request::Overlays(reply) => send(reply, self.overlay_windows()),
            Request::SetOverlay {
                app,
                enabled,
                mode,
                reply,
            } => send(reply, self.set_overlay(&app, enabled, mode)),
            Request::OverlayState { app, reply } => {
                let view = self.overlay_view(&app);
                self.save_panels(false);
                send(reply, view);
            },
            Request::OverlayPlace { app, reply } => {
                send(reply, self.overlays.place(&self.host, &app));
            },
            Request::OverlayCard { app, card, reply } => {
                self.overlays.card(&app, card);
                send(reply, ());
            },
            Request::CloseOverlay { app, mode, reply } => {
                if self.overlays.mode(&app) == Some(mode) {
                    self.save_panels(true);
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the overlay's window closed, so nothing shows the capture's error"
                    )]
                    let _ = self.overlays.close(Some(&app));
                }
                send(reply, ());
            },
        }
        if saves {
            tell_runtime();
        }
    }
}

/// Runs `owner` until [`Message::Shutdown`] arrives, then stops its capture.
///
/// # Errors
///
/// Returns the error the capture's thread ended with.
pub(crate) fn run<H: Host>(mut owner: Owner<H>, messages: &Receiver<Message>) -> io::Result<()> {
    for message in messages {
        if owner.handle(message).is_break() {
            break;
        }
    }
    owner.shut_down()
}

/// Asks the runtime to read the saved automations again. A runtime that is
/// not running reads them when it starts, and one that cannot be reached
/// keeps what it read, so a failure here changes nothing the user did.
#[mutants::skip]
fn tell_runtime() {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the save already happened; the runtime picks it up when it next starts"
    )]
    let _ = appytappy_platform::request_reload(appytappy_platform::RELOAD);
}

/// Answers a request. The command that asked may have been dropped, such as
/// when the window closed while it waited, and then nobody needs the answer.
fn send<T>(reply: Reply<T>, answer: T) {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a dropped command needs no answer"
    )]
    let _ = reply.send(answer);
}
