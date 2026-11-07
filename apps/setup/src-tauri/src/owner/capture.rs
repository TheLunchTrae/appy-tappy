//! The picked window: listing windows, starting and stopping its capture,
//! keeping screenshots, and reading the check the user sets up on its frames.

use std::io;
use std::time::Duration;

use appytappy_config::signal::Value;
use appytappy_sources::screen::{Crop, Frame, FrameHandler, Image};

use crate::check_spec::RegionCheck;
use crate::window_list::{self, WindowList};

use super::host::Host;
use super::messages::{Message, Reading};
use super::{Owner, PREVIEW_INTERVAL};

/// A preview capture's message for one frame.
fn preview_message(generation: u64, frame: io::Result<Frame<'_>>) -> Message {
    match frame {
        Ok(frame) => Message::Frame {
            generation,
            image: frame.image.clone(),
            received: frame.received,
        },
        Err(error) => Message::CaptureFailed {
            generation,
            error: error.to_string(),
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
    pub(super) fn window_list(&self) -> Result<WindowList, String> {
        let windows = self.host.windows().map_err(|error| error.to_string())?;
        let processes = self.host.processes().map_err(|error| error.to_string())?;
        Ok(window_list::list(windows, &processes, self.own_process))
    }

    /// Stops the capture and forgets its frames, screenshots, recording, and
    /// check.
    pub(super) fn stop(&mut self) {
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

    pub(super) fn pick(&mut self, window: u64) -> Result<(), String> {
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
        to_message: fn(u64, io::Result<Frame<'_>>) -> Message,
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

    pub(super) fn screenshot(&mut self) -> Result<usize, String> {
        let image = self
            .latest
            .clone()
            .ok_or_else(|| "No frame has arrived from the window yet.".to_owned())?;
        self.screenshots.push(image);
        Ok(self.screenshots.len().saturating_sub(1))
    }

    pub(super) fn reading(&self) -> Result<Option<Reading>, String> {
        let (Some(check), Some(latest)) = (&self.check, &self.latest) else {
            return Ok(None);
        };
        read(check, latest).map(|value| Some(value.into()))
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::app_file::WindowDef;
    use appytappy_sources::screen::Size;

    use crate::check_spec::CheckSpec;
    use crate::owner::messages::Request;
    use crate::owner::run;
    use crate::owner::test_support::{Harness, frame_image, harness};
    use crate::pixels;
    use crate::window_list::WindowFit;

    use super::*;

    #[test]
    fn lists_windows_without_the_setup_apps_own() {
        let mut harness = harness(false);
        let list = harness.ask(Request::ListWindows).expect("the host answers");
        assert_eq!(list.windows.len(), 1);
        assert_eq!(list.windows[0].executable, "Example.exe");
    }

    #[test]
    fn previews_the_newest_frame_once_one_arrives() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        let preview = |reply| Request::Preview {
            max_width: 100,
            reply,
        };
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
        let preview = |reply| Request::Preview {
            max_width: 100,
            reply,
        };
        assert_eq!(
            harness.ask(preview),
            Err("the captured window closed".to_owned())
        );
        let _next = harness.pick();
        assert_eq!(harness.ask(preview), Ok(None));
    }

    #[test]
    fn frames_from_a_window_the_user_left_are_dropped() {
        let mut harness = harness(false);
        let mut first = harness.pick();
        let _second = harness.pick();
        harness.deliver(&mut first, Ok(&frame_image()));
        harness.deliver(&mut first, Err(io::Error::other("closed")));
        let preview = |reply| Request::Preview {
            max_width: 100,
            reply,
        };
        assert_eq!(harness.ask(preview), Ok(None));
    }

    #[test]
    fn a_capture_that_cannot_start_says_why() {
        let mut harness = harness(true);
        assert_eq!(
            harness.ask(|reply| Request::Pick { window: 7, reply }),
            Err("no capture here".to_owned())
        );
    }

    #[test]
    fn screenshots_keep_frames_until_another_window_is_picked() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        assert!(harness.ask(Request::Screenshot).is_err());
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        assert_eq!(harness.ask(Request::Screenshot), Ok(1));
        let pixels_of = |index| {
            move |reply| Request::ScreenshotPixels {
                index,
                max_width: 2,
                reply,
            }
        };
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
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        let select = |screenshot, x| {
            move |reply| Request::Select {
                screenshot,
                selection: serde_json::from_value(serde_json::json!({
                    "tolerance": 10,
                    "steps": [{"kind": "click", "x": x, "y": 0}]
                }))
                .expect("the UI's spelling"),
                reply,
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
    fn the_check_reads_the_live_frame() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        assert_eq!(harness.ask(Request::Reading), Ok(None));
        let spec: CheckSpec = serde_json::from_str(
            r#"{"kind":"barFill","region":{"x":0,"y":0,"width":4,"height":2},
                "filled":{"red":255,"green":0,"blue":0},"empty":{"red":40,"green":40,"blue":40}}"#,
        )
        .expect("valid JSON");
        harness
            .ask(|reply| Request::SetCheck { spec, reply })
            .expect("valid check");
        assert_eq!(harness.ask(Request::Reading), Ok(None));
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(
            harness.ask(Request::Reading),
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
            .ask(|reply| Request::SetCheck { spec, reply })
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
                .ask(Request::Reading)
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
            harness.ask(|reply| Request::SetCheck { spec, reply }),
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
            harness.ask(|reply| Request::MatchWindows { window, reply }),
            Ok(vec![WindowFit {
                handle: 7,
                fallback: false,
                best: true
            }])
        );
    }

    #[test]
    fn stopping_closes_the_capture_and_forgets_the_screenshots() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        harness.ask(Request::Stop);
        assert_eq!(harness.closed.try_recv(), Ok(7));
        assert_eq!(
            harness.ask(|reply| Request::ScreenshotPixels {
                index: 0,
                max_width: 100,
                reply,
            }),
            None
        );
        assert_eq!(
            harness.ask(Request::Screenshot),
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
