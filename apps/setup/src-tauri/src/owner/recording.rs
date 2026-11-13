//! Recording a region: what both kinds of recording share, and recording
//! readings, starting it and sending each new reading it shows to the text
//! reader. Recording looks is in `looks.rs`.

use std::time::Instant;

use appytappy_sources::screen::digits::DigitReader;
use appytappy_sources::screen::{Image, Region, Size};
use appytappy_sources::system::Rgb;

use crate::check_spec::{self, CharactersSpec, RegionSpec};
use crate::owner::messages::RecordSpec;
use crate::pixels;
use crate::recording::{Recorder, Recording, RecordingView, Scope, Text, find, ocr};

use super::Owner;
use super::host::Host;

impl<H: Host> Owner<H> {
    /// Records `frame`, which arrived at `received`: sends a new reading it
    /// shows to the text reader, or keeps it as a screenshot when it shows
    /// a new look.
    pub(super) fn record(&mut self, frame: &Image, received: Instant) {
        let recording = match &mut self.recording {
            Some(Recorder::Readings(recording)) => recording,
            Some(Recorder::Looks(recording)) => {
                let host = &self.host;
                recording.offer(frame, received, || host.now(), &mut self.screenshots);
                return;
            },
            None => return,
        };
        for new in recording.offer(frame, received) {
            let job = ocr::Job {
                source: ocr::Source::Recording(recording.id),
                index: new.index,
                line: new.line,
            };
            if self.ocr.send(job).is_err() {
                recording.read(
                    new.index,
                    Text::Failed("The text reader stopped. Type what this shows.".to_owned()),
                );
            }
        }
    }

    /// The recording of readings, if that is what the capture records.
    pub(super) const fn readings(&self) -> Option<&Recording> {
        match &self.recording {
            Some(Recorder::Readings(recording)) => Some(recording),
            Some(Recorder::Looks(_)) | None => None,
        }
    }

    /// The recording of readings, to change it.
    pub(super) const fn readings_mut(&mut self) -> Option<&mut Recording> {
        match &mut self.recording {
            Some(Recorder::Readings(recording)) => Some(recording),
            Some(Recorder::Looks(_)) | None => None,
        }
    }

    /// The next recording's number.
    pub(super) const fn next_recording_id(&mut self) -> u64 {
        self.recording_id = self.recording_id.wrapping_add(1);
        self.recording_id
    }

    /// The region `region` describes and the size of the window it was
    /// drawn on, that of `screenshot`, when the picked window can be
    /// recorded there; otherwise what to do first.
    pub(super) fn recordable(
        &self,
        region: RegionSpec,
        screenshot: usize,
    ) -> Result<(Region, Size), String> {
        if self.session.is_none() {
            return Err("Pick a window to record.".to_owned());
        }
        // A closed window sends no more frames, so a recording would count
        // up with nothing in it.
        if let Some(failure) = self.failure.as_ref().filter(|failure| failure.ended) {
            return Err(format!(
                "{} Change window to pick it again.",
                check_spec::sentence(&failure.error)
            ));
        }
        if self.screenshots.get(screenshot).is_none() {
            return Err("Take a screenshot first.".to_owned());
        }
        check_spec::drawn(&self.screenshots, region, screenshot).map_err(|error| error.to_string())
    }

    /// Records what `spec` names of the picked window, keeping each reading
    /// it shows in the characters' color: in the region alone, or wherever
    /// the window shows text like the region's. With `more`, goes on with
    /// the stopped recording, which must be of the same region, ink, window
    /// size, and scope; otherwise starts a new one in place of any
    /// recording.
    pub(crate) fn start_recording(&mut self, spec: RecordSpec, more: bool) -> Result<(), String> {
        let (region, window_size) = self.recordable(spec.region, spec.screenshot)?;
        let settings: (Rgb, u8) = (spec.ink.into(), spec.tolerance);
        let now = Instant::now();
        match &mut self.recording {
            Some(Recorder::Readings(recording))
                if more && recording.records(region, settings, window_size, spec.scope) =>
            {
                recording.life.resume(now);
            },
            Some(Recorder::Readings(recording)) if more => {
                return Err(format!(
                    "Recording more needs the settings it was recorded with: the same place, \
                     the characters' color, a tolerance of {}, a screenshot of the same window \
                     size, and the same choice of where to look. Use its settings, or discard \
                     the recording to start a new one.",
                    recording.view(now).settings.tolerance
                ));
            },
            Some(Recorder::Looks(_)) | None if more => {
                return Err(
                    "That recording is gone, so there is nothing to record more of. Start a new \
                     recording."
                        .to_owned(),
                );
            },
            _ => {
                // The whole window is searched for text as tall as the
                // region's on its screenshot, so that is measured first.
                let sample = match spec.scope {
                    Scope::Place => None,
                    Scope::Window => Some(self.sample_height(region, spec.screenshot, settings)?),
                };
                let id = self.next_recording_id();
                let recording = Recording::new(id, region, settings, window_size, now);
                self.recording = Some(Recorder::Readings(match sample {
                    Some(sample) => recording.across_window(sample),
                    None => recording,
                }));
            },
        }
        Ok(())
    }

    /// How tall the characters are that `region` of screenshot `screenshot`
    /// shows in the characters' color, or what to do when it shows none.
    fn sample_height(
        &self,
        region: Region,
        screenshot: usize,
        (ink, tolerance): (Rgb, u8),
    ) -> Result<u32, String> {
        self.screenshots
            .get(screenshot)
            .and_then(|image| find::sample_height(image, region, &DigitReader::new(ink, tolerance)))
            .ok_or_else(|| {
                "That place shows no characters in that color, so there is nothing to look for in \
                 the rest of the window. Draw it around some characters and pick their color \
                 first."
                    .to_owned()
            })
    }

    /// Stops the recording, of readings or of looks, which keeps what it
    /// recorded.
    pub(crate) fn stop_recording(&mut self) {
        if let Some(recording) = &mut self.recording {
            recording.life_mut().stop(Instant::now());
        }
    }

    /// Forgets the recording, of readings or of looks. The screenshots a
    /// recording of looks kept stay.
    pub(crate) fn discard_recording(&mut self) {
        self.recording = None;
    }

    /// The recording of readings as the interface shows it, or `None`
    /// without one.
    pub(crate) fn recording_state(&self) -> Option<RecordingView> {
        self.readings()
            .map(|recording| recording.view(Instant::now()))
    }

    /// Recorded reading `index`'s pixels, at most `max_width` wide.
    pub(crate) fn recorded_pixels(&self, index: usize, max_width: u32) -> Option<Vec<u8>> {
        self.readings()
            .and_then(|recording| recording.readings().get(index))
            .map(|kept| pixels::rgba(&kept.image, max_width))
    }

    /// For each recorded reading, why `spec` reads it wrong, or `None` when
    /// it reads it right; an error when the labeled screenshots cannot be
    /// learned.
    pub(crate) fn review_problems(
        &self,
        spec: &CharactersSpec,
    ) -> Result<Vec<Option<String>>, String> {
        self.with_session(|session| spec.recorded_problems(session))
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::mpsc;

    use appytappy_sources::screen::Size;

    use crate::check_spec::ColorSpec;
    use crate::owner::messages::Message;
    use crate::owner::test_support::{
        FakeHost, Harness, capturing, characters_in, frame_image, harness, teaching,
    };
    use crate::pixels;
    use crate::recording::RecordingView;

    use super::*;

    /// A 4 by 2 frame with its left `red` columns red and the rest gray.
    fn red_columns(red: usize) -> Image {
        red_where([0, 1, 2, 3].map(|x| x < red))
    }

    /// A 4 by 2 frame whose columns are red where `red` says so and gray
    /// elsewhere.
    fn red_where(red: [bool; 4]) -> Image {
        let row: Vec<u8> = red
            .into_iter()
            .flat_map(|red| if red { [0, 0, 255, 0] } else { [40, 40, 40, 0] })
            .collect();
        Image::new(
            Size {
                width: 4,
                height: 2,
            },
            [row.clone(), row].concat(),
        )
        .expect("matching length")
    }

    /// Records the whole 4 by 2 frame, red as ink, drawn on screenshot 0.
    fn start_recording(
        red: u8,
        more: bool,
    ) -> impl FnOnce(&mut Owner<FakeHost>) -> Result<(), String> {
        let region: RegionSpec =
            serde_json::from_str(r#"{"x":0,"y":0,"width":4,"height":2}"#).expect("valid JSON");
        let ink: ColorSpec =
            serde_json::from_str(&format!(r#"{{"red":{red},"green":0,"blue":0}}"#))
                .expect("valid JSON");
        move |owner: &mut Owner<FakeHost>| owner.start_recording(characters_in(region, ink), more)
    }

    impl Harness {
        /// The recording as the UI sees it.
        fn recording(&mut self) -> Option<RecordingView> {
            self.ask(|owner| owner.recording_state())
        }

        /// Each reading's sightings and text.
        fn readings(&mut self) -> Vec<(u32, Text)> {
            self.recording()
                .expect("a recording")
                .readings
                .into_iter()
                .map(|reading| (reading.seen, reading.text))
                .collect()
        }
    }

    #[test]
    fn a_recording_keeps_each_reading_once_and_has_it_read_as_text() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        assert_eq!(harness.recording(), None);
        assert_eq!(harness.ask(start_recording(255, false)), Ok(()));
        for frame in [
            red_columns(2),
            red_columns(2),
            red_columns(0),
            red_columns(3),
        ] {
            harness.deliver(&mut handler, Ok(&frame));
        }
        let jobs: Vec<(u64, usize)> = harness
            .jobs
            .try_iter()
            .map(|job| match job.source {
                ocr::Source::Recording(recording) => (recording, job.index),
                ocr::Source::Search(_) => panic!("no search was made"),
            })
            .collect();
        assert_eq!(jobs, [(1, 0), (1, 1)], "one job per new reading");
        assert_eq!(
            harness.readings(),
            [(2, Text::Pending), (1, Text::Pending)],
            "a frame with no ink is skipped"
        );
        for (recording, text) in [(0, "stale"), (1, "1")] {
            assert!(
                harness
                    .owner
                    .handle(Message::Recognized {
                        recording,
                        index: 0,
                        text: Text::Read(text.to_owned()),
                    })
                    .is_continue()
            );
        }
        assert_eq!(
            harness.readings(),
            [(2, Text::Read("1".to_owned())), (1, Text::Pending)]
        );
        let pixels_of = |index| move |owner: &mut Owner<FakeHost>| owner.recorded_pixels(index, 4);
        assert_eq!(
            harness.ask(pixels_of(1)),
            Some(pixels::rgba(&red_columns(3), 4))
        );
        assert_eq!(harness.ask(pixels_of(2)), None);

        harness.ask(Owner::stop_recording);
        harness.deliver(&mut handler, Ok(&red_columns(4)));
        let stopped = harness.recording().expect("kept for review");
        assert!(!stopped.recording);
        assert_eq!(
            stopped.readings.len(),
            2,
            "a stopped recording keeps no more"
        );
        assert_eq!(harness.jobs.try_iter().count(), 0);
    }

    #[test]
    fn a_frame_of_shapes_already_kept_is_counted_and_not_read_as_text() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        harness.ask(start_recording(255, false)).expect("records");
        // A stroke one column wide, one two wide, then both in each order.
        for red in [
            [true, false, false, false],
            [true, true, false, false],
            [true, false, true, true],
            [true, true, false, true],
        ] {
            harness.deliver(&mut handler, Ok(&red_where(red)));
        }
        let jobs: Vec<usize> = harness.jobs.try_iter().map(|job| job.index).collect();
        assert_eq!(jobs, [0, 1], "only a reading with a new shape is read");
        let view = harness.recording().expect("a recording");
        let kept: Vec<&[usize]> = view
            .readings
            .iter()
            .map(|reading| reading.shapes.as_slice())
            .collect();
        assert_eq!(kept, [&[0][..], &[1]]);
        let sightings: Vec<(u32, u32)> = view
            .shapes
            .iter()
            .map(|shape| (shape.frames, shape.readings))
            .collect();
        assert_eq!(sightings, [(3, 3), (3, 3)]);
    }

    #[test]
    fn review_and_the_check_learn_from_recorded_readings() {
        let (mut harness, mut handler, root) = capturing("recording-review");
        harness.ask(start_recording(255, false)).expect("records");
        harness.deliver(&mut handler, Ok(&red_columns(2)));
        harness.deliver(&mut handler, Ok(&red_columns(3)));
        // Screenshot 0 shows the same two columns as reading 0.
        let review = |labels: &str, recorded: &[(usize, &str)]| {
            let recorded: Vec<String> = recorded
                .iter()
                .map(|(reading, text)| format!(r#"{{"reading":{reading},"text":"{text}"}}"#))
                .collect();
            let spec: CharactersSpec = serde_json::from_str(&format!(
                r#"{{"region":{{"x":0,"y":0,"width":4,"height":2}},"screenshot":0,
                    "ink":{{"red":255,"green":0,"blue":0}},"tolerance":0,"labels":[{labels}],
                    "recorded":[{}]}}"#,
                recorded.join(",")
            ))
            .expect("valid JSON");
            move |owner: &mut Owner<FakeHost>| owner.review_problems(&spec)
        };
        let problems = harness
            .ask(review("", &[(0, "1"), (1, "12"), (1, "7"), (9, "1")]))
            .expect("no labels to learn");
        assert_eq!(problems[0], None);
        assert_eq!(
            problems[1].as_deref(),
            Some(
                "The region shows 1 characters but the text has 2; characters that touch count \
                 as one."
            ),
            "one blob cannot be two characters"
        );
        assert_eq!(problems[2], None, "judged without the reading that failed");
        assert_eq!(
            problems[3].as_deref(),
            Some("That recording is gone. Record the region again.")
        );
        let labeled = harness
            .ask(review(
                r#"{"screenshot":0,"text":"7"}"#,
                &[(0, "1"), (1, "1")],
            ))
            .expect("the label is learned");
        assert!(
            labeled[0].is_some(),
            "the labeled screenshot gave the same shape to another character"
        );
        assert_eq!(labeled[1], None);
        let number = |reading| -> CharactersSpec {
            serde_json::from_str(&format!(
                r#"{{"region":{{"x":0,"y":0,"width":4,"height":2}},"screenshot":0,
                    "ink":{{"red":255,"green":0,"blue":0}},"tolerance":0,"labels":[],
                    "recorded":[{{"reading":{reading},"text":"1"}}]}}"#
            ))
            .expect("valid JSON")
        };
        assert_eq!(
            harness.ask(|owner| owner.set_characters(&teaching(number(0)))),
            Ok(())
        );
        harness.ask(Owner::discard_recording);
        assert_eq!(harness.recording(), None);
        assert_eq!(
            harness.ask(|owner| owner.set_characters(&teaching(number(0)))),
            Err("That recording is gone. Record the region again.".to_owned())
        );
        assert_eq!(
            harness.ask(review("", &[(0, "1")])),
            Ok(vec![Some(
                "That recording is gone. Record the region again.".to_owned()
            )])
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// Records the whole window for text like columns `x` and on of the 4
    /// by 2 screenshot 0, red as ink.
    fn start_window_recording(
        x: u32,
        more: bool,
    ) -> impl FnOnce(&mut Owner<FakeHost>) -> Result<(), String> {
        let region: RegionSpec = serde_json::from_str(&format!(
            r#"{{"x":{x},"y":0,"width":{},"height":2}}"#,
            4_u32.saturating_sub(x)
        ))
        .expect("valid JSON");
        let ink: ColorSpec =
            serde_json::from_str(r#"{"red":255,"green":0,"blue":0}"#).expect("valid JSON");
        move |owner: &mut Owner<FakeHost>| {
            owner.start_recording(
                RecordSpec {
                    scope: Scope::Window,
                    ..characters_in(region, ink)
                },
                more,
            )
        }
    }

    #[test]
    fn a_whole_window_is_recorded_for_text_as_tall_as_the_region_s_on_its_screenshot() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        // Columns 2 and 3 of the screenshot are gray.
        assert_eq!(
            harness.ask(start_window_recording(2, false)),
            Err(
                "That place shows no characters in that color, so there is nothing to look for in \
                 the rest of the window. Draw it around some characters and pick their color \
                 first."
                    .to_owned()
            )
        );
        assert_eq!(harness.recording(), None, "nothing started");
        harness
            .ask(start_window_recording(0, false))
            .expect("records");
        harness.deliver(&mut handler, Ok(&red_columns(3)));
        assert_eq!(harness.jobs.try_iter().count(), 1, "read as text");
        let view = harness.recording().expect("a recording");
        assert_eq!(view.settings.scope, Scope::Window);
        assert_eq!(view.spots.len(), 1, "where it found the text");
        harness.ask(Owner::stop_recording);
        assert!(
            harness
                .ask(start_recording(255, true))
                .expect_err("the place alone is another recording")
                .contains("the same choice of where to look")
        );
        harness
            .ask(start_window_recording(0, true))
            .expect("goes on over the whole window");
    }

    #[test]
    fn recording_goes_on_after_a_stop_and_starts_over_for_other_ink() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        assert_eq!(
            harness.ask(start_recording(255, true)),
            Err(
                "That recording is gone, so there is nothing to record more of. Start a new \
                 recording."
                    .to_owned()
            ),
            "recording more never starts a recording"
        );
        assert!(harness.recording().is_none());
        harness.ask(start_recording(255, false)).expect("records");
        harness.deliver(&mut handler, Ok(&red_columns(2)));
        harness.ask(Owner::stop_recording);
        harness.ask(start_recording(255, true)).expect("goes on");
        assert!(harness.recording().expect("kept").recording);
        harness.deliver(&mut handler, Ok(&red_columns(3)));
        assert_eq!(harness.readings().len(), 2, "the same recording went on");
        harness.ask(Owner::stop_recording);
        assert_eq!(
            harness.ask(start_recording(200, true)),
            Err(
                "Recording more needs the settings it was recorded with: the same place, the \
                 characters' color, a tolerance of 0, a screenshot of the same window size, and \
                 the same choice of where to look. Use its settings, or discard the recording \
                 to start a new one."
                    .to_owned()
            ),
            "recording more never replaces the recording"
        );
        assert_eq!(harness.readings().len(), 2, "kept as it was");
        harness
            .ask(start_recording(200, false))
            .expect("records other ink");
        assert_eq!(harness.readings().len(), 0, "a new recording");
        let jobs: Vec<ocr::Source> = harness.jobs.try_iter().map(|job| job.source).collect();
        assert_eq!(jobs, [ocr::Source::Recording(1), ocr::Source::Recording(1)]);
        assert!(
            harness
                .owner
                .handle(Message::Recognized {
                    recording: 1,
                    index: 0,
                    text: Text::Read("1".to_owned()),
                })
                .is_continue()
        );
        assert_eq!(
            harness.readings().len(),
            0,
            "text for the old one is dropped"
        );
        let _next = harness.pick();
        assert_eq!(harness.recording(), None, "another window drops it");
    }

    #[test]
    fn a_recording_needs_a_window_a_screenshot_and_a_region_inside_it() {
        let mut harness = harness(false);
        assert_eq!(
            harness.ask(start_recording(255, false)),
            Err("Pick a window to record.".to_owned())
        );
        let mut handler = harness.pick();
        assert_eq!(
            harness.ask(start_recording(255, false)),
            Err("Take a screenshot first.".to_owned())
        );
        let small = Image::new(
            Size {
                width: 2,
                height: 2,
            },
            [0, 0, 255, 0].repeat(4),
        )
        .expect("matching length");
        harness.deliver(&mut handler, Ok(&small));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        assert_eq!(
            harness.ask(start_recording(255, false)),
            Err(
                "The region reaches past the edge of the window. Draw it inside the window."
                    .to_owned()
            )
        );
        let empty: RegionSpec =
            serde_json::from_str(r#"{"x":0,"y":0,"width":0,"height":2}"#).expect("valid JSON");
        let ink: ColorSpec =
            serde_json::from_str(r#"{"red":255,"green":0,"blue":0}"#).expect("valid JSON");
        assert_eq!(
            harness.ask(|owner| owner.start_recording(characters_in(empty, ink), false)),
            Err("Draw a region on the screenshot first.".to_owned())
        );
    }

    #[test]
    fn a_capture_failure_stops_the_recording_and_says_why() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        harness.ask(start_recording(255, false)).expect("records");
        harness.deliver(&mut handler, Ok(&red_columns(2)));
        harness.deliver(&mut handler, Err(io::Error::other("a frame was lost")));
        let stopped = harness.recording().expect("kept for review");
        assert!(!stopped.recording, "nothing records while frames fail");
        assert_eq!(
            stopped.problem.as_deref(),
            Some("Recording stopped: A frame was lost. Record more once the window shows again.")
        );
        assert_eq!(stopped.readings.len(), 1, "what it kept stays");
        harness.deliver(&mut handler, Ok(&red_columns(3)));
        harness.ask(start_recording(255, true)).expect("goes on");
        let going = harness.recording().expect("the same recording");
        assert!(going.recording);
        assert_eq!(going.problem, None, "going on clears why it stopped");
    }

    #[test]
    fn a_closed_window_records_nothing_more() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        harness.ask(start_recording(255, false)).expect("records");
        harness.deliver(&mut handler, Ok(&red_columns(2)));
        harness.deliver(
            &mut handler,
            Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "the captured window closed",
            )),
        );
        let closed = "The captured window closed. Change window to pick it again.";
        for more in [true, false] {
            assert_eq!(
                harness.ask(start_recording(255, more)),
                Err(closed.to_owned())
            );
        }
        let stopped = harness.recording().expect("kept for review");
        assert!(!stopped.recording);
        assert_eq!(
            stopped.problem.as_deref(),
            Some("Recording stopped: The captured window closed. Change window to pick it again.")
        );
    }

    #[test]
    fn a_recording_stopped_before_the_window_closed_says_it_cannot_go_on() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        harness.ask(start_recording(255, false)).expect("records");
        harness.deliver(&mut handler, Ok(&red_columns(2)));
        harness.ask(Owner::stop_recording);
        harness.deliver(&mut handler, Err(io::Error::other("a frame was lost")));
        assert_eq!(
            harness.recording().expect("kept").problem,
            None,
            "a stopped recording can still go on after a passing failure"
        );
        harness.deliver(
            &mut handler,
            Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "the captured window closed",
            )),
        );
        assert_eq!(
            harness.recording().expect("kept").problem.as_deref(),
            Some("Recording stopped: The captured window closed. Change window to pick it again.")
        );
    }

    #[test]
    fn a_reading_the_text_reader_cannot_take_says_to_type_it() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        harness.ask(start_recording(255, false)).expect("records");
        let (closed, _) = mpsc::channel();
        harness.owner.ocr = closed;
        harness.deliver(&mut handler, Ok(&red_columns(2)));
        assert_eq!(
            harness.readings(),
            [(
                1,
                Text::Failed("The text reader stopped. Type what this shows.".to_owned())
            )]
        );
    }
}
