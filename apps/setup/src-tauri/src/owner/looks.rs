//! Recording a value's region for its looks: starting it, and what the UI
//! shows of it. Each frame reaches it through `Owner::record`, and stopping
//! and discarding are those of any recording, in `recording.rs`.

use std::time::Instant;

use crate::check_spec::RegionSpec;
use crate::recording::Recorder;
use crate::recording::looks::{LookRecording, LookRecordingView};

use super::Owner;
use super::host::Host;

impl<H: Host> Owner<H> {
    /// Records `region` of the picked window, drawn on `screenshot`,
    /// keeping a screenshot each time it shows a look that no screenshot of
    /// that window size shows yet, within `tolerance`. Starts a new
    /// recording in place of any recording.
    pub(crate) fn start_look_recording(
        &mut self,
        region: RegionSpec,
        tolerance: u8,
        screenshot: usize,
    ) -> Result<(), String> {
        let (region, window_size) = self.recordable(region, screenshot)?;
        let id = self.next_recording_id();
        self.recording = Some(Recorder::Looks(LookRecording::new(
            id,
            region,
            tolerance,
            window_size,
            &self.screenshots,
            Instant::now(),
        )));
        Ok(())
    }

    /// The recording of looks as the interface shows it, or `None` without
    /// one.
    pub(crate) fn look_recording_state(&self) -> Option<LookRecordingView> {
        match &self.recording {
            Some(Recorder::Looks(recording)) => Some(recording.view(Instant::now())),
            Some(Recorder::Readings(_)) | None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::time::Duration;

    use appytappy_sources::screen::{FrameHandler, Image, Size};

    use crate::check_spec::{CheckSpec, ColorSpec};
    use crate::owner::messages::Message;
    use crate::owner::test_support::{FakeHost, Harness, frame_image, harness};
    use crate::pixels;
    use crate::recording::Text;
    use crate::recording::looks::MAX_LOOKS;

    use super::*;

    /// A `width` by 2 frame whose every pixel has `shade` for its red,
    /// green, and blue.
    fn shade_of(width: u32, shade: u8) -> Image {
        let pixels = usize::try_from(width)
            .expect("a small frame")
            .checked_mul(2)
            .expect("a small frame");
        Image::new(
            Size { width, height: 2 },
            [shade, shade, shade, 0].repeat(pixels),
        )
        .expect("matching length")
    }

    /// A 4 by 2 frame of `shade`, the size of [`frame_image`].
    fn shade(shade: u8) -> Image {
        shade_of(4, shade)
    }

    /// Records the looks of the whole 4 by 2 frame, drawn on screenshot 0,
    /// where shades at most 10 apart are one look.
    fn start_looks(owner: &mut Owner<FakeHost>) -> Result<(), String> {
        let region: RegionSpec =
            serde_json::from_str(r#"{"x":0,"y":0,"width":4,"height":2}"#).expect("valid JSON");
        owner.start_look_recording(region, 10, 0)
    }

    /// Records the readings of the whole 4 by 2 frame, drawn on screenshot
    /// 0, with red as ink: a new recording, or with `more` the stopped one.
    fn start_readings(more: bool) -> impl FnOnce(&mut Owner<FakeHost>) -> Result<(), String> {
        let region: RegionSpec =
            serde_json::from_str(r#"{"x":0,"y":0,"width":4,"height":2}"#).expect("valid JSON");
        let ink: ColorSpec =
            serde_json::from_str(r#"{"red":255,"green":0,"blue":0}"#).expect("valid JSON");
        move |owner: &mut Owner<FakeHost>| owner.start_recording(region, ink, 0, 0, more)
    }

    impl Harness {
        /// The recording of looks as the UI sees it.
        fn looks(&mut self) -> Option<LookRecordingView> {
            self.ask(|owner| owner.look_recording_state())
        }

        /// The screenshots the recording of looks kept.
        fn kept(&mut self) -> Vec<usize> {
            self.looks().expect("a recording of looks").kept
        }

        /// A harness that picked the window and took [`frame_image`] as
        /// screenshot 0, with the capture's handler.
        fn with_screenshot() -> (Self, FrameHandler) {
            let mut harness = harness(false);
            let mut handler = harness.pick();
            harness.deliver(&mut handler, Ok(&frame_image()));
            assert_eq!(harness.ask(Owner::screenshot), Ok(0));
            (harness, handler)
        }
    }

    #[test]
    fn a_recording_keeps_a_screenshot_of_each_look_once() {
        let (mut harness, mut handler) = Harness::with_screenshot();
        assert_eq!(harness.looks(), None);
        assert_eq!(harness.ask(start_looks), Ok(()));
        let started = harness.looks().expect("it records");
        assert!(started.recording);
        assert_eq!(started.kept, Vec::<usize>::new());
        assert_eq!(started.problem, None);
        // The looks at shades 100 and 150; 105 and 110 are within 10 of the
        // first, and the first comes back after the second.
        for frame in [
            frame_image(),
            shade(100),
            shade(105),
            shade(110),
            shade(150),
            shade(100),
            shade(160),
        ] {
            harness.deliver(&mut handler, Ok(&frame));
        }
        assert_eq!(
            harness.kept(),
            [1, 2],
            "the look of screenshot 0 is known, and each new look is kept once"
        );
        let pixels_of =
            |index| move |owner: &mut Owner<FakeHost>| owner.screenshot_pixels(index, 4);
        assert_eq!(
            harness.ask(pixels_of(1)),
            Some(pixels::rgba(&shade(100), 4)),
            "the frame that first showed the look"
        );
        assert_eq!(
            harness.ask(pixels_of(2)),
            Some(pixels::rgba(&shade(150), 4))
        );
        assert_eq!(harness.ask(pixels_of(3)), None);
        assert_eq!(
            harness.ask(Owner::screenshot),
            Ok(3),
            "a screenshot the user takes follows the recorded ones"
        );

        harness.ask(Owner::stop_recording);
        harness.deliver(&mut handler, Ok(&shade(200)));
        let stopped = harness.looks().expect("it stays to say what it kept");
        assert!(!stopped.recording);
        assert_eq!(stopped.kept, [1, 2], "a stopped recording keeps no more");
        assert_eq!(stopped.problem, None);
        assert_eq!(stopped.id, started.id);
    }

    #[test]
    fn recording_again_keeps_only_looks_no_screenshot_shows() {
        let (mut harness, mut handler) = Harness::with_screenshot();
        // A screenshot of another window size shows other pixels in the
        // region, so its look is not known.
        harness.deliver(&mut handler, Ok(&shade_of(5, 60)));
        assert_eq!(harness.ask(Owner::screenshot), Ok(1));
        harness.ask(start_looks).expect("records");
        harness.deliver(&mut handler, Ok(&shade(100)));
        harness.ask(Owner::stop_recording);
        assert_eq!(harness.kept(), [2]);

        harness.ask(start_looks).expect("records again");
        let again = harness.looks().expect("a new recording");
        assert!(again.recording);
        assert_eq!(again.kept, Vec::<usize>::new());
        for frame in [shade(100), frame_image(), shade(60)] {
            harness.deliver(&mut handler, Ok(&frame));
        }
        assert_eq!(
            harness.kept(),
            [3],
            "only the look no screenshot of this window size shows"
        );
        harness.ask(Owner::stop_recording);
        // A removed screenshot's look is not known any more.
        assert_eq!(harness.ask(|owner| owner.remove_screenshots(&[2])), Ok(()));
        harness.ask(start_looks).expect("records once more");
        harness.deliver(&mut handler, Ok(&shade(100)));
        assert_eq!(harness.kept(), [4]);
    }

    #[test]
    fn a_recording_stops_once_it_kept_as_many_screenshots_as_it_may() {
        let (mut harness, mut handler) = Harness::with_screenshot();
        harness.ask(start_looks).expect("records");
        // Shades 15 apart are each a new look.
        let look = |index: usize| {
            let index = u8::try_from(index).expect("few looks");
            shade(index.checked_mul(15).expect("a shade"))
        };
        let last = MAX_LOOKS.checked_sub(1).expect("at least one look");
        for index in 0..last {
            harness.deliver(&mut handler, Ok(&look(index)));
        }
        let room = harness.looks().expect("it records");
        assert!(room.recording, "one more fits");
        assert_eq!(room.problem, None);
        for index in [last, MAX_LOOKS] {
            harness.deliver(&mut handler, Ok(&look(index)));
        }
        let full = harness.looks().expect("it stays to say why");
        assert!(!full.recording);
        assert_eq!(
            full.kept.len(),
            MAX_LOOKS,
            "the look after the last is not kept"
        );
        assert_eq!(
            full.problem.as_deref(),
            Some(
                "Recording stopped: it kept 16 screenshots, as many as one recording keeps. \
                 Remove the ones you do not need, then start recording again to keep more."
            )
        );
        // Starting again keeps more, as a new recording.
        harness.ask(start_looks).expect("records again");
        let again = harness.looks().expect("a new recording");
        assert!(again.recording);
        assert_eq!(again.problem, None);
        assert_ne!(again.id, full.id);
    }

    #[test]
    fn a_window_of_another_size_pauses_the_recording_until_it_fits_again() {
        let (mut harness, mut handler) = Harness::with_screenshot();
        harness.ask(start_looks).expect("records");
        harness.deliver(&mut handler, Ok(&shade_of(5, 100)));
        let paused = harness.looks().expect("it records");
        assert!(paused.recording, "it goes on once the window fits");
        assert_eq!(paused.kept, Vec::<usize>::new());
        assert_eq!(
            paused.problem.as_deref(),
            Some(
                "The region was drawn when the window was 4x2, and it is 5x2 now. Resize the \
                 window to record."
            )
        );
        harness.deliver(&mut handler, Ok(&shade(100)));
        let fits = harness.looks().expect("it records");
        assert_eq!(fits.problem, None, "a fitting frame clears it");
        assert_eq!(fits.kept, [1]);
    }

    #[test]
    fn a_frame_that_took_long_makes_the_recording_rest_and_compare_none_meanwhile() {
        let (mut harness, mut handler) = Harness::with_screenshot();
        harness.ask(start_looks).expect("records");
        // The owner handles this frame ten seconds after it arrived, as if
        // it were that far behind, so the recording rests four times that.
        harness.wait(Duration::from_secs(10));
        harness.deliver(&mut handler, Ok(&shade(100)));
        assert_eq!(harness.kept(), [1]);
        harness.wait(Duration::from_secs(30));
        harness.deliver(&mut handler, Ok(&shade(150)));
        assert_eq!(
            harness.kept(),
            [1],
            "a frame that comes while it rests is not compared"
        );
        assert_eq!(
            harness.ask(|owner| owner.preview(4)),
            Ok(Some(pixels::rgba(&shade(150), 4))),
            "the frame still shows in the preview"
        );
        harness.wait(Duration::from_secs(20));
        harness.deliver(&mut handler, Ok(&shade(150)));
        assert_eq!(harness.kept(), [1, 2], "it records again once rested");
    }

    #[test]
    fn no_screenshot_is_removed_while_looks_are_recorded() {
        let (mut harness, mut handler) = Harness::with_screenshot();
        harness.ask(start_looks).expect("records");
        harness.deliver(&mut handler, Ok(&shade(100)));
        assert_eq!(
            harness.ask(|owner| owner.remove_screenshots(&[1])),
            Err("Stop recording to remove a screenshot.".to_owned())
        );
        assert!(
            harness.ask(|owner| owner.screenshot_pixels(1, 4)).is_some(),
            "the refused removal released nothing"
        );
        harness.deliver(&mut handler, Ok(&shade(100)));
        assert_eq!(harness.kept(), [1], "so the look it kept is still known");
        harness.ask(Owner::stop_recording);
        assert_eq!(harness.ask(|owner| owner.remove_screenshots(&[1])), Ok(()));
        assert_eq!(harness.ask(|owner| owner.screenshot_pixels(1, 4)), None);

        // A recording of readings holds no look, so it refuses nothing.
        harness
            .ask(start_readings(false))
            .expect("records readings");
        assert_eq!(harness.ask(|owner| owner.remove_screenshots(&[7])), Ok(()));
    }

    #[test]
    fn a_capture_failure_stops_the_recording_and_says_why() {
        let (mut harness, mut handler) = Harness::with_screenshot();
        harness.ask(start_looks).expect("records");
        harness.deliver(&mut handler, Ok(&shade(100)));
        harness.deliver(&mut handler, Err(io::Error::other("a frame was lost")));
        let stopped = harness.looks().expect("it stays to say why");
        assert!(!stopped.recording, "nothing records while frames fail");
        assert_eq!(
            stopped.problem.as_deref(),
            Some(
                "Recording stopped: A frame was lost. Start recording again once the window shows."
            )
        );
        assert_eq!(stopped.kept, [1], "what it kept stays");
        harness.deliver(&mut handler, Ok(&shade(150)));
        assert_eq!(
            harness.kept(),
            [1],
            "frames that come again do not restart it"
        );
        harness.ask(start_looks).expect("records again");
        harness.deliver(&mut handler, Ok(&shade(150)));
        let going = harness.looks().expect("a new recording");
        assert!(going.recording);
        assert_eq!(going.problem, None);
        assert_eq!(going.kept, [2]);
    }

    #[test]
    fn a_closed_window_ends_the_recording_and_no_other_starts() {
        let (mut harness, mut handler) = Harness::with_screenshot();
        assert_eq!(
            harness.ask(|owner| {
                let region: RegionSpec =
                    serde_json::from_str(r#"{"x":0,"y":0,"width":4,"height":2}"#)
                        .expect("valid JSON");
                owner.start_look_recording(region, 10, 1)
            }),
            Err("Take a screenshot first.".to_owned()),
            "the region is drawn on a screenshot"
        );
        assert_eq!(harness.looks(), None);
        harness.ask(start_looks).expect("records");
        harness.deliver(&mut handler, Ok(&shade(100)));
        harness.ask(Owner::stop_recording);
        harness.deliver(
            &mut handler,
            Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "the captured window closed",
            )),
        );
        let ended = harness.looks().expect("it stays to say why");
        assert!(!ended.recording);
        assert_eq!(
            ended.problem.as_deref(),
            Some("Recording stopped: The captured window closed. Change window to pick it again."),
            "a recording the user stopped says so too, since no other can start"
        );
        assert_eq!(
            harness.ask(start_looks),
            Err("The captured window closed. Change window to pick it again.".to_owned())
        );
        assert_eq!(harness.kept(), [1], "the refused start left it as it was");
    }

    #[test]
    fn discarding_keeps_its_screenshots_and_leaving_the_window_drops_them_too() {
        let (mut harness, mut handler) = Harness::with_screenshot();
        harness.ask(start_looks).expect("records");
        harness.deliver(&mut handler, Ok(&shade(100)));
        harness.ask(Owner::discard_recording);
        assert_eq!(harness.looks(), None);
        assert!(
            harness.ask(|owner| owner.screenshot_pixels(1, 4)).is_some(),
            "the screenshots it kept stay"
        );
        harness.deliver(&mut handler, Ok(&shade(150)));
        assert_eq!(harness.looks(), None, "nothing records without a recording");

        harness.ask(start_looks).expect("records");
        let mut next = harness.pick();
        assert_eq!(harness.looks(), None, "another window drops it");
        assert_eq!(harness.ask(|owner| owner.screenshot_pixels(1, 4)), None);
        harness.deliver(&mut next, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        harness.ask(start_looks).expect("records");
        harness.ask(Owner::stop);
        assert_eq!(harness.looks(), None, "leaving the flow drops it");
    }

    #[test]
    fn one_recording_runs_at_a_time_and_each_has_its_own_number() {
        let (mut harness, mut handler) = Harness::with_screenshot();
        harness
            .ask(start_readings(false))
            .expect("records readings");
        harness.deliver(&mut handler, Ok(&frame_image()));
        let jobs: Vec<(u64, usize)> = harness
            .jobs
            .try_iter()
            .map(|job| (job.recording, job.index))
            .collect();
        assert_eq!(jobs, [(1, 0)]);

        harness.ask(start_looks).expect("records looks instead");
        assert_eq!(
            harness.ask(|owner| owner.recording_state()),
            None,
            "the recording of readings is gone"
        );
        assert_eq!(harness.looks().expect("it records").id, 2);
        assert_eq!(
            harness.ask(start_readings(true)),
            Err(
                "That recording is gone, so there is nothing to record more of. Start a new \
                 recording."
                    .to_owned()
            ),
            "a recording of looks has no readings to record more of"
        );
        assert!(harness.looks().is_some(), "the refused start left it");
        let late = |recording| Message::Recognized {
            recording,
            index: 0,
            text: Text::Read("1".to_owned()),
        };
        assert!(harness.owner.handle(late(1)).is_continue());
        harness.deliver(&mut handler, Ok(&shade(100)));
        assert_eq!(harness.jobs.try_iter().count(), 0, "it reads no text");
        assert_eq!(harness.kept(), [1]);

        harness
            .ask(start_readings(false))
            .expect("records readings again");
        assert_eq!(harness.looks(), None, "the recording of looks is gone");
        harness.deliver(&mut handler, Ok(&frame_image()));
        let jobs: Vec<(u64, usize)> = harness
            .jobs
            .try_iter()
            .map(|job| (job.recording, job.index))
            .collect();
        assert_eq!(jobs, [(3, 0)]);
        // Text for the first recording of readings, which a recording of
        // looks replaced, and for the recording of looks' own number.
        for recording in [1, 2] {
            assert!(harness.owner.handle(late(recording)).is_continue());
        }
        let text_of = |harness: &mut Harness| {
            let view = harness
                .ask(|owner| owner.recording_state())
                .expect("it records");
            view.readings
                .into_iter()
                .map(|reading| reading.text)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            text_of(&mut harness),
            [Text::Pending],
            "late text is dropped"
        );
        assert!(harness.owner.handle(late(3)).is_continue());
        assert_eq!(text_of(&mut harness), [Text::Read("1".to_owned())]);
        assert_eq!(
            harness.ask(|owner| owner.screenshot_pixels(1, 4)),
            Some(pixels::rgba(&shade(100), 4)),
            "the screenshot the recording of looks kept stays"
        );
    }

    #[test]
    fn a_removed_screenshot_is_gone_and_the_others_keep_their_indexes() {
        let (mut harness, mut handler) = Harness::with_screenshot();
        harness.deliver(&mut handler, Ok(&shade(100)));
        assert_eq!(harness.ask(Owner::screenshot), Ok(1));
        harness.deliver(&mut handler, Ok(&shade(150)));
        assert_eq!(harness.ask(Owner::screenshot), Ok(2));
        assert_eq!(
            harness.ask(|owner| owner.remove_screenshots(&[1, 7])),
            Ok(())
        );
        let pixels_of =
            |index| move |owner: &mut Owner<FakeHost>| owner.screenshot_pixels(index, 4);
        assert_eq!(
            harness.ask(pixels_of(0)),
            Some(pixels::rgba(&frame_image(), 4))
        );
        assert_eq!(harness.ask(pixels_of(1)), None);
        assert_eq!(
            harness.ask(pixels_of(2)),
            Some(pixels::rgba(&shade(150), 4))
        );
        let state_from = |screenshot: usize| -> CheckSpec {
            serde_json::from_str(&format!(
                r#"{{"kind":"state","region":{{"x":0,"y":0,"width":4,"height":2}},"drawnOn":0,
                    "states":[{{"name":"Shown","screenshot":{screenshot}}}],"tolerance":0}}"#
            ))
            .expect("valid JSON")
        };
        assert_eq!(
            harness.ask(|owner| owner.set_check(&state_from(1))),
            Err("That screenshot is gone. Take a new one and try again.".to_owned())
        );
        assert_eq!(harness.ask(|owner| owner.set_check(&state_from(2))), Ok(()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(3));
    }
}
