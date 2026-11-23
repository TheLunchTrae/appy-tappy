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
    /// When the value asked, which comes back with the answer and tells
    /// it from the answer to a question the value asked before.
    pub(super) since: Instant,
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
    /// When the value asked.
    pub(super) since: Instant,
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
    /// The values whose question found no thread to take it, and why, until
    /// the next frame says so.
    unasked: Vec<(usize, String)>,
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
            unasked: Vec::new(),
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
            since: now,
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
        self.unasked.push((value, NO_THREAD.to_owned()));
    }

    /// Whether a value waits for an answer, so the frame reader has to
    /// look for it though the window shows nothing new.
    pub(super) fn asked(&self) -> bool {
        let asked = |guess: &Guess| matches!(guess.asking, Asking::Asked { .. });
        self.values.iter().any(asked)
    }

    /// Takes the answers that came since the last frame. A value whose
    /// answer agrees with every character its reader has learned reads
    /// with the guessed shapes from now on, and may ask again. One whose
    /// answer does not waits before it asks again. `learned` gives the
    /// reader a value's check reads with, for a value without a guess
    /// yet. An answer to another question than the value's last is
    /// dropped. Returns why the text reader could not answer, for each
    /// value it could not, and for each whose question no thread took.
    pub(super) fn hear<'reader>(
        &mut self,
        now: Instant,
        learned: impl Fn(usize) -> Option<&'reader DigitReader>,
    ) -> Vec<(usize, String)> {
        let mut failed = std::mem::take(&mut self.unasked);
        for answer in self.answers.try_iter() {
            let Answer {
                value,
                since: asked,
                image,
                said,
            } = answer;
            let Some(guess) = self.values.get_mut(value) else {
                continue;
            };
            let Asking::Asked { since, wait } = guess.asking else {
                continue;
            };
            if since != asked {
                continue;
            }
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
    let Ok(mut ask) = asks.recv_timeout(idle) else {
        return;
    };
    let reader = panic::catch_unwind(AssertUnwindSafe(load))
        .unwrap_or_else(|_panic| Err(PANICKED.to_owned()));
    loop {
        let said = match &reader {
            Ok(reader) => panic::catch_unwind(AssertUnwindSafe(|| read(reader, &ask.line)))
                .unwrap_or_else(|_panic| Err(PANICKED.to_owned())),
            Err(why) => Err(why.clone()),
        };
        let answer = Answer {
            value: ask.value,
            since: ask.since,
            image: ask.image,
            said,
        };
        if ask.reply.send(answer).is_err() {
            return;
        }
        ask = match asks.recv_timeout(idle) {
            Ok(next) => next,
            Err(_ended) => return,
        };
    }
}

/// Why a value cannot guess when the text reader panicked.
const PANICKED: &str =
    "cannot guess at the characters its reference has not learned: the text reader failed";

/// Why a value cannot guess when the text reader's thread cannot start.
const NO_THREAD: &str =
    "cannot guess at the characters its reference has not learned: the text reader cannot start";

#[cfg(test)]
mod tests {
    use appytappy_config::signal::Number;
    use appytappy_sources::screen::Size;

    use super::*;

    /// A 12 by 7 picture, black on white: a bar, and beside it, when
    /// `hooked`, a hook, a shape unlike the bar.
    fn picture(hooked: bool) -> Image {
        let size = Size {
            width: 12,
            height: 7,
        };
        let bgra = (0..7_u32)
            .flat_map(|y| (0..12_u32).map(move |x| (x, y)))
            .flat_map(|(x, y)| {
                let bar = (1..3).contains(&x);
                let hook = hooked && (x == 6 || (y == 5 && (6..10).contains(&x)));
                let level = if (1..6).contains(&y) && (bar || hook) {
                    0
                } else {
                    255
                };
                [level, level, level, 255]
            })
            .collect();
        Image::new(size, bgra).expect("a 12 by 7 image")
    }

    /// A bar and a hook: a 1 and a shape [`knowing`] has not learned.
    fn mark() -> Image {
        picture(true)
    }

    /// A reader that knows the bar as a 1, and guesses.
    fn knowing() -> DigitReader {
        let mut reader = DigitReader::new();
        reader.learn(&picture(false), "1").expect("one character");
        reader.with_guesses(None, None)
    }

    /// Guesses for one value, the questions its text reader gets, and a
    /// mark for each time its thread started.
    fn guesses() -> (Guesses, Receiver<Ask>, Receiver<()>) {
        let (asks, asked) = mpsc::channel();
        let (started, starts) = mpsc::channel();
        let start: Start = Box::new(move || {
            started.send(()).ok()?;
            Some(asks.clone())
        });
        (Guesses::new(1, start), asked, starts)
    }

    /// Answers `ask` with `said`.
    fn reply(ask: Ask, said: Result<&str, &str>) {
        ask.reply
            .send(Answer {
                value: ask.value,
                since: ask.since,
                image: ask.image,
                said: said.map(str::to_owned).map_err(str::to_owned),
            })
            .expect("the guesses listen");
    }

    fn asking(guesses: &Guesses) -> Option<Asking> {
        guesses.values.first().map(|guess| guess.asking)
    }

    #[test]
    fn a_value_asks_once_until_its_answer_comes() {
        let (mut guesses, asked, starts) = guesses();
        let now = Instant::now();
        assert!(!guesses.asked());
        guesses.ask(0, &knowing(), &mark(), now);
        assert!(guesses.asked());
        guesses.ask(0, &knowing(), &mark(), now + Duration::from_secs(29));
        assert_eq!(asked.try_iter().count(), 1);
        assert_eq!(starts.try_iter().count(), 1);
        // An answer that never comes is asked for again.
        guesses.ask(0, &knowing(), &mark(), now + ANSWER_WITHIN);
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
        guesses.ask(0, &knowing(), &blank, now);
        // A value the frame reader does not have.
        guesses.ask(1, &knowing(), &mark(), now);
        assert_eq!(asked.try_iter().count(), 0);
        assert_eq!(starts.try_iter().count(), 0);
        assert_eq!(asking(&guesses), Some(Asking::Ready));
    }

    #[test]
    fn a_guess_a_value_cannot_take_makes_it_wait_twice_as_long_each_time() {
        let (mut guesses, asked, _starts) = guesses();
        let reader = knowing();
        let mut now = Instant::now();
        for wait in [1, 2, 4, 8, 16, 32, 60, 60].map(Duration::from_secs) {
            guesses.ask(0, &reader, &mark(), now);
            // One character for the two marks.
            reply(asked.try_recv().expect("it asks"), Ok("1"));
            assert_eq!(guesses.hear(now, |_| Some(&reader)), []);
            assert!(guesses.reader(0).is_none());
            assert!(!guesses.asked());
            // Nothing is asked until the wait is over.
            guesses.ask(0, &reader, &mark(), now + (wait / 2));
            assert_eq!(asked.try_iter().count(), 0, "after {wait:?}");
            now += wait;
        }
    }

    #[test]
    fn a_guess_that_agrees_teaches_the_value_s_reader_and_lets_it_ask_again() {
        let (mut guesses, asked, _starts) = guesses();
        let reader = knowing();
        let now = Instant::now();
        guesses.ask(0, &reader, &mark(), now);
        reply(asked.try_recv().expect("it asks"), Ok("17"));
        assert_eq!(guesses.hear(now, |_| Some(&reader)), []);
        let taught = guesses.reader(0).expect("a guessed reader").clone();
        assert_eq!(taught.read(&mark(), false).ok(), Number::new(17.0));
        // The next guess builds on the guessed reader, not on the one the
        // check reads with: a second answer for a shape it now knows
        // teaches nothing, and the value waits.
        guesses.ask(0, &reader, &mark(), now);
        reply(
            asked.try_recv().expect("it may ask again at once"),
            Ok("17"),
        );
        assert_eq!(guesses.hear(now, |_| None), []);
        assert_eq!(guesses.reader(0), Some(&taught));
        assert_eq!(asking(&guesses), Some(waiting(now, FIRST_WAIT)));
    }

    #[test]
    fn a_text_reader_that_cannot_answer_says_why_and_the_value_waits() {
        let (mut guesses, asked, _starts) = guesses();
        let reader = knowing();
        let now = Instant::now();
        guesses.ask(0, &reader, &mark(), now);
        let ask = asked.try_recv().expect("it asks");
        // An answer for a value the frame reader does not have is dropped.
        ask.reply
            .send(Answer {
                value: 5,
                since: now,
                image: mark(),
                said: Err("no model".to_owned()),
            })
            .expect("the guesses listen");
        reply(ask, Err("no model"));
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
    fn an_answer_the_value_does_not_wait_for_changes_nothing() {
        let (mut guesses, asked, _starts) = guesses();
        let reader = knowing();
        let now = Instant::now();
        let good = |since: Instant| Answer {
            value: 0,
            since,
            image: mark(),
            said: Ok("17".to_owned()),
        };
        // One that comes while the value has asked nothing.
        guesses.reply.send(good(now)).expect("the guesses listen");
        assert_eq!(guesses.hear(now, |_| Some(&reader)), []);
        assert_eq!(asking(&guesses), Some(Asking::Ready));
        // One to a question the value asked before its last.
        let again = now + ANSWER_WITHIN;
        guesses.ask(0, &reader, &mark(), now);
        guesses.ask(0, &reader, &mark(), again);
        guesses.reply.send(good(now)).expect("the guesses listen");
        assert_eq!(guesses.hear(again, |_| Some(&reader)), []);
        assert!(guesses.reader(0).is_none());
        assert!(guesses.asked(), "it still waits for its last question's");
        // One that comes while the value waits after a guess it refused.
        reply(asked.try_iter().last().expect("it asked twice"), Ok("1"));
        assert_eq!(guesses.hear(again, |_| Some(&reader)), []);
        guesses.reply.send(good(again)).expect("the guesses listen");
        assert_eq!(guesses.hear(again, |_| Some(&reader)), []);
        assert!(guesses.reader(0).is_none());
        assert_eq!(asking(&guesses), Some(waiting(again, FIRST_WAIT)));
    }

    #[test]
    fn a_thread_that_ended_is_started_again_and_one_that_cannot_start_is_said() {
        // The first thread is gone before it takes a question.
        let (asks, asked) = mpsc::channel();
        let mut threads = vec![Some(asks), Some(mpsc::channel().0)];
        let start: Start = Box::new(move || threads.pop().flatten());
        let mut guesses = Guesses::new(1, start);
        let reader = knowing();
        let now = Instant::now();
        guesses.ask(0, &reader, &mark(), now);
        assert_eq!(asked.try_iter().count(), 1);
        assert_eq!(guesses.hear(now, |_| Some(&reader)), []);
        // No thread starts: the value waits, and the next frame says why.
        drop(asked);
        let later = now + ANSWER_WITHIN;
        guesses.ask(0, &reader, &mark(), later);
        assert_eq!(asking(&guesses), Some(waiting(later, FIRST_WAIT)));
        assert_eq!(
            guesses.hear(later, |_| Some(&reader)),
            [(0, NO_THREAD.to_owned())]
        );
        assert_eq!(guesses.hear(later, |_| Some(&reader)), []);
    }

    #[test]
    fn the_text_reader_s_thread_answers_every_question_and_ends_when_none_comes() {
        let (asks, asked) = mpsc::channel();
        let (reply, answers) = mpsc::channel();
        let now = Instant::now();
        let ask = |value: usize| Ask {
            value,
            since: now,
            image: mark(),
            line: Line {
                width: 1,
                height: 1,
                pixels: vec![0],
            },
            reply: reply.clone(),
        };
        for value in 0..2 {
            asks.send(ask(value)).expect("the thread listens");
        }
        // It ends by itself while `asks` is still open.
        answer(&asked, Duration::from_millis(1), || {
            Err("no model".to_owned())
        });
        let said: Vec<_> = answers
            .try_iter()
            .map(|answer| (answer.value, answer.since, answer.said))
            .collect();
        assert_eq!(
            said,
            [
                (0, now, Err("no model".to_owned())),
                (1, now, Err("no model".to_owned()))
            ]
        );
        // A panic while the model loads is an answer too.
        asks.send(ask(3)).expect("the thread listens");
        answer(&asked, Duration::from_millis(1), || {
            panic!("a broken model")
        });
        let said: Vec<_> = answers.try_iter().map(|answer| answer.said).collect();
        assert_eq!(said, [Err(PANICKED.to_owned())]);
    }
}
