//! The runtime trace: an opt-in record of what the runtime reads and does, with
//! timings, for benchmarking.
//!
//! Each thread that records holds a [`Trace`]. Off, it holds nothing, and
//! each place that could record checks that and does no more: no clock read,
//! no record, no send. On, it holds a [`Tracer`] that knows which
//! [`TraceCategory`]s to record and sends each [`Record`] to the trace writer
//! thread, which owns the file. A record carries no text of its own, and a
//! send never waits: when the writer's buffer is full, the record is dropped
//! and counted, and the next record that gets through carries the count, so
//! the file says how many were lost. See `wiki/developer/Trace.md`.

pub(crate) mod switch;
pub(crate) mod writer;

use std::sync::mpsc::{SyncSender, TrySendError};
use std::time::{Duration, Instant};

use appytappy_config::settings_file::TraceCategory;
use appytappy_config::signal::Value;

use crate::workers::reader::SessionId;

/// A set of [`TraceCategory`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Categories(u8);

impl Categories {
    /// Whether the set holds `category`.
    pub(crate) const fn contains(self, category: TraceCategory) -> bool {
        self.0 & bit(category) != 0
    }
}

impl FromIterator<TraceCategory> for Categories {
    fn from_iter<I: IntoIterator<Item = TraceCategory>>(categories: I) -> Self {
        Self(
            categories
                .into_iter()
                .fold(0, |bits, category| bits | bit(category)),
        )
    }
}

/// `category`'s bit in [`Categories`].
const fn bit(category: TraceCategory) -> u8 {
    match category {
        TraceCategory::Readings => 1,
        TraceCategory::Changes => 2,
        TraceCategory::Rules => 4,
        TraceCategory::Actions => 8,
        TraceCategory::Lifecycle => 16,
    }
}

/// What a trace row says happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum What {
    /// A reader read a value from a frame; `took` is how long the check
    /// took.
    Read,
    /// A reader sent a value that changed.
    Changed,
    /// The owner received a changed value; `took` is how long since its
    /// frame arrived.
    Received,
    /// The owner handled an event; `took` is how long that took, and the
    /// detail says which kind of event.
    Handled,
    /// A rule fired.
    Fired,
    /// A rule would have fired, but its cooldown held it back.
    Held,
    /// A rule was released.
    Released,
    /// The owner queued an automation's actions, or their undo.
    Queued,
    /// The action thread started an automation's actions, or their undo.
    Started,
    /// The action thread finished them; `took` is how long that took, and
    /// the detail says whether something failed.
    Done,
    /// Detection took a process snapshot; `took` is how long that took.
    Snapshot,
    /// A reader started reading a session's values.
    Reading,
    /// A reader stopped.
    Stopped,
    /// The owner read the saved files again; `took` is how long that took.
    Reloaded,
}

impl What {
    /// The category whose rows this is.
    pub(crate) const fn category(self) -> TraceCategory {
        match self {
            Self::Read => TraceCategory::Readings,
            Self::Changed | Self::Received => TraceCategory::Changes,
            Self::Handled | Self::Fired | Self::Held | Self::Released => TraceCategory::Rules,
            Self::Queued | Self::Started | Self::Done => TraceCategory::Actions,
            Self::Snapshot | Self::Reading | Self::Stopped | Self::Reloaded => {
                TraceCategory::Lifecycle
            },
        }
    }
}

/// Extra words on a row, picked from a fixed set so a record holds no text
/// of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Detail {
    /// Nothing more.
    None,
    /// A word or two, such as the kind of event or `ok`.
    Note(&'static str),
    /// A count, such as how many processes a snapshot listed.
    Count(usize),
}

/// One row of the trace, as a thread records it. It holds no text of its
/// own, so building one allocates nothing; the writer turns the session and
/// item into names.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Record {
    /// When it happened.
    pub(crate) at: Instant,
    /// What happened.
    pub(crate) what: What,
    /// The session whose application it concerns.
    pub(crate) session: Option<SessionId>,
    /// The value's position in the session's plan, for readings and
    /// changes, or the rule's, for rules and actions.
    pub(crate) item: Option<usize>,
    /// The value read.
    pub(crate) value: Option<Value>,
    /// How long it took.
    pub(crate) took: Option<Duration>,
    /// When the frame arrived whose reading caused it, which ties together
    /// every row one reading causes.
    pub(crate) cause: Option<Instant>,
    /// Extra words.
    pub(crate) detail: Detail,
    /// How many records the same thread dropped since its last one got
    /// through, because the writer's buffer was full.
    pub(crate) dropped: u64,
}

impl Record {
    /// A record of `what` happening now, with nothing more.
    pub(crate) fn now(what: What) -> Self {
        Self {
            at: Instant::now(),
            what,
            session: None,
            item: None,
            value: None,
            took: None,
            cause: None,
            detail: Detail::None,
            dropped: 0,
        }
    }
}

/// A thread's handle to a running trace.
#[derive(Debug, Clone)]
pub(crate) struct Tracer {
    /// The trace's number, which tells one trace from the next.
    id: u64,
    /// What to record.
    categories: Categories,
    /// The writer's buffer.
    entries: SyncSender<writer::Entry>,
    /// How many records this handle dropped since one last got through.
    dropped: u64,
}

/// The trace as a thread holds it: off, or a [`Tracer`].
///
/// Two are equal when both are off, or both record the same categories of
/// the same trace, so a thread is given a new one only when that changes.
#[derive(Debug, Clone, Default)]
pub(crate) struct Trace(Option<Tracer>);

impl Trace {
    /// No trace.
    pub(crate) const OFF: Self = Self(None);

    /// Whether `category` is recorded.
    pub(crate) fn wants(&self, category: TraceCategory) -> bool {
        self.0
            .as_ref()
            .is_some_and(|tracer| tracer.categories.contains(category))
    }

    /// Records `what` when its category is recorded: `fill` completes the
    /// record, which starts with the time now. Off, or for a category that
    /// is not recorded, nothing is built or sent. Never waits; a record the
    /// writer has no room for is counted and the next one carries the count.
    pub(crate) fn record(&mut self, what: What, fill: impl FnOnce(Record) -> Record) {
        let Some(tracer) = self
            .0
            .as_mut()
            .filter(|tracer| tracer.categories.contains(what.category()))
        else {
            return;
        };
        let record = Record {
            dropped: tracer.dropped,
            ..fill(Record::now(what))
        };
        match tracer.entries.try_send(writer::Entry::Record(record)) {
            Ok(()) => tracer.dropped = 0,
            Err(TrySendError::Full(_)) => tracer.dropped = tracer.dropped.saturating_add(1),
            // The writer stopped, as when it could not write: there is
            // nobody left to count for.
            Err(TrySendError::Disconnected(_)) => self.0 = None,
        }
    }
}

impl PartialEq for Trace {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (None, None) => true,
            (Some(one), Some(other)) => one.id == other.id && one.categories == other.categories,
            _ => false,
        }
    }
}

#[cfg(test)]
impl Trace {
    /// A trace whose records go to the receiver it returns rather than to a
    /// writer.
    pub(crate) fn for_test(
        id: u64,
        categories: &[TraceCategory],
    ) -> (Self, std::sync::mpsc::Receiver<writer::Entry>) {
        Self::with_room(id, categories, writer::BUFFER)
    }

    fn with_room(
        id: u64,
        categories: &[TraceCategory],
        room: usize,
    ) -> (Self, std::sync::mpsc::Receiver<writer::Entry>) {
        let (entries, received) = std::sync::mpsc::sync_channel(room);
        let trace = Self(Some(Tracer {
            id,
            categories: categories.iter().copied().collect(),
            entries,
            dropped: 0,
        }));
        (trace, received)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn tracer(
        id: u64,
        categories: &[TraceCategory],
        room: usize,
    ) -> (Trace, mpsc::Receiver<writer::Entry>) {
        Trace::with_room(id, categories, room)
    }

    fn records(received: &mpsc::Receiver<writer::Entry>) -> Vec<Record> {
        received
            .try_iter()
            .filter_map(|entry| match entry {
                writer::Entry::Record(record) => Some(record),
                writer::Entry::Names(..) | writer::Entry::Stop => None,
            })
            .collect()
    }

    #[test]
    fn off_builds_no_record() {
        let mut off = Trace::OFF;
        off.record(What::Fired, |_record| panic!("built a record while off"));
        assert!(!off.wants(TraceCategory::Rules));
    }

    #[test]
    fn a_category_left_out_builds_no_record() {
        let (mut trace, received) = tracer(1, &[TraceCategory::Rules], 4);
        trace.record(What::Read, |_record| panic!("built a reading"));
        trace.record(What::Fired, |record| Record {
            item: Some(2),
            ..record
        });
        let sent = records(&received);
        assert_eq!(sent.len(), 1);
        assert_eq!((sent[0].what, sent[0].item), (What::Fired, Some(2)));
        assert!(trace.wants(TraceCategory::Rules));
        assert!(!trace.wants(TraceCategory::Readings));
    }

    #[test]
    fn a_full_buffer_drops_records_and_the_next_one_counts_them() {
        let (mut trace, received) = tracer(1, &[TraceCategory::Rules], 2);
        for _ in 0..5 {
            trace.record(What::Fired, |record| record);
        }
        let first = records(&received);
        assert_eq!(
            first
                .iter()
                .map(|record| record.dropped)
                .collect::<Vec<_>>(),
            [0, 0]
        );
        trace.record(What::Held, |record| record);
        let next = records(&received);
        assert_eq!(next.len(), 1);
        assert_eq!((next[0].what, next[0].dropped), (What::Held, 3));
        trace.record(What::Held, |record| record);
        assert_eq!(records(&received)[0].dropped, 0, "the count starts over");
    }

    #[test]
    fn a_trace_whose_writer_stopped_turns_itself_off() {
        let (mut trace, received) = tracer(1, &[TraceCategory::Rules], 2);
        drop(received);
        trace.record(What::Fired, |record| record);
        assert_eq!(trace, Trace::OFF);
    }

    #[test]
    fn traces_are_equal_when_they_record_the_same_categories_of_one_trace() {
        let (one, _one) = tracer(1, &[TraceCategory::Rules], 1);
        let (same, _same) = tracer(1, &[TraceCategory::Rules], 1);
        let (more, _more) = tracer(1, &[TraceCategory::Rules, TraceCategory::Actions], 1);
        let (next, _next) = tracer(2, &[TraceCategory::Rules], 1);
        assert_eq!(one, same);
        assert_ne!(one, more);
        assert_ne!(one, next);
        assert_ne!(one, Trace::OFF);
        assert_eq!(Trace::OFF, Trace::default());
    }

    #[test]
    fn each_row_belongs_to_one_category() {
        let readings: Categories = std::iter::once(TraceCategory::Readings).collect();
        assert!(readings.contains(What::Read.category()));
        assert!(!readings.contains(What::Changed.category()));
        assert_eq!(What::Received.category(), TraceCategory::Changes);
        assert_eq!(What::Held.category(), TraceCategory::Rules);
        assert_eq!(What::Done.category(), TraceCategory::Actions);
        assert_eq!(What::Reloaded.category(), TraceCategory::Lifecycle);
    }
}
