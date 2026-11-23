//! Guesses at the characters a value's reference has not learned.
//!
//! A number whose reference guesses
//! ([`DigitReader::guesses`]) and whose region shows a shape the reference
//! has not learned is not read. The frame reader then asks the text reader
//! what the region says, on a thread of its own, because the text reader
//! takes tens of milliseconds for a line where a frame is read in under
//! one. The answer comes back with a later frame. Where it agrees with
//! every character the reference has learned
//! ([`DigitReader::guessing`]), the value reads from then on with a reader
//! that knows the guessed shapes too, so each shape is guessed once.
//!
//! The text reader's thread starts with the first question, loads its
//! model then, and ends once it has had no question for [`IDLE`], so a
//! session whose references guess nothing, or guessed all there is, holds
//! no model. A frame reader has its own thread and its own answers, so no
//! answer reaches the frame reader of another capture.
//!
//! `wiki/State-machines.md` maps each value's guess.

use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use appytappy_sources::screen::Image;
use appytappy_sources::screen::digits::DigitReader;
use appytappy_sources::screen::text::{self, Line, TextReader};

/// How long a value waits after a guess it could not take before it asks
/// again, at first. Each such guess in a row doubles the wait, up to
/// [`LONGEST_WAIT`]: a region the text reader reads wrong would otherwise
/// keep it reading for every frame.
pub(super) const FIRST_WAIT: Duration = Duration::from_secs(1);

/// The longest a value waits before it asks again.
pub(super) const LONGEST_WAIT: Duration = Duration::from_secs(60);

/// How long a value waits for an answer before it asks again. The text
/// reader's thread answers every question it takes, and can end between a
/// question's sending and its taking.
pub(super) const ANSWER_WITHIN: Duration = Duration::from_secs(30);

/// How long the text reader's thread waits for a question before it ends,
/// and its model with it.
const IDLE: Duration = Duration::from_secs(30);

/// A question for the text reader: what a value's region says.
#[derive(Debug)]
pub(super) struct Ask {
    /// The value, by its position among its application's.
    pub(super) value: usize,
    /// The region's pixels, which come back with the answer.
    pub(super) image: Image,
    /// The line of text in them.
    pub(super) line: Line,
    /// Where the answer goes.
    pub(super) reply: Sender<Answer>,
}

/// The text reader's answer to an [`Ask`].
#[derive(Debug)]
pub(super) struct Answer {
    /// The value.
    pub(super) value: usize,
    /// The region's pixels as the question had them.
    pub(super) image: Image,
    /// What they say, or why the text reader could not say.
    pub(super) said: Result<String, String>,
}

/// Starts a text reader's thread and returns where it takes questions, or
/// `None` when it cannot start.
pub(super) type Start = Box<dyn FnMut() -> Option<Sender<Ask>> + Send>;

/// Where one value's guess stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asking {
    /// It may ask.
    Ready,
    /// It asked at `since`, and has no answer yet.
    Asked {
        since: Instant,
        /// How long it waits if it cannot take the answer.
        wait: Duration,
    },
    /// It could not take its last answer, and asks no more until `until`.
    Waiting {
        until: Instant,
        /// How long it waits after the next such answer.
        wait: Duration,
    },
}

/// One value's guess.
#[derive(Debug)]
struct Guess {
    /// The value's reader with the shapes guessed so far, once a guess
    /// was taken.
    reader: Option<DigitReader>,
    asking: Asking,
}

/// The guesses of one frame reader's values.
pub(super) struct Guesses {
    values: Vec<Guess>,
    start: Start,
    /// The text reader's thread, once a value asked.
    asks: Option<Sender<Ask>>,
    reply: Sender<Answer>,
    answers: Receiver<Answer>,
}

impl Guesses {
    /// No guesses yet for `count` values, with `start` to start the text
    /// reader's thread when one asks.
    pub(super) fn new(count: usize, start: Start) -> Self {
        let (reply, answers) = mpsc::channel();
        let values = (0..count).map(|_| Guess {
            reader: None,
            asking: Asking::Ready,
        });
        Self {
            values: values.collect(),
            start,
            asks: None,
            reply,
            answers,
        }
    }

    /// The reader `value` reads with once a guess was taken for it.
    pub(super) fn reader(&self, value: usize) -> Option<&DigitReader> {
        self.values.get(value)?.reader.as_ref()
    }

    /// Asks the text reader what `image` says, the region of `value`,
    /// which its reader found a shape in that it has not learned: the
    /// reader with the shapes guessed so far, or `learned`, the one its
    /// check reads with. It asks
    /// nothing while the value waits for an answer or after one it could
    /// not take, for a reader whose reference does not guess, or for an
    /// image without ink.
    pub(super) fn ask(&mut self, value: usize, learned: &DigitReader, image: &Image, now: Instant) {
        let Some(guess) = self.values.get_mut(value) else {
            return;
        };
        let reader = guess.reader.as_ref().unwrap_or(learned);
        let wait = match guess.asking {
            Asking::Ready => FIRST_WAIT,
            Asking::Asked { since, wait } if now.duration_since(since) >= ANSWER_WITHIN => wait,
            Asking::Waiting { until, wait } if now >= until => wait,
            Asking::Asked { .. } | Asking::Waiting { .. } => return,
        };
        if !reader.guesses() {
            return;
        }
        let Some(line) = Line::of(&reader.ink(image)) else {
            return;
        };
        let mut ask = Ask {
            value,
            image: image.clone(),
            line,
            reply: self.reply.clone(),
        };
        // The thread ends when it has had no question for a while, so a
        // question it does not take starts another, once.
        for _attempt in 0..2 {
            if self.asks.is_none() {
                self.asks = (self.start)();
            }
            match self.asks.as_ref().map(|asks| asks.send(ask)) {
                Some(Ok(())) => {
                    guess.asking = Asking::Asked { since: now, wait };
                    return;
                },
                Some(Err(mpsc::SendError(unsent))) => {
                    self.asks = None;
                    ask = unsent;
                },
                None => break,
            }
        }
        guess.asking = waiting(now, wait);
    }

    /// Takes the answers that came since the last frame. A value whose
    /// answer agrees with every character its reader has learned reads
    /// with the guessed shapes from now on, and may ask again. One whose
    /// answer does not waits before it asks again. `learned` gives the
    /// reader a value's check reads with, for a value without a guess
    /// yet. Returns why the text reader could not answer, for each value
    /// it could not.
    pub(super) fn hear<'reader>(
        &mut self,
        now: Instant,
        learned: impl Fn(usize) -> Option<&'reader DigitReader>,
    ) -> Vec<(usize, String)> {
        let mut failed = Vec::new();
        for Answer { value, image, said } in self.answers.try_iter() {
            let Some(guess) = self.values.get_mut(value) else {
                continue;
            };
            let Asking::Asked { wait, .. } = guess.asking else {
                continue;
            };
            let guessed = match said {
                Ok(said) => guess
                    .reader
                    .as_ref()
                    .or_else(|| learned(value))
                    .and_then(|reader| reader.guessing(&image, &said)),
                Err(why) => {
                    failed.push((value, why));
                    None
                },
            };
            guess.asking = match guessed {
                Some(reader) => {
                    guess.reader = Some(reader);
                    Asking::Ready
                },
                None => waiting(now, wait),
            };
        }
        failed
    }
}

/// A value that waits `wait` from `now`, and twice as long the next time,
/// up to [`LONGEST_WAIT`].
fn waiting(now: Instant, wait: Duration) -> Asking {
    Asking::Waiting {
        until: now.checked_add(wait).unwrap_or(now),
        wait: wait.saturating_mul(2).min(LONGEST_WAIT),
    }
}

/// Starts the text reader's thread, with the model beside this program.
pub(super) fn text_thread() -> Option<Sender<Ask>> {
    let program = std::env::current_exe().ok()?;
    let model = text::model_path(&program);
    let (asks, asked) = mpsc::channel();
    thread::Builder::new()
        .name("text reader".to_owned())
        .spawn(move || answer(&asked, IDLE, || load(&model)))
        .ok()
        .map(|_detached| asks)
}

/// The text reader with the model at `model`, or why it cannot guess.
fn load(model: &Path) -> Result<TextReader, String> {
    TextReader::load(model).map_err(|error| {
        format!(
            "cannot guess at the characters its reference has not learned: {error}; reinstall \
             AppyTappy to put the text reader back"
        )
    })
}

/// What `reader` reads `line` as, or why it cannot.
fn read(reader: &TextReader, line: &Line) -> Result<String, String> {
    reader.read(line).map_err(|error| {
        format!("cannot guess at the characters its reference has not learned: {error}")
    })
}

/// The text reader's thread: answers each of `asks` until none comes for
/// `idle` or the frame reader is gone. It loads the text reader with
/// `load` for its first question. A panic while it loads or reads is an
/// answer too, so no value waits for one that never comes.
fn answer(asks: &Receiver<Ask>, idle: Duration, load: impl FnOnce() -> Result<TextReader, String>) {
    let mut load = Some(load);
    let mut reader: Option<Result<TextReader, String>> = None;
    while let Ok(ask) = asks.recv_timeout(idle) {
        let Ask {
            value,
            image,
            line,
            reply,
        } = ask;
        let loaded = match (&reader, load.take()) {
            (None, Some(load)) => reader.insert(
                panic::catch_unwind(AssertUnwindSafe(load))
                    .unwrap_or_else(|_panic| Err(PANICKED.to_owned())),
            ),
            _ => reader.get_or_insert_with(|| Err(PANICKED.to_owned())),
        };
        let said = match loaded {
            Ok(reader) => panic::catch_unwind(AssertUnwindSafe(|| read(reader, &line)))
                .unwrap_or_else(|_panic| Err(PANICKED.to_owned())),
            Err(why) => Err(why.clone()),
        };
        if reply.send(Answer { value, image, said }).is_err() {
            return;
        }
    }
}

/// Why a value cannot guess when the text reader panicked.
const PANICKED: &str =
    "cannot guess at the characters its reference has not learned: the text reader failed";

#[cfg(test)]
mod tests {
    use appytappy_config::signal::Number;
    use appytappy_sources::screen::Size;

    use super::*;

    /// A black mark on white, which a reader that learned nothing finds
    /// ink in.
    fn mark() -> Image {
        let size = Size {
            width: 6,
            height: 6,
        };
        let bgra = (0..36_u32)
            .flat_map(|at| {
                let inked =
                    (2..4).contains(&at.rem_euclid(6)) && (1..5).contains(&at.div_euclid(6));
                let level = if inked { 0 } else { 255 };
                [level, level, level, 255]
            })
            .collect();
        Image::new(size, bgra).expect("a 6 by 6 image")
    }

    /// Guesses for one value, and the questions its text reader gets,
    /// with how many times the thread started.
    fn guesses() -> (Guesses, Receiver<Ask>, Receiver<()>) {
        let (asks, asked) = mpsc::channel();
        let (started, starts) = mpsc::channel();
        let start: Start = Box::new(move || {
            started.send(()).ok()?;
            Some(asks.clone())
        });
        (Guesses::new(1, start), asked, starts)
    }

    fn guessing() -> DigitReader {
        DigitReader::new().with_guesses(None, None)
    }

    #[test]
    fn a_value_asks_once_until_its_answer_comes() {
        let (mut guesses, asked, starts) = guesses();
        let now = Instant::now();
        guesses.ask(0, &guessing(), &mark(), now);
        guesses.ask(0, &guessing(), &mark(), now + Duration::from_secs(29));
        assert_eq!(asked.try_iter().count(), 1);
        assert_eq!(starts.try_iter().count(), 1);
        // An answer that never comes is asked for again.
        guesses.ask(0, &guessing(), &mark(), now + ANSWER_WITHIN);
        assert_eq!(asked.try_iter().count(), 1);
        assert_eq!(
            starts.try_iter().count(),
            0,
            "the thread still takes questions"
        );
    }

    #[test]
    fn a_value_asks_nothing_for_a_reference_that_does_not_guess_or_a_region_without_ink() {
        let (mut guesses, asked, starts) = guesses();
        let now = Instant::now();
        guesses.ask(0, &DigitReader::new(), &mark(), now);
        let blank = Image::new(
            Size {
                width: 1,
                height: 1,
            },
            vec![255; 4],
        )
        .expect("one pixel");
        guesses.ask(0, &guessing(), &blank, now);
        // A value the frame reader does not have.
        guesses.ask(1, &guessing(), &mark(), now);
        assert_eq!(asked.try_iter().count(), 0);
        assert_eq!(starts.try_iter().count(), 0);
        assert_eq!(
            guesses.values.first().map(|guess| guess.asking),
            Some(Asking::Ready)
        );
    }

    #[test]
    fn a_guess_a_value_cannot_take_makes_it_wait_twice_as_long_each_time() {
        let (mut guesses, asked, _starts) = guesses();
        let reader = guessing();
        let mut now = Instant::now();
        for wait in [1, 2, 4, 8, 16, 32, 60, 60].map(Duration::from_secs) {
            guesses.ask(0, &reader, &mark(), now);
            let ask = asked.try_recv().expect("it asks");
            // Two characters for the one mark.
            ask.reply
                .send(Answer {
                    value: ask.value,
                    image: ask.image,
                    said: Ok("11".to_owned()),
                })
                .expect("the guesses listen");
            assert_eq!(guesses.hear(now, |_| Some(&reader)), []);
            assert!(guesses.reader(0).is_none());
            // Nothing is asked until the wait is over.
            let before = now + (wait / 2);
            guesses.ask(0, &reader, &mark(), before);
            assert_eq!(asked.try_iter().count(), 0, "after {wait:?}");
            now += wait;
        }
    }

    #[test]
    fn a_guess_that_agrees_teaches_the_value_s_reader_and_lets_it_ask_again() {
        let (mut guesses, asked, _starts) = guesses();
        let reader = guessing();
        let now = Instant::now();
        guesses.ask(0, &reader, &mark(), now);
        let ask = asked.try_recv().expect("it asks");
        let answer = |said: &str| Answer {
            value: 0,
            image: mark(),
            said: Ok(said.to_owned()),
        };
        ask.reply.send(answer("7")).expect("the guesses listen");
        assert_eq!(guesses.hear(now, |_| Some(&reader)), []);
        let taught = guesses.reader(0).expect("a guessed reader").clone();
        assert_eq!(taught.read(&mark(), false).ok(), Number::new(7.0));
        // The next guess builds on the guessed reader, not on the one the
        // check reads with: a second answer for a shape it now knows
        // teaches nothing, and the value waits.
        guesses.ask(0, &taught, &mark(), now);
        assert_eq!(asked.try_iter().count(), 1, "it may ask again at once");
        ask.reply.send(answer("7")).expect("the guesses listen");
        assert_eq!(guesses.hear(now, |_| None), []);
        assert_eq!(guesses.reader(0), Some(&taught));
        guesses.ask(0, &taught, &mark(), now);
        assert_eq!(asked.try_iter().count(), 0);
    }

    #[test]
    fn a_text_reader_that_cannot_answer_says_why_and_the_value_waits() {
        let (mut guesses, asked, _starts) = guesses();
        let reader = guessing();
        let now = Instant::now();
        guesses.ask(0, &reader, &mark(), now);
        let ask = asked.try_recv().expect("it asks");
        let failed = |value: usize| Answer {
            value,
            image: mark(),
            said: Err("no model".to_owned()),
        };
        ask.reply.send(failed(0)).expect("the guesses listen");
        // An answer for a value the frame reader does not have is dropped.
        ask.reply.send(failed(5)).expect("the guesses listen");
        assert_eq!(
            guesses.hear(now, |_| Some(&reader)),
            [(0, "no model".to_owned())]
        );
        guesses.ask(0, &reader, &mark(), now);
        assert_eq!(asked.try_iter().count(), 0);
        guesses.ask(0, &reader, &mark(), now + FIRST_WAIT);
        assert_eq!(asked.try_iter().count(), 1);
    }

    #[test]
    fn an_answer_nobody_waits_for_changes_nothing() {
        let (mut guesses, _asked, _starts) = guesses();
        let reader = guessing();
        guesses
            .reply
            .send(Answer {
                value: 0,
                image: mark(),
                said: Ok("7".to_owned()),
            })
            .expect("the guesses listen");
        assert_eq!(guesses.hear(Instant::now(), |_| Some(&reader)), []);
        assert!(guesses.reader(0).is_none());
    }

    #[test]
    fn a_thread_that_ended_is_started_again_for_the_next_question() {
        // The first thread is gone before it takes a question.
        let (asks, asked) = mpsc::channel();
        let mut threads = vec![Some(asks), Some(mpsc::channel().0)];
        let start: Start = Box::new(move || threads.pop().flatten());
        let mut guesses = Guesses::new(1, start);
        let now = Instant::now();
        guesses.ask(0, &guessing(), &mark(), now);
        assert_eq!(asked.try_iter().count(), 1);
        // No thread starts: the value waits.
        drop(asked);
        guesses.ask(0, &guessing(), &mark(), now + ANSWER_WITHIN);
        assert_eq!(
            guesses.values.first().map(|guess| guess.asking),
            Some(waiting(now + ANSWER_WITHIN, FIRST_WAIT))
        );
    }

    #[test]
    fn the_text_reader_s_thread_answers_every_question_and_ends_when_none_comes() {
        let (asks, asked) = mpsc::channel();
        let (reply, answers) = mpsc::channel();
        for value in 0..2 {
            asks.send(Ask {
                value,
                image: mark(),
                line: Line {
                    width: 1,
                    height: 1,
                    pixels: vec![0],
                },
                reply: reply.clone(),
            })
            .expect("the thread listens");
        }
        // It ends by itself while `asks` is still open.
        answer(&asked, Duration::from_millis(1), || {
            Err("no model".to_owned())
        });
        let said: Vec<_> = answers
            .try_iter()
            .map(|answer| (answer.value, answer.said))
            .collect();
        assert_eq!(
            said,
            [
                (0, Err("no model".to_owned())),
                (1, Err("no model".to_owned()))
            ]
        );
        // A panic while the model loads is an answer too.
        asks.send(Ask {
            value: 3,
            image: mark(),
            line: Line {
                width: 1,
                height: 1,
                pixels: vec![0],
            },
            reply,
        })
        .expect("the thread listens");
        answer(&asked, Duration::from_millis(1), || {
            panic!("a broken model")
        });
        let said: Vec<_> = answers.try_iter().map(|answer| answer.said).collect();
        assert_eq!(said, [Err(PANICKED.to_owned())]);
    }
}
