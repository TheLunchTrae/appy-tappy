//! The overlays: which windows show one, turning one on or off, what each
//! shows, and where the user put its panels.

use appytappy_config::app_file::{OverlayDef, OverlayModeDef, PanelDef};

use crate::overlay::{self, Card, OverlayView, OverlayWindow, Place};

use super::host::Host;
use super::{Changed, Edit, Owner, known};

impl<H: Host> Owner<H> {
    /// The overlays that are on, for their windows to open.
    fn overlay_windows(&self) -> Result<Vec<OverlayWindow>, String> {
        let loaded = self
            .stores()?
            .apps
            .load()
            .map_err(|error| error.to_string())?;
        Ok(overlay::windows(loaded.iter().filter_map(|stored| {
            Some((stored.id.as_str(), stored.file.as_ref().ok()?))
        })))
    }

    /// Opens a window for each overlay that is on, and closes the rest.
    ///
    /// # Errors
    ///
    /// Returns why the applications cannot be read, or why a window could
    /// not open or close.
    pub(crate) fn sync_overlay_windows(&self) -> Result<(), String> {
        let wanted = self.overlay_windows()?;
        self.host.sync_overlay_windows(&wanted)
    }

    /// Turns the overlay of the application `app` on or off, shown as
    /// `mode`, and opens or closes its window to match. The setting is saved
    /// once it answers `Ok`, which holds why an overlay window could not
    /// open or close, if one could not.
    pub(crate) fn set_overlay(
        &mut self,
        app: &str,
        enabled: bool,
        mode: OverlayModeDef,
    ) -> Result<Option<String>, String> {
        self.update_app(app, Changed::Overlay, |_owner, found| {
            let file = &mut found.file;
            let panel = file.overlay.and_then(|overlay| overlay.panel);
            file.overlay = Some(OverlayDef {
                enabled,
                mode,
                panel,
            });
            Ok(Edit::Save((), Vec::new()))
        })
        .map(|saved| saved.windows)
    }

    /// What the overlay of the application `app` shows now, keeping where
    /// the user put its panels once they settled.
    pub(crate) fn overlay_state(&mut self, app: &str) -> Result<OverlayView, String> {
        let view = self.overlay_view(app);
        self.save_panels(false);
        view
    }

    /// Where the overlay window of the application `app` goes, or `None`
    /// while its window is not showing.
    pub(crate) fn overlay_place(&self, app: &str) -> Option<Place> {
        self.overlays.place(&self.host, app)
    }

    /// Keeps the size the page measured for the card of the application
    /// `app`'s overlay, which placing its window uses.
    pub(crate) fn overlay_card(&mut self, app: &str, card: Card) {
        self.overlays.card(app, card);
    }

    /// Closes the overlay of the application `app` that shows as `mode`,
    /// keeping where the user put its panels. An overlay shown another way
    /// since stays open.
    pub(crate) fn close_overlay(&mut self, app: &str, mode: OverlayModeDef) {
        if self.overlays.mode(app) == Some(mode) {
            self.save_panels(true);
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the overlay's window closed, so nothing shows the capture's error"
            )]
            let _ = self.overlays.close(Some(app));
        }
    }

    /// What the values of the application `app` read now, for the Values
    /// page, whether its overlay is on or not. The first call starts a
    /// capture of its window, which [`Owner::stop_live_values`] stops.
    pub(crate) fn live_values(&mut self, app: &str) -> Result<OverlayView, String> {
        let stores = known(&self.stores);
        self.live
            .view(&self.host, &self.messages, app, || stores?.app(app))
    }

    /// Stops reading the values of the application `app` for the Values
    /// page, or of every application with `None`.
    pub(crate) fn stop_live_values(&mut self, app: Option<&str>) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the Values page no longer shows the readings, so nothing shows the error"
        )]
        let _ = self.live.close(app);
    }

    pub(super) fn overlay_view(&mut self, app: &str) -> Result<OverlayView, String> {
        let stores = known(&self.stores);
        self.overlays
            .view(&self.host, &self.messages, app, || stores?.app(app))
    }

    /// Saves where the user put each panel, once it settled or, with `all`,
    /// right away. A place that cannot be saved is lost, and the panel opens
    /// where it last was saved.
    pub(super) fn save_panels(&mut self, all: bool) {
        for (app, panel) in self.overlays.settled(all) {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a panel's place is a convenience; the panel still shows"
            )]
            let _ = self.save_panel(&app, panel);
        }
    }

    /// Saves `panel` as where the user put the panel of the application
    /// `app`'s overlay, unless it has none.
    fn save_panel(&mut self, app: &str, panel: PanelDef) -> Result<(), String> {
        self.update_app(app, Changed::Panel, |_owner, found| {
            let Some(overlay) = &mut found.file.overlay else {
                return Ok(Edit::Keep(()));
            };
            overlay.panel = Some(panel);
            Ok(Edit::Save((), Vec::new()))
        })
        .map(|saved| saved.answer)
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use appytappy_config::app_file::{AppFile, SizeChangesDef, SizeDef};
    use appytappy_sources::screen::settle::SETTLE;
    use appytappy_sources::screen::{Crop, FrameHandler, Image, OutsideWindow, Region, Size};

    use crate::overlay::{
        Card, FrameRead, OVERLAY_INTERVAL, Place, RETRY_POLLS, SETTLE_POLLS, ValueRead, ValueView,
    };
    use crate::owner::messages::{Message, Reading};
    use appytappy_config::app_file::AntiCheatDef;

    use crate::owner::test_support::{
        BAR, FakeHost, Harness, WINDOW, frame_image, spec, with_saved_bar,
    };

    use super::*;

    fn overlay_state(owner: &mut Owner<FakeHost>) -> Result<OverlayView, String> {
        owner.overlay_state("example")
    }

    fn overlay_place(owner: &mut Owner<FakeHost>) -> Option<Place> {
        owner.overlay_place("example")
    }

    fn set_overlay(
        enabled: bool,
        mode: OverlayModeDef,
    ) -> impl FnOnce(&mut Owner<FakeHost>) -> Result<Option<String>, String> {
        move |owner: &mut Owner<FakeHost>| owner.set_overlay("example", enabled, mode)
    }

    fn close_overlay(mode: OverlayModeDef) -> impl FnOnce(&mut Owner<FakeHost>) {
        move |owner: &mut Owner<FakeHost>| owner.close_overlay("example", mode)
    }

    /// The overlay's view of `Progress bar` reading `reading` or failing
    /// with `problem`.
    fn bar_view(
        mode: OverlayModeDef,
        reading: Option<Reading>,
        problem: Option<&str>,
    ) -> OverlayView {
        // A reading or a problem comes from a frame of the window.
        let framed = reading.is_some() || problem.is_some();
        OverlayView {
            name: "Example App".to_owned(),
            mode,
            message: None,
            window_size: framed.then_some(SizeDef {
                width: WINDOW.width,
                height: WINDOW.height,
            }),
            values: vec![ValueView {
                id: "progress_bar".to_owned(),
                name: "Progress bar".to_owned(),
                reading,
                problem: problem.map(str::to_owned),
            }],
        }
    }

    /// Polls the overlay once and returns its capture's handler, checking it
    /// copies only `crop` of window 7.
    fn overlay_capture_of(harness: &mut Harness, crop: Region) -> FrameHandler {
        harness.ask(overlay_state).expect("shows");
        let started = harness.handlers.try_recv().expect("capture started");
        let copied = match &started.crop {
            Crop::Layout(layout) => layout.copies().iter().map(|copy| copy.from).collect(),
            Crop::Whole => Vec::new(),
        };
        assert_eq!(
            (started.window, copied, started.interval),
            (7, vec![crop], OVERLAY_INTERVAL)
        );
        started.handler
    }

    /// Polls the overlay once and returns its capture's handler, checking it
    /// captures only the part of window 7 that holds the bar.
    fn overlay_capture(harness: &mut Harness) -> FrameHandler {
        overlay_capture_of(harness, Region::new(0, 0, 4, 2).expect("not empty"))
    }

    /// Changes the saved `Example App` file with `change`, as if another
    /// save did, without telling the overlays.
    fn change_app_file(harness: &Harness, change: impl FnOnce(&mut AppFile)) {
        let stores = harness.owner.stores().expect("stores");
        let mut file = stores.app("example").expect("saved and readable").file;
        change(&mut file);
        stores.apps.save("example", &file, &[]).expect("saves");
    }

    #[test]
    fn an_overlay_reads_its_applications_values_and_follows_its_window() {
        let (mut harness, root) = with_saved_bar("overlay");
        // Read only at the size it was drawn at, the bar reads nothing on a
        // window of another size.
        change_app_file(&harness, |file| file.size_changes = SizeChangesDef::Rarely);
        assert_eq!(
            harness.ask(overlay_state),
            Err("The overlay for Example App is off.".to_owned())
        );
        assert_eq!(harness.ask(|owner| owner.overlay_windows()), Ok(Vec::new()));
        harness
            .ask(set_overlay(true, OverlayModeDef::Overlay))
            .expect("saves");
        assert_eq!(
            harness.ask(|owner| owner.overlay_windows()),
            Ok(vec![OverlayWindow {
                app: "example".to_owned(),
                name: "Example App".to_owned(),
                mode: OverlayModeDef::Overlay,
                panel: None,
            }])
        );
        // The picked window's capture is the capture screen's, not the
        // overlay's.
        assert!(harness.closed.try_recv().is_err());
        assert_eq!(harness.ask(overlay_place), None, "not open yet");

        let mut handler = overlay_capture(&mut harness);
        let over = OverlayModeDef::Overlay;
        assert_eq!(harness.ask(overlay_state), Ok(bar_view(over, None, None)));
        harness.deliver_at(&mut handler, Ok((WINDOW, &frame_image())), OVERLAY_INTERVAL);
        assert_eq!(
            harness.ask(overlay_state),
            Ok(bar_view(over, Some(Reading::Number(50.0)), None))
        );

        // It goes over the window once its card is measured, and not while
        // the window is minimized.
        assert_eq!(harness.ask(overlay_place), None, "no card yet");
        let card = Card {
            width: 180.0,
            height: 40.0,
        };
        harness.ask(|owner| owner.overlay_card("example", card));
        assert_eq!(
            harness.ask(overlay_place),
            Some(Place {
                x: 0,
                y: 0,
                width: 4,
                height: 2,
                card,
            })
        );
        harness.owner.host.minimized = true;
        assert_eq!(harness.ask(overlay_place), None);
        let minimized = harness.ask(overlay_state).expect("shows");
        assert_eq!(
            minimized.message.as_deref(),
            Some("Restore Example App to see its values.")
        );
        harness.owner.host.minimized = false;

        // A window of a size no value reads at reads nothing, and the
        // capture goes on.
        let wider = Size {
            width: 6,
            height: 2,
        };
        harness.deliver_at(&mut handler, Ok((wider, &frame_image())), OVERLAY_INTERVAL);
        let mut settling = bar_view(over, None, None);
        settling.window_size = Some(SizeDef {
            width: 6,
            height: 2,
        });
        assert_eq!(
            harness.ask(overlay_state),
            Ok(settling),
            "nothing is read until the size settles"
        );
        harness.wait(SETTLE);
        let resized = |width| {
            let mut view = bar_view(
                over,
                None,
                Some(&format!(
                    "Not read at {width} × 2, the window's size now: it is read only at the window \
                     sizes it was drawn at."
                )),
            );
            view.window_size = Some(SizeDef { width, height: 2 });
            Ok(view)
        };
        assert_eq!(harness.ask(overlay_state), resized(6));
        let narrower = Size {
            width: 3,
            height: 2,
        };
        harness.deliver_at(
            &mut handler,
            Err(OutsideWindow {
                window_size: narrower,
            }
            .into()),
            OVERLAY_INTERVAL,
        );
        harness.ask(overlay_state).expect("shows");
        harness.wait(SETTLE);
        assert_eq!(harness.ask(overlay_state), resized(3));
        assert!(harness.closed.try_recv().is_err(), "the capture goes on");
        let white = Image::new(WINDOW, vec![255; 32]).expect("matching length");
        harness.deliver_at(&mut handler, Ok((WINDOW, &white)), OVERLAY_INTERVAL);
        let view = harness.ask(overlay_state).expect("shows");
        let problem = view.values[0].problem.as_deref().expect("a problem");
        assert!(problem.starts_with("Cannot read: "), "{problem}");
        // Back at the size the capture reads at, it is not started again
        // once that size settles.
        harness.wait(SETTLE);
        harness.ask(overlay_state).expect("shows");
        assert!(harness.closed.try_recv().is_err(), "the capture goes on");
        assert!(harness.handlers.try_recv().is_err(), "and is not replaced");

        // A capture that stops closes, says why, and starts again about a
        // second later.
        harness.deliver_at(
            &mut handler,
            Err(io::Error::other("the window closed")),
            OVERLAY_INTERVAL,
        );
        assert_eq!(harness.closed.try_recv(), Ok(7));
        for _ in 0..RETRY_POLLS {
            let view = harness.ask(overlay_state).expect("shows");
            assert_eq!(
                view.message.as_deref(),
                Some("Example App cannot be read: the window closed.")
            );
            assert!(harness.handlers.try_recv().is_err(), "it waits");
        }
        let mut handler = overlay_capture(&mut harness);
        // Readings from the stopped capture are dropped.
        let message = Message::Overlay {
            purpose: crate::overlay::Purpose::Overlay,
            app: "example".to_owned(),
            generation: 0,
            read: Ok(FrameRead::Values(vec![ValueRead::Read(Reading::Flag(
                true,
            ))])),
        };
        assert!(harness.owner.handle(message).is_continue());
        assert_eq!(harness.ask(overlay_state), Ok(bar_view(over, None, None)));
        harness.deliver_at(&mut handler, Ok((WINDOW, &frame_image())), OVERLAY_INTERVAL);

        // Reading the saved file again keeps the card's size, which the page
        // sends only when it changes.
        harness.ask(set_overlay(true, over)).expect("saves");
        assert_eq!(harness.closed.try_recv(), Ok(7));
        drop(overlay_capture(&mut harness));
        assert!(harness.ask(overlay_place).is_some());

        // Changing the overlay reads the saved file again.
        harness
            .ask(set_overlay(false, OverlayModeDef::Overlay))
            .expect("saves");
        assert_eq!(harness.closed.try_recv(), Ok(7));
        assert_eq!(
            harness.ask(overlay_state),
            Err("The overlay for Example App is off.".to_owned())
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn an_overlay_reads_the_window_the_runtime_reads() {
        let (mut harness, root) = with_saved_bar("overlay-twin");
        // Another copy's window fits as well and is listed first, but it is
        // minimized, so the runtime reads window 7.
        harness.owner.host.twin = Some(40);
        harness
            .ask(set_overlay(true, OverlayModeDef::Overlay))
            .expect("saves");
        let _handler = overlay_capture(&mut harness);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn removing_an_application_stops_its_overlay_and_leaves_no_window_wanted() {
        let (mut harness, root) = with_saved_bar("overlay-removed");
        harness
            .ask(set_overlay(true, OverlayModeDef::PictureInPicture))
            .expect("saves");
        drop(overlay_capture(&mut harness));
        harness.forget_reloads();
        harness
            .ask(|owner| owner.remove_app("example"))
            .expect("removes");
        assert_eq!(harness.closed.try_recv(), Ok(7), "its capture stops");
        assert_eq!(harness.reloads(), 1, "the runtime reads the applications");
        // The window sync that follows the removal closes the panel.
        assert_eq!(harness.ask(|owner| owner.overlay_windows()), Ok(Vec::new()));
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn an_overlay_reads_the_values_drawn_at_the_windows_size_and_says_why_others_cannot() {
        let (mut harness, root) = with_saved_bar("overlay-sizes");
        change_app_file(&harness, |file| {
            let mut small = file.values[0].clone();
            small.id = "small".to_owned();
            small.name = "Small".to_owned();
            small.screen.window_size = SizeDef {
                width: 8,
                height: 4,
            };
            small.screen.region.x = 4;
            small.screen.region.y = 2;
            let mut icon = file.values[0].clone();
            icon.id = "icon".to_owned();
            icon.name = "Icon".to_owned();
            icon.screen.check = serde_json::from_value(serde_json::json!({
                "kind": "state",
                "states": [{"name": "Shown", "image": "missing.png"}],
                "tolerance": 0
            }))
            .expect("the file's spelling");
            file.values.extend([small, icon]);
            file.size_changes = SizeChangesDef::Rarely;
            file.overlay = Some(OverlayDef {
                enabled: true,
                mode: OverlayModeDef::PictureInPicture,
                panel: None,
            });
        });
        // Only the bar was drawn at 4x2, so only its region is captured.
        let mut handler = overlay_capture(&mut harness);
        harness.deliver_at(&mut handler, Ok((WINDOW, &frame_image())), OVERLAY_INTERVAL);
        let view = harness.ask(overlay_state).expect("shows");
        let lines: Vec<(&str, Option<Reading>, Option<&str>)> = view
            .values
            .iter()
            .map(|value| {
                (
                    value.name.as_str(),
                    value.reading.clone(),
                    value.problem.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            lines[0],
            ("Progress bar", Some(Reading::Number(50.0)), None)
        );
        assert_eq!(
            lines[1],
            (
                "Small",
                None,
                Some(
                    "Not read at 4 × 2, the window's size now: it is read only at the window sizes it was drawn at."
                )
            )
        );
        let (icon, reading, problem) = lines[2].clone();
        assert_eq!((icon, reading), ("Icon", None));
        assert!(
            problem.is_some_and(|problem| problem.starts_with("Cannot read: ")),
            "{problem:?}"
        );

        // At 8x4 the capture starts again with the small value's region,
        // once the size settled.
        let large = Size {
            width: 8,
            height: 4,
        };
        harness.deliver_at(&mut handler, Ok((large, &frame_image())), OVERLAY_INTERVAL);
        harness.ask(overlay_state).expect("shows");
        assert!(harness.closed.try_recv().is_err(), "not settled yet");
        harness.wait(SETTLE);
        let mut handler =
            overlay_capture_of(&mut harness, Region::new(4, 2, 4, 2).expect("not empty"));
        harness.deliver_at(&mut handler, Ok((large, &frame_image())), OVERLAY_INTERVAL);
        let view = harness.ask(overlay_state).expect("shows");
        assert_eq!(
            view.values[0].problem.as_deref(),
            Some(
                "Not read at 8 × 4, the window's size now: it is read only at the window sizes it was drawn at."
            )
        );
        assert_eq!(view.values[1].reading, Some(Reading::Number(50.0)));
        // A panel does not go over the window.
        assert_eq!(harness.ask(overlay_place), None);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn the_values_page_reads_values_scaled_to_the_windows_size_with_the_overlay_off() {
        let (mut harness, root) = with_saved_bar("overlay-live");
        let live = |owner: &mut Owner<FakeHost>| owner.live_values("example");
        assert!(harness.ask(live).is_ok(), "the overlay is off");
        let started = harness.handlers.try_recv().expect("capture started");
        let mut handler = started.handler;
        // Twice the height, the bar it reads doubles, and the capture starts
        // again for its region there.
        let large = Size {
            width: 8,
            height: 4,
        };
        let red = Image::new(large, [0, 0, 255, 0].repeat(32)).expect("matching length");
        harness.deliver_at(&mut handler, Ok((large, &red)), OVERLAY_INTERVAL);
        harness.ask(live).expect("shows");
        assert!(harness.closed.try_recv().is_err(), "not settled yet");
        // A drag passes another size first, which starts the wait again.
        let between = Size {
            width: 6,
            height: 3,
        };
        harness.wait(SETTLE / 2);
        harness.deliver_at(&mut handler, Ok((between, &red)), OVERLAY_INTERVAL);
        harness.deliver_at(&mut handler, Ok((large, &red)), OVERLAY_INTERVAL);
        harness.wait(SETTLE / 2);
        harness.ask(live).expect("shows");
        assert!(harness.closed.try_recv().is_err(), "the wait started again");
        harness.wait(SETTLE);
        harness.ask(live).expect("shows");
        assert_eq!(harness.closed.try_recv(), Ok(7), "one restart");
        let started = harness.handlers.try_recv().expect("capture started again");
        assert!(harness.handlers.try_recv().is_err(), "only one");
        let copied = match &started.crop {
            Crop::Layout(layout) => layout.copies().iter().map(|copy| copy.from).collect(),
            Crop::Whole => Vec::new(),
        };
        assert_eq!(copied, [Region::new(0, 0, 8, 4).expect("not empty")]);
        let mut handler = started.handler;
        harness.deliver_at(&mut handler, Ok((large, &red)), OVERLAY_INTERVAL);
        let view = harness.ask(live).expect("shows");
        assert_eq!(
            view.window_size,
            Some(SizeDef {
                width: 8,
                height: 4
            })
        );
        assert_eq!(view.values[0].reading, Some(Reading::Number(100.0)));
        // The overlay's own captures are apart from the page's.
        assert_eq!(
            harness.ask(overlay_state),
            Err("The overlay for Example App is off.".to_owned())
        );
        harness.ask(|owner| owner.stop_live_values(None));
        assert_eq!(harness.closed.try_recv(), Ok(7), "the page stopped reading");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn an_overlay_without_its_window_or_file_says_why_and_tries_again_about_once_a_second() {
        let (mut harness, root) = with_saved_bar("overlay-retry");
        harness
            .ask(set_overlay(true, OverlayModeDef::Overlay))
            .expect("saves");
        harness.owner.host.unlisted = true;
        let gone = OverlayView {
            name: "Example App".to_owned(),
            mode: OverlayModeDef::Overlay,
            message: Some("no windows here".to_owned()),
            window_size: None,
            values: vec![ValueView {
                id: "progress_bar".to_owned(),
                name: "Progress bar".to_owned(),
                reading: None,
                problem: None,
            }],
        };
        assert_eq!(harness.ask(overlay_state), Ok(gone.clone()));
        harness.owner.host.unlisted = false;
        for _ in 0..RETRY_POLLS {
            assert_eq!(harness.ask(overlay_state), Ok(gone.clone()));
            assert!(harness.handlers.try_recv().is_err(), "it waits");
        }
        drop(overlay_capture(&mut harness));

        // An overlay that cannot open reads its file again only after a
        // while, rather than on every poll.
        harness
            .ask(set_overlay(false, OverlayModeDef::Overlay))
            .expect("saves");
        assert_eq!(harness.closed.try_recv(), Ok(7));
        let off = Err("The overlay for Example App is off.".to_owned());
        assert_eq!(harness.ask(overlay_state), off);
        change_app_file(&harness, |file| {
            file.overlay = Some(OverlayDef {
                enabled: true,
                mode: OverlayModeDef::Overlay,
                panel: None,
            });
        });
        for _ in 0..RETRY_POLLS {
            assert_eq!(harness.ask(overlay_state), off);
        }
        drop(overlay_capture(&mut harness));
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_panel_keeps_where_the_user_put_it_once_it_settles() {
        let (mut harness, root) = with_saved_bar("overlay-panel");
        let panel_mode = OverlayModeDef::PictureInPicture;
        harness.ask(set_overlay(true, panel_mode)).expect("saves");
        drop(overlay_capture(&mut harness));
        assert_eq!(
            harness.ask(overlay_state),
            Ok(bar_view(panel_mode, None, None))
        );
        let panel = |x| PanelDef {
            x,
            y: 20,
            width: 260,
            height: 200,
        };
        let placed = |x| Message::PanelPlaced {
            app: "example".to_owned(),
            panel: panel(x),
        };
        let saved = |harness: &mut Harness| {
            harness
                .ask(|owner| owner.overlay_windows())
                .expect("listed")
                .first()
                .and_then(|overlay| overlay.panel)
        };
        assert!(harness.owner.handle(placed(-900)).is_continue());
        assert!(harness.owner.handle(placed(-800)).is_continue());
        for _ in 1..SETTLE_POLLS {
            harness.ask(overlay_state).expect("shows");
            assert_eq!(
                saved(&mut harness),
                None,
                "it waits for the panel to settle"
            );
        }
        harness.ask(overlay_state).expect("shows");
        assert_eq!(saved(&mut harness), Some(panel(-800)));

        // Saving the overlay right after a move keeps both.
        assert!(harness.owner.handle(placed(-700)).is_continue());
        harness.ask(set_overlay(true, panel_mode)).expect("saves");
        assert_eq!(saved(&mut harness), Some(panel(-700)));
        assert_eq!(harness.closed.try_recv(), Ok(7));
        drop(overlay_capture(&mut harness));

        // The overlay window of the mode it left closing later leaves it.
        harness.ask(close_overlay(OverlayModeDef::Overlay));
        assert!(harness.closed.try_recv().is_err());
        // Its own window closing saves its place at once.
        assert!(harness.owner.handle(placed(10)).is_continue());
        harness.ask(close_overlay(panel_mode));
        assert_eq!(harness.closed.try_recv(), Ok(7));
        assert_eq!(saved(&mut harness), Some(panel(10)));
        // So does the app ending.
        drop(overlay_capture(&mut harness));
        assert!(harness.owner.handle(placed(30)).is_continue());
        harness.owner.shut_down().expect("closes");
        assert_eq!(saved(&mut harness), Some(panel(30)));
        // A place for an overlay that is not open is dropped.
        assert!(harness.owner.handle(placed(50)).is_continue());
        harness.owner.shut_down().expect("closes");
        assert_eq!(saved(&mut harness), Some(panel(30)));
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn only_a_save_that_worked_tells_the_runtime_and_the_overlays() {
        let (mut harness, root) = with_saved_bar("overlay-saves");
        assert_eq!(
            harness.reloads(),
            2,
            "the application and its value were saved"
        );
        harness
            .ask(set_overlay(true, OverlayModeDef::Overlay))
            .expect("saves");
        assert_eq!(harness.reloads(), 0, "the runtime shows no overlay");
        drop(overlay_capture(&mut harness));

        // Saves that fail change nothing, so the overlay keeps reading and
        // the runtime keeps what it read.
        assert!(
            harness
                .ask(|owner| owner.save(&spec("missing", "Progress bar", BAR)))
                .is_err()
        );
        assert!(
            harness
                .ask(|owner| owner.set_anti_cheat("missing", AntiCheatDef::None))
                .is_err()
        );
        assert!(
            harness
                .ask(|owner| owner.set_automation_enabled("missing", false))
                .is_err()
        );
        assert!(harness.closed.try_recv().is_err(), "the overlay goes on");
        assert_eq!(harness.reloads(), 0);

        // A setting only the runtime reads leaves the overlay alone.
        harness
            .ask(|owner| owner.set_anti_cheat("example", AntiCheatDef::UserMode))
            .expect("saves");
        assert_eq!(harness.reloads(), 1);
        assert!(harness.closed.try_recv().is_err(), "the overlay goes on");

        // A change to the values tells both.
        harness
            .ask(|owner| owner.save(&spec("example", "Another bar", BAR)))
            .expect("saves");
        assert_eq!(harness.reloads(), 1);
        assert_eq!(harness.closed.try_recv(), Ok(7), "the overlay reads again");
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
