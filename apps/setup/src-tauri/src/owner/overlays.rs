//! The overlays: which windows show one, turning one on or off, what each
//! shows, and where the user put its panels.

use appytappy_config::app_file::{OverlayDef, OverlayModeDef, PanelDef};

use crate::overlay::{self, OverlayView, OverlayWindow};

use super::Owner;
use super::host::Host;

impl<H: Host> Owner<H> {
    pub(super) fn overlay_windows(&self) -> Result<Vec<OverlayWindow>, String> {
        let loaded = self
            .stores()?
            .apps
            .load()
            .map_err(|error| error.to_string())?;
        Ok(overlay::windows(loaded.iter().filter_map(|stored| {
            Some((stored.id.as_str(), stored.file.as_ref().ok()?))
        })))
    }

    pub(super) fn set_overlay(
        &self,
        app: &str,
        enabled: bool,
        mode: OverlayModeDef,
    ) -> Result<(), String> {
        let mut file = self.stores()?.app_file(app)?;
        let panel = file.overlay.and_then(|overlay| overlay.panel);
        file.overlay = Some(OverlayDef {
            enabled,
            mode,
            panel,
        });
        self.stores()?
            .apps
            .save(app, &file, &[])
            .map_err(|error| error.to_string())
    }

    pub(super) fn overlay_view(&mut self, app: &str) -> Result<OverlayView, String> {
        let stores = self.stores.as_ref().map_err(Clone::clone);
        self.overlays.view(&self.host, &self.messages, app, || {
            let stores = stores?;
            let file = stores.app_file(app)?;
            Ok((file, stores.apps.folder(app)))
        })
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

    fn save_panel(&self, app: &str, panel: PanelDef) -> Result<(), String> {
        let mut file = self.stores()?.app_file(app)?;
        let Some(overlay) = &mut file.overlay else {
            return Ok(());
        };
        overlay.panel = Some(panel);
        self.stores()?
            .apps
            .save(app, &file, &[])
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use appytappy_config::app_file::{AppFile, SizeDef};
    use appytappy_sources::screen::{Crop, FrameHandler, Image, OutsideWindow, Region, Size};

    use crate::overlay::{
        Card, FrameRead, OVERLAY_INTERVAL, Place, RETRY_POLLS, SETTLE_POLLS, ValueRead, ValueView,
    };
    use crate::owner::messages::{Message, Reading, Reply, Request};
    use crate::owner::test_support::{Harness, WINDOW, frame_image, with_saved_bar};

    use super::*;

    fn overlay_state(reply: Reply<Result<OverlayView, String>>) -> Request {
        Request::OverlayState {
            app: "example".to_owned(),
            reply,
        }
    }

    fn overlay_place(reply: Reply<Option<Place>>) -> Request {
        Request::OverlayPlace {
            app: "example".to_owned(),
            reply,
        }
    }

    fn set_overlay(
        enabled: bool,
        mode: OverlayModeDef,
    ) -> impl FnOnce(Reply<Result<(), String>>) -> Request {
        move |reply| Request::SetOverlay {
            app: "example".to_owned(),
            enabled,
            mode,
            reply,
        }
    }

    fn close_overlay(mode: OverlayModeDef) -> impl FnOnce(Reply<()>) -> Request {
        move |reply| Request::CloseOverlay {
            app: "example".to_owned(),
            mode,
            reply,
        }
    }

    /// The overlay's view of `Progress bar` reading `reading` or failing
    /// with `problem`.
    fn bar_view(
        mode: OverlayModeDef,
        reading: Option<Reading>,
        problem: Option<&str>,
    ) -> OverlayView {
        OverlayView {
            name: "Example App".to_owned(),
            mode,
            message: None,
            values: vec![ValueView {
                name: "Progress bar".to_owned(),
                reading,
                problem: problem.map(str::to_owned),
            }],
        }
    }

    /// Polls the overlay once and returns its capture's handler, checking it
    /// captures only `crop` of window 7.
    fn overlay_capture_of(harness: &mut Harness, crop: Region) -> FrameHandler {
        harness.ask(overlay_state).expect("shows");
        let started = harness.handlers.try_recv().expect("capture started");
        assert_eq!(
            (started.window, started.crop, started.interval),
            (7, Crop::Region(crop), OVERLAY_INTERVAL)
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
        let mut file = stores
            .apps
            .load()
            .expect("loads")
            .into_iter()
            .find(|stored| stored.id == "example")
            .expect("saved")
            .file
            .expect("readable");
        change(&mut file);
        stores.apps.save("example", &file, &[]).expect("saves");
    }

    #[test]
    fn an_overlay_reads_its_applications_values_and_follows_its_window() {
        let (mut harness, root) = with_saved_bar("overlay");
        assert_eq!(
            harness.ask(overlay_state),
            Err("The overlay for Example App is off.".to_owned())
        );
        assert_eq!(harness.ask(Request::Overlays), Ok(Vec::new()));
        harness
            .ask(set_overlay(true, OverlayModeDef::Overlay))
            .expect("saves");
        assert_eq!(
            harness.ask(Request::Overlays),
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
        harness.ask(|reply| Request::OverlayCard {
            app: "example".to_owned(),
            card,
            reply,
        });
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

        // A window of a size no value was drawn at reads nothing, and the
        // capture goes on.
        let wider = Size {
            width: 6,
            height: 2,
        };
        harness.deliver_at(&mut handler, Ok((wider, &frame_image())), OVERLAY_INTERVAL);
        let resized = |width| {
            Ok(bar_view(
                over,
                None,
                Some(&format!("Drawn on a 4x2 window; it is {width}x2 now.")),
            ))
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
        assert_eq!(harness.ask(overlay_state), resized(3));
        assert!(harness.closed.try_recv().is_err(), "the capture goes on");
        let white = Image::new(WINDOW, vec![255; 32]).expect("matching length");
        harness.deliver_at(&mut handler, Ok((WINDOW, &white)), OVERLAY_INTERVAL);
        let view = harness.ask(overlay_state).expect("shows");
        let problem = view.values[0].problem.as_deref().expect("a problem");
        assert!(problem.starts_with("Cannot read: "), "{problem}");

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
                "kind": "reference",
                "image": "missing.png",
                "tolerance": 0
            }))
            .expect("the file's spelling");
            file.values.extend([small, icon]);
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
            .map(|value| (value.name.as_str(), value.reading, value.problem.as_deref()))
            .collect();
        assert_eq!(
            lines[0],
            ("Progress bar", Some(Reading::Number(50.0)), None)
        );
        assert_eq!(
            lines[1],
            ("Small", None, Some("Drawn on a 8x4 window; it is 4x2 now."))
        );
        let (icon, reading, problem) = lines[2];
        assert_eq!((icon, reading), ("Icon", None));
        assert!(
            problem.is_some_and(|problem| problem.starts_with("Cannot read: ")),
            "{problem:?}"
        );

        // At 8x4 the capture starts again with the small value's region.
        let large = Size {
            width: 8,
            height: 4,
        };
        harness.deliver_at(&mut handler, Ok((large, &frame_image())), OVERLAY_INTERVAL);
        assert_eq!(harness.closed.try_recv(), Ok(7));
        let mut handler =
            overlay_capture_of(&mut harness, Region::new(4, 2, 4, 2).expect("not empty"));
        harness.deliver_at(&mut handler, Ok((large, &frame_image())), OVERLAY_INTERVAL);
        let view = harness.ask(overlay_state).expect("shows");
        assert_eq!(
            view.values[0].problem.as_deref(),
            Some("Drawn on a 4x2 window; it is 8x4 now.")
        );
        assert_eq!(view.values[1].reading, Some(Reading::Number(50.0)));
        // A panel does not go over the window.
        assert_eq!(harness.ask(overlay_place), None);
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
            values: vec![ValueView {
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
                .ask(Request::Overlays)
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
}
