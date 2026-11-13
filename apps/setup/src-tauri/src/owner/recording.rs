//! Recording a region: what both kinds of recording share, and recording
//! readings, starting it and sending each new reading it shows to the text
//! reader. Recording looks is in `looks.rs`.

use std::collections::BTreeMap;
use std::time::Instant;

use appytappy_sources::screen::digits::DigitReader;
use appytappy_sources::screen::{Image, Region, Rgb, Size};
use serde::Deserialize;

use appytappy_config::app_file::{ColorDef, RegionDef};

use crate::check_spec::{self, CharactersSpec};
use crate::pixels;
use crate::recording::follow::Follows;
use crate::recording::{
    NewReading, Recorder, Recording, RecordingView, Scope, Text, find, look, ocr,
};
use crate::words::sentence;
use appytappy_config::app_file::PinDef;

use super::Owner;
use super::host::Host;

/// What a recording of readings records, as the UI describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordSpec {
    /// The saved application whose window is recorded, whose answer on how
    /// often its window changes size says whether the recording follows
    /// the window to another size.
    pub(crate) app: String,
    /// The region of the window to record.
    pub(crate) region: RegionDef,
    /// The screenshot the region was drawn on, whose window size the
    /// recording keeps.
    pub(crate) screenshot: usize,
    /// The side of the window the region stays at as the window changes
    /// size, or `None` for the side the region is nearest.
    pub(crate) pin: Option<PinDef>,
    /// The characters' color.
    pub(crate) ink: ColorDef,
    /// How far from `ink` a pixel of a character may be.
    pub(crate) tolerance: u8,
    /// Where to look for characters: in the region alone, or wherever the
    /// window shows text like the region's.
    pub(crate) scope: Scope,
    /// The reference's boxes: more examples of its text, which a recording
    /// of the whole window looks for the sizes of too.
    pub(crate) examples: Vec<ExampleSpec>,
}

/// One more example of the text a recording of the whole window looks for:
/// a box the user drew around some of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExampleSpec {
    /// The box.
    pub(crate) region: RegionDef,
    /// The screenshot it is drawn on.
    pub(crate) screenshot: usize,
}

impl<H: Host> Owner<H> {
    /// Records `frame`, which arrived at `received`: sends a new reading it
    /// shows to the text reader, or keeps it as a screenshot when it shows
    /// a new look.
    pub(super) fn record(&mut self, frame: &Image, received: Instant) {
        let Some(picked) = self.capture.picked_mut() else {
            return;
        };
        let recording = match &mut picked.recording {
            Some(Recorder::Readings(recording)) => recording,
            Some(Recorder::Looks(recording)) => {
                let host = &self.host;
                recording.offer(frame, received, || host.now(), &mut picked.screenshots);
                return;
            },
            None => return,
        };
        // A recording of the whole window keeps nothing from the frame
        // here: it sends the frame to be looked over when a look is due.
        if let Some(job) = recording.look(frame, received)
            && self.looks.send(job).is_err()
        {
            recording.stop_looking(Instant::now());
        }
        let new = recording.offer(frame, received);
        self.read_new(new);
    }

    /// Takes the answer to a look for the recording of readings, of the
    /// frame that arrived at `at`: sends each new reading it `seen` to the
    /// text reader.
    pub(super) fn looked(&mut self, at: Instant, seen: Option<Vec<look::Seen>>) {
        let Some(current) = self.readings_mut() else {
            return;
        };
        let new = current.looked(at, seen, Instant::now());
        self.read_new(new);
    }

    /// Sends each of the recording's `new` readings to the text reader.
    fn read_new(&mut self, new: impl IntoIterator<Item = NewReading>) {
        let Some(Recorder::Readings(recording)) = self.capture.recording_mut() else {
            return;
        };
        for new in new {
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
    pub(super) fn readings(&self) -> Option<&Recording> {
        match self.capture.recording() {
            Some(Recorder::Readings(recording)) => Some(recording),
            Some(Recorder::Looks(_)) | None => None,
        }
    }

    /// The recording of readings, to change it.
    pub(super) fn readings_mut(&mut self) -> Option<&mut Recording> {
        match self.capture.recording_mut() {
            Some(Recorder::Readings(recording)) => Some(recording),
            Some(Recorder::Looks(_)) | None => None,
        }
    }

    /// The region `region` describes and the size of the window it was
    /// drawn on, that of `screenshot`, when the picked window can be
    /// recorded there; otherwise what to do first.
    pub(super) fn recordable(
        &self,
        region: RegionDef,
        screenshot: usize,
    ) -> Result<(Region, Size), String> {
        let Some(picked) = self.capture.picked() else {
            return Err("Pick a window to record.".to_owned());
        };
        // A closed window sends no more frames, so a recording would count
        // up with nothing in it.
        if let Some(failure) = picked.failure.as_ref().filter(|failure| failure.ended) {
            return Err(format!(
                "{} Change window to pick it again.",
                sentence(&failure.error)
            ));
        }
        if self.capture.screenshots().get(screenshot).is_none() {
            return Err("Take a screenshot first.".to_owned());
        }
        check_spec::drawn(self.capture.screenshots(), region, screenshot)
            .map_err(|error| error.to_string())
    }

    /// Records what `spec` names of the picked window, keeping each reading
    /// it shows in the characters' color: in the region alone, or wherever
    /// the window shows text like the region's. With `more`, goes on with
    /// the stopped recording, which must be of the same region, ink, window
    /// size, and scope; otherwise starts a new one in place of any
    /// recording. The recording follows the window to another size as
    /// `spec`'s application says its window changes size ([`Follows`]). A
    /// recording of the whole window looks for text of the size of each of
    /// `spec`'s examples too, and going on with one adds the sizes of the
    /// examples drawn since.
    pub(crate) fn start_recording(&mut self, spec: RecordSpec, more: bool) -> Result<(), String> {
        let (region, window_size) = self.recordable(spec.region, spec.screenshot)?;
        let size_changes = self.stores()?.app(&spec.app)?.file.size_changes;
        let follows = Follows::of(size_changes, spec.pin, region, window_size);
        let settings: (Rgb, u8) = (spec.ink.into(), spec.tolerance);
        let now = Instant::now();
        // The whole window is searched for text as tall as each example's
        // on its screenshot.
        let reader = DigitReader::new(settings.0, settings.1);
        let examples = match spec.scope {
            Scope::Place => Vec::new(),
            Scope::Window => self.example_heights(&spec.examples, window_size, &reader),
        };
        match self.capture.recording_mut() {
            Some(Recorder::Readings(recording))
                if more && recording.records(region, settings, window_size, spec.scope) =>
            {
                recording.resume(follows, now);
                recording.look_for(examples);
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
                // The region's own text is the first example, so it is
                // measured before anything is replaced.
                let sample = match spec.scope {
                    Scope::Place => None,
                    Scope::Window => Some(self.sample_height(region, spec.screenshot, &reader)?),
                };
                let id = self.tickets.next();
                let mut recording =
                    Recording::new(id, region, settings, (window_size, follows), now);
                if let Some(sample) = sample {
                    recording = recording.across_window(sample);
                    recording.look_for(examples);
                }
                self.record_with(Recorder::Readings(recording));
            },
        }
        Ok(())
    }

    /// How tall the characters are that `region` of screenshot `screenshot`
    /// shows in the ink `reader` tells from background, or what to do when
    /// it shows none.
    fn sample_height(
        &self,
        region: Region,
        screenshot: usize,
        reader: &DigitReader,
    ) -> Result<u32, String> {
        self.capture
            .screenshots()
            .get(screenshot)
            .and_then(|image| find::sample_heights(image, &[region], reader).pop()?)
            .ok_or_else(|| {
                "That place shows no characters in that color, so there is nothing to look for in \
                 the rest of the window. Draw it around some characters and pick their color \
                 first."
                    .to_owned()
            })
    }

    /// How tall the characters are that each of `examples` shows in the ink
    /// `reader` tells from background. An example that shows none, or that
    /// is on a screenshot of another size than `window_size`, whose pixels
    /// measure another window, is no example of a size and is left out.
    /// Each screenshot's ink is read once for all the examples on it.
    fn example_heights(
        &self,
        examples: &[ExampleSpec],
        window_size: Size,
        reader: &DigitReader,
    ) -> Vec<u32> {
        let mut drawn: BTreeMap<usize, Vec<Region>> = BTreeMap::new();
        for example in examples {
            if let Ok(region) = Region::try_from(example.region) {
                drawn.entry(example.screenshot).or_default().push(region);
            }
        }
        drawn
            .into_iter()
            .filter_map(|(screenshot, regions)| {
                let image = self
                    .capture
                    .screenshots()
                    .get(screenshot)
                    .filter(|image| image.size() == window_size)?;
                Some(find::sample_heights(image, &regions, reader))
            })
            .flatten()
            .flatten()
            .collect()
    }

    /// Stops the recording, of readings or of looks, which keeps what it
    /// recorded.
    pub(crate) fn stop_recording(&mut self) {
        if let Some(recording) = self.capture.recording_mut() {
            recording.stop(Instant::now());
        }
    }

    /// Forgets the recording, of readings or of looks. The screenshots a
    /// recording of looks kept stay.
    pub(crate) fn discard_recording(&mut self) {
        if let Some(picked) = self.capture.picked_mut() {
            picked.recording = None;
        }
    }

    /// Makes `recorder` what the picked window's capture records, in place
    /// of any recording.
    pub(super) fn record_with(&mut self, recorder: Recorder) {
        if let Some(picked) = self.capture.picked_mut() {
            picked.recording = Some(recorder);
        }
    }

    /// The recording of readings as the interface shows it, or `None`
    /// without one. A window size that settled since the last frame places
    /// the recording there first, so a window that stopped drawing is
    /// placed too, and the newest frame, which shows that size, is then
    /// recorded once.
    pub(crate) fn recording_state(&mut self) -> Option<RecordingView> {
        let now = self.host.now();
        let placed = self
            .readings_mut()
            .is_some_and(|recording| recording.settled(now));
        let newest = self
            .capture
            .picked_mut()
            .and_then(|picked| picked.latest.take());
        if let Some((frame, arrived)) = newest {
            if placed {
                self.record(&frame, now);
            }
            if let Some(picked) = self.capture.picked_mut() {
                picked.latest = Some((frame, arrived));
            }
        }
        self.readings()
            .map(|recording| recording.view(Instant::now()))
    }

    /// Reading `index`'s pixels of recording `recording`, at most
    /// `max_width` wide; `None` when that recording was replaced or
    /// forgotten.
    pub(crate) fn recorded_pixels(
        &self,
        recording: u64,
        index: usize,
        max_width: u32,
    ) -> Option<Vec<u8>> {
        self.readings()
            .filter(|kept| kept.id.number() == recording)
            .and_then(|kept| kept.readings().get(index))
            .map(|kept| pixels::rgba(&kept.image, max_width))
    }

    /// For each recorded reading, why `spec` reads it wrong, or `None` when
    /// it reads it right; an error when the labeled screenshots cannot be
    /// learned.
    pub(crate) fn review_problems(
        &mut self,
        spec: &CharactersSpec,
    ) -> Result<Vec<Option<String>>, String> {
        self.keep_reference(spec.reference.as_ref());
        self.with_session(|session| spec.recorded_problems(session))
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::mpsc;

    use appytappy_config::app_file::{PinDef, SizeChangesDef};
    use appytappy_sources::screen::Size;
    use appytappy_sources::screen::settle::SETTLE;

    use crate::owner::messages::{Event, Message};
    use crate::owner::test_support::{
        FakeHost, Harness, capturing, characters_in, example_app, frame_image, harness, recognized,
        teaching, with_example_app,
    };
    use crate::pixels;
    use crate::recording::RecordingView;
    use crate::ticket::Ticket;
    use appytappy_config::app_file::ColorDef;

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
        let region: RegionDef =
            serde_json::from_str(r#"{"x":0,"y":0,"width":4,"height":2}"#).expect("valid JSON");
        let ink: ColorDef = serde_json::from_str(&format!(r#"{{"red":{red},"green":0,"blue":0}}"#))
            .expect("valid JSON");
        move |owner: &mut Owner<FakeHost>| owner.start_recording(characters_in(region, ink), more)
    }

    impl Harness {
        /// The recording as the UI sees it.
        fn recording(&mut self) -> Option<RecordingView> {
            self.ask(Owner::recording_state)
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
        let (mut harness, root) = with_example_app("rec-keeps-once");
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
        let id = harness.recording().expect("a recording").id;
        let jobs: Vec<(u64, usize)> = harness
            .jobs
            .try_iter()
            .map(|job| match job.source {
                ocr::Source::Recording(recording) => (recording.number(), job.index),
                ocr::Source::Search(_) => panic!("no search was made"),
            })
            .collect();
        assert_eq!(jobs, [(id, 0), (id, 1)], "one job per new reading");
        assert_eq!(
            harness.readings(),
            [(2, Text::Pending), (1, Text::Pending)],
            "a frame with no ink is skipped"
        );
        // Text under another ticket than the recording's, here its
        // capture's, is dropped.
        for (recording, text) in [(id - 1, "stale"), (id, "1")] {
            assert!(
                harness
                    .owner
                    .handle(recognized(recording, 0, Text::Read(text.to_owned())))
                    .is_continue()
            );
        }
        assert_eq!(
            harness.readings(),
            [(2, Text::Read("1".to_owned())), (1, Text::Pending)]
        );
        let recording = harness.recording().expect("a recording").id;
        let pixels_of =
            |index| move |owner: &mut Owner<FakeHost>| owner.recorded_pixels(recording, index, 4);
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
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_reading_s_pixels_are_answered_only_for_its_own_recording() {
        let (mut harness, root) = with_example_app("rec-pixels-of-its-own");
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        assert_eq!(harness.ask(start_recording(255, false)), Ok(()));
        harness.deliver(&mut handler, Ok(&red_columns(2)));
        let old = harness.recording().expect("a recording").id;
        let pixels = |harness: &mut Harness, recording| {
            harness.ask(move |owner| owner.recorded_pixels(recording, 0, 64))
        };
        let first = pixels(&mut harness, old).expect("the reading's pixels");

        // Discarded and recorded again at once: the page's late ask for the
        // old recording's first reading gets none of the new one's pixels.
        harness.ask(Owner::discard_recording);
        assert_eq!(harness.ask(start_recording(255, false)), Ok(()));
        harness.deliver(&mut handler, Ok(&red_columns(3)));
        let new = harness.recording().expect("a recording").id;
        assert_ne!(new, old);
        assert_eq!(pixels(&mut harness, old), None, "the old ID is dropped");
        let second = pixels(&mut harness, new).expect("the new reading's pixels");
        assert_ne!(second, first);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_frame_of_shapes_already_kept_is_counted_and_not_read_as_text() {
        let (mut harness, root) = with_example_app("rec-counted-frame");
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
        std::fs::remove_dir_all(root).expect("clean up");
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
        let region: RegionDef = serde_json::from_str(&format!(
            r#"{{"x":{x},"y":0,"width":{},"height":2}}"#,
            4_u32.saturating_sub(x)
        ))
        .expect("valid JSON");
        let ink: ColorDef =
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
        let (mut harness, root) = with_example_app("rec-window-sample");
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
        // More frames arrive while the look is out; the owner holds none.
        harness.deliver(&mut handler, Ok(&red_columns(3)));
        harness.deliver(&mut handler, Ok(&red_columns(3)));
        assert_eq!(harness.jobs.try_iter().count(), 0, "nothing is read yet");
        let looks: Vec<look::Job> = harness.looks.try_iter().collect();
        assert_eq!(looks.len(), 1, "one frame goes to be looked over");
        let job = &looks[0];
        // A look for a recording that was discarded or replaced is dropped.
        let looked = |ticket| Message::Sent {
            ticket,
            event: Event::Looked {
                at: job.at,
                seen: Some(look::look(job)),
            },
        };
        let stale = looked(Ticket::numbered(job.recording.number() - 1));
        assert!(harness.owner.handle(stale).is_continue());
        assert_eq!(harness.recording().expect("a recording").spots, []);
        assert_eq!(harness.jobs.try_iter().count(), 0);
        assert!(harness.owner.handle(looked(job.recording)).is_continue());
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
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// A `width` by 30 frame with a red stroke 4 tall at its top left,
    /// one 8 tall in its middle, and one 16 tall at its right.
    fn three_sizes(width: u32) -> Image {
        let strokes: [(u32, u32, u32); 3] = [(1, 1, 4), (10, 10, 8), (20, 10, 16)];
        let pixels = (0..30_u32)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .flat_map(|(x, y)| {
                let red = strokes.iter().any(|&(left, top, height)| {
                    (left..left.saturating_add(2)).contains(&x)
                        && (top..top.saturating_add(height)).contains(&y)
                });
                if red { [0, 0, 255, 0] } else { [40, 40, 40, 0] }
            })
            .collect();
        Image::new(Size { width, height: 30 }, pixels).expect("matching length")
    }

    /// Records the whole window for text like the 4 tall stroke of
    /// [`three_sizes`] on screenshot 0, and like each of `examples`, as a
    /// screenshot and a box's left and top; each box is 4 by 18.
    fn start_recording_sizes(
        scope: Scope,
        examples: &[(usize, u32, u32)],
        more: bool,
    ) -> impl FnOnce(&mut Owner<FakeHost>) -> Result<(), String> {
        let region = |x: u32, y: u32, height: u32| -> RegionDef {
            serde_json::from_str(&format!(
                r#"{{"x":{x},"y":{y},"width":4,"height":{height}}}"#
            ))
            .expect("valid JSON")
        };
        let ink: ColorDef =
            serde_json::from_str(r#"{"red":255,"green":0,"blue":0}"#).expect("valid JSON");
        let spec = RecordSpec {
            scope,
            examples: examples
                .iter()
                .map(|&(screenshot, x, y)| ExampleSpec {
                    region: region(x, y, 18),
                    screenshot,
                })
                .collect(),
            ..characters_in(region(0, 0, 6), ink)
        };
        move |owner: &mut Owner<FakeHost>| owner.start_recording(spec, more)
    }

    #[test]
    fn a_whole_window_is_recorded_for_text_as_tall_as_each_box_on_a_screenshot_of_its_size() {
        let (mut harness, root) = with_example_app("rec-box-sizes");
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&three_sizes(30)));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        // A screenshot of a wider window, whose pixels measure another size.
        harness.deliver(&mut handler, Ok(&three_sizes(40)));
        assert_eq!(harness.ask(Owner::screenshot), Ok(1));
        harness.deliver(&mut handler, Ok(&three_sizes(30)));
        // The boxes: the stroke 8 tall, a place with no ink, a box past the
        // screenshot's edge, a screenshot that was never taken, and the
        // stroke 16 tall on the screenshot of the other size.
        let boxes = [(0, 9, 9), (0, 25, 9), (0, 28, 9), (5, 9, 9), (1, 19, 9)];
        harness
            .ask(start_recording_sizes(Scope::Place, &boxes, false))
            .expect("records");
        let place = harness.recording().expect("a recording");
        assert_eq!(place.sizes, [0_u32; 0], "one place looks for no size");
        harness
            .ask(start_recording_sizes(Scope::Window, &boxes, false))
            .expect("records");
        assert_eq!(harness.recording().expect("a recording").sizes, [4, 8]);
        harness.deliver(&mut handler, Ok(&three_sizes(30)));
        let job = harness.looks.try_iter().next().expect("a look goes out");
        assert_eq!(job.sizes.heights(), [4, 8]);
        harness.ask(Owner::stop_recording);
        // Going on adds the size of a box drawn since, and keeps the others.
        harness
            .ask(start_recording_sizes(Scope::Window, &[(0, 19, 9)], true))
            .expect("goes on");
        assert_eq!(harness.recording().expect("a recording").sizes, [4, 8, 16]);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// A gray frame `width` wide and 2 tall whose `red` columns are red.
    fn red_at(width: u32, red: &[u32]) -> Image {
        let pixels = (0..2_u32)
            .flat_map(|_| 0..width)
            .flat_map(|x| {
                if red.contains(&x) {
                    [0, 0, 255, 0]
                } else {
                    [40, 40, 40, 0]
                }
            })
            .collect();
        Image::new(Size { width, height: 2 }, pixels).expect("matching length")
    }

    /// Records columns 2 and 3 of the 4 by 2 screenshot 0, red as ink,
    /// staying at `pin`.
    fn start_recording_right(
        pin: Option<PinDef>,
    ) -> impl FnOnce(&mut Owner<FakeHost>) -> Result<(), String> {
        let region: RegionDef =
            serde_json::from_str(r#"{"x":2,"y":0,"width":2,"height":2}"#).expect("valid JSON");
        let ink: ColorDef =
            serde_json::from_str(r#"{"red":255,"green":0,"blue":0}"#).expect("valid JSON");
        move |owner: &mut Owner<FakeHost>| {
            owner.start_recording(
                RecordSpec {
                    pin,
                    ..characters_in(region, ink)
                },
                false,
            )
        }
    }

    #[test]
    fn a_recording_follows_the_window_to_another_size_as_its_application_s_window_changes_size() {
        let (mut harness, mut handler, root) = capturing("rec-follows");
        // Twice as wide as it was and as tall: red at its right edge alone.
        let wide = red_at(8, &[7]);
        let mut kept_at = |harness: &mut Harness, pin| {
            harness.ask(start_recording_right(pin)).expect("records");
            harness.deliver(&mut handler, Ok(&wide));
            harness.wait(SETTLE);
            harness.deliver(&mut handler, Ok(&wide));
            let view = harness.recording().expect("a recording");
            // Back at the size it was drawn at for the next recording.
            harness.deliver(&mut handler, Ok(&frame_image()));
            (view.readings.len(), view.problem)
        };
        assert_eq!(
            kept_at(&mut harness, None),
            (1, None),
            "nearest the right edge, the region stays at it"
        );
        assert_eq!(
            kept_at(&mut harness, Some(PinDef::TopLeft)),
            (0, None),
            "staying at the left edge, it shows no ink"
        );
        harness
            .ask(|owner| owner.save_app(Some("example"), example_app(SizeChangesDef::Rarely)))
            .expect("saves");
        assert_eq!(
            kept_at(&mut harness, None),
            (
                0,
                Some(
                    "The region was drawn when the window was 4 × 2, and it is 8 × 2 now. \
                     Resize the window to record."
                        .to_owned()
                )
            )
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_window_that_stops_drawing_at_a_new_size_is_placed_when_the_recording_is_shown() {
        let (mut harness, mut handler, root) = capturing("rec-settles-unasked");
        let wide = red_at(8, &[7]);
        // One frame at the new size, and no more: nothing on it changes.
        harness.ask(start_recording_right(None)).expect("records");
        harness.deliver(&mut handler, Ok(&wide));
        let view = harness.recording().expect("a recording");
        assert_eq!((view.at.width, view.readings.len()), (4, 0), "not yet");
        harness.wait(SETTLE);
        let view = harness.recording().expect("a recording");
        assert_eq!(view.at.width, 8);
        assert_eq!(view.problem, None);
        assert_eq!(view.readings.len(), 1, "the newest frame is recorded");
        let again = harness.recording().expect("a recording");
        assert_eq!(again.readings[0].seen, 1, "once");
        // Where the recording does not follow the window, it says why.
        harness
            .ask(|owner| owner.save_app(Some("example"), example_app(SizeChangesDef::Rarely)))
            .expect("saves");
        harness.deliver(&mut handler, Ok(&frame_image()));
        harness.ask(start_recording_right(None)).expect("records");
        harness.deliver(&mut handler, Ok(&wide));
        harness.wait(SETTLE);
        let view = harness.recording().expect("a recording");
        assert_eq!(view.at.width, 8);
        assert_eq!(
            view.problem.as_deref(),
            Some(
                "The region was drawn when the window was 4 × 2, and it is 8 × 2 now. Resize the \
                 window to record."
            )
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_recording_needs_its_application_saved() {
        let (mut harness, _handler, root) = capturing("rec-no-app");
        let region: RegionDef =
            serde_json::from_str(r#"{"x":0,"y":0,"width":4,"height":2}"#).expect("valid JSON");
        let ink: ColorDef =
            serde_json::from_str(r#"{"red":255,"green":0,"blue":0}"#).expect("valid JSON");
        let spec = RecordSpec {
            app: "other".to_owned(),
            ..characters_in(region, ink)
        };
        assert!(
            harness
                .ask(|owner| owner.start_recording(spec, false))
                .is_err()
        );
        assert_eq!(harness.recording(), None, "nothing started");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_whole_window_recording_stops_with_a_reason_when_the_look_thread_is_gone() {
        let (mut harness, root) = with_example_app("rec-look-thread-gone");
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        harness
            .ask(start_window_recording(0, false))
            .expect("records");
        // The look thread ended, so nothing takes the frames.
        drop(std::mem::replace(&mut harness.looks, mpsc::channel().1));
        harness.deliver(&mut handler, Ok(&red_columns(3)));
        let view = harness.recording().expect("a recording");
        assert!(!view.recording);
        assert_eq!(
            view.problem.as_deref(),
            Some(
                "Recording stopped: the window could not be looked over. Record more to try \
                 again."
            )
        );
        // Record more sends a frame again, and stops the same way.
        harness
            .ask(start_window_recording(0, true))
            .expect("goes on");
        assert!(harness.recording().expect("a recording").recording);
        harness.deliver(&mut handler, Ok(&red_columns(3)));
        assert!(!harness.recording().expect("a recording").recording);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn recording_goes_on_after_a_stop_and_starts_over_for_other_ink() {
        let (mut harness, root) = with_example_app("rec-goes-on");
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
        let replaced = harness.recording().expect("a recording").id;
        harness
            .ask(start_recording(200, false))
            .expect("records other ink");
        assert_eq!(harness.readings().len(), 0, "a new recording");
        let jobs: Vec<ocr::Source> = harness.jobs.try_iter().map(|job| job.source).collect();
        let source = ocr::Source::Recording(Ticket::numbered(replaced));
        assert_eq!(jobs, [source, source]);
        assert!(
            harness
                .owner
                .handle(recognized(replaced, 0, Text::Read("1".to_owned())))
                .is_continue()
        );
        assert_eq!(
            harness.readings().len(),
            0,
            "text for the old one is dropped"
        );
        let _next = harness.pick();
        assert_eq!(harness.recording(), None, "another window drops it");
        std::fs::remove_dir_all(root).expect("clean up");
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
        let empty: RegionDef =
            serde_json::from_str(r#"{"x":0,"y":0,"width":0,"height":2}"#).expect("valid JSON");
        let ink: ColorDef =
            serde_json::from_str(r#"{"red":255,"green":0,"blue":0}"#).expect("valid JSON");
        assert_eq!(
            harness.ask(|owner| owner.start_recording(characters_in(empty, ink), false)),
            Err("Draw a region on the screenshot first.".to_owned())
        );
    }

    #[test]
    fn a_capture_failure_stops_the_recording_and_says_why() {
        let (mut harness, root) = with_example_app("rec-capture-failure");
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
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_closed_window_records_nothing_more() {
        let (mut harness, root) = with_example_app("rec-closed-window");
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
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_recording_stopped_before_the_window_closed_says_it_cannot_go_on() {
        let (mut harness, root) = with_example_app("rec-stopped-then-closed");
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
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_reading_the_text_reader_cannot_take_says_to_type_it() {
        let (mut harness, root) = with_example_app("rec-reader-gone");
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
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
