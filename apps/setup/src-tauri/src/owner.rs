//! The thread that owns the setup app's state.
//!
//! Tauri runs commands on its own threads, and the capture hands frames over
//! on another. None of them touches the state: each sends a [`Message`] here.
//! A command sends a [`Run`], work that runs on this thread with the state
//! and answers on a oneshot channel. The state is
//! the picked window's capture, its newest frame, the session's screenshots,
//! and the check the user is setting up.
//!
//! This file keeps the state and the thread's loop, and the one way each
//! save of an application reaches the runtime, the overlays and their
//! windows: [`Owner::update_app`]. The host, the messages and the methods
//! each command calls live in modules below it, and `test_support` holds the
//! stand-ins the tests share.

mod capture;
mod clicks;
mod fixes;
mod host;
mod messages;
mod overlays;
mod recording;
mod references;
mod settings;
#[cfg(test)]
mod test_support;
mod update;
mod values;
mod watch;

use std::collections::BTreeMap;
use std::io;
use std::ops::ControlFlow;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::app_file::{AppFile, BuildDef};
use appytappy_sources::screen::{CaptureSession, Image};

use crate::check_spec::{Pictures, RegionCheck};
use crate::overlay::{Overlays, Purpose};
use crate::recording::{Recording, ocr};
use crate::stores::{SavedApp, Stores};
use crate::updates::Downloaded;

pub(crate) use self::clicks::{ClickPictureView, FoundClick};
pub(crate) use self::host::{Desktop, Host};
use self::messages::CaptureFailure;
pub(crate) use self::messages::{
    AppSummary, FixSpec, Live, Message, Reading, ReferenceSaveSpec, Run, SaveSpec,
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
    /// The Values page's readings of the application it shows, with a
    /// capture of their own.
    live: Overlays<H::Session>,
    /// The update downloaded and checked, until the user installs it.
    update: Option<Downloaded>,
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
            overlays: Overlays::new(Purpose::Overlay),
            live: Overlays::new(Purpose::ValuesPage),
            update: None,
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
                        Ok(seen) => self.watch_seen(seen),
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
                purpose,
                app,
                generation,
                read,
            } => {
                let overlays = match purpose {
                    Purpose::Overlay => &mut self.overlays,
                    Purpose::ValuesPage => &mut self.live,
                };
                // The overlay or the Values page shows why its capture
                // stopped, and looks for the window again.
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the capture already failed, and the overlay shows why"
                )]
                let _ = overlays.read(&app, generation, read, self.host.now());
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
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the Values page showed its capture's error already, and it is closing"
        )]
        let _ = self.live.close(None);
        self.close()
    }

    /// Where the applications', automations' and settings files live, or
    /// why that is unknown.
    pub(crate) fn stores(&self) -> Result<&Stores, String> {
        self.stores.as_ref().map_err(Clone::clone)
    }

    /// Loads the saved application `app`, lets `change` change its file,
    /// and saves it as [`Owner::write_app`] does. A change that keeps the
    /// file as it was saves nothing and tells no one, but the pictures the
    /// file no longer names, which an earlier save left, are still removed.
    pub(super) fn update_app<T>(
        &mut self,
        app: &str,
        changed: Changed,
        change: impl FnOnce(&mut Self, &mut SavedApp) -> Result<Edit<T>, String>,
    ) -> Result<Saved<T>, String> {
        let mut saved = self.stores()?.app(app)?;
        match change(self, &mut saved)? {
            Edit::Save(answer, pictures) => {
                let windows = self.write_app(app, changed, Write::Save(&saved.file, &pictures))?;
                Ok(Saved { answer, windows })
            },
            Edit::Keep(answer) => {
                self.prune(app, &saved.file)?;
                Ok(Saved {
                    answer,
                    windows: None,
                })
            },
        }
    }

    /// Writes `write` to the folder of the application `app`, then tells who
    /// reads what `changed` names, opens or closes the overlay windows to
    /// match when `changed` can change which show, and removes the pictures
    /// a saved file no longer names. Answers why an overlay window could not
    /// open or close, if one could not.
    ///
    /// A save that failed changed nothing, so no one is told. A removal that
    /// failed partway may have removed some of the application's files, so
    /// they are read again either way. No one reads a picture the file no
    /// longer names, so a failure to remove one, which the answer gives,
    /// changes nothing they read.
    pub(super) fn write_app(
        &mut self,
        app: &str,
        changed: Changed,
        write: Write<'_>,
    ) -> Result<Option<String>, String> {
        let apps = &self.stores()?.apps;
        let written = match write {
            Write::Save(file, pictures) => apps.save(app, file, pictures),
            Write::Remove => apps.remove(app),
        }
        .map_err(|error| error.to_string());
        if written.is_ok() || matches!(write, Write::Remove) {
            self.tell(changed);
        }
        written?;
        let windows = changed
            .windows()
            .then(|| self.sync_overlay_windows().err())
            .flatten();
        if let Write::Save(file, _) = write {
            self.prune(app, file)?;
        }
        Ok(windows)
    }

    /// Saves what only the runtime reads, the settings or the automations,
    /// with `save`, and has the runtime read the saved files again once it
    /// worked.
    pub(crate) fn update_runtime<T>(
        &mut self,
        save: impl FnOnce(&Stores) -> Result<T, String>,
    ) -> Result<T, String> {
        let saved = self.stores().and_then(save)?;
        self.tell(Changed::Runtime);
        Ok(saved)
    }

    /// Removes the pictures of the application `app` that `file` no longer
    /// names.
    fn prune(&self, app: &str, file: &AppFile) -> Result<(), String> {
        self.stores()?
            .apps
            .prune(app, file)
            .map_err(|error| error.to_string())
    }

    /// Does what follows from a change to the saved files: each overlay
    /// reads again what `changed` names, and the runtime reads the saved
    /// files again when it reads what changed.
    fn tell(&mut self, changed: Changed) {
        if changed.overlays() {
            // Each overlay reads its values again from their saved files,
            // after keeping where the user just put its panel.
            self.save_panels(true);
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a capture that ended with an error stopped already, and the next poll starts another"
            )]
            let _ = self.overlays.close(None);
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a capture that ended with an error stopped already, and the next poll starts another"
            )]
            let _ = self.live.close(None);
        }
        if changed.runtime() {
            self.host.reload_runtime();
        }
    }
}

/// What a change to an application's file answers, and whether it saves the
/// file.
#[derive(Debug)]
pub(super) enum Edit<T> {
    /// The file changed: it is saved, with these new pictures.
    Save(T, Pictures),
    /// The file is as it was, so nothing is saved.
    Keep(T),
}

/// What [`Owner::write_app`] writes to an application's folder.
#[derive(Debug, Clone, Copy)]
pub(super) enum Write<'a> {
    /// The application's file, with its new pictures.
    Save(&'a AppFile, &'a Pictures),
    /// The removal of the whole folder, with everything in it.
    Remove,
}

/// What a saved change answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Saved<T> {
    /// What the change answered.
    pub(super) answer: T,
    /// Why an overlay window could not open or close, if one could not.
    pub(super) windows: Option<String>,
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
    /// An application's overlay, which the overlays and their windows read.
    Overlay,
    /// Which applications are saved, as when one is removed, which the
    /// runtime, the overlays and their windows read.
    Applications,
    /// Where the user put an overlay's panel, which its window reads only
    /// when it opens, so no one reads it again.
    Panel,
}

impl Changed {
    /// Whether the overlays read it.
    const fn overlays(self) -> bool {
        matches!(self, Self::Values | Self::Overlay | Self::Applications)
    }

    /// Whether the runtime reads it.
    const fn runtime(self) -> bool {
        matches!(self, Self::Runtime | Self::Values | Self::Applications)
    }

    /// Whether it can change which overlay windows show.
    const fn windows(self) -> bool {
        matches!(self, Self::Overlay | Self::Applications)
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

#[cfg(test)]
mod tests {
    use appytappy_config::app_file::{AntiCheatDef, OverlayModeDef};

    use crate::overlay::OverlayWindow;

    use super::test_support::with_saved_bar;
    use super::*;

    #[test]
    fn each_change_reaches_who_reads_it_once_it_is_saved() {
        let (mut harness, root) = with_saved_bar("changed");
        harness
            .ask(|owner| owner.set_overlay("example", true, OverlayModeDef::Overlay))
            .expect("saves");
        for (number, (changed, runtime, overlays, windows)) in [
            (Changed::Runtime, true, false, false),
            (Changed::Values, true, true, false),
            (Changed::Overlay, false, true, true),
            (Changed::Applications, true, true, true),
            (Changed::Panel, false, false, false),
        ]
        .into_iter()
        .enumerate()
        {
            // The overlay reads, with a capture of its own.
            harness
                .ask(|owner| owner.overlay_state("example"))
                .expect("shows");
            harness.handlers.try_iter().for_each(drop);
            harness.forget_reloads();
            harness.synced.try_iter().for_each(drop);
            let name = format!("Example App {number}");
            let saved = harness.ask(|owner| {
                owner.update_app("example", changed, |_owner, found| {
                    found.file.name.clone_from(&name);
                    Ok(Edit::Save(number, Vec::new()))
                })
            });
            assert_eq!(
                saved,
                Ok(Saved {
                    answer: number,
                    windows: None
                }),
                "{changed:?}"
            );
            let file = harness
                .ask(|owner| owner.stores()?.app("example"))
                .expect("saved")
                .file;
            assert_eq!(file.name, name, "{changed:?} is saved");
            assert_eq!(
                harness.reloads() == 1,
                runtime,
                "{changed:?} and the runtime"
            );
            assert_eq!(
                harness.closed.try_recv().is_ok(),
                overlays,
                "{changed:?} and the overlays"
            );
            assert_eq!(
                harness.synced.try_recv().is_ok(),
                windows,
                "{changed:?} and the overlay windows"
            );
        }

        // A change that keeps the file, or fails, saves nothing and tells
        // no one.
        assert_eq!(
            harness.ask(|owner| {
                owner.update_app("example", Changed::Applications, |_owner, found| {
                    found.file.anti_cheat = AntiCheatDef::None;
                    Ok(Edit::Keep(()))
                })
            }),
            Ok(Saved {
                answer: (),
                windows: None
            })
        );
        assert!(
            harness
                .ask(|owner| {
                    owner.update_app("example", Changed::Applications, |_owner, found| {
                        found.file.anti_cheat = AntiCheatDef::None;
                        // A file that fails its own checks is not saved.
                        found.file.name.clear();
                        Ok(Edit::Save((), Vec::new()))
                    })
                })
                .is_err()
        );
        let file = harness
            .ask(|owner| owner.stores()?.app("example"))
            .expect("saved")
            .file;
        assert_eq!(file.anti_cheat, AntiCheatDef::Unknown, "kept as saved");
        assert_eq!(harness.reloads(), 0);
        assert!(harness.closed.try_recv().is_err());
        assert!(harness.synced.try_recv().is_err());
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_window_that_cannot_open_or_close_leaves_the_change_saved_and_says_why() {
        let (mut harness, root) = with_saved_bar("changed-windows");
        let problem = Some("The overlay of example could not close.".to_owned());
        harness.owner.host.unsynced.clone_from(&problem);
        assert_eq!(
            harness.ask(|owner| owner.set_overlay("example", true, OverlayModeDef::Overlay)),
            Ok(problem.clone())
        );
        assert_eq!(
            harness.synced.try_recv(),
            Ok(vec![OverlayWindow {
                app: "example".to_owned(),
                name: "Example App".to_owned(),
                mode: OverlayModeDef::Overlay,
                panel: None,
            }]),
            "the overlay that is on now"
        );
        assert_eq!(
            harness.ask(|owner| owner.remove_app("example")),
            Ok(problem)
        );
        assert_eq!(harness.synced.try_recv(), Ok(Vec::new()), "none is on");
        assert_eq!(
            harness.ask(|owner| owner.stores()?.app("example").map(drop)),
            Err("That application is no longer saved.".to_owned())
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
