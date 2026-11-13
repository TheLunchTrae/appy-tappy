//! The picked window: listing windows, starting and stopping its capture,
//! keeping screenshots, and reading the check the user sets up on its frames.

use std::io;
use std::time::{Duration, Instant};

use appytappy_config::app_file::WindowDef;
use appytappy_sources::screen::{Crop, Frame, FrameHandler, Image};
use appytappy_sources::system::{ProcessEntry, WindowEntry};

use crate::check_spec::bar::{self, BarDetection, BarMeasure};
use crate::check_spec::{self, ColorSpec, RegionSpec};
use crate::debug::workings::{self, ReadingDetails};
use crate::match_preview::{self, MatchRule, MatchView};
use crate::pixels;
use crate::recording::Recorder;
use crate::selection::{self, SelectionSpec, SelectionView};
use crate::window_list::{self, WindowChoice, WindowFit, WindowList, WindowScore};

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

    /// Stops the capture and forgets its frames, screenshots, recording,
    /// search, and check.
    pub(crate) fn stop(&mut self) {
        // A capture that failed already said why; what comes next matters now.
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the old capture's error is moot once the user moved on"
        )]
        let _ = self.close();
        self.generation = self.generation.wrapping_add(1);
        self.latest = None;
        self.latest_at = None;
        self.failure = None;
        self.screenshots.clear();
        self.check = None;
        self.recording = None;
        self.found = None;
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
        if let Some(check) = &mut self.check {
            check.place(image.size());
        }
        self.latest = Some(image);
        self.latest_at = Some(received);
    }

    /// Notes that the capture failed, and stops a recording, which records
    /// nothing until frames come again, with the failure as its problem.
    pub(super) fn capture_failed(&mut self, failure: CaptureFailure) {
        if let Some(recording) = &mut self.recording {
            let next = recording.after_failure();
            let recording = recording.life_mut();
            let error = check_spec::sentence(&failure.error);
            if failure.ended {
                recording.end(
                    Instant::now(),
                    format!("Recording stopped: {error} Change window to pick it again."),
                );
            } else {
                recording.fail(Instant::now(), format!("Recording stopped: {error} {next}"));
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
        Ok(self.screenshots.push(image))
    }

    /// Releases each of the screenshots `indexes` names. The others keep
    /// their indexes. Refused while a recording of looks records: it knows
    /// each look it kept, so it would not keep a removed one again.
    pub(crate) fn remove_screenshots(&mut self, indexes: &[usize]) -> Result<(), String> {
        if let Some(Recorder::Looks(recording)) = &self.recording
            && recording.life.is_recording()
        {
            return Err("Stop recording to remove a screenshot.".to_owned());
        }
        for &index in indexes {
            self.screenshots.remove(index);
        }
        Ok(())
    }

    /// What the check reads on the newest frame: `None` until a check is
    /// set up and a frame arrived.
    pub(crate) fn reading(&self) -> Result<Option<Reading>, String> {
        let (Some(check), Some(latest)) = (&self.check, &self.latest) else {
            return Ok(None);
        };
        check.read(latest).map(|value| Some(value.into()))
    }

    /// How the check reads the newest frame, for the debug details: `None`
    /// until a check is set up and a frame arrived.
    pub(crate) fn explain_reading(&self) -> Result<Option<ReadingDetails>, String> {
        let (Some(check), Some(latest), Some(arrived)) =
            (&self.check, &self.latest, self.latest_at)
        else {
            return Ok(None);
        };
        let (check, region) = check.region_of(latest)?;
        let age = self.host.now().saturating_duration_since(arrived);
        Ok(Some(workings::details(check, &region, age)))
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
        bar::detect(self.screenshots.slots(), region, screenshot).map_err(|error| error.to_string())
    }

    /// What a bar of the colors `filled` and `empty` reads in `region`,
    /// drawn on screenshot `screenshot`, on each screenshot of that one's
    /// window size.
    pub(crate) fn measure_bar(
        &self,
        region: RegionSpec,
        screenshot: usize,
        filled: ColorSpec,
        empty: ColorSpec,
    ) -> Result<BarMeasure, String> {
        bar::measure(self.screenshots.slots(), region, screenshot, filled, empty)
            .map_err(|error| error.to_string())
    }

    /// The pixels of `area` on screenshot `screenshot` that `rule` takes.
    pub(crate) fn match_pixels(
        &self,
        screenshot: usize,
        area: RegionSpec,
        rule: MatchRule,
    ) -> Result<MatchView, String> {
        self.screenshots
            .get(screenshot)
            .ok_or(check_spec::SpecError::NoScreenshot)
            .and_then(|image| match_preview::find(image, area, rule))
            .map_err(|error| error.to_string())
    }

    /// The running windows that `window` matches, the one a new session
    /// would read first.
    pub(crate) fn match_windows(&self, window: &WindowDef) -> Result<Vec<WindowFit>, String> {
        let (offered, read) = self.offered_and_read(window)?;
        Ok(window_list::matching(&offered, window, read))
    }

    /// How each open window compares with `window`, for the debug details.
    pub(crate) fn window_scores(&self, window: &WindowDef) -> Result<Vec<WindowScore>, String> {
        let (offered, read) = self.offered_and_read(window)?;
        Ok(window_list::scores(&offered, window, read))
    }

    /// The windows the user can pick, and the one a new session would read
    /// for `window`, if any.
    fn offered_and_read(
        &self,
        window: &WindowDef,
    ) -> Result<(Vec<WindowChoice>, Option<u64>), String> {
        let (processes, windows) = self.desktop()?;
        let read = self
            .host
            .read_window(&window.to_match(), processes.clone(), windows.clone())
            .map(|read| read.entry.handle);
        Ok((
            window_list::list(windows, &processes, self.own_process).windows,
            read,
        ))
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
    use tokio::sync::oneshot;

    use crate::check_spec::{ColorSpec, RegionSpec};
    use crate::owner::messages::Run;
    use crate::owner::run;
    use crate::owner::test_support::{
        FakeHost, Harness, STATE, capturing, characters_in, frame_image, harness, trial,
    };
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
                    .measure_bar(region, screenshot, red, gray)
                    .map(|measure| serde_json::to_value(measure).expect("plain data"))
            }
        };
        assert_eq!(
            harness.ask(measure(0)),
            Ok(serde_json::json!({
                "readings": [{"screenshot": 0, "fill": 50.0}],
                "leftOut": 0,
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
    fn matching_pixels_are_found_on_the_named_screenshot() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        let matching = |screenshot, width| {
            move |owner: &mut Owner<FakeHost>| {
                owner
                    .match_pixels(
                        screenshot,
                        serde_json::from_value(serde_json::json!(
                            {"x": 0, "y": 0, "width": width, "height": 2}
                        ))
                        .expect("the UI's spelling"),
                        serde_json::from_value(serde_json::json!({
                            "kind": "ink",
                            "ink": {"red": 255, "green": 0, "blue": 0},
                            "tolerance": 0
                        }))
                        .expect("the UI's spelling"),
                    )
                    .map(|view| serde_json::to_value(view).expect("plain data"))
            }
        };
        assert_eq!(
            harness.ask(matching(0, 4)),
            Ok(serde_json::json!({
                "kind": "ink",
                "ink": {"count": 4, "spans": [[0, 2], [4, 6]]}
            })),
            "the red half of each row"
        );
        assert_eq!(
            harness.ask(matching(0, 5)),
            Err(
                "The region reaches past the edge of the window. Draw it inside the window."
                    .to_owned()
            )
        );
        assert_eq!(
            harness.ask(matching(1, 4)),
            Err("That screenshot is gone. Take a new one and try again.".to_owned())
        );
    }

    #[test]
    fn the_reading_s_details_wait_for_a_check_and_a_frame() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        assert_eq!(harness.ask(|owner| owner.explain_reading()), Ok(None));
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(
            harness.ask(|owner| owner.explain_reading()),
            Ok(None),
            "no check yet"
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
                best: true,
                tied: true,
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
        // The two copies fit equally well, so each ties with the one read.
        let fit = |handle, best| WindowFit {
            handle,
            fallback: false,
            best,
            tied: true,
        };
        assert_eq!(
            harness.ask(|owner| owner.match_windows(&window)),
            Ok(vec![fit(7, true), fit(6, false)])
        );
        assert_eq!(harness.ask(|owner| owner.read_window(&window)), Ok(Some(7)));
        let scores = harness
            .ask(|owner| owner.window_scores(&window))
            .expect("scores");
        assert_eq!(
            scores
                .iter()
                .map(|score| (score.handle, score.fit.map(|fit| fit.best)))
                .collect::<Vec<_>>(),
            [(6, Some(false)), (7, Some(true))],
            "each listed window, with the one the runtime reads marked"
        );
        harness.owner.host.minimized = true;
        assert_eq!(
            harness.ask(|owner| owner.match_windows(&window)),
            Ok(vec![fit(6, true), fit(7, false)]),
            "neither shows, so the first listed"
        );
    }

    #[test]
    fn stopping_closes_the_capture_and_releases_every_picture_it_held() {
        let (mut harness, mut handler, root) = capturing("capture-stop");
        let left: RegionSpec =
            serde_json::from_str(r#"{"x":0,"y":0,"width":2,"height":2}"#).expect("valid JSON");
        let red: ColorSpec =
            serde_json::from_str(r#"{"red":255,"green":0,"blue":0}"#).expect("valid JSON");
        harness
            .ask(|owner| owner.start_recording(characters_in(left, red), false))
            .expect("records");
        harness.deliver(&mut handler, Ok(&frame_image()));
        harness
            .ask(|owner| owner.set_check(&trial(STATE)))
            .expect("a valid check");
        // What the page can ask for of the picked window: the newest frame,
        // a screenshot, a recorded reading, the recording, and what the
        // check, which holds a state's picture, reads.
        let held = |owner: &mut Owner<FakeHost>| {
            [
                owner.preview(4).expect("no failure").is_some(),
                owner.screenshot_pixels(0, 4).is_some(),
                owner.recorded_pixels(0, 4).is_some(),
                owner.recording_state().is_some(),
                owner.reading().expect("reads").is_some(),
            ]
        };
        assert_eq!(harness.ask(held), [true; 5]);

        // The page's `stop_capture`, as it reaches the owner when a flow
        // closes or the user selects Change window.
        let (reply, mut answer) = oneshot::channel();
        let stop = Message::Run(Run::new(reply, Owner::stop));
        assert!(harness.owner.handle(stop).is_continue());
        assert_eq!(answer.try_recv(), Ok(()));
        assert_eq!(harness.closed.try_recv(), Ok(7));
        assert_eq!(harness.ask(held), [false; 5]);
        // A frame the stopped capture sends late is not kept either.
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(held), [false; 5]);
        assert_eq!(
            harness.ask(Owner::screenshot),
            Err("No frame has arrived from the window yet.".to_owned())
        );
        std::fs::remove_dir_all(root).expect("clean up");
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
