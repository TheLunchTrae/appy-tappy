//! Recording a region: starting it, and sending each new reading it shows to
//! the text reader.

use std::time::Instant;

use appytappy_sources::screen::{Image, Region};

use crate::check_spec::{ColorSpec, RegionSpec};
use crate::recording::{Recording, Text, ocr};

use super::Owner;
use super::host::Host;

impl<H: Host> Owner<H> {
    /// Records `frame`, which arrived at `received`, and sends a new reading
    /// it shows to the text reader.
    pub(super) fn record(&mut self, frame: &Image, received: Instant) {
        let Some(recording) = &mut self.recording else {
            return;
        };
        let Some(new) = recording.offer(frame, received) else {
            return;
        };
        let job = ocr::Job {
            recording: recording.id,
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

    pub(super) fn start_recording(
        &mut self,
        region: RegionSpec,
        ink: ColorSpec,
        tolerance: u8,
        screenshot: usize,
        more: bool,
    ) -> Result<(), String> {
        if self.session.is_none() {
            return Err("Pick a window to record.".to_owned());
        }
        let region = Region::new(region.x, region.y, region.width, region.height)
            .ok_or_else(|| "Draw the region to record first.".to_owned())?;
        let drawn_on = self
            .screenshots
            .get(screenshot)
            .ok_or_else(|| "Take a screenshot first.".to_owned())?;
        if drawn_on.crop(region).is_none() {
            return Err(
                "The region reaches past the edge of the window. Draw it inside the window."
                    .to_owned(),
            );
        }
        let window_size = drawn_on.size();
        let settings = (ink.into(), tolerance);
        let now = Instant::now();
        match &mut self.recording {
            Some(recording) if more && recording.records(region, settings, window_size) => {
                recording.resume(now);
            },
            Some(recording) if more => {
                return Err(format!(
                    "Recording more needs the settings it was recorded with: the same region, \
                     the characters' color, a tolerance of {}, and a screenshot of the same \
                     window size. Use its settings, or discard the recording to start a new one.",
                    recording.view(now).settings.tolerance
                ));
            },
            None if more => {
                return Err(
                    "That recording is gone, so there is nothing to record more of. Start a new \
                     recording."
                        .to_owned(),
                );
            },
            _ => {
                self.recording_id = self.recording_id.wrapping_add(1);
                self.recording = Some(Recording::new(
                    self.recording_id,
                    region,
                    settings,
                    window_size,
                    now,
                ));
            },
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use appytappy_sources::screen::Size;

    use crate::check_spec::CheckSpec;
    use crate::owner::messages::{Message, Reply, Request};
    use crate::owner::test_support::{Harness, frame_image, harness};
    use crate::pixels;
    use crate::recording::RecordingView;

    use super::*;

    /// A 4 by 2 frame with its left `red` columns red and the rest gray.
    fn red_columns(red: usize) -> Image {
        let row: Vec<u8> = (0..4)
            .flat_map(|x| {
                if x < red {
                    [0, 0, 255, 0]
                } else {
                    [40, 40, 40, 0]
                }
            })
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
    fn start_recording(red: u8, more: bool) -> impl FnOnce(Reply<Result<(), String>>) -> Request {
        let region: RegionSpec =
            serde_json::from_str(r#"{"x":0,"y":0,"width":4,"height":2}"#).expect("valid JSON");
        let ink: ColorSpec =
            serde_json::from_str(&format!(r#"{{"red":{red},"green":0,"blue":0}}"#))
                .expect("valid JSON");
        move |reply| Request::StartRecording {
            region,
            ink,
            tolerance: 0,
            screenshot: 0,
            more,
            reply,
        }
    }

    impl Harness {
        /// The recording as the UI sees it.
        fn recording(&mut self) -> Option<RecordingView> {
            self.ask(Request::RecordingState)
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
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
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
            .map(|job| (job.recording, job.index))
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
        let pixels_of = |index| {
            move |reply| Request::RecordedPixels {
                index,
                max_width: 4,
                reply,
            }
        };
        assert_eq!(
            harness.ask(pixels_of(1)),
            Some(pixels::rgba(&red_columns(3), 4))
        );
        assert_eq!(harness.ask(pixels_of(2)), None);

        harness.ask(Request::StopRecording);
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
    fn review_and_the_check_learn_from_recorded_readings() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        harness.ask(start_recording(255, false)).expect("records");
        harness.deliver(&mut handler, Ok(&red_columns(2)));
        harness.deliver(&mut handler, Ok(&red_columns(3)));
        // Screenshot 0 shows the same two columns as reading 0.
        let review = |labels: &str, recorded: &[(usize, &str)]| {
            let recorded: Vec<String> = recorded
                .iter()
                .map(|(reading, text)| format!(r#"{{"reading":{reading},"text":"{text}"}}"#))
                .collect();
            let spec: CheckSpec = serde_json::from_str(&format!(
                r#"{{"kind":"characters","region":{{"x":0,"y":0,"width":4,"height":2}},
                    "ink":{{"red":255,"green":0,"blue":0}},"tolerance":0,"labels":[{labels}],
                    "recorded":[{}]}}"#,
                recorded.join(",")
            ))
            .expect("valid JSON");
            move |reply| Request::ReviewProblems { spec, reply }
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
        let number = |reading| -> CheckSpec {
            serde_json::from_str(&format!(
                r#"{{"kind":"characters","region":{{"x":0,"y":0,"width":4,"height":2}},
                    "ink":{{"red":255,"green":0,"blue":0}},"tolerance":0,"labels":[],
                    "recorded":[{{"reading":{reading},"text":"1"}}]}}"#
            ))
            .expect("valid JSON")
        };
        assert_eq!(
            harness.ask(|reply| Request::SetCheck {
                spec: number(0),
                reply
            }),
            Ok(())
        );
        harness.ask(Request::DiscardRecording);
        assert_eq!(harness.recording(), None);
        assert_eq!(
            harness.ask(|reply| Request::SetCheck {
                spec: number(0),
                reply
            }),
            Err("That recording is gone. Record the region again.".to_owned())
        );
        assert_eq!(
            harness.ask(review("", &[(0, "1")])),
            Ok(vec![Some(
                "That recording is gone. Record the region again.".to_owned()
            )])
        );
    }

    #[test]
    fn recording_goes_on_after_a_stop_and_starts_over_for_other_ink() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
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
        harness.ask(Request::StopRecording);
        harness.ask(start_recording(255, true)).expect("goes on");
        assert!(harness.recording().expect("kept").recording);
        harness.deliver(&mut handler, Ok(&red_columns(3)));
        assert_eq!(harness.readings().len(), 2, "the same recording went on");
        harness.ask(Request::StopRecording);
        assert_eq!(
            harness.ask(start_recording(200, true)),
            Err(
                "Recording more needs the settings it was recorded with: the same region, the \
                 characters' color, a tolerance of 0, and a screenshot of the same window size. \
                 Use its settings, or discard the recording to start a new one."
                    .to_owned()
            ),
            "recording more never replaces the recording"
        );
        assert_eq!(harness.readings().len(), 2, "kept as it was");
        harness
            .ask(start_recording(200, false))
            .expect("records other ink");
        assert_eq!(harness.readings().len(), 0, "a new recording");
        let jobs: Vec<u64> = harness.jobs.try_iter().map(|job| job.recording).collect();
        assert_eq!(jobs, [1, 1]);
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
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
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
            harness.ask(|reply| Request::StartRecording {
                region: empty,
                ink,
                tolerance: 0,
                screenshot: 0,
                more: false,
                reply,
            }),
            Err("Draw the region to record first.".to_owned())
        );
    }

    #[test]
    fn a_reading_the_text_reader_cannot_take_says_to_type_it() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
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
