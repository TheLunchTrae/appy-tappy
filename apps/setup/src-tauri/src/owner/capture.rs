//! The picked window: listing windows, starting and stopping its capture,
//! keeping screenshots, and reading the check the user sets up on its frames.

use std::io;
use std::time::{Duration, Instant};

use appytappy_config::app_file::WindowDef;
use appytappy_config::signal::Value;
use appytappy_sources::screen::{Crop, Frame, FrameHandler, Image};
use appytappy_sources::system::{ProcessEntry, WindowEntry};

use crate::check_spec::bar::{self, BarDetection, BarMeasure};
use crate::check_spec::{self, CheckSpec, ColorSpec, RegionCheck, RegionSpec};
use crate::pixels;
use crate::selection::{self, SelectionSpec, SelectionView};
use crate::window_list::{self, WindowFit, WindowList};

use super::host::Host;
use super::messages::{CaptureFailure, Message, Reading};
use super::{Owner, PREVIEW_INTERVAL};

/// A preview capture's message for one frame.
fn preview_message<H: Host>(generation: u64, frame: io::Result<Frame<'_>>) -> Message<H> {
    match frame {
        Ok(frame) => Message::Frame {
            generation,
            image: frame.image.clone(),
            received: frame.received,
        },
        Err(error) => Message::CaptureFailed {
            generation,
            failure: CaptureFailure {
                ended: error.kind() == io::ErrorKind::NotConnected,
                error: error.to_string(),
            },
        },
    }
}

/// What `check` reads on `frame`.
fn read(check: &RegionCheck, frame: &Image) -> Result<Value, String> {
    let cropped = frame.crop(check.region).ok_or_else(|| {
        "The region reaches past the edge of the window. Draw it inside the window.".to_owned()
    })?;
    check
        .check
        .read(&cropped)
        .map_err(|error| error.to_string())
}

impl<H: Host> Owner<H> {
    /// Lists the windows the user can pick.
    pub(crate) fn window_list(&self) -> Result<WindowList, String> {
        let (processes, windows) = self.desktop()?;
        Ok(window_list::list(windows, &processes, self.own_process))
    }

    /// Every running process and top-level window.
    pub(super) fn desktop(&self) -> Result<(Vec<ProcessEntry>, Vec<WindowEntry>), String> {
        let windows = self.host.windows().map_err(|error| error.to_string())?;
        let processes = self.host.processes().map_err(|error| error.to_string())?;
        Ok((processes, windows))
    }

    /// Stops the capture and forgets its frames, screenshots, recording, and
    /// check.
    pub(crate) fn stop(&mut self) {
        // A capture that failed already said why; what comes next matters now.
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the old capture's error is moot once the user moved on"
        )]
        let _ = self.close();
        self.generation = self.generation.wrapping_add(1);
        self.latest = None;
        self.failure = None;
        self.screenshots.clear();
        self.check = None;
        self.recording = None;
        self.watched = None;
    }

    /// Captures `window`, dropping the previous window's screenshots and
    /// check, and answers once the capture started.
    pub(crate) fn pick(&mut self, window: u64) -> Result<(), String> {
        self.start(window, Crop::Whole, PREVIEW_INTERVAL, preview_message)
            .map(drop)
    }

    /// Stops the capture and starts capturing `crop` of `window`, sending
    /// each frame as `to_message` makes it. Answers the new capture's
    /// generation.
    pub(super) fn start(
        &mut self,
        window: u64,
        crop: Crop,
        interval: Duration,
        to_message: fn(u64, io::Result<Frame<'_>>) -> Message<H>,
    ) -> Result<u64, String> {
        self.stop();
        let generation = self.generation;
        let messages = self.messages.clone();
        let handler: FrameHandler = Box::new(move |frame| {
            // The owner stopped when the send fails, and the capture is
            // about to be stopped too, so the wait does not matter.
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a failed send means the owner stopped and will close this capture"
            )]
            let _ = messages.send(to_message(generation, frame));
            interval
        });
        let session = self
            .host
            .capture(window, crop, interval, handler)
            .map_err(|error| error.to_string())?;
        self.session = Some(session);
        Ok(generation)
    }

    /// Keeps `image`, a frame of the picked window that arrived at
    /// `received`, unless the capture ended. A good frame ends a failure
    /// that did not end the capture.
    pub(super) fn frame(&mut self, image: Image, received: Instant) {
        if self.failure.as_ref().is_some_and(|failure| failure.ended) {
            return;
        }
        self.failure = None;
        self.record(&image, received);
        self.latest = Some(image);
    }

    /// Notes that the capture failed, and stops a recording, which records
    /// nothing until frames come again, with the failure as its problem.
    pub(super) fn capture_failed(&mut self, failure: CaptureFailure) {
        if let Some(recording) = &mut self.recording {
            let error = check_spec::sentence(&failure.error);
            if failure.ended {
                recording.end(
                    Instant::now(),
                    format!("Recording stopped: {error} Change window to pick it again."),
                );
            } else {
                recording.fail(
                    Instant::now(),
                    format!("Recording stopped: {error} Record more once the window shows again."),
                );
            }
        }
        self.failure = Some(failure);
    }

    /// Keeps the newest frame as a screenshot and answers its index.
    pub(crate) fn screenshot(&mut self) -> Result<usize, String> {
        if let Some(failure) = &self.failure {
            let next = if failure.ended {
                "Change window to pick it again."
            } else {
                "Take it once the window shows again."
            };
            return Err(format!("{} {next}", check_spec::sentence(&failure.error)));
        }
        let image = self
            .latest
            .clone()
            .ok_or_else(|| "No frame has arrived from the window yet.".to_owned())?;
        self.screenshots.push(image);
        Ok(self.screenshots.len().saturating_sub(1))
    }

    /// What the check reads on the newest frame: `None` until a check is
    /// set up and a frame arrived.
    pub(crate) fn reading(&self) -> Result<Option<Reading>, String> {
        let (Some(check), Some(latest)) = (&self.check, &self.latest) else {
            return Ok(None);
        };
        read(check, latest).map(|value| Some(value.into()))
    }

    /// The newest frame's pixels, at most `max_width` wide, or `None`
    /// before one arrived. A failed capture answers why it failed.
    pub(crate) fn preview(&self, max_width: u32) -> Result<Option<Vec<u8>>, String> {
        match (&self.failure, &self.latest) {
            (Some(failure), _) => Err(failure.error.clone()),
            (None, latest) => Ok(latest.as_ref().map(|image| pixels::rgba(image, max_width))),
        }
    }

    /// Screenshot `index`'s pixels, at most `max_width` wide.
    pub(crate) fn screenshot_pixels(&self, index: usize, max_width: u32) -> Option<Vec<u8>> {
        self.screenshots
            .get(index)
            .map(|image| pixels::rgba(image, max_width))
    }

    /// What `selection` picks on screenshot `screenshot`.
    pub(crate) fn select(
        &self,
        screenshot: usize,
        selection: &SelectionSpec,
    ) -> Result<Option<SelectionView>, String> {
        self.screenshots
            .get(screenshot)
            .map(|image| {
                selection::select(image, selection)
                    .as_ref()
                    .map(SelectionView::from)
            })
            .ok_or_else(|| check_spec::SpecError::NoScreenshot.to_string())
    }

    /// The colors of the bar in `region`, drawn on screenshot `screenshot`,
    /// or why none are proposed.
    pub(crate) fn detect_bar(
        &self,
        region: RegionSpec,
        screenshot: usize,
    ) -> Result<BarDetection, String> {
        bar::detect(&self.screenshots, region, screenshot).map_err(|error| error.to_string())
    }

    /// What a bar of the colors `filled` and `empty` reads in `region`,
    /// drawn on screenshot `screenshot`, on each screenshot of that one's
    /// window size, and which pixels it counts on screenshot `shown`.
    pub(crate) fn measure_bar(
        &self,
        region: RegionSpec,
        screenshot: usize,
        filled: ColorSpec,
        empty: ColorSpec,
        shown: Option<usize>,
    ) -> Result<BarMeasure, String> {
        bar::measure(&self.screenshots, region, screenshot, filled, empty, shown)
            .map_err(|error| error.to_string())
    }

    /// Makes `spec` the check the live frame is read with.
    pub(crate) fn set_check(&mut self, spec: &CheckSpec) -> Result<(), String> {
        let check = self
            .with_session(|session| check_spec::build(spec, session))
            .map_err(|error| error.to_string())?;
        self.check = Some(check);
        Ok(())
    }

    /// The running windows that `window` matches, the one a new session
    /// would read first.
    pub(crate) fn match_windows(&self, window: &WindowDef) -> Result<Vec<WindowFit>, String> {
        let (processes, windows) = self.desktop()?;
        let read = self
            .host
            .read_window(&window.to_match(), processes.clone(), windows.clone())
            .map(|read| read.entry.handle);
        let list = window_list::list(windows, &processes, self.own_process);
        Ok(window_list::matching(&list.windows, window, read))
    }

    /// The window a new session would read for `window`, or `None` while
    /// none fits.
    pub(crate) fn read_window(&self, window: &WindowDef) -> Result<Option<u64>, String> {
        let (processes, windows) = self.desktop()?;
        Ok(self
            .host
            .read_window(&window.to_match(), processes, windows)
            .map(|read| read.entry.handle))
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::app_file::WindowDef;
    use appytappy_sources::screen::Size;

    use crate::check_spec::CheckSpec;
    use crate::owner::run;
    use crate::owner::test_support::{FakeHost, Harness, frame_image, harness};
    use crate::pixels;
    use crate::window_list::WindowFit;

    use super::*;

    #[test]
    fn lists_windows_without_the_setup_apps_own() {
        let mut harness = harness(false);
        let list = harness
            .ask(|owner| owner.window_list())
            .expect("the host answers");
        assert_eq!(list.windows.len(), 1);
        assert_eq!(list.windows[0].executable, "Example.exe");
    }

    #[test]
    fn previews_the_newest_frame_once_one_arrives() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        let preview = |owner: &mut Owner<FakeHost>| owner.preview(100);
        assert_eq!(harness.ask(preview), Ok(None));
        harness.deliver(&mut handler, Ok(&frame_image()));
        let bytes = harness.ask(preview).expect("running").expect("a frame");
        assert_eq!(bytes, pixels::rgba(&frame_image(), 100));
    }

    #[test]
    fn a_stopped_capture_explains_itself_until_another_window_is_picked() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(
            &mut handler,
            Err(io::Error::other("the captured window closed")),
        );
        let preview = |owner: &mut Owner<FakeHost>| owner.preview(100);
        assert_eq!(
            harness.ask(preview),
            Err("the captured window closed".to_owned())
        );
        let _next = harness.pick();
        assert_eq!(harness.ask(preview), Ok(None));
    }

    #[test]
    fn a_frame_error_lasts_until_a_good_frame_but_a_closed_window_ends_the_capture() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        let preview = |owner: &mut Owner<FakeHost>| owner.preview(100);
        harness.deliver(&mut handler, Ok(&frame_image()));
        harness.deliver(&mut handler, Err(io::Error::other("a frame was lost")));
        assert_eq!(harness.ask(preview), Err("a frame was lost".to_owned()));
        assert_eq!(
            harness.ask(Owner::screenshot),
            Err("A frame was lost. Take it once the window shows again.".to_owned()),
            "no screenshot of a frame from before the failure"
        );
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert!(
            harness.ask(preview).is_ok(),
            "the next good frame clears it"
        );
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));

        harness.deliver(
            &mut handler,
            Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "the captured window closed",
            )),
        );
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(
            harness.ask(preview),
            Err("the captured window closed".to_owned()),
            "a closed window is final"
        );
        assert_eq!(
            harness.ask(Owner::screenshot),
            Err("The captured window closed. Change window to pick it again.".to_owned())
        );
    }

    #[test]
    fn frames_from_a_window_the_user_left_are_dropped() {
        let mut harness = harness(false);
        let mut first = harness.pick();
        let _second = harness.pick();
        harness.deliver(&mut first, Ok(&frame_image()));
        harness.deliver(&mut first, Err(io::Error::other("closed")));
        let preview = |owner: &mut Owner<FakeHost>| owner.preview(100);
        assert_eq!(harness.ask(preview), Ok(None));
    }

    #[test]
    fn a_capture_that_cannot_start_says_why() {
        let mut harness = harness(true);
        assert_eq!(
            harness.ask(|owner| owner.pick(7)),
            Err("no capture here".to_owned())
        );
    }

    #[test]
    fn screenshots_keep_frames_until_another_window_is_picked() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        assert!(harness.ask(Owner::screenshot).is_err());
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        assert_eq!(harness.ask(Owner::screenshot), Ok(1));
        let pixels_of =
            |index| move |owner: &mut Owner<FakeHost>| owner.screenshot_pixels(index, 2);
        assert_eq!(
            harness.ask(pixels_of(1)),
            Some(pixels::rgba(&frame_image(), 2))
        );
        assert_eq!(harness.ask(pixels_of(2)), None);
        let _next = harness.pick();
        assert_eq!(harness.ask(pixels_of(0)), None);
    }

    #[test]
    fn a_selection_is_made_on_the_named_screenshot() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        let select = |screenshot, x| {
            move |owner: &mut Owner<FakeHost>| {
                owner.select(
                    screenshot,
                    &serde_json::from_value(serde_json::json!({
                        "tolerance": 10,
                        "steps": [{"kind": "click", "x": x, "y": 0}]
                    }))
                    .expect("the UI's spelling"),
                )
            }
        };
        assert_eq!(
            harness
                .ask(select(0, 3))
                .map(|view| view.map(|view| serde_json::to_value(view).expect("plain data"))),
            Ok(Some(serde_json::json!({
                "region": {"x": 2, "y": 0, "width": 2, "height": 2},
                "spans": [[0, 4]]
            }))),
            "the click picks the gray half"
        );
        assert_eq!(
            harness.ask(select(0, 9)),
            Ok(None),
            "a click off the screenshot"
        );
        assert_eq!(
            harness.ask(select(1, 0)),
            Err("That screenshot is gone. Take a new one and try again.".to_owned())
        );
    }

    #[test]
    fn a_bar_is_looked_for_and_measured_on_the_screenshots() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        let region = RegionSpec {
            x: 0,
            y: 0,
            width: 4,
            height: 2,
        };
        let detected = harness
            .ask(move |owner| owner.detect_bar(region, 0))
            .map(|detection| serde_json::to_value(detection).expect("plain data"));
        assert_eq!(
            detected.map(|detection| detection["kind"].clone()),
            Ok(serde_json::json!("declined")),
            "four pixels are too few to find a bar in"
        );
        let colors = |filled: &str, empty: &str| -> (ColorSpec, ColorSpec) {
            let color = |text: &str| serde_json::from_str(text).expect("the UI's spelling");
            (color(filled), color(empty))
        };
        let (red, gray) = colors(
            r#"{"red":255,"green":0,"blue":0}"#,
            r#"{"red":40,"green":40,"blue":40}"#,
        );
        let measure = |screenshot| {
            move |owner: &mut Owner<FakeHost>| {
                owner
                    .measure_bar(region, screenshot, red, gray, Some(0))
                    .map(|measure| serde_json::to_value(measure).expect("plain data"))
            }
        };
        assert_eq!(
            harness.ask(measure(0)),
            Ok(serde_json::json!({
                "readings": [{"screenshot": 0, "fill": 50.0}],
                "marks": {"filled": [[0, 2], [4, 6]], "empty": [[2, 4], [6, 8]]},
            }))
        );
        let gone = "That screenshot is gone. Take a new one and try again.".to_owned();
        assert_eq!(harness.ask(measure(1)), Err(gone.clone()));
        assert_eq!(
            harness.ask(move |owner| owner.detect_bar(region, 1)),
            Err(gone)
        );
    }

    #[test]
    fn the_check_reads_the_live_frame() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        assert_eq!(harness.ask(|owner| owner.reading()), Ok(None));
        let spec: CheckSpec = serde_json::from_str(
            r#"{"kind":"barFill","region":{"x":0,"y":0,"width":4,"height":2},
                "filled":{"red":255,"green":0,"blue":0},"empty":{"red":40,"green":40,"blue":40}}"#,
        )
        .expect("valid JSON");
        harness
            .ask(|owner| owner.set_check(&spec))
            .expect("valid check");
        assert_eq!(harness.ask(|owner| owner.reading()), Ok(None));
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(
            harness.ask(|owner| owner.reading()),
            Ok(Some(Reading::Number(50.0)))
        );
    }

    #[test]
    fn a_region_outside_the_live_frame_is_reported() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        let spec: CheckSpec = serde_json::from_str(
            r#"{"kind":"color","region":{"x":0,"y":0,"width":4,"height":2},
                "color":{"red":255,"green":0,"blue":0},"tolerance":0}"#,
        )
        .expect("valid JSON");
        harness
            .ask(|owner| owner.set_check(&spec))
            .expect("valid check");
        let small = Image::new(
            Size {
                width: 1,
                height: 1,
            },
            vec![0, 0, 255, 0],
        )
        .expect("matching length");
        harness.deliver(&mut handler, Ok(&small));
        assert!(
            harness
                .ask(|owner| owner.reading())
                .expect_err("outside")
                .contains("past the edge")
        );
    }

    #[test]
    fn a_check_that_cannot_be_built_says_what_to_fix() {
        let mut harness = harness(false);
        let spec: CheckSpec = serde_json::from_str(
            r#"{"kind":"color","region":{"x":0,"y":0,"width":0,"height":2},
                "color":{"red":255,"green":0,"blue":0},"tolerance":0}"#,
        )
        .expect("valid JSON");
        assert_eq!(
            harness.ask(|owner| owner.set_check(&spec)),
            Err("Draw a region on the screenshot first.".to_owned())
        );
    }

    #[test]
    fn a_window_match_finds_the_running_window() {
        let mut harness = harness(false);
        let window = WindowDef::exact(appytappy_config::window_match::WindowFacts {
            title: "Example App",
            class: "ExampleWindow",
            executable: "Example.exe",
        });
        assert_eq!(
            harness.ask(|owner| owner.match_windows(&window)),
            Ok(vec![WindowFit {
                handle: 7,
                fallback: false,
                best: true
            }])
        );
        assert_eq!(harness.ask(|owner| owner.read_window(&window)), Ok(Some(7)));
        let elsewhere = WindowDef {
            executable: "Other.exe".to_owned(),
            ..window.clone()
        };
        assert_eq!(harness.ask(|owner| owner.read_window(&elsewhere)), Ok(None));
        harness.owner.host.unlisted = true;
        assert!(harness.ask(|owner| owner.read_window(&window)).is_err());
    }

    #[test]
    fn the_window_marked_best_is_the_one_the_runtime_reads() {
        let mut harness = harness(false);
        // Window 6 of another copy fits as well and is listed first, but it
        // is minimized, so the runtime reads window 7.
        harness.owner.host.twin = Some(40);
        let window = WindowDef::exact(appytappy_config::window_match::WindowFacts {
            title: "Example App",
            class: "ExampleWindow",
            executable: "Example.exe",
        });
        let fit = |handle, best| WindowFit {
            handle,
            fallback: false,
            best,
        };
        assert_eq!(
            harness.ask(|owner| owner.match_windows(&window)),
            Ok(vec![fit(7, true), fit(6, false)])
        );
        assert_eq!(harness.ask(|owner| owner.read_window(&window)), Ok(Some(7)));
        harness.owner.host.minimized = true;
        assert_eq!(
            harness.ask(|owner| owner.match_windows(&window)),
            Ok(vec![fit(6, true), fit(7, false)]),
            "neither shows, so the first listed"
        );
    }

    #[test]
    fn stopping_closes_the_capture_and_forgets_the_screenshots() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        harness.ask(Owner::stop);
        assert_eq!(harness.closed.try_recv(), Ok(7));
        assert_eq!(harness.ask(|owner| owner.screenshot_pixels(0, 100)), None);
        assert_eq!(
            harness.ask(Owner::screenshot),
            Err("No frame has arrived from the window yet.".to_owned())
        );
    }

    #[test]
    fn picking_another_window_and_shutting_down_close_the_capture() {
        let mut harness = harness(false);
        let _first = harness.pick();
        assert!(harness.closed.try_recv().is_err());
        let _second = harness.pick();
        assert_eq!(harness.closed.try_recv(), Ok(7));
        let Harness {
            owner,
            messages,
            closed,
            ..
        } = harness;
        owner
            .messages
            .send(Message::Shutdown)
            .expect("owner listens");
        assert!(run(owner, &messages).is_ok());
        assert_eq!(closed.try_recv(), Ok(7));
    }
}
