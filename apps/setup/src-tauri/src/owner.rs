//! The thread that owns the setup app's state.
//!
//! Tauri runs commands on its own threads, and the capture hands frames over
//! on another. None of them touches the state: each sends a [`Message`] here.
//! A command sends a [`Run`], work that runs on this thread with the state
//! and answers on a oneshot channel. The state is
//! the picked window's capture, its newest frame, the session's screenshots,
//! and the check the user is setting up.
//!
//! This file keeps the state and the thread's loop. The host, the messages
//! and the methods each command calls live in modules below it, and
//! `test_support` holds the stand-ins the tests share. A method that saves
//! says what it changed with [`Owner::after`], which tells the runtime and
//! the overlays only once the save worked.

mod automations;
mod capture;
mod host;
mod messages;
mod overlays;
mod recording;
mod references;
mod settings;
#[cfg(test)]
mod test_support;
mod values;
mod watch;

use std::collections::BTreeMap;
use std::io;
use std::ops::ControlFlow;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::app_file::BuildDef;
use appytappy_sources::screen::{CaptureSession, Image};

use crate::check_spec::RegionCheck;
use crate::overlay::Overlays;
use crate::recording::{Recording, ocr};
use crate::stores::Stores;

pub(crate) use self::host::Host;
use self::messages::CaptureFailure;
pub(crate) use self::messages::{
    AppSummary, Live, Message, Reading, ReferenceSaveSpec, Run, SaveSpec, SavedReferenceId,
    SavedValue,
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
    /// Where the applications', automations' and settings files live, or
    /// why that is unknown.
    stores: Result<Stores, String>,
    /// The setup app's own process, left out of the window list.
    own_process: u32,
    /// A sender for this thread's messages, handed to each capture.
    messages: Sender<Message<H>>,
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
    /// The build each running process runs, read once per process, since
    /// the application list asks again every few seconds.
    builds: BTreeMap<ProcessId, BuildDef>,
}

impl<H: Host> Owner<H> {
    /// An owner with no window picked.
    pub(crate) const fn new(
        host: H,
        stores: Result<Stores, String>,
        own_process: u32,
        messages: Sender<Message<H>>,
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
            builds: BTreeMap::new(),
        }
    }

    /// Applies one message; breaks on [`Message::Shutdown`].
    pub(crate) fn handle(&mut self, message: Message<H>) -> ControlFlow<()> {
        match message {
            Message::Run(run) => run.run(self),
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

    /// Does what follows from a save once `saved` shows that it worked, as
    /// [`Owner::tell`] says. A save that failed before it wrote anything
    /// changed nothing, so nothing follows from it.
    pub(super) fn after<T>(
        &mut self,
        changed: Changed,
        saved: Result<T, String>,
    ) -> Result<T, String> {
        if saved.is_ok() {
            self.tell(changed);
        }
        saved
    }

    /// Does what follows from a change to the saved files: each overlay
    /// reads again what `changed` names, and the runtime reads the saved
    /// files again when it reads what changed.
    pub(super) fn tell(&mut self, changed: Changed) {
        if changed.overlays() {
            // Each overlay reads its values again from their saved files,
            // after keeping where the user just put its panel.
            self.save_panels(true);
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a capture that ended with an error stopped already, and the next poll starts another"
            )]
            let _ = self.overlays.close(None);
        }
        if changed.runtime() {
            self.host.reload_runtime();
        }
    }
}

/// What a save changed, which says who reads it again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Changed {
    /// What only the runtime reads: the settings, the automations, an
    /// application's anti-cheat setting, and the build its values were
    /// checked against.
    Runtime,
    /// An application, its values, or the references they read with, which
    /// the runtime and the overlays read.
    Values,
    /// An application's overlay, which only the overlays read.
    Overlay,
}

impl Changed {
    /// Whether the overlays read it.
    const fn overlays(self) -> bool {
        matches!(self, Self::Values | Self::Overlay)
    }

    /// Whether the runtime reads it.
    const fn runtime(self) -> bool {
        matches!(self, Self::Runtime | Self::Values)
    }
}

/// Runs `owner` until [`Message::Shutdown`] arrives, then stops its capture.
///
/// # Errors
///
/// Returns the error the capture's thread ended with.
pub(crate) fn run<H: Host>(mut owner: Owner<H>, messages: &Receiver<Message<H>>) -> io::Result<()> {
    for message in messages {
        if owner.handle(message).is_break() {
            break;
        }
    }
    owner.shut_down()
}
