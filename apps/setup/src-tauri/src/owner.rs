//! The thread that owns the setup app's state.
//!
//! Tauri runs commands on its own threads, and the capture hands frames over
//! on another. None of them touches the state: each sends a [`Message`] here.
//! A command sends a [`Run`], work that runs on this thread with the state
//! and answers on a oneshot channel. The state is the one capture slot,
//! which holds the picked window's capture with its newest frame, the
//! session's screenshots, what it records, and the check the user is setting
//! up, or the automation editor's watch; the overlays; and what the Debug
//! page reads.
//!
//! This file keeps the state and the thread's loop, and the one way each
//! save of an application reaches the runtime, the overlays and their
//! windows: [`Owner::update_app`]. The host, the messages and the methods
//! each command calls live in modules below it, and `test_support` holds the
//! stand-ins the tests share.

mod automations;
mod capture;
mod clicks;
mod debug;
mod drafts;
mod edits;
mod fixes;
mod found;
mod host;
mod live;
mod looks;
mod messages;
mod overlays;
mod recording;
mod references;
#[cfg(test)]
mod retention;
mod settings;
mod sharing;
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

use appytappy_config::app_file::{AppFile, BuildDef};
use appytappy_config::process::ProcessId;

use crate::check_spec::Pictures;
use crate::debug::log::SetupLog;
use crate::debug::trace_file::TraceTail;
use crate::debug::trace_view::TraceView;
use crate::overlay::{Overlays, Purpose};
use crate::recording::{look, ocr};
use crate::stores::{SavedApp, Stores};
use crate::ticket::{Ticket, Tickets};
use crate::updates::Downloaded;

use self::capture::Capture;
pub(crate) use self::clicks::{ClickPictureView, ClickSent, ClickSpec, FoundClick};
pub(crate) use self::found::FoundView;
pub(crate) use self::host::{Desktop, Host};
pub(crate) use self::messages::{
    AppSummary, AppWindows, EditPreview, EditSpec, Event, FixSpec, Live, Message, Reading,
    RecordSpec, ReferenceSaveSpec, ReferenceTrial, Run, SaveSpec, SavedReferenceId, SavedValue,
    TrialSpec, at_size,
};
pub(crate) use self::sharing::{ImportSpec, SharedView};
use self::watch::Watching;

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
    /// why their folders are unknown.
    stores: Result<Stores, String>,
    /// The setup app's own process, left out of the window list.
    own_process: u32,
    /// A sender for this thread's messages, handed to each capture.
    messages: Sender<Message<H>>,
    /// Hands out the ticket of each capture, recording, and search, so a
    /// message about one the user left is dropped.
    tickets: Tickets,
    /// The one capture the capture screen and the automation editor share,
    /// with what it keeps.
    capture: Capture<H::Session>,
    /// Sends new readings to the text reader's thread.
    ocr: Sender<ocr::Job>,
    /// Where whole-window recordings send the frames to look over.
    looks: Sender<look::Job>,
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
    /// The newest trace file as the Debug page last read it, so the next
    /// read takes only the rows added since.
    trace_tail: Option<TraceTail>,
    /// The trace file the Debug page's metrics and rows read, from its start.
    trace_view: Option<TraceView>,
    /// `setup.log`, open while the settings show debug details.
    setup_log: SetupLog,
}

impl<H: Host> Owner<H> {
    /// An owner with no window picked.
    pub(crate) fn new(
        host: H,
        stores: Result<Stores, String>,
        own_process: u32,
        messages: Sender<Message<H>>,
        ocr: Sender<ocr::Job>,
        looks: Sender<look::Job>,
    ) -> Self {
        Self {
            host,
            own_process,
            messages,
            tickets: Tickets::default(),
            capture: Capture::Idle,
            ocr,
            looks,
            overlays: Overlays::new(Purpose::Overlay),
            live: Overlays::new(Purpose::ValuesPage),
            update: None,
            builds: BTreeMap::new(),
            trace_tail: None,
            trace_view: None,
            setup_log: SetupLog::new(match &stores {
                Ok(stores) => Some(stores.logs.clone()),
                Err(_unknown) => None,
            }),
            stores,
        }
    }

    /// Applies one message; breaks on [`Message::Shutdown`].
    pub(crate) fn handle(&mut self, message: Message<H>) -> ControlFlow<()> {
        match message {
            Message::Run(run) => run.run(self),
            Message::Sent { ticket, event } => {
                if self.is_current(ticket, &event) {
                    self.apply(event);
                }
            },
            Message::PanelPlaced { app, panel } => self.overlays.placed(&app, panel),
            Message::Shutdown => return ControlFlow::Break(()),
        }
        ControlFlow::Continue(())
    }

    /// Whether the work `event` is about still holds `ticket`: the one
    /// guard every late message from a capture or a worker thread passes.
    /// It drops what the picked window's capture sent after the user left
    /// it, what a watch's capture sent after it stopped or started again,
    /// text and looks for a recording or a search that was discarded or
    /// replaced, and readings from an overlay capture that closed.
    fn is_current(&self, ticket: Ticket, event: &Event) -> bool {
        let current = match event {
            Event::Frame { .. } | Event::CaptureFailed(_) => self.capture.picked_ticket(),
            Event::Watch(_) => self.capture.watching().and_then(Watching::ticket),
            Event::Recognized { .. } | Event::Looked { .. } => {
                self.readings().map(|recording| recording.id)
            },
            Event::FoundText { .. } => self.found().map(|found| found.id),
            Event::Overlay { purpose, app, .. } => self.overlays_for(*purpose).ticket(app),
        };
        current == Some(ticket)
    }

    /// Applies `event`, which [`Owner::is_current`] passed.
    fn apply(&mut self, event: Event) {
        match event {
            Event::Frame { image, received } => self.frame(image, received),
            Event::CaptureFailed(failure) => self.capture_failed(failure),
            Event::Watch(seen) => self.watch_sent(seen),
            Event::Recognized { index, text } => {
                if let Some(recording) = self.readings_mut() {
                    recording.read(index, text);
                }
            },
            Event::Looked { at, seen } => self.looked(at, seen),
            Event::FoundText { index, text } => self.found_text(index, text),
            Event::Overlay { purpose, app, read } => {
                let now = self.host.now();
                // The overlay or the Values page shows why its capture
                // stopped, and looks for the window again.
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the capture already failed, and the overlay shows why"
                )]
                let _ = self.overlays_mut(purpose).read(&app, read, now);
            },
        }
    }

    /// The captures that read for `purpose`.
    const fn overlays_for(&self, purpose: Purpose) -> &Overlays<H::Session> {
        match purpose {
            Purpose::Overlay => &self.overlays,
            Purpose::ValuesPage => &self.live,
        }
    }

    /// The captures that read for `purpose`, to change them.
    const fn overlays_mut(&mut self, purpose: Purpose) -> &mut Overlays<H::Session> {
        match purpose {
            Purpose::Overlay => &mut self.overlays,
            Purpose::ValuesPage => &mut self.live,
        }
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
        self.end_capture()
    }

    /// Where the applications', automations' and settings files live, or
    /// why that is unknown, as the user reads it.
    pub(crate) fn stores(&self) -> Result<&Stores, String> {
        known(&self.stores)
    }

    /// Loads the saved application `app`, lets `change` change its file,
    /// and saves it as [`Owner::write_app`] does. A change that keeps the
    /// file as it was writes nothing and tells no one.
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
            Edit::Keep(answer) => Ok(Saved {
                answer,
                windows: None,
            }),
        }
    }

    /// Writes `write` to the folder of the application `app`, then tells who
    /// reads what `changed` names, and opens or closes the overlay windows
    /// to match when `changed` can change which show. Answers why an overlay
    /// window could not open or close, if one could not.
    ///
    /// A save that failed changed nothing anyone reads, so no one is told.
    /// A removal that failed partway may have removed some of the
    /// application's files, so they are read again either way.
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
    /// The file is as it was, so nothing is saved, no one is told and no
    /// picture is removed.
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

/// `stores`, or why their folders are unknown, as the user reads it.
fn known(stores: &Result<Stores, String>) -> Result<&Stores, String> {
    stores
        .as_ref()
        .map_err(|reason| format!("AppyTappy cannot find where to keep your values: {reason}."))
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

    use super::test_support::{Harness, STATE, spec, with_saved_bar};
    use super::*;

    /// The folder of `Example App`, holding the state `Icon visible` with
    /// its picture and two files of the user's own.
    fn folder_with_a_picture(harness: &mut Harness) -> std::path::PathBuf {
        harness
            .ask(|owner| owner.save(&spec("example", "Icon visible", STATE)))
            .expect("saves");
        let folder = harness
            .ask(|owner| owner.stores().map(|stores| stores.apps.folder("example")))
            .expect("stores");
        for mine in ["holiday.png", "notes.txt"] {
            std::fs::write(folder.join(mine), b"mine").expect("writes");
        }
        folder
    }

    /// The names of the files in `folder`, sorted.
    fn names_in(folder: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(folder)
            .expect("lists")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .into_string()
                    .expect("utf-8")
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_save_that_fails_leaves_none_of_the_pictures_it_wrote() {
        let (mut harness, root) = with_saved_bar("failed-save");
        let folder = folder_with_a_picture(&mut harness);
        harness.forget_reloads();
        let failed = harness.ask(|owner| {
            owner.update_app("example", Changed::Values, |_owner, found| {
                found.file.name = "Renamed".to_owned();
                // The second picture cannot be written, so the save stops
                // after it wrote the first and before it writes the file.
                Ok(Edit::Save(
                    (),
                    vec![
                        ("status-1.png".to_owned(), vec![1]),
                        ("Not A Name.png".to_owned(), vec![2]),
                    ],
                ))
            })
        });
        assert_eq!(
            failed,
            Err("\"Not A Name.png\" is not a valid picture name".to_owned())
        );
        assert_eq!(
            names_in(&folder),
            ["app.json", "holiday.png", "icon_visible-1.png", "notes.txt"],
            "the picture it wrote is gone, and nothing else is"
        );
        let file = harness
            .ask(|owner| owner.stores()?.app("example"))
            .expect("saved")
            .file;
        assert_eq!(file.name, "Example App", "kept as saved");
        assert_eq!(harness.reloads(), 0, "no one is told");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_save_removes_the_pictures_an_interrupted_save_left() {
        let (mut harness, root) = with_saved_bar("interrupted-save");
        let folder = folder_with_a_picture(&mut harness);
        // The program ended after a save wrote a value's, a fix's and a
        // reference's pictures and before it wrote the file that names them.
        for left in ["status-1.png", "icon_visible-1-1.png", "ref-digits-1.png"] {
            std::fs::write(folder.join(left), b"left").expect("writes");
        }
        harness
            .ask(|owner| owner.set_anti_cheat("example", AntiCheatDef::None))
            .expect("the next save of the application");
        assert_eq!(
            names_in(&folder),
            ["app.json", "holiday.png", "icon_visible-1.png", "notes.txt"]
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

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
                owner.update_app("example", Changed::Applications, |_owner, _found| {
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
    fn a_removal_that_fails_still_tells_who_reads_the_files() {
        let (mut harness, root) = with_saved_bar("changed-removal");
        harness
            .ask(|owner| owner.set_overlay("example", true, OverlayModeDef::Overlay))
            .expect("saves");
        harness
            .ask(|owner| owner.overlay_state("example"))
            .expect("shows");
        harness.forget_reloads();
        harness.synced.try_iter().for_each(drop);
        // Part of the folder may be gone, so the runtime and the overlays
        // read again whether or not the removal finished.
        assert!(
            harness
                .ask(|owner| owner.write_app("Not An Id", Changed::Applications, Write::Remove))
                .is_err()
        );
        assert_eq!(harness.reloads(), 1, "the runtime reads again");
        assert!(harness.closed.try_recv().is_ok(), "the overlay reads again");
        assert!(
            harness.synced.try_recv().is_err(),
            "the windows stay until a removal is done"
        );
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
