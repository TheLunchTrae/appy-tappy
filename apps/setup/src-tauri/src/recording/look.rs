//! Looking over a whole window's frame for text like a region's, for a
//! recording of the whole window.
//!
//! It runs on a thread of its own, because one look takes from tens of
//! milliseconds to seconds, by the window's size and the build, where the
//! owner thread answers the UI in microseconds. The owner sends one frame
//! at a time and sends the next only once this one is answered, so a look
//! that takes longer than the time between looks makes the looks rarer
//! rather than queuing frames.

use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use appytappy_sources::screen::digits::{DigitReader, Shape};
use appytappy_sources::screen::{Image, Region};

use crate::owner::Message;

use super::find::{self, Sizes};

/// One frame to look over.
#[derive(Debug)]
pub(crate) struct Job {
    /// The recording it is for, by its number; a look for one the user
    /// discarded is dropped.
    pub(crate) recording: u64,
    /// The window's whole frame.
    pub(crate) frame: Image,
    /// When the frame arrived.
    pub(crate) at: Instant,
    /// How tall the characters it looks for are, at each size.
    pub(crate) sizes: Sizes,
    /// The recording's region, which the text it takes first is nearest to.
    pub(crate) near: Region,
    /// What tells the characters' ink from background.
    pub(crate) reader: DigitReader,
}

/// One run of text a look found.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Seen {
    /// Where it is on the frame.
    pub(crate) spot: Region,
    /// How tall the characters are that the look took it for, in pixels:
    /// the one of the job's sizes it has.
    pub(crate) size: u32,
    /// Its pixels and the shape of each of its characters. `None` for a run
    /// of more characters than a reading holds, which is a sentence in the
    /// characters' color and not a value.
    pub(crate) reading: Option<(Image, Vec<Shape>)>,
}

/// Each run of text `job`'s frame shows like the region's, nearest the
/// region first.
pub(crate) fn look(job: &Job) -> Vec<Seen> {
    find::lines(&job.frame, &job.sizes, job.near, &job.reader)
        .into_iter()
        .map(|find::Spot { region, size }| Seen {
            spot: region,
            size,
            reading: job.frame.crop(region).and_then(|crop| {
                let shapes = job.reader.shapes(&crop).ok()?.characters().to_vec();
                Some((crop, shapes))
            }),
        })
        .collect()
}

/// Has `look` look over each job's frame and sends the owner what it
/// shows, until the owner drops its sender. A look that panics answers
/// `None`, so the owner stops the recording with a reason rather than
/// waiting for it.
pub(crate) fn run(
    jobs: &Receiver<Job>,
    messages: &Sender<Message>,
    look: impl Fn(&Job) -> Vec<Seen>,
) {
    while let Ok(job) = jobs.recv() {
        let seen = panic::catch_unwind(AssertUnwindSafe(|| look(&job))).ok();
        let looked = Message::Looked {
            recording: job.recording,
            at: job.at,
            seen,
        };
        if messages.send(looked).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use appytappy_sources::system::Rgb;

    use super::*;

    const RED: Rgb = Rgb {
        red: 255,
        green: 0,
        blue: 0,
    };

    #[test]
    fn a_look_that_panics_answers_that_it_failed_and_the_next_is_looked_over() {
        let size = appytappy_sources::screen::Size {
            width: 2,
            height: 2,
        };
        let job = |recording| Job {
            recording,
            frame: Image::new(size, [40, 40, 40, 0].repeat(4)).expect("matching length"),
            at: Instant::now(),
            sizes: Sizes::of(2),
            near: Region::new(0, 0, 2, 2).expect("not empty"),
            reader: DigitReader::new(RED, 0),
        };
        let (jobs, inbox) = mpsc::channel();
        let (sender, messages) = mpsc::channel();
        jobs.send(job(1)).expect("open");
        jobs.send(job(2)).expect("open");
        drop(jobs);
        run(&inbox, &sender, |job| {
            assert_ne!(job.recording, 1, "a look that fails");
            Vec::new()
        });
        let answers: Vec<(u64, Option<usize>)> = messages
            .try_iter()
            .map(|message| match message {
                Message::Looked {
                    recording, seen, ..
                } => (recording, seen.map(|seen| seen.len())),
                _ => panic!("only looks are answered"),
            })
            .collect();
        assert_eq!(answers, [(1, None), (2, Some(0))]);
    }

    #[test]
    fn each_frame_sent_is_answered_for_its_recording_until_the_owner_goes() {
        // A 6 by 6 frame with a red stroke 4 tall at its left.
        let pixels = (0..6_u32)
            .flat_map(|y| (0..6_u32).map(move |x| (x, y)))
            .flat_map(|(x, y)| {
                if x < 2 && (1..5).contains(&y) {
                    [0, 0, 255, 0]
                } else {
                    [40, 40, 40, 0]
                }
            })
            .collect();
        let size = appytappy_sources::screen::Size {
            width: 6,
            height: 6,
        };
        let frame = Image::new(size, pixels).expect("matching length");
        let at = Instant::now();
        let (jobs, inbox) = mpsc::channel();
        let (sender, messages) = mpsc::channel();
        jobs.send(Job {
            recording: 7,
            frame,
            at,
            sizes: Sizes::of(4),
            near: Region::new(0, 0, 3, 6).expect("not empty"),
            reader: DigitReader::new(RED, 0),
        })
        .expect("open");
        drop(jobs);
        run(&inbox, &sender, look);
        let Ok(Message::Looked {
            recording,
            at: answered,
            seen: Some(seen),
        }) = messages.try_recv()
        else {
            panic!("the look is answered");
        };
        assert_eq!((recording, answered), (7, at));
        let [seen] = seen.as_slice() else {
            panic!("one run of text");
        };
        let (crop, shapes) = seen.reading.as_ref().expect("few characters");
        assert_eq!(seen.size, 4);
        assert_eq!(crop.size(), seen.spot.size());
        assert_eq!(shapes.len(), 1);
        assert!(messages.try_recv().is_err(), "and it ends with the owner");
    }
}
