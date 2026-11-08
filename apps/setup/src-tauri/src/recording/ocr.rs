//! The text reader, which guesses what each recorded reading says so the
//! user only corrects it.
//!
//! It runs on a thread of its own, because a read takes tens of
//! milliseconds where the owner thread answers the UI in microseconds. The
//! owner sends it each new reading's ink, and it sends back the text. It
//! loads its model on the first reading, so a setup session that never
//! records never loads it. Only the setup app reads text this way; the
//! runtime reads numbers with the shapes the user's labels taught it.

use std::collections::VecDeque;
use std::error::Error;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};

use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;
use rten_imageproc::{Point, Rect, RotatedRect};

use crate::owner::Message;

use super::Text;
use super::ink::Line;

/// The text reader's model file, in the `models` folder beside the setup
/// app's program.
pub(crate) const MODEL_FILE: &str = "text-recognition.onnx";

/// One reading for the text reader.
#[derive(Debug)]
pub(crate) struct Job {
    /// The recording it belongs to; text for a recording the user discarded
    /// is dropped.
    pub(crate) recording: u64,
    /// The reading's index in the recording.
    pub(crate) index: usize,
    /// Its ink.
    pub(crate) line: Line,
}

/// Where the model file is for the program at `program`: in the `models`
/// folder beside it, where the installer puts it.
pub(crate) fn model_path(program: &Path) -> PathBuf {
    program
        .parent()
        .unwrap_or(program)
        .join("models")
        .join(MODEL_FILE)
}

/// Reads each job's line as text and sends the owner the result, until the
/// owner drops its sender. The model at `model` loads on the first job;
/// when it cannot, every reading says why, and the user types the text.
/// Jobs of a recording older than the newest one queued are skipped: the
/// user discarded or replaced it, and the owner would drop their text.
pub(crate) fn run(jobs: &Receiver<Job>, messages: &Sender<Message>, model: &Path) {
    let mut engine: Option<Result<OcrEngine, String>> = None;
    let mut queue = VecDeque::new();
    let mut newest = 0_u64;
    loop {
        queue.extend(jobs.try_iter());
        if queue.is_empty() {
            match jobs.recv() {
                Ok(job) => queue.push_back(job),
                Err(_closed) => break,
            }
        }
        newest = queue.iter().map(|job| job.recording).fold(newest, u64::max);
        let Some(job) = queue.pop_front() else {
            continue;
        };
        if job.recording < newest {
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
        let recognized = Message::Recognized {
            recording: job.recording,
            index: job.index,
            text,
        };
        if messages.send(recognized).is_err() {
            break;
        }
    }
}

/// The text reader with the recognition model at `model`.
// Telling a mutant apart needs the real model file, which is not in the
// repository or the test lanes; a missing file is tested through `run`.
#[mutants::skip]
fn load(model: &Path) -> Result<OcrEngine, String> {
    let model = Model::load_file(model).map_err(|error| {
        format!(
            "The text reader's model, {}, could not be loaded ({error}). Reinstall AppyTappy to \
             put it back",
            model.display()
        )
    })?;
    OcrEngine::new(OcrEngineParams {
        recognition_model: Some(model),
        ..OcrEngineParams::default()
    })
    .map_err(|error| format!("The text reader could not start: {error}"))
}

/// What `engine` reads on `line`.
fn read(engine: &OcrEngine, line: &Line) -> Text {
    match recognize(engine, line) {
        Ok(text) => Text::Read(text),
        Err(error) => Text::Failed(format!("The text reader failed: {error}")),
    }
}

/// The text `engine` recognizes on `line`, taken as one line of text: the
/// region holds one counter, so no text needs finding first.
// Only the real model can read text, and it is not in the repository or the
// test lanes, so no test here can tell a mutant of this body apart.
#[mutants::skip]
fn recognize(engine: &OcrEngine, line: &Line) -> Result<String, Box<dyn Error + Send + Sync>> {
    let source = ImageSource::from_bytes(&line.pixels, (line.width, line.height))?;
    let input = engine.prepare_input(source)?;
    let whole = Rect::new(
        Point::from_yx(0.0, 0.0),
        Point::from_yx(line.height as f32, line.width as f32),
    );
    let lines = engine.recognize_text(&input, &[vec![RotatedRect::from_rect(whole)]])?;
    Ok(lines
        .into_iter()
        .flatten()
        .map(|text| text.to_string().trim().to_owned())
        .collect::<Vec<_>>()
        .join(" "))
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn the_model_is_beside_the_program() {
        let program = Path::new("install").join("appytappy-setup.exe");
        assert_eq!(
            model_path(&program),
            Path::new("install").join("models").join(MODEL_FILE)
        );
    }

    #[test]
    fn a_missing_model_says_why_for_each_reading_and_the_reader_ends_with_its_jobs() {
        let (jobs, inbox) = mpsc::channel();
        let (messages, outbox) = mpsc::channel();
        let model = std::env::temp_dir().join("appytappy-no-model.onnx");
        for index in 0..2 {
            jobs.send(Job {
                recording: 4,
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
                Message::Recognized {
                    recording: 4,
                    index,
                    text: Text::Failed(why),
                } => {
                    assert_eq!(index, expected);
                    assert!(why.contains("appytappy-no-model.onnx"), "{why}");
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
                recording,
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
                Message::Recognized {
                    recording, index, ..
                } => (recording, index),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(answered, [(2, 0)]);
    }
}
