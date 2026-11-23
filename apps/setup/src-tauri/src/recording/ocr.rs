//! The text reader, which guesses what each recorded reading, and each run
//! of text a search finds like a reference's sample, says so the user only
//! corrects it.
//!
//! It runs on a thread of its own, because a read takes tens of
//! milliseconds where the owner thread answers the UI in microseconds. The
//! owner sends it each new reading's ink, and it sends back the text. It
//! loads its model on the first reading, so a setup session that never
//! records never loads it. The runtime reads numbers with the shapes the
//! user's labels taught it, and has a text reader thread of its own for a
//! reference that guesses.

use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};

use appytappy_sources::screen::text::{Line, TextError, TextReader};

use crate::owner::{Event, Message};
use crate::ticket::Ticket;

use super::Text;

/// What a job's line belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    /// A recording, by its ticket; text for one the user discarded is
    /// dropped.
    Recording(Ticket),
    /// A search for text like a reference's sample, by its ticket; text for
    /// one the user dismissed or replaced is dropped.
    Search(Ticket),
}

/// One line for the text reader.
#[derive(Debug)]
pub(crate) struct Job {
    /// What it belongs to.
    pub(crate) source: Source,
    /// Its index there: a recorded reading's, or a find's.
    pub(crate) index: usize,
    /// Its ink.
    pub(crate) line: Line,
}

/// The newest recording and search queued so far, whose jobs are the only
/// ones still worth reading.
#[derive(Debug, Default, Clone, Copy)]
struct Newest {
    recording: Option<Ticket>,
    search: Option<Ticket>,
}

impl Newest {
    fn see(&mut self, source: Source) {
        match source {
            Source::Recording(recording) => self.recording = self.recording.max(Some(recording)),
            Source::Search(search) => self.search = self.search.max(Some(search)),
        }
    }

    fn is_stale(self, source: Source) -> bool {
        match source {
            Source::Recording(recording) => Some(recording) < self.recording,
            Source::Search(search) => Some(search) < self.search,
        }
    }
}

/// Reads each job's line as text and sends the owner the result, until the
/// owner drops its sender. The model at `model` loads on the first job;
/// when it cannot, every reading says why, and the user types the text.
/// Jobs of a recording or search older than the newest one of its kind
/// queued are skipped: the user discarded or replaced it, and the owner
/// would drop their text.
pub(crate) fn run(jobs: &Receiver<Job>, messages: &Sender<Message>, model: &Path) {
    let mut engine: Option<Result<TextReader, String>> = None;
    let mut queue = VecDeque::new();
    let mut newest = Newest::default();
    loop {
        queue.extend(jobs.try_iter());
        if queue.is_empty() {
            match jobs.recv() {
                Ok(job) => queue.push_back(job),
                Err(_closed) => break,
            }
        }
        for job in &queue {
            newest.see(job.source);
        }
        let Some(job) = queue.pop_front() else {
            continue;
        };
        if newest.is_stale(job.source) {
            continue;
        }
        // A panic while loading the model fails every reading with a reason,
        // as a missing model does, rather than ending the reader.
        let loaded = engine.get_or_insert_with(|| {
            panic::catch_unwind(|| load(model)).unwrap_or_else(|_panic| {
                Err(format!(
                    "The text reader's model, {}, could not be loaded. Reinstall AppyTappy to put \
                     it back",
                    model.display()
                ))
            })
        });
        let text = match loaded {
            // A panic inside the model's code fails this reading alone,
            // rather than leaving it pending and ending the reader.
            Ok(engine) => panic::catch_unwind(AssertUnwindSafe(|| read(engine, &job.line)))
                .unwrap_or_else(|_panic| {
                    Text::Failed("The text reader failed on this reading".to_owned())
                }),
            Err(why) => Text::Failed(why.clone()),
        };
        let index = job.index;
        let recognized = match job.source {
            Source::Recording(ticket) => Message::Sent {
                ticket,
                event: Event::Recognized { index, text },
            },
            Source::Search(ticket) => Message::Sent {
                ticket,
                event: Event::FoundText { index, text },
            },
        };
        if messages.send(recognized).is_err() {
            break;
        }
    }
}

/// The text reader with the model at `model`, or why it could not load,
/// in the user's words.
fn load(model: &Path) -> Result<TextReader, String> {
    TextReader::load(model).map_err(|error| match &error {
        TextError::Model { path, .. } => format!(
            "The text reader's model, {}, could not be loaded ({}). Reinstall AppyTappy to put \
             it back",
            path.display(),
            error.cause()
        ),
        TextError::Start(_) | TextError::Read(_) => {
            format!("The text reader could not start: {}", error.cause())
        },
    })
}

/// What `engine` reads `line` as, or why it could not.
fn read(engine: &TextReader, line: &Line) -> Text {
    match engine.read(line) {
        Ok(text) => Text::Read(text),
        Err(error) => Text::Failed(format!("The text reader failed: {}", error.cause())),
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::mpsc;

    use appytappy_sources::screen::digits::DigitReader;
    use appytappy_sources::screen::{Image, Region};

    use appytappy_sources::screen::text::MODEL_FILE;

    use super::*;

    /// The model that `mise run ocr:model` downloads: where
    /// `APPYTAPPY_TEXT_READER_MODEL` says, which the mutation lanes set
    /// because they build in a copy of the tree without ignored files, or
    /// else in this package's `models` folder.
    fn model() -> PathBuf {
        env::var_os("APPYTAPPY_TEXT_READER_MODEL").map_or_else(
            || {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("models")
                    .join(MODEL_FILE)
            },
            PathBuf::from,
        )
    }

    /// Each row of `tests/fixtures/readings.png`, a region as a recording
    /// crops it, by its top and height, and the text it shows: numbers
    /// whose first character a wide margin drops or doubles, one character
    /// alone, letters with a dot and a descender, dark text on light, and
    /// small text.
    const READINGS: [((u32, u32), &str); 8] = [
        ((0, 35), "98.6%"),
        ((35, 41), "45210"),
        ((76, 35), "100%"),
        ((111, 41), "1234"),
        ((152, 21), "7"),
        ((173, 26), "jig"),
        ((199, 26), "Level 12"),
        ((225, 18), "1:23"),
    ];

    #[test]
    fn the_model_reads_each_fixture_reading_as_the_text_it_shows() {
        let model = model();
        assert!(
            model.is_file(),
            "the text reader's model is not at {}: run `mise run ocr:model` to download it",
            model.display()
        );
        let engine = load(&model).expect("the model loads");
        assert_eq!(format!("{engine:?}"), "TextReader");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/readings.png");
        let sheet = Image::decode_png(&fs::read(fixture).expect("the fixture reads"))
            .expect("the fixture is a PNG");
        let reader = DigitReader::new();
        let read: Vec<Text> = READINGS
            .iter()
            .map(|&((top, height), _)| {
                let region = Region::new(0, top, sheet.size().width, height).expect("a region");
                let reading = sheet.crop(region).expect("a row of the fixture");
                let line = Line::of(&reader.ink(&reading)).expect("the row shows ink");
                super::read(&engine, &line)
            })
            .collect();
        let shown: Vec<Text> = READINGS
            .iter()
            .map(|&(_, text)| Text::Read(text.to_owned()))
            .collect();
        assert_eq!(read, shown);
    }

    #[test]
    fn a_missing_model_says_why_for_each_reading_and_the_reader_ends_with_its_jobs() {
        let (jobs, inbox) = mpsc::channel();
        let (messages, outbox) = mpsc::channel();
        let model = std::env::temp_dir().join("appytappy-no-model.onnx");
        for index in 0..2 {
            jobs.send(Job {
                source: Source::Recording(Ticket::numbered(4)),
                index,
                line: Line {
                    width: 1,
                    height: 1,
                    pixels: vec![0],
                },
            })
            .expect("reader listens");
        }
        drop(jobs);
        run(&inbox, &messages, &model);
        for expected in 0..2 {
            match outbox.try_recv().expect("one answer per reading") {
                Message::Sent {
                    ticket,
                    event:
                        Event::Recognized {
                            index,
                            text: Text::Failed(why),
                        },
                } => {
                    assert_eq!((ticket.number(), index), (4, expected));
                    assert!(why.contains("appytappy-no-model.onnx"), "{why}");
                    let cause = TextReader::load(&model)
                        .expect_err("no such file")
                        .cause()
                        .to_string();
                    assert!(why.contains(&format!("({cause})")), "{why}");
                },
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn a_newer_recording_skips_the_older_ones_jobs() {
        let (jobs, inbox) = mpsc::channel();
        let (messages, outbox) = mpsc::channel();
        let model = std::env::temp_dir().join("appytappy-no-model.onnx");
        for (recording, index) in [(1, 0), (1, 1), (2, 0), (1, 2)] {
            jobs.send(Job {
                source: Source::Recording(Ticket::numbered(recording)),
                index,
                line: Line {
                    width: 1,
                    height: 1,
                    pixels: vec![0],
                },
            })
            .expect("reader listens");
        }
        drop(jobs);
        run(&inbox, &messages, &model);
        let answered: Vec<(u64, usize)> = outbox
            .try_iter()
            .map(|message| match message {
                Message::Sent {
                    ticket,
                    event: Event::Recognized { index, .. },
                } => (ticket.number(), index),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(answered, [(2, 0)]);
    }

    #[test]
    fn a_search_skips_only_older_searches_and_answers_as_a_find() {
        let (jobs, inbox) = mpsc::channel();
        let (messages, outbox) = mpsc::channel();
        let model = std::env::temp_dir().join("appytappy-no-model.onnx");
        let sources = [
            Source::Search(Ticket::numbered(1)),
            Source::Recording(Ticket::numbered(5)),
            Source::Search(Ticket::numbered(2)),
            Source::Search(Ticket::numbered(1)),
        ];
        for (index, source) in sources.into_iter().enumerate() {
            jobs.send(Job {
                source,
                index,
                line: Line {
                    width: 1,
                    height: 1,
                    pixels: vec![0],
                },
            })
            .expect("reader listens");
        }
        drop(jobs);
        run(&inbox, &messages, &model);
        let answered: Vec<String> = outbox
            .try_iter()
            .map(|message| match message {
                Message::Sent {
                    ticket,
                    event: Event::Recognized { index, .. },
                } => format!("recording {} reading {index}", ticket.number()),
                Message::Sent {
                    ticket,
                    event: Event::FoundText { index, .. },
                } => format!("search {} find {index}", ticket.number()),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        // The newer search does not drop the recording's reading.
        assert_eq!(answered, ["recording 5 reading 1", "search 2 find 2"]);
    }
}
